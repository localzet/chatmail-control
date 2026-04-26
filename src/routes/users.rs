use askama_axum::Template;
use axum::{
    extract::{Form, Query, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::Deserialize;
use serde_json::json;

use crate::{audit, auth, users, AppState};

#[derive(Debug, Deserialize)]
struct ActionForm {
    csrf_token: String,
    address: String,
}

#[derive(Debug, Deserialize, Default)]
struct UsersQuery {
    metadata: Option<String>,
}

#[derive(Template)]
#[template(path = "users.html")]
struct UsersTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    users: Vec<crate::users::UserMailbox>,
    selected_metadata: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(index))
        .route("/admin/users/delete", post(delete_mailbox))
        .route("/admin/users/block", post(block_user))
        .route("/admin/users/unblock", post(unblock_user))
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Query(query): Query<UsersQuery>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let blocked = crate::bans::active_values(&state.pool).await?;
    let users = users::list_users(&state.shell, &state.config, &blocked).await;
    let selected_metadata = query.metadata.and_then(|address| {
        users
            .iter()
            .find(|user| user.address == address)
            .and_then(|user| user.metadata.clone())
    });
    Ok(UsersTemplate {
        page_title: "Users".into(),
        current_path: "/admin/users".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        users,
        selected_metadata,
    })
}

async fn delete_mailbox(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let output = state
        .shell
        .run_with_replacements(
            &state.config.users.delete_command,
            &[("{address}", &form.address)],
        )
        .await?;
    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "mailbox_deleted",
        "user",
        &form.address,
        json!({ "status": output.status, "stdout": output.stdout, "stderr": output.stderr }),
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/users"))
}

async fn block_user(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    crate::bans::add(
        &state.pool,
        &state.shell,
        &state.config,
        crate::bans::CreateBan {
            admin_id: current.admin.id,
            kind: "address",
            value: &form.address,
            reason: "blocked from users page",
            expires_at: None,
            ip_address: None,
        },
    )
    .await?;
    Ok(Redirect::to("/admin/users"))
}

async fn unblock_user(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    crate::bans::set_active_for_value(
        &state.pool,
        &state.shell,
        &state.config,
        crate::bans::SetBanActiveByValue {
            admin_id: current.admin.id,
            kind: "address",
            value: &form.address,
            is_active: false,
            ip_address: None,
        },
    )
    .await?;
    Ok(Redirect::to("/admin/users"))
}
