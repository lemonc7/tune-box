use sqlx::{SqlitePool, types::Json};
use time::OffsetDateTime;

use crate::{
    model::{DownloadRequest, Metadata},
    server::db::model::{Download, Status},
};

#[derive(Debug, Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // 原子领取下一个任务，优先领取下载好的准备打标签的任务
    pub async fn claim_next(&self) -> Result<Option<Download>, sqlx::Error> {
        sqlx::query_as!(
            Download,
            r#"
            UPDATE downloads
            SET status = CASE status
                    WHEN 'pending' THEN 'resolving'
                    WHEN 'downloaded' THEN 'tagging'
                END,
                version = version + 1,
                next_retry_at = NULL,
                finished_at = NULL,
                error_message = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = (
                SELECT id FROM downloads
                WHERE status IN ('pending', 'downloaded')
                  AND (
                      next_retry_at IS NULL
                      OR julianday(next_retry_at) <= julianday('now')
                  )
                ORDER BY
                    CASE status
                        WHEN 'downloaded' THEN 0
                        ELSE 1
                    END,
                    COALESCE(next_retry_at, created_at),
                    created_at,
                    id
                LIMIT 1
            )
            RETURNING
                id,
                provider,
                track_id,
                quality,
                metadata AS "metadata!: Json<Metadata>",
                status AS "status!: Status",
                downloaded_bytes,
                total_bytes,
                version,
                retry_count,
                next_retry_at AS "next_retry_at?: OffsetDateTime",
                path,
                error_message,
                created_at AS "created_at!: OffsetDateTime",
                finished_at AS "finished_at?: OffsetDateTime",
                updated_at AS "updated_at!: OffsetDateTime"
            "#,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn mark_downloading(&self, id: i64, version: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'downloading',
                error_message = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'resolving'
              AND version = ?
            "#,
            id,
            version,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn mark_downloaded(
        &self,
        id: i64,
        version: i64,
        path: &str,
        size: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'downloaded',
                path = ?,
                downloaded_bytes = ?,
                total_bytes = ?,
                retry_count = 0,
                next_retry_at = NULL,
                error_message = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'downloading'
              AND version = ?
            "#,
            path,
            size,
            size,
            id,
            version
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn add_download(
        &self,
        req: DownloadRequest,
    ) -> Result<Option<Download>, sqlx::Error> {
        let DownloadRequest {
            provider,
            track_id,
            quality,
            metadata,
        } = req;

        let metadata =
            serde_json::to_string(&metadata).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

        sqlx::query_as!(
            Download,
            r#"
            INSERT INTO downloads (
                provider,
                track_id,
                quality,
                metadata
            )
            VALUES (?, ?, ?, ?)
            ON CONFLICT(provider, track_id, quality)
            DO NOTHING
            RETURNING
                id,
                provider,
                track_id,
                quality,
                metadata AS "metadata!: Json<Metadata>",
                status AS "status!: Status",
                downloaded_bytes,
                total_bytes,
                version,
                retry_count,
                next_retry_at AS "next_retry_at?: OffsetDateTime",
                path,
                error_message,
                created_at AS "created_at!: OffsetDateTime",
                finished_at AS "finished_at?: OffsetDateTime",
                updated_at AS "updated_at!: OffsetDateTime"
            "#,
            provider,
            track_id,
            quality,
            metadata,
        )
        .fetch_optional(&self.pool)
        .await
    }

    // 更新下载进度
    pub async fn update_progress(
        &self,
        id: i64,
        version: i64,
        downloaded: i64,
        total: Option<i64>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET downloaded_bytes = ?,
                total_bytes = COALESCE(?, total_bytes),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'downloading'
              AND version = ?
            "#,
            downloaded,
            total,
            id,
            version
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    /// 原子写入曲库数据并完成下载任务。
    pub async fn complete_with_song(
        &self,
        id: i64,
        version: i64,
        final_path: &str,
        extension: &str,
        size: i64,
        metadata: &Metadata,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let updated = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'completed',
                path = ?,
                retry_count = 0,
                next_retry_at = NULL,
                error_message = NULL,
                finished_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'tagging'
              AND version = ?
            "#,
            final_path,
            id,
            version,
        )
        .execute(&mut *tx)
        .await?;

        if updated.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(false);
        }

        let song_id = sqlx::query_scalar!(
            r#"
            INSERT INTO songs (path, name, extension, size)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(path) DO UPDATE SET
                name = excluded.name,
                extension = excluded.extension,
                size = excluded.size,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id
            "#,
            final_path,
            metadata.name,
            extension,
            size,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!("DELETE FROM song_artists WHERE song_id = ?", song_id)
            .execute(&mut *tx)
            .await?;

        for artist in &metadata.artists {
            let artist = artist.trim();
            if artist.is_empty() {
                continue;
            }

            sqlx::query!(
                r#"
                INSERT INTO song_artists (song_id, artist)
                VALUES (?, ?)
                ON CONFLICT(song_id, artist) DO NOTHING
                "#,
                song_id,
                artist,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(true)
    }

    // 下载失败后重新排队
    pub async fn requeue_download(
        &self,
        id: i64,
        version: i64,
        next_retry_at: OffsetDateTime,
        message: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'pending',
                retry_count = retry_count + 1,
                next_retry_at = ?,
                error_message = ?,
                finished_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status IN ('resolving', 'downloading')
              AND version = ?
            "#,
            next_retry_at,
            message,
            id,
            version
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    // 标签失败后重新排队
    pub async fn requeue_tagging(
        &self,
        id: i64,
        version: i64,
        next_retry_at: OffsetDateTime,
        message: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'downloaded',
                retry_count = retry_count + 1,
                next_retry_at = ?,
                error_message = ?,
                finished_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'tagging'
              AND version = ?
            "#,
            next_retry_at,
            message,
            id,
            version
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    /// pause/cancel/shutdown 引起的中断不会消耗重试次数。
    pub async fn requeue_interrupted_download(
        &self,
        id: i64,
        version: i64,
        message: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'pending',
                next_retry_at = NULL,
                error_message = ?,
                finished_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status IN ('resolving', 'downloading')
              AND version = ?
            "#,
            message,
            id,
            version,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn requeue_interrupted_tagging(
        &self,
        id: i64,
        version: i64,
        message: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'downloaded',
                next_retry_at = NULL,
                error_message = ?,
                finished_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'tagging'
              AND version = ?
            "#,
            message,
            id,
            version,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    // 达到指定重试次数，标记失败
    pub async fn mark_failed(
        &self,
        id: i64,
        version: i64,
        message: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'failed',
                next_retry_at = NULL,
                error_message = ?,
                finished_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status IN ('resolving', 'downloading', 'tagging')
              AND version = ?
            "#,
            message,
            id,
            version
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn cancel_download(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'cancelled',
                next_retry_at = NULL,
                finished_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status NOT IN ('completed', 'failed', 'cancelled')
            "#,
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn pause_download(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'paused',
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status IN ('pending', 'resolving', 'downloading')
            "#,
            id,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn resume_download(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'pending',
                next_retry_at = NULL,
                error_message = NULL,
                finished_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'paused'
            "#,
            id,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn recover_interrupted(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = CASE
                    WHEN status IN ('resolving', 'downloading') THEN 'pending'
                    WHEN status = 'tagging' THEN 'downloaded'
                END,
                version = version + 1,
                next_retry_at = NULL,
                error_message = '上一次执行意外中断，任务已恢复',
                finished_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE status IN ('resolving', 'downloading', 'tagging')
            "#
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// 恢复一个异常退出的 Worker；版本条件防止旧 Worker 覆盖后来重新领取的任务。
    pub async fn recover_worker(
        &self,
        id: i64,
        version: i64,
        message: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = CASE
                    WHEN status IN ('resolving', 'downloading') THEN 'pending'
                    WHEN status = 'tagging' THEN 'downloaded'
                END,
                next_retry_at = NULL,
                error_message = ?,
                finished_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND version = ?
              AND status IN ('resolving', 'downloading', 'tagging')
            "#,
            message,
            id,
            version,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{DownloadRequest, Metadata},
        server::db,
    };

    fn request(track_id: &str) -> DownloadRequest {
        DownloadRequest {
            provider: "test".to_string(),
            track_id: track_id.to_string(),
            quality: "lossless".to_string(),
            metadata: Metadata {
                name: "song".to_string(),
                artists: vec!["artist".to_string()],
                album: String::new(),
                album_artist: String::new(),
                date: String::new(),
                cover_url: String::new(),
            },
        }
    }

    #[tokio::test]
    async fn worker_recovery_is_fenced_by_version() {
        let repository = Repository::new(db::connect_memory().await.unwrap());
        let task = repository
            .add_download(request("worker-panic"))
            .await
            .unwrap()
            .unwrap();
        let claim = repository.claim_next().await.unwrap().unwrap();

        assert!(
            !repository
                .recover_worker(task.id, claim.version + 1, "stale worker")
                .await
                .unwrap()
        );
        assert!(
            repository
                .recover_worker(task.id, claim.version, "worker panic")
                .await
                .unwrap()
        );

        let reclaimed = repository.claim_next().await.unwrap().unwrap();
        assert_eq!(reclaimed.id, task.id);
        assert!(reclaimed.version > claim.version);
    }

    #[tokio::test]
    async fn pause_invalidates_the_claimed_state_and_resume_reclaims() {
        let repository = Repository::new(db::connect_memory().await.unwrap());
        let task = repository
            .add_download(request("pause"))
            .await
            .unwrap()
            .unwrap();

        let first_claim = repository.claim_next().await.unwrap().unwrap();
        assert_eq!(first_claim.id, task.id);
        assert_eq!(first_claim.status, Status::Resolving);

        assert!(repository.pause_download(task.id).await.unwrap());
        assert!(
            !repository
                .mark_downloading(task.id, first_claim.version)
                .await
                .unwrap()
        );

        assert!(repository.resume_download(task.id).await.unwrap());
        let second_claim = repository.claim_next().await.unwrap().unwrap();
        assert_eq!(second_claim.id, task.id);
        assert_eq!(second_claim.status, Status::Resolving);
        assert!(second_claim.version > first_claim.version);
    }

    #[tokio::test]
    async fn downloaded_task_is_claimed_for_tagging_and_cancel_rejects_stale_commit() {
        let repository = Repository::new(db::connect_memory().await.unwrap());
        let task = repository
            .add_download(request("tag"))
            .await
            .unwrap()
            .unwrap();
        let download_claim = repository.claim_next().await.unwrap().unwrap();

        assert!(
            repository
                .mark_downloading(task.id, download_claim.version)
                .await
                .unwrap()
        );
        assert!(
            repository
                .mark_downloaded(task.id, download_claim.version, "/tmp/song.ready", 10)
                .await
                .unwrap()
        );

        let tagging_claim = repository.claim_next().await.unwrap().unwrap();
        assert_eq!(tagging_claim.status, Status::Tagging);
        assert!(repository.cancel_download(task.id).await.unwrap());
        assert!(
            !repository
                .complete_with_song(
                    task.id,
                    tagging_claim.version,
                    "/tmp/song.flac",
                    "flac",
                    10,
                    &tagging_claim.metadata,
                )
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn completing_task_writes_song_and_artists_in_order() {
        let repository = Repository::new(db::connect_memory().await.unwrap());
        let mut request = request("complete");
        request.metadata.artists = vec!["first".to_string(), "second".to_string()];
        let task = repository.add_download(request).await.unwrap().unwrap();
        let download_claim = repository.claim_next().await.unwrap().unwrap();

        assert!(
            repository
                .mark_downloading(task.id, download_claim.version)
                .await
                .unwrap()
        );
        assert!(
            repository
                .mark_downloaded(task.id, download_claim.version, "/tmp/song.ready.flac", 10)
                .await
                .unwrap()
        );

        let tagging_claim = repository.claim_next().await.unwrap().unwrap();
        assert!(
            repository
                .complete_with_song(
                    task.id,
                    tagging_claim.version,
                    "/tmp/song - first, second.flac",
                    "flac",
                    12,
                    &tagging_claim.metadata,
                )
                .await
                .unwrap()
        );

        let song_id = sqlx::query_scalar!(
            "SELECT id FROM songs WHERE path = ?",
            "/tmp/song - first, second.flac"
        )
        .fetch_one(&repository.pool)
        .await
        .unwrap();
        let artists = sqlx::query_scalar!(
            "SELECT artist FROM song_artists WHERE song_id = ? ORDER BY id",
            song_id
        )
        .fetch_all(&repository.pool)
        .await
        .unwrap();

        assert_eq!(artists, ["first", "second"]);
    }

    #[tokio::test]
    async fn retry_waits_until_due_and_successful_stage_resets_counter() {
        let repository = Repository::new(db::connect_memory().await.unwrap());
        let task = repository
            .add_download(request("retry"))
            .await
            .unwrap()
            .unwrap();
        let first_claim = repository.claim_next().await.unwrap().unwrap();
        let retry_at = OffsetDateTime::now_utc() + time::Duration::minutes(1);

        assert!(
            repository
                .requeue_download(task.id, first_claim.version, retry_at, "temporary failure")
                .await
                .unwrap()
        );
        assert!(repository.claim_next().await.unwrap().is_none());

        let due_at = OffsetDateTime::now_utc() - time::Duration::seconds(1);
        sqlx::query!(
            "UPDATE downloads SET next_retry_at = ? WHERE id = ?",
            due_at,
            task.id
        )
        .execute(&repository.pool)
        .await
        .unwrap();

        let second_claim = repository.claim_next().await.unwrap().unwrap();
        assert_eq!(second_claim.retry_count, 1);
        assert!(second_claim.next_retry_at.is_none());
        assert!(
            repository
                .mark_downloading(task.id, second_claim.version)
                .await
                .unwrap()
        );
        assert!(
            repository
                .mark_downloaded(task.id, second_claim.version, "/tmp/retry.ready.flac", 10)
                .await
                .unwrap()
        );

        let tagging_claim = repository.claim_next().await.unwrap().unwrap();
        assert_eq!(tagging_claim.retry_count, 0);
        assert!(tagging_claim.next_retry_at.is_none());
    }
}
