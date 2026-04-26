use askama_axum::Template;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::Deserialize;

use crate::{auth, logs, AppState};

#[derive(Debug, Clone)]
struct LogSourceView {
    name: String,
    selected: bool,
}

#[derive(Debug, Deserialize, Default)]
struct LogsQuery {
    source: Option<String>,
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Template)]
#[template(path = "logs.html")]
struct LogsTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    query: String,
    limit: usize,
    sources: Vec<LogSourceView>,
    lines: Vec<crate::logs::LogLine>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/admin/logs", get(index))
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Query(query): Query<LogsQuery>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let source = query.source.unwrap_or_else(|| {
        state
            .config
            .logs
            .sources
            .first()
            .map(|src| src.name.clone())
            .unwrap_or_else(|| "dovecot".into())
    });
    let source_cfg = state
        .config
        .logs
        .sources
        .iter()
        .find(|src| src.name == source)
        .or_else(|| state.config.logs.sources.first())
        .ok_or_else(|| crate::error::AppError::Config("no log sources configured".into()))?;
    let limit = query.limit.unwrap_or(200);
    let lines = logs::read_logs(&state.shell, source_cfg, query.q.as_deref(), limit).await;
    Ok(LogsTemplate {
        page_title: "Logs".into(),
        current_path: "/admin/logs".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        query: query.q.unwrap_or_default(),
        limit,
        sources: state
            .config
            .logs
            .sources
            .iter()
            .map(|src| LogSourceView {
                selected: src.name == source_cfg.name,
                name: src.name.clone(),
            })
            .collect(),
        lines,
    })
}
