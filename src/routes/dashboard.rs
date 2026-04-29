use askama_axum::Template;
use axum::{
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};

use crate::{audit, auth, health, services, users, AppState};

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    service_statuses: Vec<crate::services::ServiceStatus>,
    mail_queue_size: usize,
    users_count: usize,
    active_bans_count: i64,
    warnings: Vec<crate::health::HealthCheck>,
    audit_events: Vec<crate::audit::AuditEvent>,
}

#[derive(Template)]
#[template(path = "admin_app.html")]
struct AdminAppTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(|| async { Redirect::to("/admin") }))
        .route("/admin", get(index))
        .route("/admin/app", get(app_shell))
}

async fn index(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: PrivateCookieJar<Key>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let blocked = crate::bans::active_values(&state.pool).await?;
    let users = users::list_users(&state.shell, &blocked).await;
    let stats =
        services::collect_dashboard_stats(&state.pool, &state.shell, &state.config, users.len())
            .await?;
    let audit_events = audit::latest(&state.pool, 20).await?;
    let warnings = health::run_health_checks(&state.shell, &state.config)
        .await
        .into_iter()
        .filter(|check| check.status != "ok")
        .collect();
    Ok(DashboardTemplate {
        page_title: "Dashboard".into(),
        current_path: "/admin".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        service_statuses: stats.services,
        mail_queue_size: stats.mail_queue_size,
        users_count: stats.users_count,
        active_bans_count: stats.active_bans_count,
        warnings,
        audit_events,
    })
}

async fn app_shell(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: PrivateCookieJar<Key>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    Ok(AdminAppTemplate {
        page_title: "Control App".into(),
        current_path: "/admin/app".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
    })
}
