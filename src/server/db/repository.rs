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
                attempt,
                resource_url,
                path,
                error_message,
                created_at AS "created_at!: OffsetDateTime",
                started_at AS "started_at?: OffsetDateTime",
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

    pub async fn get_download(&self, id: i64) -> Result<Option<Download>, sqlx::Error> {
        sqlx::query_as!(
            Download,
            r#"
            SELECT
                id,
                provider,
                track_id,
                quality,
                metadata AS "metadata!: Json<Metadata>",
                status AS "status!: Status",
                downloaded_bytes,
                total_bytes,
                attempt,
                resource_url,
                path,
                error_message,
                created_at AS "created_at!: OffsetDateTime",
                started_at AS "started_at?: OffsetDateTime",
                finished_at AS "finished_at?: OffsetDateTime",
                updated_at AS "updated_at!: OffsetDateTime"
            FROM downloads
            WHERE id = ?
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn claim_download(&self, id: i64) -> Result<Option<Download>, sqlx::Error> {
        sqlx::query_as!(
            Download,
            r#"
            UPDATE downloads
            SET status = 'resolving',
                started_at = CURRENT_TIMESTAMP,
                finished_at = NULL,
                error_message = NULL,
                attempt = attempt + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'pending'
            RETURNING
                id,
                provider,
                track_id,
                quality,
                metadata AS "metadata!: Json<Metadata>",
                status AS "status!: Status",
                downloaded_bytes,
                total_bytes,
                attempt,
                resource_url,
                path,
                error_message,
                created_at AS "created_at!: OffsetDateTime",
                started_at AS "started_at?: OffsetDateTime",
                finished_at AS "finished_at?: OffsetDateTime",
                updated_at AS "updated_at!: OffsetDateTime"
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn pending_download_ids(&self) -> Result<Vec<i64>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"
            SELECT id FROM downloads
            WHERE status = 'pending'
            ORDER BY id;
            "#,
        )
        .fetch_all(&self.pool)
        .await
    }
}
