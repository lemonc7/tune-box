use sqlx::{SqlitePool, types::Json};
use time::OffsetDateTime;

use crate::{
    server::db::model::DownloadRow,
    shared::{download::DownloadStatus, music::Metadata},
};

#[derive(Debug, Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn add_download(
        &self,
        provider: &str,
        track_id: &str,
        quality: &str,
        metadata: &Metadata,
    ) -> Result<Option<i64>, sqlx::Error> {
        let metadata =
            serde_json::to_string(metadata).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

        sqlx::query_scalar!(
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
            RETURNING id
            "#,
            provider,
            track_id,
            quality,
            metadata,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_downloads(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DownloadRow>, sqlx::Error> {
        sqlx::query_as!(
            DownloadRow,
            r#"
            SELECT
                id,
                provider,
                track_id,
                quality,
                metadata AS "metadata!: Json<Metadata>",
                status AS "status!: DownloadStatus",
                downloaded_bytes,
                total_bytes,
                version,
                retry_count,
                next_retry_at,
                path,
                error_message,
                created_at,
                finished_at,
                updated_at
            FROM downloads
            ORDER BY id DESC
            LIMIT ? OFFSET ?
            "#,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await
    }

    // 原子领取下一个任务，优先领取下载好的准备打标签的任务
    pub async fn claim_next(&self) -> Result<Option<DownloadRow>, sqlx::Error> {
        sqlx::query_as!(
            DownloadRow,
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
                status AS "status!: DownloadStatus",
                downloaded_bytes,
                total_bytes,
                version,
                retry_count,
                next_retry_at,
                path,
                error_message,
                created_at,
                finished_at,
                updated_at
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
