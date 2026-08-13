pub mod codec;
pub mod download;
pub mod download_state;
pub mod library;

use std::{path::Path, str::FromStr, time::Duration};

use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use tokio::fs;

use crate::error::DatabaseError;

#[derive(Debug, Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn connect(db_url: &str) -> Result<SqlitePool, DatabaseError> {
    if let Some(path) = db_url.strip_prefix("sqlite://")
        && let Some(parent) = Path::new(path).parent()
    {
        fs::create_dir_all(parent).await?;
    }

    let options = SqliteConnectOptions::from_str(db_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
pub async fn connect_memory() -> Result<SqlitePool, DatabaseError> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    MIGRATOR.run(&pool).await?;

    Ok(pool)
}
