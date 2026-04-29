pub mod auth;
pub mod bans;
pub mod dashboard;
pub mod health;
pub mod logs;
pub mod services;
pub mod users;

use axum::Router;

pub fn router() -> Router<crate::AppState> {
    Router::new()
        .merge(auth::router())
        .merge(dashboard::router())
        .merge(users::router())
        .merge(bans::router())
        .merge(logs::router())
        .merge(services::router())
        .merge(health::router())
}
