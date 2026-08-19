use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use tokio::fs;
use tokio_util::sync::CancellationToken;

use crate::error::DownloadError;

pub struct Downloader {
    client: reqwest::Client,
    dir: PathBuf,
    max_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub downloaded: u64,
    /// `None` 表示服务器没有提供 Content-Length/Content-Range。
    pub total: Option<u64>,
}

pub struct DownloadOptions<F>
where
    F: Fn(DownloadProgress) + Send + Sync,
{
    pub task_id: i64,
    pub cancellation: CancellationToken,
    pub progress: Option<F>,
}

pub struct DownloadResult {
    pub path: PathBuf,
    pub size: u64,
}

impl Downloader {
    pub fn new(client: reqwest::Client, dir: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            client,
            dir: dir.into(),
            max_bytes,
        }
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }

    // 移除临时文件
    pub async fn remove_task_files(&self, task_id: i64) -> Result<(), DownloadError> {
        let part = self.dir.join(format!(".download-{task_id}.part"));
        remove_if_exists(&part).await?;

        let ready_prefix = format!(".download-{task_id}.read.");
        let mut entries = match fs::read_dir(&self.dir).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };

        // 移除tag临时文件
        while let Some(entry) = entries.next_entry().await? {
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&ready_prefix))
            {
                remove_if_exists(&entry.path()).await?;
            }
        }
        Ok(())
    }
}

async fn remove_if_exists(path: &Path) -> Result<(), DownloadError> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
