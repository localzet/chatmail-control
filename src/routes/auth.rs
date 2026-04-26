use askama_axum::Template;
use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use axum_extra::extract::{
    cookie::{Cookie, Key, SameSite},
    PrivateCookieJar,
};
use uuid::Uuid;

use crate::{auth, AppState};

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    csrf_token: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page).post(auth::authenticate))
        .route("/logout", post(auth::logout))
}

async fn login_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
) -> impl IntoResponse {
    let csrf_token = Uuid::new_v4().to_string();
    let cookie = Cookie::build((auth::LOGIN_CSRF_COOKIE, csrf_token.clone()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.server.secure_cookies)
        .build();
    (jar.add(cookie), LoginTemplate { csrf_token })
}
