use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

use crate::error::AppResult;

pub async fn connect(database_url: &str) -> AppResult<SqlitePool> {
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
