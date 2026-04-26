use crate::{
    chatmail,
    config::Config,
    error::AppResult,
    shell::{CommandOutput, Shell},
};

#[derive(Debug, Clone)]
pub struct PolicySyncResult {
    pub changed: Vec<String>,
    pub unchanged: Vec<String>,
}

pub async fn ensure_ban_policy(shell: &Shell, config: &Config) -> AppResult<PolicySyncResult> {
    let recipient_required = vec![
        format!(
            "check_recipient_access texthash:{}",
            config.bans.address_file
        ),
        "reject_unauth_destination".to_string(),
    ];
    let sender_required = vec![
        format!("check_sender_access texthash:{}", config.bans.address_file),
        format!("check_sender_access texthash:{}", config.bans.domain_file),
    ];
    let client_required = vec![format!(
        "check_client_access texthash:{}",
        config.bans.ip_file
    )];

    let mut changed = Vec::new();
    let mut unchanged = Vec::new();

    apply_setting(
        shell,
        "smtpd_recipient_restrictions",
        &recipient_required,
        &mut changed,
        &mut unchanged,
    )
    .await?;
    apply_setting(
        shell,
        "smtpd_sender_restrictions",
        &sender_required,
        &mut changed,
        &mut unchanged,
    )
    .await?;
    apply_setting(
        shell,
        "smtpd_client_restrictions",
        &client_required,
        &mut changed,
        &mut unchanged,
    )
    .await?;

    let _ = shell.run(&chatmail::postfix_reload_command()).await;

    Ok(PolicySyncResult { changed, unchanged })
}

async fn apply_setting(
    shell: &Shell,
    key: &str,
    required: &[String],
    changed: &mut Vec<String>,
    unchanged: &mut Vec<String>,
) -> AppResult<()> {
    let current = read_postconf(shell, key).await?;
    let merged = merge_restrictions(&current, required);
    if normalize_restriction_list(&current) == normalize_restriction_list(&merged) {
        unchanged.push(format!("{key} unchanged"));
        return Ok(());
    }
    let output = shell
        .run(&chatmail::postfix_set_param_command(key, &merged))
        .await?;
    changed.push(format!("{key} updated: {}", format_output(&output)));
    Ok(())
}

async fn read_postconf(shell: &Shell, key: &str) -> AppResult<String> {
    let output = shell
        .run(&chatmail::postfix_show_param_command(key))
        .await?;
    if output.status == 0 {
        Ok(output.stdout.trim().to_string())
    } else {
        Ok(String::new())
    }
}

fn merge_restrictions(current: &str, required: &[String]) -> String {
    let mut items = split_restrictions(current);
    for req in required.iter().rev() {
        if !items
            .iter()
            .any(|item| normalize_rule(item) == normalize_rule(req))
        {
            items.insert(0, req.clone());
        }
    }
    items.join(", ")
}

fn split_restrictions(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn normalize_restriction_list(raw: &str) -> Vec<String> {
    split_restrictions(raw)
        .into_iter()
        .map(|item| normalize_rule(&item))
        .collect()
}

fn normalize_rule(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn format_output(output: &CommandOutput) -> String {
    if output.stderr.is_empty() {
        output.stdout.clone()
    } else {
        output.stderr.clone()
    }
}
