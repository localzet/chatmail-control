use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};

use crate::error::AppResult;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: i64,
    pub admin_id: Option<i64>,
    pub action: String,
    pub target_type: String,
    pub target_value: String,
    pub details_json: String,
    pub ip_address: Option<String>,
    pub created_at: String,
}

pub async fn log_event(
    pool: &SqlitePool,
    admin_id: Option<i64>,
    action: &str,
    target_type: &str,
    target_value: &str,
    details: Value,
    ip_address: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO audit_log (admin_id, action, target_type, target_value, details_json, ip_address)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(admin_id)
    .bind(action)
    .bind(target_type)
    .bind(target_value)
    .bind(details.to_string())
    .bind(ip_address)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn latest(pool: &SqlitePool, limit: i64) -> AppResult<Vec<AuditEvent>> {
    let rows = sqlx::query_as::<_, AuditEvent>(
        "SELECT id, admin_id, action, target_type, target_value, details_json, ip_address, created_at
         FROM audit_log
         ORDER BY id DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
