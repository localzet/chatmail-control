use askama_axum::Template;
use axum::{
    extract::{Form, Query, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::Deserialize;

use crate::{auth, services, AppState};

#[derive(Debug, Deserialize, Default)]
struct ServiceQuery {
    status: Option<String>,
    service: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ServiceActionForm {
    csrf_token: String,
    service: String,
    action: String,
}

#[derive(Debug, Clone)]
struct ServiceView {
    name: String,
}

#[derive(Template)]
#[template(path = "services.html")]
struct ServicesTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    status_kind: Option<String>,
    status_title: Option<String>,
    selected_service: Option<String>,
    services: Vec<ServiceView>,
    action_result: Option<crate::services::ServiceActionResult>,
    tail_lines: Vec<crate::logs::LogLine>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/services", get(index))
        .route("/admin/services/action", post(action))
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Query(query): Query<ServiceQuery>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let selected = query.service.unwrap_or_else(|| {
        state
            .config
            .health
            .services
            .first()
            .cloned()
            .unwrap_or_default()
    });
    let tail_lines = if selected.is_empty() {
        Vec::new()
    } else {
        crate::logs::read_journal_unit(&state.shell, &selected, None, 120).await
    };
    let (status_kind, status_title) = service_status(query.status.as_deref());
    Ok(ServicesTemplate {
        page_title: "Services".into(),
        current_path: "/admin/services".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        status_kind,
        status_title,
        selected_service: Some(selected),
        services: state
            .config
            .health
            .services
            .iter()
            .map(|name| ServiceView { name: name.clone() })
            .collect(),
        action_result: None,
        tail_lines,
    })
}

async fn action(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<ServiceActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    let result = services::run_service_action(&state.shell, &form.service, &form.action).await?;
    crate::audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "service_action",
        "systemd_service",
        &form.service,
        serde_json::json!({
            "action": form.action,
            "status": result.status,
            "stdout": result.stdout,
            "stderr": result.stderr
        }),
        None,
    )
    .await?;
    let status = if result.status == 0 {
        "service-ok"
    } else {
        "service-failed"
    };
    Ok(Redirect::to(&format!(
        "/admin/services?status={status}&service={}",
        form.service
    )))
}

fn service_status(status: Option<&str>) -> (Option<String>, Option<String>) {
    match status {
        Some("service-ok") => (
            Some("success".into()),
            Some("Service action completed.".into()),
        ),
        Some("service-failed") => (Some("error".into()), Some("Service action failed.".into())),
        _ => (None, None),
    }
}
