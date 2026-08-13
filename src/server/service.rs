use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use ::time::Duration as TimeDuration;
use sqlx::types::time::OffsetDateTime;
use tokio::{
    fs,
    sync::{Mutex, watch},
    time,
};
use tokio_util::sync::CancellationToken;

use crate::{
    error::{DownloadError, ProviderError, ServiceError, TagError},
    server::{
        db::Repository,
        download::{DownloadOptions, DownloadProgress, DownloadResult, Downloader},
        model::{DOWNLOAD_QUALITY_ID, PREVIEW_QUALITY_ID, ResolvedResource},
        provider::Provider,
        tag::Embedder,
    },
    shared::{
        download::{Download, DownloadPage, DownloadStatus},
        library::SongPage,
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
    preview_client: reqwest::Client,
    embedder: Embedder,
    /// 串行化曲库扫描、歌曲发布和歌曲删除，保证文件系统与数据库视图一致。
    library_lock: Mutex<()>,
}

impl Service {
    pub fn new(
        repository: Repository,
        providers: HashMap<String, Box<dyn Provider>>,
        downloader: Downloader,
        preview_client: reqwest::Client,
        embedder: Embedder,
    ) -> Self {
        Self {
            repository,
            providers,
            downloader,
            preview_client,
            embedder,
            library_lock: Mutex::new(()),
        }
    }
    pub async fn search(&self, req: SearchRequest) -> Result<SearchResult, ServiceError> {
        let provider = self.provider(&req.provider)?;
        provider
            .search(&req.query, req.page, req.limit)
            .await
            .map_err(Into::into)
    }

    pub async fn preview(
        &self,
        provider: &str,
        track_id: &str,
        range: Option<&str>,
    ) -> Result<reqwest::Response, ServiceError> {
        let resource = self
            .provider(provider)?
            .resolve(track_id, PREVIEW_QUALITY_ID)
            .await?;
        let mut request = self.preview_client.get(resource.url);
        for (name, value) in resource.headers {
            request = request.header(name, value);
        }
        if let Some(range) = range {
            request = request.header(reqwest::header::RANGE, range);
        }
        Ok(request.send().await.map_err(DownloadError::from)?)
    }

    pub async fn create_download(
        &self,
        req: CreateDownloadRequest,
    ) -> Result<Option<i64>, ServiceError> {
        if req.quality != DOWNLOAD_QUALITY_ID {
            return Err(ServiceError::UnsupportedDownloadQuality(req.quality));
        }
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

    fn provider(&self, name: &str) -> Result<&dyn Provider, ServiceError> {
        self.providers
            .get(name)
            .map(|p| p.as_ref())
            .ok_or(ServiceError::InvalidProvider)
    }

    pub fn providers(&self) -> Vec<String> {
        let mut providers = self
            .providers
            .values()
            .map(|provider| provider.id().to_owned())
            .collect::<Vec<_>>();
        providers.sort_unstable();
        providers
    }

    // task必须由claim_next原子领取
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

    pub async fn list_songs(
        &self,
        keyword: &str,
        before_id: Option<i64>,
        limit: i64,
    ) -> Result<SongPage, ServiceError> {
        let limit = limit.clamp(1, 100);
        let fetch_limit = limit + 1;
        let before_id = before_id.unwrap_or(i64::MAX);
        let keywords: Vec<&str> = keyword.split_whitespace().collect();
        let mut rows = self
            .repository
            .list_songs(&keywords, before_id, fetch_limit)
            .await?;

        let has_more = rows.len() > limit as usize;
        if has_more {
            rows.truncate(limit as usize);
        }

        let next_before_id = if has_more {
            rows.last().map(|row| row.id)
        } else {
            None
        };

        Ok(SongPage {
            songs: rows,
            next_before_id,
        })
    }

    pub async fn count_songs(&self) -> Result<i64, ServiceError> {
        Ok(self.repository.count_songs().await?)
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

    pub async fn sync_library(&self) -> Result<i64, ServiceError> {
        use crate::server::db::library::ScannedSong;

        let _library_guard = self.library_lock.lock().await;
        let directory = self.downloader.directory();
        fs::create_dir_all(directory)
            .await
            .map_err(ServiceError::LibraryIo)?;
        let mut entries = fs::read_dir(directory)
            .await
            .map_err(ServiceError::LibraryIo)?;
        let mut songs = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(ServiceError::LibraryIo)?
        {
            if !entry
                .file_type()
                .await
                .map_err(ServiceError::LibraryIo)?
                .is_file()
            {
                continue;
            }
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            if file_name.starts_with('.') {
                continue;
            }

            let path = entry.path();
            let Some(extension) = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
            else {
                continue;
            };
            if !matches!(
                extension.as_str(),
                "flac" | "mp3" | "wav" | "m4a" | "aac" | "ape" | "ogg" | "opus"
            ) {
                continue;
            }

            let mut name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("未知歌曲")
                .to_owned();
            let mut artists = Vec::new();
            if extension == "flac" {
                match Embedder::read_flac_metadata(&path).await {
                    Ok(info) => {
                        if !info.title.trim().is_empty() {
                            name = info.title;
                        }
                        artists = info.artists;
                    }
                    Err(error) => {
                        tracing::warn!(path = ?path, %error, "读取 FLAC 元数据失败，使用文件名");
                    }
                }
            }

            let size = i64::try_from(
                entry
                    .metadata()
                    .await
                    .map_err(ServiceError::LibraryIo)?
                    .len(),
            )
            .map_err(|_| ServiceError::SizeOverflow)?;
            let path = path
                .to_str()
                .ok_or(ServiceError::UnsafeLibraryPath)?
                .to_owned();
            songs.push(ScannedSong {
                path,
                name,
                extension,
                size,
                artists,
            });
        }

        let scan_token = OffsetDateTime::now_utc().unix_timestamp_nanos().to_string();
        self.repository.sync_library(&songs, &scan_token).await?;
        self.count_songs().await
    }

    pub async fn delete_song(&self, id: i64) -> Result<bool, ServiceError> {
        let _library_guard = self.library_lock.lock().await;
        let Some(song) = self.repository.get_song_file(id).await? else {
            return Ok(false);
        };

        fs::create_dir_all(self.downloader.directory())
            .await
            .map_err(ServiceError::LibraryIo)?;
        let root = fs::canonicalize(self.downloader.directory())
            .await
            .map_err(ServiceError::LibraryIo)?;
        let path = PathBuf::from(&song.path);

        let staged = match fs::canonicalize(&path).await {
            Ok(canonical) => {
                if !canonical.starts_with(&root) {
                    return Err(ServiceError::UnsafeLibraryPath);
                }
                let staged = root.join(format!(".delete-song-{id}.tmp"));
                fs::rename(&canonical, &staged)
                    .await
                    .map_err(ServiceError::LibraryIo)?;
                Some((canonical, staged))
            }
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(ServiceError::LibraryIo(error)),
        };

        let deleted = match self.repository.delete_song_with_download(id).await {
            Ok(deleted) => deleted,
            Err(error) => {
                if let Some((original, staged)) = &staged
                    && let Err(restore_error) = fs::rename(staged, original).await
                {
                    tracing::error!(?original, ?staged, %restore_error, "恢复待删除音乐文件失败");
                }
                return Err(error.into());
            }
        };

        if !deleted {
            if let Some((original, staged)) = &staged {
                fs::rename(staged, original)
                    .await
                    .map_err(ServiceError::LibraryIo)?;
            }
            return Ok(false);
        }

        if let Some((_, staged)) = staged
            && let Err(error) = fs::remove_file(&staged).await
        {
            tracing::error!(path = ?staged, %error, "清理已从曲库删除的音乐文件失败");
        }
        Ok(true)
    }

    // 下载阶段
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
        task: Download,
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

    // 发布文件并写入曲库
    async fn publish_and_complete(
        &self,
        task: &Download,
        ready_path: &str,
    ) -> Result<(), ServiceError> {
        let _library_guard = self.library_lock.lock().await;
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
                &task.metadata,
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
        task: &Download,
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
    use std::{collections::HashMap, sync::Arc, time::SystemTime};

    use super::{Service, retry_delay, sanitize_filename_component};
    use crate::{
        error::ServiceError,
        server::{db, download::Downloader, tag::Embedder},
        shared::request::CreateDownloadRequest,
    };

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

    #[tokio::test]
    async fn library_sync_waits_for_library_mutation_lock() {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tune-box-library-lock-{}-{nonce}",
            std::process::id()
        ));
        tokio::fs::create_dir_all(&directory).await.unwrap();
        tokio::fs::write(directory.join("song.mp3"), b"audio")
            .await
            .unwrap();

        let pool = db::connect_memory().await.unwrap();
        let repository = db::Repository::new(pool);
        let client = reqwest::Client::new();
        let service = Arc::new(Service::new(
            repository,
            HashMap::new(),
            Downloader::new(client.clone(), &directory, 1024),
            client.clone(),
            Embedder::new(client),
        ));

        let guard = service.library_lock.lock().await;
        let syncing_service = Arc::clone(&service);
        let syncing = tokio::spawn(async move { syncing_service.sync_library().await });
        tokio::task::yield_now().await;
        assert!(!syncing.is_finished());

        drop(guard);
        let count = tokio::time::timeout(std::time::Duration::from_secs(1), syncing)
            .await
            .expect("曲库锁释放后同步应当完成")
            .unwrap()
            .unwrap();
        assert_eq!(count, 1);

        tokio::fs::remove_dir_all(directory).await.unwrap();
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
            Embedder::new(client),
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
}
