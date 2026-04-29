use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{audit, auth, health, services, users, AppState};

#[derive(Debug, Deserialize, Default)]
struct UsersApiQuery {
    limit: Option<usize>,
    q: Option<String>,
}

#[derive(Debug, Serialize)]
struct DashboardApiResponse {
    services: Vec<crate::services::ServiceStatus>,
    mail_queue_size: usize,
    users_count: usize,
    active_bans_count: i64,
    warnings: Vec<crate::health::HealthCheck>,
    audit_events: Vec<crate::audit::AuditEvent>,
}

#[derive(Debug, Serialize)]
struct UsersApiResponse {
    users: Vec<crate::users::UserMailbox>,
    total: usize,
}

#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    address: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct AddressActionRequest {
    address: String,
}

#[derive(Debug, Serialize)]
struct ActionResponse {
    ok: bool,
    action: String,
    address: String,
    message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/dashboard", get(dashboard))
        .route("/api/v1/admin/users", get(users_list))
        .route("/api/v1/admin/users/create", post(create_user))
        .route("/api/v1/admin/users/block", post(block_user))
        .route("/api/v1/admin/users/unblock", post(unblock_user))
        .route("/api/v1/admin/users/delete-account", post(delete_account))
}

async fn dashboard(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
) -> crate::error::AppResult<impl IntoResponse> {
    auth::require_admin(&state, &jar).await?;

    let blocked = crate::bans::active_values(&state.pool).await?;
    let user_rows = users::list_users(&state.shell, &blocked).await;
    let stats = services::collect_dashboard_stats(
        &state.pool,
        &state.shell,
        &state.config,
        user_rows.len(),
    )
    .await?;
    let audit_events = audit::latest(&state.pool, 20).await?;
    let warnings = health::run_health_checks(&state.shell, &state.config)
        .await
        .into_iter()
        .filter(|check| check.status != "ok")
        .collect();

    Ok(Json(DashboardApiResponse {
        services: stats.services,
        mail_queue_size: stats.mail_queue_size,
        users_count: stats.users_count,
        active_bans_count: stats.active_bans_count,
        warnings,
        audit_events,
    }))
}

async fn users_list(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Query(query): Query<UsersApiQuery>,
) -> crate::error::AppResult<impl IntoResponse> {
    auth::require_admin(&state, &jar).await?;

    let blocked = crate::bans::active_values(&state.pool).await?;
    let mut rows = users::list_users(&state.shell, &blocked).await;
    if let Some(filter) = query.q.as_deref() {
        let q = filter.trim().to_ascii_lowercase();
        if !q.is_empty() {
            rows.retain(|item| item.address.to_ascii_lowercase().contains(&q));
        }
    }
    let total = rows.len();
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }

    Ok(Json(UsersApiResponse { users: rows, total }))
}

fn csrf_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

async fn create_user(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    headers: HeaderMap,
    Json(payload): Json<CreateUserRequest>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let csrf = csrf_from_headers(&headers).ok_or(crate::error::AppError::Forbidden)?;
    auth::validate_csrf(&current, &csrf)?;

    match users::create_user_account(
        &state.shell,
        &state.config.health.domain,
        &payload.address,
        &payload.password,
    )
    .await
    {
        Ok(details) => {
            audit::log_event(
                &state.pool,
                Some(current.admin.id),
                "user_created",
                "user",
                &payload.address,
                json!({ "details": details }),
                None,
            )
            .await?;
            Ok(Json(ActionResponse {
                ok: true,
                action: "create_user".into(),
                address: payload.address,
                message: "User created".into(),
            }))
        }
        Err(err) => {
            audit::log_event(
                &state.pool,
                Some(current.admin.id),
                "user_create_failed",
                "user",
                &payload.address,
                json!({ "error": err.to_string() }),
                None,
            )
            .await?;
            Err(err)
        }
    }
}

async fn block_user(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    headers: HeaderMap,
    Json(payload): Json<AddressActionRequest>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let csrf = csrf_from_headers(&headers).ok_or(crate::error::AppError::Forbidden)?;
    auth::validate_csrf(&current, &csrf)?;

    crate::bans::add(
        &state.pool,
        &state.shell,
        &state.config,
        crate::bans::CreateBan {
            admin_id: current.admin.id,
            kind: "address",
            value: &payload.address,
            reason: "blocked from control app",
            expires_at: None,
            ip_address: None,
        },
    )
    .await?;

    Ok(Json(ActionResponse {
        ok: true,
        action: "block_user".into(),
        address: payload.address,
        message: "User blocked".into(),
    }))
}

async fn unblock_user(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    headers: HeaderMap,
    Json(payload): Json<AddressActionRequest>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let csrf = csrf_from_headers(&headers).ok_or(crate::error::AppError::Forbidden)?;
    auth::validate_csrf(&current, &csrf)?;

    crate::bans::set_active_for_value(
        &state.pool,
        &state.shell,
        &state.config,
        crate::bans::SetBanActiveByValue {
            admin_id: current.admin.id,
            kind: "address",
            value: &payload.address,
            is_active: false,
            ip_address: None,
        },
    )
    .await?;

    Ok(Json(ActionResponse {
        ok: true,
        action: "unblock_user".into(),
        address: payload.address,
        message: "User unblocked".into(),
    }))
}

async fn delete_account(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    headers: HeaderMap,
    Json(payload): Json<AddressActionRequest>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let csrf = csrf_from_headers(&headers).ok_or(crate::error::AppError::Forbidden)?;
    auth::validate_csrf(&current, &csrf)?;

    let mut ban_warnings = Vec::new();
    match crate::bans::ensure_active_address_ban(
        &state.pool,
        &state.shell,
        &state.config,
        current.admin.id,
        &payload.address,
        "auto-blocked on account delete",
        None,
    )
    .await
    {
        Ok(warnings) => {
            ban_warnings = warnings;
        }
        Err(err) => {
            let err_text = err.to_string();
            ban_warnings.push(err_text.clone());
            audit::log_event(
                &state.pool,
                Some(current.admin.id),
                "account_delete_preban_failed",
                "user",
                &payload.address,
                json!({ "error": err_text }),
                None,
            )
            .await?;
        }
    }

    match users::delete_account_lifecycle(&state.shell, &payload.address).await {
        Ok(details) => {
            audit::log_event(
                &state.pool,
                Some(current.admin.id),
                "account_lifecycle_deleted",
                "user",
                &payload.address,
                json!({ "details": details, "ban_warnings": ban_warnings }),
                None,
            )
            .await?;
            Ok(Json(ActionResponse {
                ok: true,
                action: "delete_account".into(),
                address: payload.address,
                message: "User deleted".into(),
            }))
        }
        Err(err) => {
            let err_text = err.to_string();
            audit::log_event(
                &state.pool,
                Some(current.admin.id),
                "account_lifecycle_delete_failed",
                "user",
                &payload.address,
                json!({ "error": err_text, "ban_warnings": ban_warnings }),
                None,
            )
            .await?;
            Err(err)
        }
    }
}
