use std::{path::Path, time::Duration};

use serde_json::{json, Value};
use tokio::{fs, process::Command, time::timeout};
use tracing::warn;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct Shell {
    timeout: Duration,
}

impl Shell {
    pub fn new(timeout_seconds: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_seconds),
        }
    }

    pub async fn run(&self, argv: &[String]) -> AppResult<CommandOutput> {
        self.run_with_replacements(argv, &[]).await
    }

    pub async fn run_with_replacements(
        &self,
        argv: &[String],
        replacements: &[(&str, &str)],
    ) -> AppResult<CommandOutput> {
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| AppError::Validation("command argv is empty".into()))?;

        let args = substitute_template_args(args, replacements)?;
        let mut command = Command::new(program);
        command.args(args);
        command.kill_on_drop(true);

        let output = timeout(self.timeout, command.output())
            .await
            .map_err(|_| AppError::Internal(format!("command timed out: {program}")))??;

        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }

    pub async fn write_file(&self, path: &str, content: &str) -> AppResult<()> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(path, content).await?;
        Ok(())
    }
}

pub fn substitute_template_args(
    argv: &[String],
    replacements: &[(&str, &str)],
) -> AppResult<Vec<String>> {
    let mut result = Vec::with_capacity(argv.len());
    for item in argv {
        let mut value = item.clone();
        for (key, replacement) in replacements {
            value = value.replace(key, replacement);
        }
        if value.contains('{') || value.contains('}') {
            return Err(AppError::Validation(format!(
                "unresolved placeholder in command argument: {value}"
            )));
        }
        result.push(value);
    }
    Ok(result)
}

pub fn command_result_details(output: &CommandOutput) -> Value {
    json!({
        "status": output.status,
        "stdout": output.stdout,
        "stderr": output.stderr,
    })
}

pub async fn write_text_file(path: &str, content: &str) -> AppResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, content).await?;
    Ok(())
}

pub async fn run_reload_commands(
    shell: &Shell,
    commands: &[Vec<String>],
) -> Vec<anyhow::Result<CommandOutput>> {
    let mut results = Vec::new();
    for command in commands {
        let output = shell.run(command).await.map_err(anyhow::Error::from);
        if let Err(err) = &output {
            warn!(error = %err, ?command, "reload command failed");
        }
        results.push(output);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::substitute_template_args;

    #[test]
    fn substitutes_address_placeholder() {
        let argv = vec!["quota".into(), "{address}".into(), "x".into()];
        let got = substitute_template_args(&argv, &[("{address}", "user@example.com")]).unwrap();
        assert_eq!(got, vec!["quota", "user@example.com", "x"]);
    }
}
