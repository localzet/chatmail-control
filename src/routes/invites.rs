use askama_axum::Template;
use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_extra::extract::{cookie::Key, PrivateCookieJar};
use serde::Deserialize;

use crate::{auth, invites, settings, AppState};

#[derive(Debug, Deserialize)]
struct InviteForm {
    csrf_token: String,
    comment: String,
    max_uses: i64,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InviteActionForm {
    csrf_token: String,
    id: i64,
}

#[derive(Template)]
#[template(path = "invites.html")]
struct InvitesTemplate {
    page_title: String,
    current_path: String,
    username: String,
    csrf_token: String,
    registration_mode: String,
    invites: Vec<crate::invites::Invite>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/invites", get(index).post(create))
        .route("/admin/invites/deactivate", post(deactivate))
        .route("/admin/invites/reactivate", post(reactivate))
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    let registration_mode = settings::load(&state.pool).await?.registration_mode;
    let invites = invites::list(&state.pool).await?;
    Ok(InvitesTemplate {
        page_title: "Invites".into(),
        current_path: "/admin/invites".into(),
        username: current.admin.username,
        csrf_token: current.session.csrf_token,
        registration_mode,
        invites,
    })
}

async fn create(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<InviteForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    invites::create(
        &state.pool,
        &state.config,
        current.admin.id,
        &form.comment,
        form.max_uses,
        form.expires_at.as_deref(),
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/invites"))
}

async fn deactivate(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<InviteActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    invites::set_active(
        &state.pool,
        &state.config,
        current.admin.id,
        form.id,
        false,
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/invites"))
}

async fn reactivate(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<InviteActionForm>,
) -> crate::error::AppResult<impl IntoResponse> {
    let current = auth::require_admin(&state, &jar).await?;
    auth::validate_csrf(&current, &form.csrf_token)?;
    invites::set_active(
        &state.pool,
        &state.config,
        current.admin.id,
        form.id,
        true,
        None,
    )
    .await?;
    Ok(Redirect::to("/admin/invites"))
}
