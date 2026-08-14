use std::time::Duration;

use ::time::Duration as TimeDuration;
use garde::Validate;
use sqlx::types::time::OffsetDateTime;
use tokio::{sync::watch, time};
use tokio_util::sync::CancellationToken;

use crate::{
    error::{DownloadError, ProviderError, ServiceError, TagError},
    server::{
        db::Repository,
        download::{DownloadOptions, DownloadProgress, DownloadResult},
        model::{DOWNLOAD_QUALITY_ID, ResolvedResource},
    },
    shared::{
        download::{Download, DownloadPage, DownloadStatus},
        music::Metadata,
        request::CreateDownloadRequest,
    },
};

use super::Service;

const MAX_RETRIES: i64 = 5;
const BASE_RETRY_DELAY_SECS: u64 = 2;
const MAX_RETRY_DELAY_SECS: u64 = 5 * 60;

impl Service {
    pub async fn create_download(
        &self,
        request: CreateDownloadRequest,
    ) -> Result<Option<i64>, ServiceError> {
        let request = request.normalize();
        request
            .validate()
            .map_err(|error| ServiceError::InvalidRequest(error.to_string()))?;
        if request.quality != DOWNLOAD_QUALITY_ID {
            return Err(ServiceError::UnsupportedDownloadQuality(request.quality));
        }
        self.provider(&request.provider)?;
        let id = self
            .repository
            .add_download(
                &request.provider,
                &request.track_id,
                &request.quality,
                &Metadata {
                    name: request.name,
                    artists: request.artists,
                    album: request.album,
                    album_artist: request.album_artist,
                    date: request.date,
                    cover_url: request.cover_url,
                },
            )
            .await?;
        Ok(id)
    }

    pub async fn list_downloads(
        &self,
        before_id: Option<i64>,
        limit: i64,
    ) -> Result<DownloadPage, ServiceError> {
        let limit = limit.clamp(1, 100);
        let fetch_limit = limit + 1;
        let cursor = before_id.unwrap_or(i64::MAX);
        let mut rows = self.repository.list_downloads(cursor, fetch_limit).await?;

        let has_more = rows.len() > limit as usize;
        if has_more {
            rows.truncate(limit as usize);
        }

        let next_before_id = if has_more {
            rows.last().map(|row| row.id)
        } else {
            None
        };

        Ok(DownloadPage {
            downloads: rows,
            next_before_id,
        })
    }

    pub async fn get_download(&self, id: i64) -> Result<Option<Download>, ServiceError> {
        Ok(self.repository.get_download(id).await?)
    }

    pub async fn cleanup_download_files(&self, id: i64) -> Result<(), ServiceError> {
        self.downloader.remove_task_files(id).await?;
        Ok(())
    }

    pub async fn delete_download(&self, id: i64) -> Result<bool, ServiceError> {
        let deleted = self.repository.delete_download(id).await?;
        if deleted {
            self.cleanup_download_files(id).await?;
        }
        Ok(deleted)
    }

    /// 执行一个已经由 `Repository::claim_next` 原子领取的任务。
    pub async fn execute_claimed(
        &self,
        task: Download,
        cancellation: CancellationToken,
    ) -> Result<(), ServiceError> {
        match task.status {
            DownloadStatus::Resolving => self.execute_download(task, cancellation).await,
            DownloadStatus::Tagging => self.execute_tagging(task, cancellation).await,
            actual => Err(ServiceError::InvalidState {
                expected: DownloadStatus::Resolving,
                actual,
            }),
        }
    }

