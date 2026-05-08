use serde::Serialize;

use crate::{chatmail, shell::Shell};

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub level: String,
    pub text: String,
}

pub async fn read_logs(
    shell: &Shell,
    source: chatmail::LogSource,
    query: Option<&str>,
    limit: usize,
) -> Vec<LogLine> {
    if source.identifiers.is_empty() {
        return read_journal_unit(shell, source.unit, query, limit).await;
    }

    read_journal_identifiers(shell, source.identifiers, query, limit).await
}

pub async fn read_journal_unit(
    shell: &Shell,
    unit: &str,
    query: Option<&str>,
    limit: usize,
) -> Vec<LogLine> {
    let command = vec![
        "journalctl".into(),
        "-u".into(),
        unit.to_string(),
        "-n".into(),
        limit.to_string(),
        "--no-pager".into(),
    ];
    let output = shell.run(&command).await;
    let mut lines = match output {
        Ok(output) => output
            .stdout
            .lines()
            .map(|line| LogLine {
                level: classify_log_line(line),
                text: line.to_string(),
            })
            .collect::<Vec<_>>(),
        Err(err) => vec![LogLine {
            level: "error".into(),
            text: err.to_string(),
        }],
    };
    if let Some(query) = query {
        let query = query.to_ascii_lowercase();
        lines.retain(|line| line.text.to_ascii_lowercase().contains(&query));
    }
    lines
}

pub async fn read_journal_identifiers(
    shell: &Shell,
    identifiers: &[&str],
    query: Option<&str>,
    limit: usize,
) -> Vec<LogLine> {
    let mut command = vec!["journalctl".into()];
    for identifier in identifiers {
        command.push("-t".into());
        command.push((*identifier).to_string());
    }
    command.extend([
        "-n".into(),
        limit.to_string(),
        "--no-pager".into(),
        "--output".into(),
        "short-iso".into(),
    ]);

    let output = shell.run(&command).await;
    let mut lines = match output {
        Ok(output) => output
            .stdout
            .lines()
            .map(|line| LogLine {
                level: classify_log_line(line),
                text: line.to_string(),
            })
            .collect::<Vec<_>>(),
        Err(err) => vec![LogLine {
            level: "error".into(),
            text: err.to_string(),
        }],
    };
    if let Some(query) = query {
        let query = query.to_ascii_lowercase();
        lines.retain(|line| line.text.to_ascii_lowercase().contains(&query));
    }
    lines
}

fn classify_log_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if lower.contains("reject") || lower.contains("error") {
        "error".into()
    } else if lower.contains("warn") {
        "warn".into()
    } else {
        "info".into()
    }
}
