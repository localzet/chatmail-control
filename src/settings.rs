use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, SqlitePool};

use crate::{
    audit, chatmail,
    config::Config,
    error::AppResult,
    shell::{command_result_details, run_reload_commands, write_text_file, Shell},
};

#[derive(Debug, Clone, FromRow)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationSettings {
    pub registration_mode: String,
    pub max_accounts_per_ip_per_day: i64,
    pub max_accounts_per_day: i64,
    pub cleanup_empty_mailboxes_after_days: i64,
    pub notes: String,
}

impl Default for RegistrationSettings {
    fn default() -> Self {
        Self {
            registration_mode: "public".into(),
            max_accounts_per_ip_per_day: 3,
            max_accounts_per_day: 100,
            cleanup_empty_mailboxes_after_days: 30,
            notes: String::new(),
        }
    }
}

pub async fn load(pool: &SqlitePool) -> AppResult<RegistrationSettings> {
    let rows = sqlx::query_as::<_, SettingRow>("SELECT key, value FROM server_settings")
        .fetch_all(pool)
        .await?;
    if rows.is_empty() {
        return Ok(RegistrationSettings::default());
    }
    let mut data = RegistrationSettings::default();
    for row in rows {
        match row.key.as_str() {
            "registration_mode" => data.registration_mode = row.value,
            "max_accounts_per_ip_per_day" => {
                data.max_accounts_per_ip_per_day = row.value.parse().unwrap_or(3)
            }
            "max_accounts_per_day" => data.max_accounts_per_day = row.value.parse().unwrap_or(100),
            "cleanup_empty_mailboxes_after_days" => {
                data.cleanup_empty_mailboxes_after_days = row.value.parse().unwrap_or(30)
            }
            "notes" => data.notes = row.value,
            _ => {}
        }
    }
    Ok(data)
}

pub async fn save(
    pool: &SqlitePool,
    shell: &Shell,
    config: &Config,
    admin_id: i64,
    settings: &RegistrationSettings,
    ip_address: Option<&str>,
) -> AppResult<Vec<String>> {
    let data = BTreeMap::from([
        ("registration_mode", settings.registration_mode.clone()),
        (
            "max_accounts_per_ip_per_day",
            settings.max_accounts_per_ip_per_day.to_string(),
        ),
        (
            "max_accounts_per_day",
            settings.max_accounts_per_day.to_string(),
        ),
        (
            "cleanup_empty_mailboxes_after_days",
            settings.cleanup_empty_mailboxes_after_days.to_string(),
        ),
        ("notes", settings.notes.clone()),
    ]);

    for (key, value) in data {
        sqlx::query(
            "INSERT INTO server_settings (key, value, updated_at)
             VALUES (?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    }

    generate_policy_file(config, settings).await?;
    audit::log_event(
        pool,
        Some(admin_id),
        "settings_updated",
        "server_settings",
        "registration",
        json!(settings),
        ip_address,
    )
    .await?;

    let results = run_reload_commands(shell, &chatmail::settings_reload_commands()).await;
    let mut warnings = Vec::new();
    for (idx, result) in results.into_iter().enumerate() {
        match result {
            Ok(output) => {
                audit::log_event(
                    pool,
                    Some(admin_id),
                    "reload_command_success",
                    "settings_reload",
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
                    Some(admin_id),
                    "reload_command_failed",
                    "settings_reload",
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

pub async fn generate_policy_file(
    config: &Config,
    settings: &RegistrationSettings,
) -> AppResult<()> {
    let raw = toml::to_string_pretty(settings)
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    write_text_file(&config.settings.generated_policy_file, &raw).await
}

#[cfg(test)]
mod tests {
    use super::{load, save, RegistrationSettings};
    use crate::{config::Config, db, shell::Shell};
    use tempfile::tempdir;

    #[tokio::test]
    async fn saves_and_loads_settings() {
        let dir = tempdir().unwrap();
        let cfg_raw = include_str!("../config.example.toml")
            .replace("/var/lib/chatmail-control/chatmail-control.db", ":memory:")
            .replace(
                "/etc/chatmail-control/policy.toml",
                &dir.path().join("policy.toml").display().to_string(),
            );
        let config: Config = toml::from_str(&cfg_raw).unwrap();
        let pool = db::connect("sqlite::memory:").await.unwrap();
        let shell = Shell::new(1);
        crate::auth::upsert_admin(&pool, "admin", "secret")
            .await
            .unwrap();
        let admin_id: i64 = sqlx::query_scalar("SELECT id FROM admins WHERE username = ?")
            .bind("admin")
            .fetch_one(&pool)
            .await
            .unwrap();
        let settings = RegistrationSettings {
            registration_mode: "invite_only".into(),
            max_accounts_per_ip_per_day: 1,
            max_accounts_per_day: 5,
            cleanup_empty_mailboxes_after_days: 14,
            notes: "note".into(),
        };
        save(&pool, &shell, &config, admin_id, &settings, None)
            .await
            .unwrap();
        let loaded = load(&pool).await.unwrap();
        assert_eq!(loaded.registration_mode, "invite_only");
        assert_eq!(loaded.max_accounts_per_day, 5);
    }
}
