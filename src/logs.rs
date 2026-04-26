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
    let command = chatmail::log_source_command(source, limit);
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
