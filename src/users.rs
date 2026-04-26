use regex::Regex;
use serde::Serialize;

use crate::{config::Config, shell::Shell};

#[derive(Debug, Clone, Serialize)]
pub struct UserMailbox {
    pub address: String,
    pub blocked: bool,
    pub mailbox_size: Option<String>,
    pub message_count: Option<String>,
    pub last_seen: Option<String>,
    pub metadata: Option<String>,
}

pub async fn list_users(
    shell: &Shell,
    config: &Config,
    blocked_values: &[String],
) -> Vec<UserMailbox> {
    let output = shell.run(&config.users.list_command).await;
    let addresses = match output {
        Ok(output) if output.status == 0 => parse_addresses(&output.stdout),
        _ => Vec::new(),
    };

    let mut users = Vec::new();
    for address in addresses {
        let mailbox_size = run_optional(shell, &config.users.size_command, &address).await;
        let message_count =
            run_optional(shell, &config.users.message_count_command, &address).await;
        let metadata = run_optional(shell, &config.users.metadata_command, &address).await;
        let last_seen = metadata
            .as_ref()
            .and_then(|raw| find_last_seen(raw))
            .or_else(|| metadata.clone().map(|_| "unknown".into()));
        users.push(UserMailbox {
            blocked: blocked_values.iter().any(|v| v == &address),
            address,
            mailbox_size,
            message_count,
            last_seen,
            metadata,
        });
    }
    users
}

fn parse_addresses(stdout: &str) -> Vec<String> {
    let email_re = Regex::new(r"([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})").unwrap();
    let mut addresses = Vec::new();
    for line in stdout.lines() {
        if let Some(caps) = email_re.captures(line) {
            addresses.push(caps[1].to_string());
        }
    }
    addresses.sort();
    addresses.dedup();
    addresses
}

async fn run_optional(shell: &Shell, command: &[String], address: &str) -> Option<String> {
    if command.is_empty() {
        return None;
    }
    let output = shell
        .run_with_replacements(command, &[("{address}", address)])
        .await
        .ok()?;
    if output.status == 0 && !output.stdout.is_empty() {
        Some(output.stdout)
    } else {
        None
    }
}

fn find_last_seen(metadata: &str) -> Option<String> {
    metadata
        .lines()
        .find(|line| line.to_ascii_lowercase().contains("last"))
        .map(|line| line.trim().to_string())
}
