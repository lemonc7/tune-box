use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use ::time::{Duration as TimeDuration, OffsetDateTime};
use tokio::{fs, sync::mpsc, time};
use tokio_util::sync::CancellationToken;

use crate::{
    error::{DownloadError, ProviderError, ServiceError, TagError},
    model::Resource,
    server::{
        db::{
            model::{Download, Status},
            repository::Repository,
        },
        download::{DownloadOptions, DownloadProgress, DownloadResult, Downloader},
        provider::Provider,
        tag::Embedder,
    },
};

const MAX_RETRIES: i64 = 5;
const BASE_RETRY_DELAY_SECS: u64 = 2;
const MAX_RETRY_DELAY_SECS: u64 = 5 * 60;

pub struct Service {
    repository: Repository,
    providers: HashMap<String, Box<dyn Provider>>,
    downloader: Downloader,
    embedder: Embedder,
}

impl Service {
    pub fn new(
        repository: Repository,
        providers: HashMap<String, Box<dyn Provider>>,
        downloader: Downloader,
        embedder: Embedder,
    ) -> Self {
        Self {
            repository,
            providers,
            downloader,
            embedder,
        }
    }

    pub fn provider(&self, name: &str) -> Result<&dyn Provider, ServiceError> {
        self.providers
            .get(name)
            .map(|p| p.as_ref())
            .ok_or(ServiceError::InvalidProvider)
    }

    // task必须由claim_next原子领取
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

        // 解析资源
        let resolved = tokio::select! {
            _ = cancellation.cancelled() => Err(ServiceError::Interrupted),
            result = provider.resolve(&task.track_id, &task.quality) => {
                result.map_err(ServiceError::from)
            }
        };
        let resource = match resolved {
            Ok(r) => r,
            Err(err) => {
                self.handle_download_failure(&task, Status::Resolving, &err)
                    .await?;
                if is_interrupted(&err) {
                    return Ok(());
                }
                return Err(err);
            }
        };

        // resolving -> downloading
        let transitioned = self
            .repository
            .mark_downloading(task.id, task.version)
            .await?;
        if !transitioned {
            // 用户可能已经暂停或取消任务
            return Ok(());
        }

        // 下载文件
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

        // downloading -> downloaded
        let size = i64::try_from(result.size).map_err(|_| ServiceError::SizeOverflow)?;
        let path = result
            .path
            .to_str()
            .map(|s| s.to_owned())
            .ok_or(ServiceError::MissingDownloadPath)?;
        let transitioned = self
            .repository
            .mark_downloaded(task.id, task.version, &path, size)
            .await?;
        if !transitioned {
            // 文件已经下载完成，但任务被取消
            tracing::warn!(task_id = task.id, path, "任务状态已变化，未提交下载结果")
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
        let result = tokio::select! {
            _ = cancellation.cancelled() => Err(ServiceError::Interrupted),
            result = async {
                let lyrics = provider.lyrics(&task.track_id).await?;
                self.embedder
                    .embed_flac(path, task.metadata.0.clone(), lyrics)
                    .await?;
                self.publish_and_complete(&task, path).await?;
                Ok::<(), ServiceError>(())
            } => result,
        };

        if let Err(error) = result {
            self.handle_tagging_failure(&task, &error).await?;
            if is_interrupted(&error) {
                return Ok(());
            }
            return Err(error);
        }

        Ok(())
    }

    async fn publish_and_complete(
        &self,
        task: &Download,
        ready_path: &str,
    ) -> Result<(), ServiceError> {
        let ready_path = Path::new(ready_path);
        let published = publish_ready_file(task, ready_path).await?;
        let file_metadata = fs::metadata(&published.path)
            .await
            .map_err(DownloadError::Io)?;
        let size = i64::try_from(file_metadata.len()).map_err(|_| ServiceError::SizeOverflow)?;
        let extension = published
            .path
            .extension()
            .and_then(|value| value.to_str())
            .ok_or(ServiceError::Download(DownloadError::InvalidFileName))?;
        let final_path = published
            .path
            .to_str()
            .ok_or(ServiceError::Download(DownloadError::InvalidFileName))?;

        let completed = self
            .repository
            .complete_with_song(
                task.id,
                task.version,
                final_path,
                extension,
                size,
                &task.metadata.0,
            )
            .await?;

        if !completed {
            if published.created
                && let Err(error) = fs::remove_file(&published.path).await
            {
                tracing::warn!(path = ?published.path, %error, "清理未提交的最终文件失败");
            }
            return Ok(());
        }

        if let Err(error) = fs::remove_file(ready_path).await {
            tracing::warn!(path = ?ready_path, %error, "清理 ready 文件失败");
        }

        Ok(())
    }

