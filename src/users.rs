use std::path::{Path, PathBuf};
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::Serialize;
use tokio::fs;
use tokio::time::sleep;

use crate::{
    chatmail,
    error::{AppError, AppResult},
    shell::{CommandOutput, Shell},
};

#[derive(Debug, Clone, Serialize)]
pub struct UserMailbox {
    pub address: String,
    pub blocked: bool,
    pub mailbox_size: Option<String>,
    pub message_count: Option<String>,
    pub last_seen: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedUser {
    pub address: String,
    pub home_path: Option<String>,
    pub login_disabled: bool,
    pub mailboxes: Vec<String>,
}

pub async fn list_users(shell: &Shell, blocked_values: &[String]) -> Vec<UserMailbox> {
    let output = shell.run(&chatmail::users_list_command()).await;
    let addresses = match output {
        Ok(output) if output.status == 0 => parse_addresses(&output.stdout),
        _ => Vec::new(),
    };

    let mut users = Vec::new();
    for address in addresses {
        let mailbox_size = run_optional(shell, &chatmail::user_size_command(&address)).await;
        let message_count =
            run_optional(shell, &chatmail::user_message_count_command(&address)).await;
        let metadata = run_optional(shell, &chatmail::user_metadata_command(&address)).await;
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

pub async fn load_managed_user(shell: &Shell, address: &str) -> AppResult<ManagedUser> {
    let home_path = resolve_home_path(shell, address).await?;
    let mailboxes = list_mailboxes(shell, address).await.unwrap_or_default();
    let login_disabled = match &home_path {
        Some(path) => is_login_disabled(path),
        None => false,
    };
    Ok(ManagedUser {
        address: address.to_string(),
        home_path,
        login_disabled,
        mailboxes,
    })
}

pub async fn delete_mailbox(shell: &Shell, address: &str) -> AppResult<CommandOutput> {
    let initial = shell
        .run(&chatmail::user_delete_mailbox_command(address))
        .await?;
    if initial.status == 0 {
        return Ok(initial);
    }

    // Live sessions and ongoing LMTP delivery may make INBOX deletion unstable.
    // Fallback to a safer operational flow used on production mail hosts.
    let _ = shell.run(&chatmail::user_kick_command(address)).await;
    let expunge = shell
        .run(&chatmail::user_mailbox_expunge_command(address, "INBOX"))
        .await?;
    let resync = shell
        .run(&chatmail::user_force_resync_command(address))
        .await?;

    if expunge.status == 0 {
        return Ok(CommandOutput {
            status: 0,
            stdout: format!(
                "delete fallback applied; expunge status={}, resync status={}; initial stderr={}",
                expunge.status, resync.status, initial.stderr
            ),
            stderr: String::new(),
        });
    }

    Ok(CommandOutput {
        status: initial.status,
        stdout: format!(
            "delete failed; fallback expunge status={}, resync status={}",
            expunge.status, resync.status
        ),
        stderr: format!(
            "delete stderr: {}; expunge stderr: {}; resync stderr: {}",
            initial.stderr, expunge.stderr, resync.stderr
        ),
    })
}

pub async fn disable_login(shell: &Shell, address: &str) -> AppResult<String> {
    let home = resolve_home_path(shell, address)
        .await?
        .ok_or_else(|| AppError::Validation("user home directory is unavailable".into()))?;
    let password = Path::new(&home).join("password");
    let blocked = Path::new(&home).join("password.blocked");
    if blocked.exists() {
        return Ok("login already disabled".into());
    }
    if !password.exists() {
        return Err(AppError::Validation(format!(
            "password file does not exist: {}",
            password.display()
        )));
    }
    fs::rename(&password, &blocked).await?;
    Ok(format!(
        "renamed {} to {}",
        password.display(),
        blocked.display()
    ))
}

pub async fn enable_login(shell: &Shell, address: &str) -> AppResult<String> {
    let home = resolve_home_path(shell, address)
        .await?
        .ok_or_else(|| AppError::Validation("user home directory is unavailable".into()))?;
    let password = Path::new(&home).join("password");
    let blocked = Path::new(&home).join("password.blocked");
    if password.exists() {
        return Ok("login already enabled".into());
    }
    if !blocked.exists() {
        return Err(AppError::Validation(format!(
            "blocked password file does not exist: {}",
            blocked.display()
        )));
    }
    fs::rename(&blocked, &password).await?;
    Ok(format!(
        "renamed {} to {}",
        blocked.display(),
        password.display()
    ))
}

pub async fn delete_account_lifecycle(shell: &Shell, address: &str) -> AppResult<String> {
    let home = resolve_home_path(shell, address)
        .await?
        .ok_or_else(|| AppError::Validation("user home directory is unavailable".into()))?;
    let home_path = PathBuf::from(&home);
    validate_mail_home_path(&home_path)?;
    if !home_path.exists() {
        return Ok(format!("already absent {}", home_path.display()));
    }

    // Best effort: close active sessions before touching filesystem.
    let _ = shell.run(&chatmail::user_kick_command(address)).await;

    let file_name = home_path
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .ok_or_else(|| AppError::Validation("invalid user home directory".into()))?;
    let parent = home_path
        .parent()
        .ok_or_else(|| AppError::Validation("invalid user home parent".into()))?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| StdDuration::from_secs(0))
        .as_secs();
    let tombstone = parent.join(format!(".deleted-{file_name}-{ts}"));

    fs::rename(&home_path, &tombstone).await?;
    remove_dir_all_with_retries(&tombstone, 8).await?;
    Ok(format!(
        "deleted {} via {}",
        home_path.display(),
        tombstone.display()
    ))
}

pub async fn expunge_mailbox(
    shell: &Shell,
    address: &str,
    mailbox: &str,
) -> AppResult<CommandOutput> {
    validate_mailbox_name(mailbox)?;
    shell
        .run(&chatmail::user_mailbox_expunge_command(address, mailbox))
        .await
}

pub async fn quota_recalc(shell: &Shell, address: &str) -> AppResult<CommandOutput> {
    shell
        .run(&chatmail::user_quota_recalc_command(address))
        .await
}

pub async fn force_resync(shell: &Shell, address: &str) -> AppResult<CommandOutput> {
    shell
        .run(&chatmail::user_force_resync_command(address))
        .await
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

async fn run_optional(shell: &Shell, command: &[String]) -> Option<String> {
    let output = shell.run(command).await.ok()?;
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

async fn resolve_home_path(shell: &Shell, address: &str) -> AppResult<Option<String>> {
    let output = shell.run(&chatmail::user_home_command(address)).await?;
    if output.status != 0 || output.stdout.is_empty() {
        return Ok(None);
    }
    let path = output
        .stdout
        .lines()
        .map(|line| line.trim())
        .find(|line| line.starts_with('/'))
        .or_else(|| {
            output
                .stdout
                .split_whitespace()
                .find(|part| part.starts_with('/'))
        })
        .map(|line| line.to_string());
    Ok(path)
}

async fn list_mailboxes(shell: &Shell, address: &str) -> AppResult<Vec<String>> {
    let output = shell
        .run(&chatmail::user_mailbox_list_command(address))
        .await?;
    if output.status != 0 {
        return Ok(Vec::new());
    }
    let mut rows = output
        .stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    rows.sort();
    rows.dedup();
    Ok(rows)
}

fn validate_mail_home_path(path: &Path) -> AppResult<()> {
    let raw = path.to_string_lossy();
    let allowed = raw.starts_with("/home/vmail/") || raw.starts_with("/var/vmail/");
    if !allowed {
        return Err(AppError::Validation(format!(
            "refusing to delete path outside chatmail maildir roots: {raw}"
        )));
    }
    if raw.len() < 16 {
        return Err(AppError::Validation(format!(
            "refusing to delete suspicious path: {raw}"
        )));
    }
    Ok(())
}

fn validate_mailbox_name(mailbox: &str) -> AppResult<()> {
    let re = Regex::new(r"^[A-Za-z0-9._/-]+$").unwrap();
    if !re.is_match(mailbox) {
        return Err(AppError::Validation(format!(
            "invalid mailbox name: {mailbox}"
        )));
    }
    Ok(())
}

fn is_login_disabled(home: &str) -> bool {
    let password = Path::new(home).join("password");
    let blocked = Path::new(home).join("password.blocked");
    !password.exists() && blocked.exists()
}

async fn remove_dir_all_with_retries(path: &Path, attempts: usize) -> AppResult<()> {
    for attempt in 0..attempts {
        match fs::remove_dir_all(path).await {
            Ok(_) => return Ok(()),
            Err(err) => {
                let should_retry = err.kind() == std::io::ErrorKind::DirectoryNotEmpty
                    || err.kind() == std::io::ErrorKind::Other;
                if !should_retry || attempt + 1 == attempts {
                    return Err(AppError::Internal(format!(
                        "failed to remove {}: {}",
                        path.display(),
                        err
                    )));
                }
                sleep(StdDuration::from_millis(250)).await;
            }
        }
    }
    Err(AppError::Internal(format!(
        "failed to remove {} after retries",
        path.display()
    )))
}
