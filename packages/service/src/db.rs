pub mod download;
pub mod download_state;
pub mod library;

use std::{path::Path, str::FromStr, time::Duration};

use serde::{Deserialize, Serialize};
use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    types::Json,
};

use shared::{
    download::{Download as SharedDownload, Status as SharedStatus},
    music::Metadata,
};
use tokio::fs;

use crate::error::DatabaseError;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Resolving,
    Downloading,
    Downloaded,
    Tagging,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl From<Status> for SharedStatus {
    fn from(value: Status) -> Self {
        match value {
            Status::Pending => SharedStatus::Pending,
            Status::Resolving => SharedStatus::Resolving,
            Status::Downloading => SharedStatus::Downloading,
            Status::Downloaded => SharedStatus::Downloaded,
            Status::Tagging => SharedStatus::Tagging,
            Status::Paused => SharedStatus::Paused,
            Status::Completed => SharedStatus::Completed,
            Status::Failed => SharedStatus::Failed,
            Status::Cancelled => SharedStatus::Cancelled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Download {
    pub id: i64,
    pub provider: String,
    pub track_id: String,
    pub metadata: Json<Metadata>,
    pub status: Status,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub version: i64,
    pub retry_count: i64,
    pub path: Option<String>,
    pub error_message: Option<String>,
}

impl From<Download> for SharedDownload {
    fn from(value: Download) -> Self {
        Self {
            id: value.id,
            provider: value.provider,
            track_id: value.track_id,
            metadata: value.metadata.0,
            status: value.status.into(),
            downloaded_bytes: value.downloaded_bytes,
            total_bytes: value.total_bytes,
            error_message: value.error_message,
        }
    }
}

pub struct ScannedSong {
    pub path: String,
    pub name: String,
    pub size: i64,
    pub artists: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

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
