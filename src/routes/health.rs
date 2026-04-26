use askama_axum::Template;
use axum::{extract::State, response::IntoResponse, routing::get, Router};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};

use crate::{auth, health, AppState};

#[derive(Template)]
#[template(path = "health.html")]
struct HealthTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    checks: Vec<crate::health::HealthCheck>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/admin/health", get(index))
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let checks = health::run_health_checks(&state.shell, &state.config).await;
    Ok(HealthTemplate {
        page_title: "Health".into(),
        current_path: "/admin/health".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        checks,
    })
}