    async fn execute_download(
        &self,
        task: Download,
        cancellation: CancellationToken,
    ) -> Result<(), ServiceError> {
        let provider = match self.provider(&task.provider) {
            Ok(provider) => provider,
            Err(error) => {
                self.handle_download_failure(&task, DownloadStatus::Resolving, &error)
                    .await?;
                return Err(error);
            }
        };

        let resolved = tokio::select! {
            _ = cancellation.cancelled() => Err(ServiceError::Interrupted),
            result = provider.resolve(&task.track_id, &task.quality) => result.map_err(Into::into),
        };
        let resource = match resolved {
            Ok(resource) => resource,
            Err(error) => {
                self.handle_download_failure(&task, DownloadStatus::Resolving, &error)
                    .await?;
                if is_interrupted(&error) {
                    return Ok(());
                }
                return Err(error);
            }
        };

        let transitioned = self
            .repository
            .mark_downloading(task.id, task.version)
            .await?;
        if !transitioned {
            return Ok(());
        }

        let result = match self
            .download_resource(task.id, task.version, resource, cancellation)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                self.handle_download_failure(&task, DownloadStatus::Downloading, &error)
                    .await?;
                if is_interrupted(&error) {
                    return Ok(());
                }
                return Err(error);
            }
        };

        let size = i64::try_from(result.size).map_err(|_| ServiceError::SizeOverflow)?;
        let path = result
            .path
            .to_str()
            .map(str::to_owned)
            .ok_or(ServiceError::MissingDownloadPath)?;
        let transitioned = self
            .repository
            .mark_downloaded(task.id, task.version, &path, size)
            .await?;
        if !transitioned {
            tracing::warn!(task_id = task.id, path, "任务状态已变化，未提交下载结果");
        }
        Ok(())
    }

    async fn execute_tagging(
        &self,
        task: Download,
        cancellation: CancellationToken,
    ) -> Result<(), ServiceError> {
        let path = match task.path.as_deref() {
            Some(path) => path,
            None => {
                let error = ServiceError::MissingDownloadPath;
                self.handle_tagging_failure(&task, &error).await?;
                return Err(error);
            }
        };

        let provider = match self.provider(&task.provider) {
            Ok(provider) => provider,
            Err(error) => {
                self.handle_tagging_failure(&task, &error).await?;
                return Err(error);
            }
        };
        let lyrics = tokio::select! {
            _ = cancellation.cancelled() => Err(ServiceError::Interrupted),
            result = provider.lyrics(&task.track_id) => result.map_err(Into::into),
        };
        let lyrics = match lyrics {
            Ok(lyrics) => lyrics,
            Err(error) => {
                self.handle_tagging_failure(&task, &error).await?;
                if is_interrupted(&error) {
                    return Ok(());
                }
                return Err(error);
            }
        };

        if cancellation.is_cancelled() {
            let error = ServiceError::Interrupted;
            self.handle_tagging_failure(&task, &error).await?;
            return Ok(());
        }

        // Lofty 使用 spawn_blocking；阻塞任务启动后无法安全强制终止。
        // 从写标签开始进入不可中断区，最终由 status + version 决定是否提交结果。
        let result = async {
            self.embedder
                .embed_flac(path, task.metadata.clone(), lyrics)
                .await?;
            self.publish_and_complete(&task, path).await?;
            Ok::<(), ServiceError>(())
        }
        .await;
        if let Err(error) = result {
            self.handle_tagging_failure(&task, &error).await?;
            return Err(error);
        }

        Ok(())
    }

    async fn download_resource(
        &self,
        id: i64,
        version: i64,
        resource: ResolvedResource,
        cancellation: CancellationToken,
    ) -> Result<DownloadResult, ServiceError> {
        // 进度是快照而不是事件，只保留最新值，避免网络较快时消息无限积压。
        let (progress_tx, progress_rx) = watch::channel(None::<DownloadProgress>);

        let repository = self.repository.clone();
        let progress_task = tokio::spawn(persist_progress(repository, id, version, progress_rx));
        let result = self
            .downloader
            .download(
                resource,
                DownloadOptions {
                    task_id: id,
                    cancellation,
                    progress: Some(move |progress| {
                        progress_tx.send_replace(Some(progress));
                    }),
                },
            )
            .await;

        // 不提前返回下载错误，先等待最后一批进度写入数据库。
        progress_task.await??;
        Ok(result?)
    }

    async fn handle_download_failure(
        &self,
        task: &Download,
        stage: DownloadStatus,
        error: &ServiceError,
    ) -> Result<(), ServiceError> {
        let message = error.to_string();

        if is_interrupted(error) {
            self.repository
                .requeue_interrupted_download(task.id, task.version, &message)
                .await?;
            return Ok(());
        }

        if task.retry_count >= MAX_RETRIES || !is_retryable(error) {
            self.repository
                .mark_failed(task.id, task.version, &message)
                .await?;
            return Ok(());
        }

        let next_retry_count = task.retry_count + 1;
        let delay = retry_delay(next_retry_count);
        let next_retry_at = next_retry_at(delay);
        self.repository
            .requeue_download(task.id, task.version, next_retry_at, &message)
            .await?;

        tracing::warn!(
            task_id = task.id,
            version = task.version,
            retry_count = next_retry_count,
            delay_seconds = delay.as_secs(),
            ?stage,
            error = %error,
            "下载任务等待重试"
        );
        Ok(())
    }

    async fn handle_tagging_failure(
        &self,
        task: &Download,
        error: &ServiceError,
    ) -> Result<(), ServiceError> {
        let message = error.to_string();

        if is_interrupted(error) {
            self.repository
                .requeue_interrupted_tagging(task.id, task.version, &message)
                .await?;
            return Ok(());
        }

        if task.retry_count >= MAX_RETRIES || !is_retryable(error) {
            self.repository
                .mark_failed(task.id, task.version, &message)
                .await?;
            return Ok(());
        }

        let next_retry_count = task.retry_count + 1;
        let delay = retry_delay(next_retry_count);
        let next_retry_at = next_retry_at(delay);
        self.repository
            .requeue_tagging(task.id, task.version, next_retry_at, &message)
            .await?;

        tracing::warn!(
            task_id = task.id,
            version = task.version,
            retry_count = next_retry_count,
            delay_seconds = delay.as_secs(),
            error = %error,
            "标签任务等待重试"
        );
        Ok(())
    }
}

