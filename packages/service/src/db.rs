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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_artist: String,
    pub date: String,
    pub cover_url: String,
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
    pub async fn new(path: &str) -> Result<Self, DatabaseError> {
        if let Some(parent) = Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let url = format!("sqlite://{}", path);

        let options = SqliteConnectOptions::from_str(&url)?
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

        Ok(Self { pool })
    }
}