    async fn download_resource(
        &self,
        id: i64,
        version: i64,
        resource: Resource,
        cancellation: CancellationToken,
    ) -> Result<DownloadResult, ServiceError> {
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();

        let repo = self.repository.clone();
        let progress_task = tokio::spawn(persist_progress(repo, id, version, progress_rx));
        let result = self
            .downloader
            .download(
                resource,
                DownloadOptions {
                    task_id: id,
                    cancellation,
                    progress: Some(move |progress| {
                        let _ = progress_tx.send(progress);
                    }),
                },
            )
            .await;

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

        // 已经执行过 MAX_RETRIES 次重试，或者错误不可重试，标记失败。
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

// 每500ms合并写一次当前进度
async fn persist_progress(
    repository: Repository,
    id: i64,
    version: i64,
    mut receiver: mpsc::UnboundedReceiver<DownloadProgress>,
) -> Result<(), ServiceError> {
    let mut ticker = time::interval(Duration::from_millis(500));
    let mut latest = None;
    loop {
        tokio::select! {
          progress = receiver.recv() => {
            match progress {
                Some(progress) => {
                  latest = Some(progress)
                }
                None => {
                  if let Some(progress) = latest.take() {
                    write_progress(&repository,id,version,progress).await?;
                  }
                  return Ok(());
                }
            }
          }
          _ = ticker.tick() => {
            if let Some(progress) = latest.take() {
              write_progress(&repository, id, version, progress).await?;
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

struct PublishedFile {
    path: PathBuf,
    created: bool,
}

async fn publish_ready_file(
    task: &Download,
    ready_path: &Path,
) -> Result<PublishedFile, ServiceError> {
    let parent = ready_path
        .parent()
        .ok_or(ServiceError::Download(DownloadError::InvalidFileName))?;
    let extension = ready_path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or(ServiceError::Download(DownloadError::InvalidFileName))?;
    let title = sanitize_filename_component(&task.metadata.name, "未知歌曲");
    let artists = sanitize_filename_component(&task.metadata.artists.join(", "), "未知歌手");
    let stem = format!("{title} - {artists}");

    let candidates = [
        parent.join(format!("{stem}.{extension}")),
        parent.join(format!("{stem} ({}).{extension}", task.id)),
    ];

    for target in candidates {
        match fs::hard_link(ready_path, &target).await {
            Ok(()) => {
                return Ok(PublishedFile {
                    path: target,
                    created: true,
                });
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                if files_are_same(ready_path, &target).await? {
                    return Ok(PublishedFile {
                        path: target,
                        created: false,
                    });
                }
            }
            Err(error) => return Err(ServiceError::Download(DownloadError::Io(error))),
        }
    }

    Err(ServiceError::Download(DownloadError::TargetExists))
}

fn sanitize_filename_component(value: &str, fallback: &str) -> String {
    let value: String = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .take(100)
        .collect();
    let value = value.trim().trim_matches('.');

    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

#[cfg(unix)]
async fn files_are_same(left: &Path, right: &Path) -> Result<bool, ServiceError> {
    use std::os::unix::fs::MetadataExt;

    let left = fs::metadata(left).await.map_err(DownloadError::Io)?;
    let right = fs::metadata(right).await.map_err(DownloadError::Io)?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(not(unix))]
async fn files_are_same(_left: &Path, _right: &Path) -> Result<bool, ServiceError> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::{retry_delay, sanitize_filename_component};

    #[test]
    fn sanitizes_file_name_components() {
        assert_eq!(
            sanitize_filename_component("  song/name:*?  ", "fallback"),
            "song_name___"
        );
        assert_eq!(sanitize_filename_component("...", "fallback"), "fallback");
    }

    #[test]
    fn retry_delay_uses_exponential_backoff_with_cap() {
        assert_eq!(retry_delay(1).as_secs(), 2);
        assert_eq!(retry_delay(2).as_secs(), 4);
        assert_eq!(retry_delay(3).as_secs(), 8);
        assert_eq!(retry_delay(10).as_secs(), 300);
    }
}
