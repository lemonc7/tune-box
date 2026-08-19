use sqlx::types::Json;

use crate::db::{Download, Metadata, Repository, Status};

impl Repository {
    pub async fn add_download(
        &self,
        provider: &str,
        track_id: &str,
        metadata: &Metadata,
    ) -> Result<Option<i64>, sqlx::Error> {
        let metadata = Json(metadata);

        sqlx::query_scalar!(
            r#"
            INSERT INTO downloads (
                provider,
                track_id,
                metadata
            )
            VALUES (?, ?, ?)
            ON CONFLICT(provider, track_id)
            DO NOTHING
            RETURNING id
            "#,
            provider,
            track_id,
            metadata,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_downloads(
        &self,
        before_id: i64,
        limit: i64,
    ) -> Result<Vec<Download>, sqlx::Error> {
        sqlx::query_as!(
            Download,
            r#"
            SELECT
                id,
                provider,
                track_id,
                metadata AS "metadata!: Json<Metadata>",
                status AS "status!: Status",
                downloaded_bytes,
                total_bytes,
                version,
                retry_count,
                path,
                error_message
            FROM downloads
            WHERE id < ?
            ORDER BY id DESC
            LIMIT ?
            "#,
            before_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    // 原子领取下一个任务，优先领取准备打标签的任务
    pub async fn claim_next(&self, running_ids: &[i64]) -> Result<Option<Download>, sqlx::Error> {
        // 暂停/取消后的旧worker可能尚未完全退出，把当前仍在运行的任务排除掉
        let running_ids = Json(running_ids);
        sqlx::query_as!(
            Download,
            r#"
            UPDATE downloads
            SET status = CASE status
                    WHEN 'pending' THEN 'resolving'
                    WHEN 'downloading' THEN 'tagging'
                END,
                version = version + 1,
                next_retry_at = NULL,
                finished_at = NULL,
                error_message = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = (
                SELECT id FROM downloads
                WHERE status IN ('pending', 'downloaded')
                  AND id NOT IN (
                      SELECT CAST(value AS INTEGER)
                      FROM json_each(?)
                  )
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
                metadata AS "metadata!: Json<Metadata>",
                status AS "status!: Status",
                downloaded_bytes,
                total_bytes,
                version,
                retry_count,
                path,
                error_message
            "#,
            running_ids,
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
            version,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

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
            version,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
