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
    status: Option<String>,
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
    status_kind: Option<String>,
    status_title: Option<String>,
    status_message: Option<String>,
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
    let users = users::list_users(&state.shell, &blocked).await;
    let selected_metadata = query.metadata.and_then(|address| {
        users
            .iter()
            .find(|user| user.address == address)
            .and_then(|user| user.metadata.clone())
    });
    let (status_kind, status_title, status_message) = status_banner(query.status.as_deref());
    Ok(UsersTemplate {
        page_title: "Users".into(),
        current_path: "/admin/users".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        users,
        selected_metadata,
        status_kind,
        status_title,
        status_message,
    })
}

async fn delete_mailbox(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;

    let output = users::delete_mailbox(&state.shell, &form.address).await?;
    let action = if output.status == 0 {
        "mailbox_deleted"
    } else {
        "mailbox_delete_failed"
    };
    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        action,
        "user",
        &form.address,
        json!({ "status": output.status, "stdout": output.stdout, "stderr": output.stderr }),
        None,
    )
    .await?;

    if output.status == 0 {
        Ok(Redirect::to("/admin/users?status=delete-ok"))
    } else {
        Ok(Redirect::to("/admin/users?status=delete-failed"))
    }
}

fn status_banner(status: Option<&str>) -> (Option<String>, Option<String>, Option<String>) {
    match status {
        Some("delete-ok") => (
            Some("success".into()),
            Some("Delete command completed.".into()),
            Some("The built-in mailbox delete command returned exit code 0.".into()),
        ),
        Some("delete-failed") => (
            Some("error".into()),
            Some("Delete command failed.".into()),
            Some(
                "The built-in mailbox delete command returned a non-zero exit code. Check the audit log or service journal for stderr/stdout."
                    .into(),
            ),
        ),
        _ => (None, None, None),
    }
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
