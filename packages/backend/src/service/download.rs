use std::time::Duration;

use ::time::{Duration as TimeDuration, OffsetDateTime};
use contracts::{download::DownloadPage, music::Metadata, request::CreateDownloadRequest};
use garde::Validate;
use tokio::{sync::watch, time};
use tokio_util::sync::CancellationToken;

use crate::{
    db::{Download, Repository, Status},
    download::{DownloadOptions, DownloadProgress, DownloadResult},
    error::{DownloadError, ProviderError, ServiceError, TagError},
    provider::{DOWNLOAD_QUALITY_ID, ResolvedResource},
    service::Service,
};

const MAX_RETRIES: i64 = 5;
const BASE_RETRY_DELAY_SECS: u64 = 2;
const MAX_RETRY_DELAY_SECS: u64 = 5 * 60;

impl Service {
    pub async fn get_download(
        &self,
        id: i64,
    ) -> Result<Option<contracts::download::Download>, ServiceError> {
        Ok(self.repository.get_download(id).await?.map(Into::into))
    }

    pub async fn create_download(
        &self,
        request: CreateDownloadRequest,
    ) -> Result<Option<i64>, ServiceError> {
        request
            .validate()
            .map_err(|error| ServiceError::InvalidRequest(error.to_string()))?;
        self.provider(&request.provider)?;
        let id = self
            .repository
            .add_download(
                &request.provider,
                &request.track_id,
                &Metadata {
                    name: request.name,
                    artists: request.artists,
                    album: request.album,
                    album_artist: request.album_artist,
                    date: request.date,
                    cover_url: request.cover_url,
                    track_number: request.track_number,
                    disc_number: request.disc_number,
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
            downloads: rows.into_iter().map(Into::into).collect(),
            next_before_id,
        })
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

    // 执行原子领取的任务
    pub async fn execute_claimed(
        &self,
        task: Download,
        cancellation: CancellationToken,
    ) -> Result<(), ServiceError> {
        match task.status {
            Status::Resolving => self.execute_download(task, cancellation).await,
            Status::Tagging => self.execute_tagging(task, cancellation).await,
            actual => Err(ServiceError::InvalidState {
                expected: Status::Resolving,
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
                self.handle_download_failure(&task, Status::Resolving, &error)
                    .await?;
                return Err(error);
            }
        };

        let resolved = tokio::select! {
            _ = cancellation.cancelled() => Err(ServiceError::Interrupted),
            result = provider.resolve(&task.track_id, DOWNLOAD_QUALITY_ID) => result.map_err(Into::into),
        };
        let resource = match resolved {
            Ok(resource) => resource,
            Err(error) => {
                self.handle_download_failure(&task, Status::Resolving, &error)
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
                self.handle_download_failure(&task, Status::Downloading, &error)
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

        // FLAC 流式重写使用 spawn_blocking；阻塞任务启动后无法安全强制终止。
        // 从写标签开始进入不可中断区，最终由 status + version 决定是否提交结果。
        let result = async {
            self.embedder
                .embed_flac(path, task.metadata.0.clone(), lyrics)
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
        stage: Status,
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
        ServiceError::InvalidProvider
            | ServiceError::MissingDownloadPath
            | ServiceError::Provider(ProviderError::InvalidArgument(_),)
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
