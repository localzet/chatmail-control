use std::{fs, path::Path};

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

use crate::error::AppResult;

pub async fn connect(database_url: &str) -> AppResult<SqlitePool> {
    ensure_sqlite_path(database_url)?;

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(pool)
}

fn ensure_sqlite_path(database_url: &str) -> AppResult<()> {
    let Some(path) = sqlite_file_path(database_url) else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if !path.exists() {
        fs::File::create(path)?;
    }

    Ok(())
}

fn sqlite_file_path(database_url: &str) -> Option<&Path> {
    if database_url == "sqlite::memory:" {
        return None;
    }

    let raw = database_url.strip_prefix("sqlite://")?;
    let path = raw.split('?').next().unwrap_or(raw);
    if path.is_empty() {
        return None;
    }

    Some(Path::new(path))
}
