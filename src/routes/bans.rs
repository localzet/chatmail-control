use askama_axum::Template;
use axum::{
    extract::{Form, Query, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::Deserialize;

use crate::{auth, bans, AppState};

#[derive(Debug, Deserialize)]
struct BanForm {
    csrf_token: String,
    kind: String,
    value: String,
    reason: String,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BanActionForm {
    csrf_token: String,
    id: i64,
}

#[derive(Debug, Deserialize, Default)]
struct BanQuery {
    q: Option<String>,
}

#[derive(Template)]
#[template(path = "bans.html")]
struct BansTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    bans: Vec<crate::bans::Ban>,
    query: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/bans", get(index).post(create))
        .route("/admin/bans/deactivate", post(deactivate))
        .route("/admin/bans/reactivate", post(reactivate))
        .route("/admin/bans/delete", post(delete))
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Query(query): Query<BanQuery>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let bans = bans::list(&state.pool, query.q.as_deref()).await?;
    Ok(BansTemplate {
        page_title: "Bans".into(),
        current_path: "/admin/bans".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        bans,
        query: query.q.unwrap_or_default(),
    })
}

async fn create(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<BanForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    bans::add(
        &state.pool,
        &state.shell,
        &state.config,
        bans::CreateBan {
            admin_id: current.admin.id,
            kind: &form.kind,
            value: &form.value,
            reason: &form.reason,
            expires_at: form.expires_at.as_deref(),
            ip_address: None,
        },
    )
    .await?;
    Ok(Redirect::to("/admin/bans"))
}

async fn deactivate(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<BanActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    bans::set_active(
        &state.pool,
        &state.shell,
        &state.config,
        current.admin.id,
        form.id,
        false,
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/bans"))
}

async fn reactivate(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<BanActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    bans::set_active(
        &state.pool,
        &state.shell,
        &state.config,
        current.admin.id,
        form.id,
        true,
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/bans"))
}

async fn delete(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<BanActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    bans::delete(
        &state.pool,
        &state.shell,
        &state.config,
        current.admin.id,
        form.id,
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/bans"))
}
