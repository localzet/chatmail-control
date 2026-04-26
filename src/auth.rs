use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{ConnectInfo, Form, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    cookie::{Cookie, Key, SameSite},
    PrivateCookieJar,
};
use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::{FromRow, SqlitePool};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    audit,
    error::{AppError, AppResult},
    AppState,
};

const SESSION_COOKIE: &str = "chatmail_control_session";
pub const LOGIN_CSRF_COOKIE: &str = "chatmail_control_login_csrf";

#[derive(Debug, Clone, FromRow)]
pub struct Admin {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct Session {
    pub id: String,
    pub admin_id: i64,
    pub csrf_token: String,
}

#[derive(Clone, Default)]
pub struct LoginRateLimiter {
    inner: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

impl LoginRateLimiter {
    pub async fn check(&self, key: &str) -> AppResult<()> {
        let mut state = self.inner.write().await;
        let entries = state.entry(key.to_string()).or_default();
        let now = Instant::now();
        entries.retain(|v| now.duration_since(*v) < Duration::from_minutes(10));
        if entries.len() >= 10 {
            return Err(AppError::Validation(
                "too many login attempts, please try again later".into(),
            ));
        }
        entries.push(now);
        Ok(())
    }
}

trait DurationExt {
    fn from_minutes(minutes: u64) -> Duration;
}

impl DurationExt for Duration {
    fn from_minutes(minutes: u64) -> Duration {
        Duration::from_secs(minutes * 60)
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub csrf_token: String,
}

#[derive(Debug, Deserialize)]
pub struct CsrfForm {
    pub csrf_token: String,
}

#[derive(Debug, Clone)]
pub struct CurrentAdmin {
    pub admin: Admin,
    pub session: Session,
}

pub async fn require_admin(
    state: &AppState,
    jar: &PrivateCookieJar<Key>,
) -> AppResult<CurrentAdmin> {
    let Some(cookie) = jar.get(SESSION_COOKIE) else {
        return Err(AppError::Unauthorized);
    };
    let session_id = cookie.value().to_string();

    let session = sqlx::query_as::<_, Session>(
        "SELECT id, admin_id, csrf_token, created_at, expires_at, last_seen_at
         FROM sessions
         WHERE id = ? AND expires_at > CURRENT_TIMESTAMP",
    )
    .bind(&session_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let admin = sqlx::query_as::<_, Admin>(
        "SELECT id, username, password_hash
         FROM admins WHERE id = ?",
    )
    .bind(session.admin_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    sqlx::query("UPDATE sessions SET last_seen_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&session_id)
        .execute(&state.pool)
        .await?;

    Ok(CurrentAdmin { admin, session })
}

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> AppResult<bool> {
    let parsed = PasswordHash::new(hash).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub async fn upsert_admin(pool: &SqlitePool, username: &str, password: &str) -> AppResult<()> {
    let hash = hash_password(password)?;
    sqlx::query(
        "INSERT INTO admins (username, password_hash)
         VALUES (?, ?)
         ON CONFLICT(username) DO UPDATE SET
           password_hash = excluded.password_hash,
           updated_at = CURRENT_TIMESTAMP",
    )
    .bind(username)
    .bind(hash)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn authenticate(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    let login_cookie = jar
        .get(LOGIN_CSRF_COOKIE)
        .ok_or(AppError::Forbidden)?
        .value()
        .to_string();
    if login_cookie != form.csrf_token {
        return Err(AppError::Forbidden);
    }

    state
        .login_rate_limiter
        .check(&format!("{}:{}", addr.ip(), form.username))
        .await?;

    let admin = sqlx::query_as::<_, Admin>(
        "SELECT id, username, password_hash
         FROM admins WHERE username = ?",
    )
    .bind(&form.username)
    .fetch_optional(&state.pool)
    .await?;

    let admin = match admin {
        Some(admin) if verify_password(&form.password, &admin.password_hash)? => admin,
        _ => {
            audit::log_event(
                &state.pool,
                None,
                "login_failed",
                "admin",
                &form.username,
                json!({ "reason": "invalid credentials" }),
                Some(&addr.ip().to_string()),
            )
            .await?;
            return Err(AppError::Unauthorized);
        }
    };

    let session_id = Uuid::new_v4().to_string();
    let csrf_token = Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + ChronoDuration::hours(state.config.auth.session_ttl_hours))
        .naive_utc()
        .to_string();

    sqlx::query(
        "INSERT INTO sessions (id, admin_id, csrf_token, expires_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(admin.id)
    .bind(&csrf_token)
    .bind(&expires_at)
    .execute(&state.pool)
    .await?;

    sqlx::query("UPDATE admins SET last_login_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(admin.id)
        .execute(&state.pool)
        .await?;

    audit::log_event(
        &state.pool,
        Some(admin.id),
        "login_success",
        "admin",
        &admin.username,
        json!({}),
        Some(&addr.ip().to_string()),
    )
    .await?;

    let cookie = Cookie::build((SESSION_COOKIE, session_id))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.server.secure_cookies)
        .build();

    let clear_login_cookie = Cookie::build((LOGIN_CSRF_COOKIE, "")).path("/").build();

    Ok((
        jar.add(cookie).remove(clear_login_cookie),
        Redirect::to("/admin"),
    )
        .into_response())
}

pub async fn logout(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: PrivateCookieJar<Key>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    let current = require_admin(&state, &jar).await?;
    validate_csrf(&current, &form.csrf_token)?;
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(&current.session.id)
        .execute(&state.pool)
        .await?;

    audit::log_event(
        &state.pool,
        Some(current.admin.id),
        "logout",
        "admin",
        &current.admin.username,
        json!({}),
        Some(&addr.ip().to_string()),
    )
    .await?;

    let cookie = Cookie::build((SESSION_COOKIE, "")).path("/").build();
    Ok((jar.remove(cookie), Redirect::to("/login")).into_response())
}

pub fn validate_csrf(current: &CurrentAdmin, token: &str) -> AppResult<()> {
    if current.session.csrf_token != token {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{hash_password, verify_password};

    #[test]
    fn hashes_and_verifies_password() {
        let hash = hash_password("secret").expect("hash");
        assert!(verify_password("secret", &hash).expect("verify"));
        assert!(!verify_password("other", &hash).expect("verify mismatch"));
    }
}
