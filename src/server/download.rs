use futures_util::StreamExt;
use reqwest::Url;
use std::{io::SeekFrom, path::PathBuf};
use tokio::{
    fs,
    io::{AsyncSeekExt, AsyncWriteExt},
};

use crate::{error::DownloadError, model::Resource};

pub struct Downloader {
    client: reqwest::Client,
    dir: PathBuf,
    max_bytes: u64,
}

pub struct DownloadOptions<F>
where
    F: Fn(u64, u64) + Send + Sync,
{
    pub task_id: i64,
    pub file_name: String,
    pub expected_size: Option<u64>,
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

    pub async fn download<F>(
        &self,
        resource: Resource,
        options: DownloadOptions<F>,
    ) -> Result<DownloadResult, DownloadError>
    where
        F: Fn(u64, u64) + Send + Sync,
    {
        let url = Url::parse(&resource.url).map_err(|_| DownloadError::InvalidUrl)?;
        let schema = url.scheme();
        if schema != "http" || schema != "https" {
            return Err(DownloadError::InvalidUrl);
        }

        // 创建目录
        fs::create_dir_all(&self.dir).await?;

        // 临时文件
        let temp = self.dir.join(format!(".download-{}.part", options.task_id));
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&temp)
            .await?;

        // 当前已有大小
        let mut downloaded = file.metadata().await?.len();

        // 创建请求
        let mut request = self.client.get(&resource.url);

        // 断点续传
        if downloaded > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded));
        }

        // 添加headers
        for (key, value) in resource.headers {
            request = request.header(key, value);
        }

        let resp = request.send().await?;
        let status = resp.status();

        if status != reqwest::StatusCode::OK && status != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(DownloadError::HTTP(status));
        }

        // 如果服务器不支持端点续传(206)，从头开始下载
        if downloaded > 0 && status == reqwest::StatusCode::OK {
            file.set_len(0).await?;
            file.seek(SeekFrom::Start(0)).await?;
            downloaded = 0;
        }

        // 计算总大小
        let mut total = 0u64;
        if let Some(length) = resp.content_length() {
            total += length
        }

        if let Some(range) = resp.headers().get("content-range")
            && let Ok(value) = range.to_str()
            && let Some((_, size)) = value.split_once('/')
            && let Ok(size) = size.parse::<u64>()
        {
            total = size
        }

        if total > self.max_bytes {
            return Err(DownloadError::TooLarge);
        }

        if let Some(callback) = &options.progress {
            callback(downloaded, total)
        }

        // 流式下载
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;

            downloaded += bytes.len() as u64;
            // 限制文件大小
            if downloaded > self.max_bytes {
                return Err(DownloadError::TooLarge);
            }

            file.write_all(&bytes).await?;

            // 回调函数待处理
            if let Some(callback) = &options.progress {
                callback(downloaded, total);
            }
        }

        if downloaded == 0 {
            return Err(DownloadError::Empty);
        }

        file.sync_all().await?;
        drop(file);

        // 最终文件
        let target = self.dir.join(&options.file_name);
        fs::rename(&temp, &target).await?;

        Ok(DownloadResult {
            path: target,
            size: downloaded,
        })
    }
}
