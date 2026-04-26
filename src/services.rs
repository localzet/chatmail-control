use serde::Serialize;
use sqlx::SqlitePool;

use crate::{chatmail, config::Config, error::AppResult, shell::Shell};

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    pub name: String,
    pub active: bool,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardStats {
    pub services: Vec<ServiceStatus>,
    pub mail_queue_size: usize,
    pub users_count: usize,
    pub active_bans_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceActionResult {
    pub service: String,
    pub action: String,
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub async fn collect_dashboard_stats(
    pool: &SqlitePool,
    shell: &Shell,
    config: &Config,
    users_count: usize,
) -> AppResult<DashboardStats> {
    let mut services = Vec::new();
    for service in &config.health.services {
        let output = shell
            .run(&["systemctl".into(), "is-active".into(), service.to_string()])
            .await;
        match output {
            Ok(output) => services.push(ServiceStatus {
                name: service.clone(),
                active: output.stdout.trim() == "active",
                details: if output.stdout.is_empty() {
                    output.stderr
                } else {
                    output.stdout
                },
            }),
            Err(err) => services.push(ServiceStatus {
                name: service.clone(),
                active: false,
                details: err.to_string(),
            }),
        }
    }

    let queue_output = shell
        .run(&["postqueue".into(), "-p".into()])
        .await
        .ok()
        .map(|o| o.stdout)
        .unwrap_or_default();
    let mail_queue_size = queue_output
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.contains("Mail queue is empty"))
        .count()
        .saturating_sub(1);

    let active_bans_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bans WHERE is_active = 1")
            .fetch_one(pool)
            .await?;

    Ok(DashboardStats {
        services,
        mail_queue_size,
        users_count,
        active_bans_count,
    })
}

pub async fn run_service_action(
    shell: &Shell,
    service: &str,
    action: &str,
) -> AppResult<ServiceActionResult> {
    let argv = match action {
        "status" => chatmail::systemctl_command("status", service),
        "restart" => chatmail::systemctl_command("restart", service),
        "reload" => chatmail::systemctl_command("reload", service),
        _ => chatmail::systemctl_command("status", service),
    };
    let output = shell.run(&argv).await?;
    Ok(ServiceActionResult {
        service: service.to_string(),
        action: action.to_string(),
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}
