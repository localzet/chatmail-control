use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, SqlitePool};

use crate::{audit, config::Config, error::AppResult, shell::write_text_file};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Invite {
    pub id: i64,
    pub token: String,
    pub comment: String,
    pub max_uses: i64,
    pub used_count: i64,
    pub created_by: Option<i64>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub is_active: i64,
}

pub fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Invite>> {
    let rows = sqlx::query_as::<_, Invite>(
        "SELECT id, token, comment, max_uses, used_count, created_by, created_at, expires_at, is_active
         FROM invites ORDER BY id DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn create(
    pool: &SqlitePool,
    config: &Config,
    admin_id: i64,
    comment: &str,
    max_uses: i64,
    expires_at: Option<&str>,
    ip_address: Option<&str>,
) -> AppResult<String> {
    let token = generate_token();
    sqlx::query(
        "INSERT INTO invites (token, comment, max_uses, created_by, expires_at, is_active)
         VALUES (?, ?, ?, ?, ?, 1)",
    )
    .bind(&token)
    .bind(comment)
    .bind(max_uses)
    .bind(admin_id)
    .bind(expires_at)
    .execute(pool)
    .await?;

    export_active_tokens(pool, config).await?;
    audit::log_event(
        pool,
        Some(admin_id),
        "invite_created",
        "invite",
        &token,
        json!({ "comment": comment, "max_uses": max_uses, "expires_at": expires_at }),
        ip_address,
    )
    .await?;

    Ok(token)
}

pub async fn set_active(
    pool: &SqlitePool,
    config: &Config,
    admin_id: i64,
    id: i64,
    is_active: bool,
    ip_address: Option<&str>,
) -> AppResult<()> {
    sqlx::query("UPDATE invites SET is_active = ? WHERE id = ?")
        .bind(if is_active { 1 } else { 0 })
        .bind(id)
        .execute(pool)
        .await?;

    let invite = sqlx::query_as::<_, Invite>(
        "SELECT id, token, comment, max_uses, used_count, created_by, created_at, expires_at, is_active
         FROM invites WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    export_active_tokens(pool, config).await?;
    audit::log_event(
        pool,
        Some(admin_id),
        if is_active {
            "invite_reactivated"
        } else {
            "invite_deactivated"
        },
        "invite",
        &invite.token,
        json!({}),
        ip_address,
    )
    .await?;
    Ok(())
}

pub async fn export_active_tokens(pool: &SqlitePool, config: &Config) -> AppResult<()> {
    let invites = sqlx::query_as::<_, Invite>(
        "SELECT id, token, comment, max_uses, used_count, created_by, created_at, expires_at, is_active
         FROM invites
         WHERE is_active = 1 AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)
         ORDER BY id DESC",
    )
    .fetch_all(pool)
    .await?;
    let content = invites
        .into_iter()
        .map(|invite| invite.token)
        .collect::<Vec<_>>()
        .join("\n");
    write_text_file(&config.invites.export_file, &content).await
}

#[cfg(test)]
mod tests {
    use super::generate_token;

    #[test]
    fn generates_random_token() {
        let token = generate_token();
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
