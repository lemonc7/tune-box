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

use contracts::{
    download::{Download as SharedDownload, Status as SharedStatus},
    music::Metadata,
};
use tokio::fs;

use crate::{error::DatabaseError, events::DownloadEvents};

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
    download_events: DownloadEvents,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            download_events: DownloadEvents::default(),
        }
    }

    pub fn download_events(&self) -> DownloadEvents {
        self.download_events.clone()
    }

    pub(crate) fn notify_download_progress(
        &self,
        id: i64,
        downloaded_bytes: i64,
        total_bytes: Option<i64>,
    ) {
        self.download_events
            .progress(id, downloaded_bytes, total_bytes);
    }

    pub(crate) fn notify_download_changed(&self, id: i64) {
        self.download_events.changed(id);
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use contracts::{download::DownloadEvent, music::Metadata};

    use super::{Repository, Status, connect_memory};

    fn metadata() -> Metadata {
        Metadata {
            name: "song".to_owned(),
            artists: vec!["artist".to_owned()],
            album: String::new(),
            album_artist: String::new(),
            date: String::new(),
            cover_url: String::new(),
        }
    }

    #[tokio::test]
    async fn persisted_progress_publishes_an_event() {
        let repository = Repository::new(connect_memory().await.unwrap());
        let id = repository
            .add_download("provider", "track", &metadata())
            .await
            .unwrap()
            .unwrap();
        let task = repository.claim_next(&[]).await.unwrap().unwrap();
        assert!(repository.mark_downloading(id, task.version).await.unwrap());
        let mut events = repository.download_events().subscribe();

        assert!(
            repository
                .update_progress(id, task.version, 512, Some(1024))
                .await
                .unwrap()
        );

        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            event,
            DownloadEvent::Progress {
                id,
                downloaded_bytes: 512,
                total_bytes: Some(1024),
            }
        );
    }

    #[tokio::test]
    async fn downloaded_and_retried_files_are_claimed_for_tagging() {
        let repository = Repository::new(connect_memory().await.unwrap());
        repository
            .add_download("provider", "track", &metadata())
            .await
            .unwrap();

        let resolving = repository.claim_next(&[]).await.unwrap().unwrap();
        assert!(matches!(resolving.status, Status::Resolving));
        assert!(
            repository
                .mark_downloading(resolving.id, resolving.version)
                .await
                .unwrap()
        );
        let mut events = repository.download_events().subscribe();
        assert!(
            repository
                .mark_downloaded(resolving.id, resolving.version, "song.flac", 42)
                .await
                .unwrap()
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), events.recv())
                .await
                .unwrap()
                .unwrap(),
            DownloadEvent::Changed { id: resolving.id }
        );

        let tagging = repository.claim_next(&[]).await.unwrap().unwrap();
        assert!(matches!(tagging.status, Status::Tagging));
        assert!(
            repository
                .mark_failed(tagging.id, tagging.version, "tagging failed")
                .await
                .unwrap()
        );
        assert!(repository.retry_download(tagging.id).await.unwrap());

        let retried = repository.claim_next(&[]).await.unwrap().unwrap();
        assert!(matches!(retried.status, Status::Tagging));
    }

    #[tokio::test]
    async fn completing_tagging_publishes_a_changed_event() {
        let repository = Repository::new(connect_memory().await.unwrap());
        repository
            .add_download("provider", "track", &metadata())
            .await
            .unwrap();

        let resolving = repository.claim_next(&[]).await.unwrap().unwrap();
        assert!(
            repository
                .mark_downloading(resolving.id, resolving.version)
                .await
                .unwrap()
        );
        assert!(
            repository
                .mark_downloaded(resolving.id, resolving.version, "ready.flac", 42)
                .await
                .unwrap()
        );
        let tagging = repository.claim_next(&[]).await.unwrap().unwrap();
        assert!(matches!(tagging.status, Status::Tagging));

        let mut events = repository.download_events().subscribe();
        assert!(
            repository
                .complete_with_song(
                    tagging.id,
                    tagging.version,
                    "completed.flac",
                    42,
                    &metadata(),
                )
                .await
                .unwrap()
        );

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), events.recv())
                .await
                .unwrap()
                .unwrap(),
            DownloadEvent::Changed { id: tagging.id }
        );
    }
}
