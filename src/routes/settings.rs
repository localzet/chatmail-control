use askama_axum::Template;
use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::Deserialize;

use crate::{auth, settings, AppState};

#[derive(Debug, Deserialize)]
struct SettingsForm {
    csrf_token: String,
    registration_mode: String,
    max_accounts_per_ip_per_day: i64,
    max_accounts_per_day: i64,
    cleanup_empty_mailboxes_after_days: i64,
    notes: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    settings: crate::settings::RegistrationSettings,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/admin/settings", get(index).post(update))
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let settings = settings::load(&state.pool).await?;
    Ok(SettingsTemplate {
        page_title: "Settings".into(),
        current_path: "/admin/settings".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        settings,
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
    Ok(Redirect::to("/admin/settings"))
}
