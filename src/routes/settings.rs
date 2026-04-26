use askama_axum::Template;
use axum::{
    extract::{Form, Query, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::Deserialize;

use crate::{auth, postfix, settings, AppState};

#[derive(Debug, Deserialize)]
struct SettingsForm {
    csrf_token: String,
    registration_mode: String,
    max_accounts_per_ip_per_day: i64,
    max_accounts_per_day: i64,
    cleanup_empty_mailboxes_after_days: i64,
    notes: String,
}

#[derive(Debug, Deserialize, Default)]
struct SettingsQuery {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ActionForm {
    csrf_token: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    settings: crate::settings::RegistrationSettings,
    status_kind: Option<String>,
    status_title: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/settings", get(index).post(update))
        .route("/admin/settings/postfix-sync", post(postfix_sync))
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Query(query): Query<SettingsQuery>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let settings = settings::load(&state.pool).await?;
    let (status_kind, status_title) = status_view(query.status.as_deref());
    Ok(SettingsTemplate {
        page_title: "Settings".into(),
        current_path: "/admin/settings".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        settings,
        status_kind,
        status_title,
    })
}

async fn update(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<SettingsForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let settings = crate::settings::RegistrationSettings {
        registration_mode: form.registration_mode,
        max_accounts_per_ip_per_day: form.max_accounts_per_ip_per_day,
        max_accounts_per_day: form.max_accounts_per_day,
        cleanup_empty_mailboxes_after_days: form.cleanup_empty_mailboxes_after_days,
        notes: form.notes,
    };
    settings::save(
        &state.pool,
        &state.shell,
        &state.config,
        current.admin.id,
        &settings,
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/settings?status=settings-saved"))
}

async fn postfix_sync(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let result = postfix::ensure_ban_policy(&state.shell, &state.config).await?;
    crate::audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "postfix_policy_synced",
        "postfix",
        "ban_policy",
        serde_json::json!({
            "changed": result.changed,
            "unchanged": result.unchanged,
        }),
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/settings?status=postfix-synced"))
}

fn status_view(status: Option<&str>) -> (Option<String>, Option<String>) {
    match status {
        Some("settings-saved") => (Some("success".into()), Some("Settings saved.".into())),
        Some("postfix-synced") => (
            Some("success".into()),
            Some("Postfix policy synchronized.".into()),
        ),
        _ => (None, None),
    }
}
