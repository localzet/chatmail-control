use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::{Deserialize, Serialize};

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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/dashboard", get(dashboard))
        .route("/api/v1/admin/users", get(users_list))
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
