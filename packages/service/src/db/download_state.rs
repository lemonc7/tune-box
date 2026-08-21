use time::OffsetDateTime;

use crate::db::Repository;

impl Repository {
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
            version,
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
            version,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    // pause/cancel/shutdown引起的中断不会消耗重试次数
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
            version,
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
                downloaded_bytes = 0,
                total_bytes = NULL,
                path = NULL,
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

    pub async fn retry_download(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = CASE WHEN path IS NULL THEN 'pending' ELSE 'downloaded' END,
                retry_count = 0,
                next_retry_at = NULL,
                error_message = NULL,
                finished_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status IN ('failed', 'cancelled')
            "#,
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn delete_download(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM downloads
            WHERE id = ?
              AND status IN ('completed', 'failed', 'cancelled')
            "#,
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    // 恢复数据库中执行到一半的下载任务状态
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
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // 恢复一个异常退出的worker,通过版本条件防止旧worker覆盖后来重新领取的任务
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
