use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use ::time::{Duration as TimeDuration, OffsetDateTime};
use tokio::{fs, sync::watch, time};
use tokio_util::sync::CancellationToken;

use crate::{
    error::{DownloadError, ProviderError, ServiceError, TagError},
    server::{
        db::{model::DownloadRow, repository::Repository},
        download::{DownloadOptions, DownloadProgress, DownloadResult, Downloader},
        model::ResolvedResource,
        provider::Provider,
        tag::Embedder,
    },
    shared::{
        download::DownloadStatus,
        music::{Metadata, SearchResult},
        request::{CreateDownloadRequest, SearchRequest},
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
    pub async fn search(&self, req: SearchRequest) -> Result<SearchResult, ServiceError> {
        let provider = self.provider(&req.provider)?;
        let mut result = provider.search(&req.query, req.page, req.limit).await?;

        if req.lossless {
            result.tracks.iter_mut().for_each(|t| {
                t.qualities.retain(|q| q.lossless);
            });
        }
        Ok(result)
    }

    pub async fn add_download(
        &self,
        req: CreateDownloadRequest,
    ) -> Result<Option<i64>, ServiceError> {
        let id = self
            .repository
            .add_download(
                &req.provider,
                &req.track_id,
                &req.quality,
                &Metadata {
                    name: req.name,
                    artists: req.artists,
                    album: req.album,
                    album_artist: req.album_artist,
                    date: req.date,
                    cover_url: req.cover_url,
                },
            )
            .await?;
        Ok(id)
    }

    fn provider(&self, name: &str) -> Result<&dyn Provider, ServiceError> {
        self.providers
            .get(name)
            .map(|p| p.as_ref())
            .ok_or(ServiceError::InvalidProvider)
    }

    // task必须由claim_next原子领取
    pub async fn execute_claimed(
        &self,
        task: DownloadRow,
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

    // 下载阶段
    async fn execute_download(
        &self,
        task: DownloadRow,
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

        // 解析资源，支持取消
        let resolved = tokio::select! {
            _ = cancellation.cancelled() => Err(ServiceError::Interrupted),
            result = provider.resolve(&task.track_id, &task.quality) => result.map_err(Into::into),
        };
        let resource = match resolved {
            Ok(r) => r,
            Err(err) => {
                // 调用失败处理
                self.handle_download_failure(&task, DownloadStatus::Resolving, &err)
                    .await?;
                // 正常中断
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
        // 用户可能已经暂停或取消任务
        if !transitioned {
            return Ok(());
        }

        // 下载文件
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

    // 打标签阶段
    async fn execute_tagging(
        &self,
        task: DownloadRow,
        cancellation: CancellationToken,
    ) -> Result<(), ServiceError> {
        // 获取ready路径
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
        // 获取歌词，支持取消
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

        // 写标签之前再次检查是否已取消
        if cancellation.is_cancelled() {
            let error = ServiceError::Interrupted;
            self.handle_tagging_failure(&task, &error).await?;
            return Ok(());
        }

        // Lofty使用spawn_blocking；阻塞任务启动后无法安全强制终止。
        // 因此从写标签开始进入不可中断区，最终由 status + version 决定是否提交结果。
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

    // 发布文件并写入曲库
    async fn publish_and_complete(
        &self,
        task: &DownloadRow,
        ready_path: &str,
    ) -> Result<(), ServiceError> {
        let ready_path = Path::new(ready_path);
        // 发布文件，使用硬链接
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

        // 完成任务，并写入歌曲
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

        // 数据库提交失败
        if !completed {
            // 最终文件是新创建的，就删除这个硬链接
            if published.created
                && let Err(error) = fs::remove_file(&published.path).await
            {
                tracing::warn!(path = ?published.path, %error, "清理未提交的最终文件失败");
            }
            return Ok(());
        }

        // 完成后删除临时ready路径
        if let Err(error) = fs::remove_file(ready_path).await {
            tracing::warn!(path = ?ready_path, %error, "清理 ready 文件失败");
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
                        // 将watch中的旧进度替换为新进度
                        progress_tx.send_replace(Some(progress));
                    }),
                },
            )
            .await; // 不立即返回错误，先保存进度

        // 等待进度任务结束：Result<Result<(), ServiceError>, JoinError>
        progress_task.await??;
        Ok(result?)
    }

    // 下载失败处理
    async fn handle_download_failure(
        &self,
        task: &DownloadRow,
        stage: DownloadStatus,
        error: &ServiceError,
    ) -> Result<(), ServiceError> {
        let message = error.to_string();

        // 主动中断，将任务重新调整成"pending"状态，等待重试
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

        // 可重试错误
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
        task: &DownloadRow,
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
    // 内存中的最新进度
    let mut latest = None;
    // 当前进度有没有尚未写入数据库
    let mut dirty = false;
    loop {
        tokio::select! {
          // watch中出现新进度
          changed = receiver.changed() => {
            match changed {
                Ok(()) => {
                    // 只更新内存，暂时不写入数据库
                    latest = *receiver.borrow_and_update();
                    dirty = latest.is_some();
                }
                Err(_) => {
                    // downloader结束时发送端销毁，这里最后刷新一次进度
                    if dirty && let Some(progress) = latest.take() {
                        write_progress(&repository, id, version, progress).await?;
                    }
                    return Ok(());
                }
            }
          }
          // 定时器
          _ = ticker.tick() => {
            // 将数据写入到数据库
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

// 指数退避
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
    task: &DownloadRow,
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
    let stem = format!("{artists} - {title}");

    let target = parent.join(format!("{stem}.{extension}"));

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
