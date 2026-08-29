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
            track_number: String::new(),
            disc_number: String::new(),
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
    async fn gets_a_download_by_id() {
        let repository = Repository::new(connect_memory().await.unwrap());
        let id = repository
            .add_download("provider", "track", &metadata())
            .await
            .unwrap()
            .unwrap();

        let download = repository.get_download(id).await.unwrap().unwrap();
        assert_eq!(download.id, id);
        assert_eq!(download.track_id, "track");
        assert!(repository.get_download(id + 1).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn readding_a_cancelled_download_updates_metadata_and_requeues_it() {
        let repository = Repository::new(connect_memory().await.unwrap());
        let id = repository
            .add_download("provider", "track", &metadata())
            .await
            .unwrap()
            .unwrap();
        assert!(repository.cancel_download(id).await.unwrap());

        let mut replacement = metadata();
        replacement.name = "updated song".to_owned();
        replacement.album = "updated album".to_owned();

        let requeued_id = repository
            .add_download("provider", "track", &replacement)
            .await
            .unwrap();
        assert_eq!(requeued_id, Some(id));

        let download = repository.get_download(id).await.unwrap().unwrap();
        assert!(matches!(download.status, Status::Pending));
        assert_eq!(download.metadata.0, replacement);
        assert_eq!(download.downloaded_bytes, 0);
        assert_eq!(download.total_bytes, None);
        assert_eq!(download.retry_count, 0);
        assert_eq!(download.path, None);
        assert_eq!(download.error_message, None);
    }

    #[tokio::test]
    async fn duplicate_non_cancelled_download_keeps_its_original_metadata() {
        let repository = Repository::new(connect_memory().await.unwrap());
        let original = metadata();
        let id = repository
            .add_download("provider", "track", &original)
            .await
            .unwrap()
            .unwrap();

        let mut replacement = metadata();
        replacement.name = "replacement".to_owned();
        assert_eq!(
            repository
                .add_download("provider", "track", &replacement)
                .await
                .unwrap(),
            None
        );

        let download = repository.get_download(id).await.unwrap().unwrap();
        assert_eq!(download.metadata.0, original);
    }

    #[tokio::test]
    async fn download_categories_only_return_matching_tasks() {
        use contracts::download::DownloadCategory;

        let repository = Repository::new(connect_memory().await.unwrap());

        let failed_id = repository
            .add_download("provider", "failed", &metadata())
            .await
            .unwrap()
            .unwrap();
        let failed = repository.claim_next(&[]).await.unwrap().unwrap();
        assert_eq!(failed.id, failed_id);
        assert!(
            repository
                .mark_failed(failed.id, failed.version, "failed")
                .await
                .unwrap()
        );

        let paused_id = repository
            .add_download("provider", "paused", &metadata())
            .await
            .unwrap()
            .unwrap();
        assert!(repository.pause_download(paused_id).await.unwrap());

        let pending_id = repository
            .add_download("provider", "pending", &metadata())
            .await
            .unwrap()
            .unwrap();
        let cancelled_id = repository
            .add_download("provider", "cancelled", &metadata())
            .await
            .unwrap()
            .unwrap();
        assert!(repository.cancel_download(cancelled_id).await.unwrap());

        let active = repository
            .list_downloads_by_category(DownloadCategory::Active, i64::MAX, 20)
            .await
            .unwrap();
        assert_eq!(
            active
                .iter()
                .map(|download| download.id)
                .collect::<Vec<_>>(),
            vec![pending_id]
        );

        let attention = repository
            .list_downloads_by_category(DownloadCategory::Attention, i64::MAX, 20)
            .await
            .unwrap();
        assert_eq!(
            attention
                .iter()
                .map(|download| download.id)
                .collect::<Vec<_>>(),
            vec![paused_id, failed_id]
        );
        assert_eq!(repository.count_attention_downloads().await.unwrap(), 2);

        let history = repository
            .list_downloads_by_category(DownloadCategory::History, i64::MAX, 20)
            .await
            .unwrap();
        assert_eq!(
            history
                .iter()
                .map(|download| download.id)
                .collect::<Vec<_>>(),
            vec![cancelled_id]
        );
    }

    #[tokio::test]
    async fn category_pagination_uses_its_expression_index() {
        let pool = connect_memory().await.unwrap();
        let plan = sqlx::query_as::<_, (i64, i64, i64, String)>(
            r#"
            EXPLAIN QUERY PLAN
            SELECT id
            FROM downloads
            WHERE
                CASE status
                    WHEN 'pending' THEN 0
                    WHEN 'resolving' THEN 0
                    WHEN 'downloading' THEN 0
                    WHEN 'downloaded' THEN 0
                    WHEN 'tagging' THEN 0
                    WHEN 'failed' THEN 1
                    WHEN 'paused' THEN 1
                    ELSE 2
                END = ?
              AND id < ?
            ORDER BY id DESC
            LIMIT ?
            "#,
        )
        .bind(1_i64)
        .bind(i64::MAX)
        .bind(20_i64)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert!(
            plan.iter()
                .any(|(_, _, _, detail)| detail.contains("idx_downloads_category_id")),
            "unexpected query plan: {plan:?}"
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
