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

#[derive(Debug, Deserialize)]
struct MailboxActionForm {
    csrf_token: String,
    address: String,
    mailbox: String,
}

#[derive(Debug, Deserialize, Default)]
struct UsersQuery {
    metadata: Option<String>,
    manage: Option<String>,
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
    managed_user: Option<crate::users::ManagedUser>,
    status_kind: Option<String>,
    status_title: Option<String>,
    status_message: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(index))
        .route("/admin/users/delete", post(delete_mailbox))
        .route("/admin/users/account-disable", post(disable_login))
        .route("/admin/users/account-enable", post(enable_login))
        .route("/admin/users/account-delete", post(delete_account))
        .route("/admin/users/mailbox-expunge", post(expunge_mailbox))
        .route("/admin/users/quota-recalc", post(quota_recalc))
        .route("/admin/users/force-resync", post(force_resync))
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
    let managed_user = if let Some(address) = query.manage.as_deref() {
        users::load_managed_user(&state.shell, address).await.ok()
    } else {
        None
    };
    let (status_kind, status_title, status_message) = status_banner(query.status.as_deref());
    Ok(UsersTemplate {
        page_title: "Users".into(),
        current_path: "/admin/users".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        users,
        selected_metadata,
        managed_user,
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
        Ok(Redirect::to(&format!(
            "/admin/users?manage={}&status=delete-ok",
            form.address
        )))
    } else {
        Ok(Redirect::to(&format!(
            "/admin/users?manage={}&status=delete-failed",
            form.address
        )))
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
        Some("account-disabled") => (
            Some("success".into()),
            Some("Login disabled.".into()),
            Some("The user password file was moved to password.blocked.".into()),
        ),
        Some("account-enabled") => (
            Some("success".into()),
            Some("Login enabled.".into()),
            Some("The blocked password file was restored.".into()),
        ),
        Some("account-deleted") => (
            Some("success".into()),
            Some("Account lifecycle directory removed.".into()),
            Some("The chatmail maildir path was deleted.".into()),
        ),
        Some("mailbox-expunged") => (
            Some("success".into()),
            Some("Mailbox expunged.".into()),
            Some("Expunge completed for the selected mailbox.".into()),
        ),
        Some("quota-recalc-ok") => (
            Some("success".into()),
            Some("Quota recalculated.".into()),
            Some("doveadm quota recalc completed.".into()),
        ),
        Some("force-resync-ok") => (
            Some("success".into()),
            Some("Force resync completed.".into()),
            Some("doveadm force-resync finished.".into()),
        ),
        _ => (None, None, None),
    }
}

async fn disable_login(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let details = users::disable_login(&state.shell, &form.address).await?;
    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "account_login_disabled",
        "user",
        &form.address,
        json!({ "details": details }),
        None,
    )
    .await?;
    Ok(Redirect::to(&format!(
        "/admin/users?manage={}&status=account-disabled",
        form.address
    )))
}

async fn enable_login(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let details = users::enable_login(&state.shell, &form.address).await?;
    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "account_login_enabled",
        "user",
        &form.address,
        json!({ "details": details }),
        None,
    )
    .await?;
    Ok(Redirect::to(&format!(
        "/admin/users?manage={}&status=account-enabled",
        form.address
    )))
}

async fn delete_account(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let details = users::delete_account_lifecycle(&state.shell, &form.address).await?;
    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "account_lifecycle_deleted",
        "user",
        &form.address,
        json!({ "details": details }),
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/users?status=account-deleted"))
}

async fn expunge_mailbox(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<MailboxActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let output = users::expunge_mailbox(&state.shell, &form.address, &form.mailbox).await?;
    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "mailbox_expunged",
        "user",
        &form.address,
        json!({
            "mailbox": form.mailbox,
            "status": output.status,
            "stdout": output.stdout,
            "stderr": output.stderr
        }),
        None,
    )
    .await?;
    Ok(Redirect::to(&format!(
        "/admin/users?manage={}&status=mailbox-expunged",
        form.address
    )))
}

async fn quota_recalc(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let output = users::quota_recalc(&state.shell, &form.address).await?;
    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "quota_recalc",
        "user",
        &form.address,
        json!({ "status": output.status, "stdout": output.stdout, "stderr": output.stderr }),
        None,
    )
    .await?;
    Ok(Redirect::to(&format!(
        "/admin/users?manage={}&status=quota-recalc-ok",
        form.address
    )))
}

async fn force_resync(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let output = users::force_resync(&state.shell, &form.address).await?;
    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "force_resync",
        "user",
        &form.address,
        json!({ "status": output.status, "stdout": output.stdout, "stderr": output.stderr }),
        None,
    )
    .await?;
    Ok(Redirect::to(&format!(
        "/admin/users?manage={}&status=force-resync-ok",
        form.address
    )))
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
