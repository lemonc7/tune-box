use crate::shared::{
    download::{Download, DownloadStatus},
    music::Metadata,
};

use super::Repository;

impl Repository {
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
                quality,
                metadata AS "metadata!: Metadata",
                status AS "status!: DownloadStatus",
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

    pub async fn get_download(&self, id: i64) -> Result<Option<Download>, sqlx::Error> {
        sqlx::query_as!(
            Download,
            r#"
            SELECT
                id,
                provider,
                track_id,
                quality,
                metadata AS "metadata!: Metadata",
                status AS "status!: DownloadStatus",
                downloaded_bytes,
                total_bytes,
                version,
                retry_count,
                path,
                error_message
            FROM downloads
            WHERE id = ?
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
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
                metadata AS "metadata!: Metadata",
                status AS "status!: DownloadStatus",
                downloaded_bytes,
                total_bytes,
                version,
                retry_count,
                path,
                error_message
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
}

#[cfg(test)]
mod tests {
    use crate::{server::db, shared::music::Metadata};

    use super::Repository;

    #[tokio::test]
    async fn cursor_loads_older_rows_without_duplicates() {
        let pool = db::connect_memory().await.unwrap();
        let repository = Repository::new(pool);
        let metadata = Metadata {
            name: "song".to_owned(),
            artists: vec!["artist".to_owned()],
            album: String::new(),
            album_artist: String::new(),
            date: String::new(),
            cover_url: String::new(),
        };

        for track_id in ["1", "2", "3"] {
            repository
                .add_download("kuwo", track_id, "flac", &metadata)
                .await
                .unwrap();
        }

        let first = repository.list_downloads(i64::MAX, 2).await.unwrap();
        assert_eq!(first.iter().map(|row| row.id).collect::<Vec<_>>(), [3, 2]);

        let second = repository
            .list_downloads(first.last().unwrap().id, 2)
            .await
            .unwrap();
        assert_eq!(second.iter().map(|row| row.id).collect::<Vec<_>>(), [1]);
    }
}
