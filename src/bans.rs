use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, SqlitePool};

use crate::{
    audit, chatmail,
    config::Config,
    error::AppResult,
    shell::{command_result_details, run_reload_commands, write_text_file, Shell},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Ban {
    pub id: i64,
    pub kind: String,
    pub value: String,
    pub reason: String,
    pub created_by: Option<i64>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub is_active: i64,
}

#[derive(Debug, Clone)]
pub struct CreateBan<'a> {
    pub admin_id: i64,
    pub kind: &'a str,
    pub value: &'a str,
    pub reason: &'a str,
    pub expires_at: Option<&'a str>,
    pub ip_address: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct SetBanActiveByValue<'a> {
    pub admin_id: i64,
    pub kind: &'a str,
    pub value: &'a str,
    pub is_active: bool,
    pub ip_address: Option<&'a str>,
}

pub async fn list(pool: &SqlitePool, query: Option<&str>) -> AppResult<Vec<Ban>> {
    let rows = if let Some(query) = query {
        sqlx::query_as::<_, Ban>(
            "SELECT id, kind, value, reason, created_by, created_at, expires_at, is_active
             FROM bans
             WHERE value LIKE ? OR reason LIKE ?
             ORDER BY id DESC",
        )
        .bind(format!("%{query}%"))
        .bind(format!("%{query}%"))
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, Ban>(
            "SELECT id, kind, value, reason, created_by, created_at, expires_at, is_active
             FROM bans
             ORDER BY id DESC",
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

pub async fn active_values(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>("SELECT value FROM bans WHERE is_active = 1")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn add(
    pool: &SqlitePool,
    shell: &Shell,
    config: &Config,
    create: CreateBan<'_>,
) -> AppResult<Vec<String>> {
    sqlx::query(
        "INSERT INTO bans (kind, value, reason, created_by, expires_at, is_active)
         VALUES (?, ?, ?, ?, ?, 1)",
    )
    .bind(create.kind)
    .bind(create.value)
    .bind(create.reason)
    .bind(create.admin_id)
    .bind(create.expires_at)
    .execute(pool)
    .await?;

    audit::log_event(
        pool,
        Some(create.admin_id),
        "ban_added",
        create.kind,
        create.value,
        json!({ "reason": create.reason, "expires_at": create.expires_at }),
        create.ip_address,
    )
    .await?;

    sync_policy_files(pool, config).await?;
    execute_reload_commands(pool, shell, Some(create.admin_id), create.ip_address).await
}

pub async fn set_active(
    pool: &SqlitePool,
    shell: &Shell,
    config: &Config,
    admin_id: i64,
    id: i64,
    is_active: bool,
    ip_address: Option<&str>,
) -> AppResult<Vec<String>> {
    sqlx::query("UPDATE bans SET is_active = ? WHERE id = ?")
        .bind(if is_active { 1 } else { 0 })
        .bind(id)
        .execute(pool)
        .await?;

    let ban = sqlx::query_as::<_, Ban>(
        "SELECT id, kind, value, reason, created_by, created_at, expires_at, is_active FROM bans WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    audit::log_event(
        pool,
        Some(admin_id),
        if is_active {
            "ban_reactivated"
        } else {
            "ban_deactivated"
        },
        &ban.kind,
        &ban.value,
        json!({}),
        ip_address,
    )
    .await?;

    sync_policy_files(pool, config).await?;
    execute_reload_commands(pool, shell, Some(admin_id), ip_address).await
}

pub async fn delete(
    pool: &SqlitePool,
    shell: &Shell,
    config: &Config,
    admin_id: i64,
    id: i64,
    ip_address: Option<&str>,
) -> AppResult<Vec<String>> {
    let ban = sqlx::query_as::<_, Ban>(
        "SELECT id, kind, value, reason, created_by, created_at, expires_at, is_active FROM bans WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    sqlx::query("DELETE FROM bans WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    audit::log_event(
        pool,
        Some(admin_id),
        "ban_deleted",
        &ban.kind,
        &ban.value,
        json!({}),
        ip_address,
    )
    .await?;

    sync_policy_files(pool, config).await?;
    execute_reload_commands(pool, shell, Some(admin_id), ip_address).await
}

pub async fn set_active_for_value(
    pool: &SqlitePool,
    shell: &Shell,
    config: &Config,
    update: SetBanActiveByValue<'_>,
) -> AppResult<Vec<String>> {
    sqlx::query("UPDATE bans SET is_active = ? WHERE kind = ? AND value = ?")
        .bind(if update.is_active { 1 } else { 0 })
        .bind(update.kind)
        .bind(update.value)
        .execute(pool)
        .await?;
    audit::log_event(
        pool,
        Some(update.admin_id),
        if update.is_active {
            "ban_reactivated"
        } else {
            "ban_deactivated"
        },
        update.kind,
        update.value,
        json!({ "bulk": true }),
        update.ip_address,
    )
    .await?;
    sync_policy_files(pool, config).await?;
    execute_reload_commands(pool, shell, Some(update.admin_id), update.ip_address).await
}

pub async fn sync_policy_files(pool: &SqlitePool, config: &Config) -> AppResult<()> {
    let active_bans = sqlx::query_as::<_, Ban>(
        "SELECT id, kind, value, reason, created_by, created_at, expires_at, is_active
         FROM bans
         WHERE is_active = 1 AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)
         ORDER BY kind, value",
    )
    .fetch_all(pool)
    .await?;

    let mut addresses = Vec::new();
    let mut domains = Vec::new();
    let mut ips = Vec::new();
    for ban in active_bans {
        match ban.kind.as_str() {
            "address" => addresses.push(format!("{} REJECT blocked by admin", ban.value)),
            "domain" => domains.push(format!("{} REJECT domain blocked by admin", ban.value)),
            "ip" => ips.push(format!("{} REJECT ip blocked by admin", ban.value)),
            "subnet" => ips.push(format!("{} REJECT subnet blocked by admin", ban.value)),
            _ => {}
        }
    }
    write_text_file(&config.bans.address_file, &addresses.join("\n")).await?;
    write_text_file(&config.bans.domain_file, &domains.join("\n")).await?;
    write_text_file(&config.bans.ip_file, &ips.join("\n")).await?;
    Ok(())
}

async fn execute_reload_commands(
    pool: &SqlitePool,
    shell: &Shell,
    admin_id: Option<i64>,
    ip_address: Option<&str>,
) -> AppResult<Vec<String>> {
    let results = run_reload_commands(shell, &chatmail::bans_reload_commands()).await;
    let mut warnings = Vec::new();
    for (idx, result) in results.into_iter().enumerate() {
        match result {
            Ok(output) => {
                audit::log_event(
                    pool,
                    admin_id,
                    "reload_command_success",
                    "ban_reload",
                    &idx.to_string(),
                    command_result_details(&output),
                    ip_address,
                )
                .await?;
            }
            Err(err) => {
                warnings.push(err.to_string());
                audit::log_event(
                    pool,
                    admin_id,
                    "reload_command_failed",
                    "ban_reload",
                    &idx.to_string(),
                    json!({ "error": err.to_string() }),
                    ip_address,
                )
                .await?;
            }
        }
    }
    Ok(warnings)
}

#[cfg(test)]
pub fn render_ban_files(bans: &[Ban]) -> (String, String, String) {
    let mut addresses = Vec::new();
    let mut domains = Vec::new();
    let mut ips = Vec::new();
    for ban in bans.iter().filter(|ban| ban.is_active == 1) {
        match ban.kind.as_str() {
            "address" => addresses.push(format!("{} REJECT blocked by admin", ban.value)),
            "domain" => domains.push(format!("{} REJECT domain blocked by admin", ban.value)),
            "ip" => ips.push(format!("{} REJECT ip blocked by admin", ban.value)),
            "subnet" => ips.push(format!("{} REJECT subnet blocked by admin", ban.value)),
            _ => {}
        }
    }
    (addresses.join("\n"), domains.join("\n"), ips.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{render_ban_files, Ban};
    use chrono::Utc;

    #[test]
    fn generates_policy_files() {
        let bans = vec![
            Ban {
                id: 1,
                kind: "address".into(),
                value: "bad@example.com".into(),
                reason: "x".into(),
                created_by: Some(1),
                created_at: Utc::now().to_rfc3339(),
                expires_at: None,
                is_active: 1,
            },
            Ban {
                id: 2,
                kind: "subnet".into(),
                value: "198.51.100.0/24".into(),
                reason: "x".into(),
                created_by: Some(1),
                created_at: Utc::now().to_rfc3339(),
                expires_at: None,
                is_active: 1,
            },
        ];
        let (addresses, _, ips) = render_ban_files(&bans);
        assert!(addresses.contains("bad@example.com REJECT blocked by admin"));
        assert!(ips.contains("198.51.100.0/24 REJECT subnet blocked by admin"));
    }
}