async fn persist_progress(
    repository: Repository,
    id: i64,
    version: i64,
    mut receiver: watch::Receiver<Option<DownloadProgress>>,
) -> Result<(), ServiceError> {
    let mut ticker = time::interval(Duration::from_secs(1));
    let mut latest = None;
    let mut dirty = false;

    loop {
        tokio::select! {
            changed = receiver.changed() => {
                match changed {
                    Ok(()) => {
                        latest = *receiver.borrow_and_update();
                        dirty = latest.is_some();
                    }
                    Err(_) => {
                        if dirty && let Some(progress) = latest.take() {
                            write_progress(&repository, id, version, progress).await?;
                        }
                        return Ok(());
                    }
                }
            }
            _ = ticker.tick() => {
                if dirty && let Some(progress) = latest.take() {
                    write_progress(&repository, id, version, progress).await?;
                    dirty = false;
                }
            }
        }
    }
}

async fn write_progress(
    repository: &Repository,
    id: i64,
    version: i64,
    progress: DownloadProgress,
) -> Result<(), ServiceError> {
    let downloaded = i64::try_from(progress.downloaded).map_err(|_| ServiceError::SizeOverflow)?;
    let total = progress
        .total
        .map(i64::try_from)
        .transpose()
        .map_err(|_| ServiceError::SizeOverflow)?;
    repository
        .update_progress(id, version, downloaded, total)
        .await?;
    Ok(())
}

fn is_retryable(error: &ServiceError) -> bool {
    !matches!(
        error,
        ServiceError::InvalidRequest(_)
            | ServiceError::InvalidProvider
            | ServiceError::MissingDownloadPath
            | ServiceError::Provider(
                ProviderError::UnsupportedQuality(_) | ProviderError::InvalidArgument(_),
            )
            | ServiceError::Download(
                DownloadError::InvalidUrl
                    | DownloadError::InvalidFileName
                    | DownloadError::TooLarge,
            )
            | ServiceError::Tag(TagError::InvalidCoverUrl | TagError::UnsupportedCoverFormat)
    )
}

fn is_interrupted(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::Interrupted | ServiceError::Download(DownloadError::Cancelled)
    )
}

fn retry_delay(retry_count: i64) -> Duration {
    let exponent = retry_count.saturating_sub(1).clamp(0, 8) as u32;
    let seconds = BASE_RETRY_DELAY_SECS
        .saturating_mul(2_u64.saturating_pow(exponent))
        .min(MAX_RETRY_DELAY_SECS);

    Duration::from_secs(seconds)
}

fn next_retry_at(delay: Duration) -> OffsetDateTime {
    OffsetDateTime::now_utc() + TimeDuration::seconds(delay.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::retry_delay;
    use crate::{
        error::ServiceError,
        server::{db, download::Downloader, service::Service, tag::Embedder},
        shared::request::CreateDownloadRequest,
    };

    #[test]
    fn retry_delay_uses_exponential_backoff_with_cap() {
        assert_eq!(retry_delay(1).as_secs(), 2);
        assert_eq!(retry_delay(2).as_secs(), 4);
        assert_eq!(retry_delay(3).as_secs(), 8);
        assert_eq!(retry_delay(10).as_secs(), 300);
    }

    #[tokio::test]
    async fn rejects_non_flac_downloads_before_inserting_task() {
        let pool = db::connect_memory().await.unwrap();
        let repository = db::Repository::new(pool);
        let client = reqwest::Client::new();
        let service = Service::new(
            repository,
            HashMap::new(),
            Downloader::new(client.clone(), std::env::temp_dir(), 1024),
            client.clone(),
            Embedder::new(std::time::Duration::from_secs(30)).unwrap(),
        );

        let result = service
            .create_download(CreateDownloadRequest {
                provider: "kuwo".to_owned(),
                track_id: "1".to_owned(),
                quality: "1000kape".to_owned(),
                name: "song".to_owned(),
                artists: vec!["artist".to_owned()],
                album: String::new(),
                album_artist: String::new(),
                date: String::new(),
                cover_url: String::new(),
            })
            .await;

        assert!(matches!(
            result,
            Err(ServiceError::UnsupportedDownloadQuality(quality)) if quality == "1000kape"
        ));
        assert!(
            service
                .list_downloads(None, 20)
                .await
                .unwrap()
                .downloads
                .is_empty()
        );
    }

    #[tokio::test]
    async fn rejects_unknown_provider_before_inserting_task() {
        let pool = db::connect_memory().await.unwrap();
        let repository = db::Repository::new(pool);
        let client = reqwest::Client::new();
        let service = Service::new(
            repository,
            HashMap::new(),
            Downloader::new(client.clone(), std::env::temp_dir(), 1024),
            client,
            Embedder::new(std::time::Duration::from_secs(30)).unwrap(),
        );

        let result = service
            .create_download(CreateDownloadRequest {
                provider: "unknown".to_owned(),
                track_id: "1".to_owned(),
                quality: crate::server::model::DOWNLOAD_QUALITY_ID.to_owned(),
                name: "song".to_owned(),
                artists: vec!["artist".to_owned()],
                album: String::new(),
                album_artist: String::new(),
                date: String::new(),
                cover_url: String::new(),
            })
            .await;

        assert!(matches!(result, Err(ServiceError::InvalidProvider)));
        assert!(
            service
                .list_downloads(None, 20)
                .await
                .unwrap()
                .downloads
                .is_empty()
        );
    }
}
