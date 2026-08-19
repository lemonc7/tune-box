use std::{
    io::ErrorKind,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use shared::library::SongPage;
use time::OffsetDateTime;
use tokio::fs;

use crate::{
    db::{Download, ScannedSong},
    error::{DownloadError, ServiceError},
    service::Service,
    tag::embed::Embedder,
};

impl Service {
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

    // 手动扫描曲库；请求会等待扫描完成后返回结果。
    pub async fn sync_library(&self) -> Result<i64, ServiceError> {
        // 获取锁，防止 扫描曲库/发布刚下载完成的歌曲/删除曲库歌 同时进行
        // Mutex实现了Drop trait，离开作用域自动释放
        let _library_guard = self.library_lock.lock().await;
        let directory = self.downloader.directory();
        // 确保下载目录存在
        fs::create_dir_all(directory)
            .await
            .map_err(ServiceError::LibraryIo)?;
        // 异步打开目录
        let mut entries = fs::read_dir(directory)
            .await
            .map_err(ServiceError::LibraryIo)?;
        let mut songs = Vec::new();

        // 逐个读取目录文件
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(ServiceError::LibraryIo)?
        {
            // 忽略目录
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
            // 跳过隐藏文件
            if file_name.starts_with('.') {
                continue;
            }

            // 过滤支持的音频格式
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

            // 默认使用文件名作为歌名
            let mut name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("未知歌曲")
                .to_owned();
            let mut artists = Vec::new();
            // 读取flac标签
            if extension == "flac" {
                match Embedder::read_flac_metadata(&path).await {
                    Ok(info) => {
                        if !info.title.trim().is_empty() {
                            // 使用flac中的歌曲名
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
                size,
                artists,
            });
        }

        // 生成本轮扫描标记
        let scan_token = OffsetDateTime::now_utc().unix_timestamp_nanos().to_string();
        self.repository.sync_library(&songs, &scan_token).await?;
        self.count_songs().await
    }

    // 删除歌曲和下载记录
    pub async fn delete_song(&self, id: i64) -> Result<bool, ServiceError> {
        // 获取锁
        let _library_guard = self.library_lock.lock().await;
        let Some(song_path) = self.repository.get_song_path(id).await? else {
            return Ok(false);
        };

        // 确保目录存在
        fs::create_dir_all(self.downloader.directory())
            .await
            .map_err(ServiceError::LibraryIo)?;
        // 获取规范路径
        let root = fs::canonicalize(self.downloader.directory())
            .await
            .map_err(ServiceError::LibraryIo)?;
        let path = PathBuf::from(&song_path);

        // 检查文件是否存在，并暂存文件
        let staged = match fs::canonicalize(&path).await {
            Ok(canonical) => {
                // 安全检查，确保文件在曲库目录，防止操作其他文件
                if !canonical.starts_with(&root) {
                    return Err(ServiceError::UnsafeLibraryPath);
                }
                // 创建暂存路径
                let staged = root.join(format!(".delete-song-{id}.tmp"));
                // 使用重命名暂替删除
                fs::rename(&canonical, &staged)
                    .await
                    .map_err(ServiceError::LibraryIo)?;
                // 返回原始路径和临时路径
                Some((canonical, staged))
            }
            // 本地文件已经不存在，进行后续清理动作
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            // 操作错误，直接返回
            Err(error) => return Err(ServiceError::LibraryIo(error)),
        };

        // 删除数据库记录
        let deleted = match self.repository.delete_song_with_download(id).await {
            Ok(deleted) => deleted,
            Err(error) => {
                // 删除失败时，恢复文件
                if let Some((original, staged)) = &staged
                    && let Err(restore_error) = fs::rename(staged, original).await
                {
                    tracing::error!(?original, ?staged, %restore_error, "恢复待删除音乐文件失败");
                }
                // 返回数据库错误
                return Err(error.into());
            }
        };

        // 数据库删除歌曲时，歌曲不存在
        if !deleted {
            // 恢复文件
            if let Some((original, staged)) = &staged {
                fs::rename(staged, original)
                    .await
                    .map_err(ServiceError::LibraryIo)?;
            }
            // 返回false，告诉调用方删除没成功
            return Ok(false);
        }

        // 数据库删除成功，真正删除文件
        if let Some((_, staged)) = staged
            && let Err(error) = fs::remove_file(&staged).await
        {
            tracing::error!(path = ?staged, %error, "清理已从曲库删除的音乐文件失败");
        }
        Ok(true)
    }

    pub(super) async fn publish_and_complete(
        &self,
        task: &Download,
        ready_path: &str,
    ) -> Result<(), ServiceError> {
        // 获取锁
        let _library_guard = self.library_lock.lock().await;
        // 创建发布正式文件
        let ready_path = Path::new(ready_path);
        let published = publish_ready_file(task, ready_path).await?;

        // 获取文件信息
        let file_metadata = fs::metadata(&published.path)
            .await
            .map_err(DownloadError::Io)?;
        let size = i64::try_from(file_metadata.len()).map_err(|_| ServiceError::SizeOverflow)?;
        let final_path = published
            .path
            .to_str()
            .ok_or(ServiceError::Download(DownloadError::InvalidFileName))?;

        // 写入曲库数据，标记下载任务完成
        let completed = self
            .repository
            .complete_with_song(task.id, task.version, final_path, size, &task.metadata.0)
            .await?;

        // 数据库写入未完成
        if !completed {
            // 如果是新创建的，删除该文件
            if published.created
                && let Err(error) = fs::remove_file(&published.path).await
            {
                tracing::warn!(path = ?published.path, %error, "清理未提交的最终文件失败");
            }
            return Ok(());
        }

        // 移除ready文件
        if let Err(error) = fs::remove_file(ready_path).await {
            tracing::warn!(path = ?ready_path, %error, "清理 ready 文件失败");
        }

        Ok(())
    }
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

    // 使用硬链接创建正式曲库文件
    match fs::hard_link(ready_path, &target).await {
        Ok(()) => Ok(PublishedFile {
            path: target,
            created: true,
        }),
        // 目标文件已存在
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            // 是同一个文件，不需要额外创建，直接返回
            if files_are_same(ready_path, &target).await? {
                Ok(PublishedFile {
                    path: target,
                    created: false,
                })
            } else {
                // 不是同一个文件，返回错误，避免覆盖现有歌曲
                Err(ServiceError::Download(DownloadError::TargetExists))
            }
        }
        Err(error) => Err(ServiceError::Download(DownloadError::Io(error))),
    }
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

async fn files_are_same(left: &Path, right: &Path) -> Result<bool, ServiceError> {
    cfg_select! {
      unix => {
        let left = fs::metadata(left).await.map_err(DownloadError::Io)?;
        let right = fs::metadata(right).await.map_err(DownloadError::Io)?;
        Ok(left.dev() == right.dev() && left.ino() == right.ino())
      }
      _ => {
        let _ = (left,right);
        Ok(false)
      }
    }
}
