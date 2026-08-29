use futures_util::StreamExt;
use reqwest::{StatusCode, header};
use std::{
    io::{ErrorKind, SeekFrom},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncSeekExt, AsyncWriteExt},
};
use tokio_util::sync::CancellationToken;

use crate::{error::DownloadError, net::validate_public_url, provider::ResolvedResource};

// 校验下载目标时允许花在 DNS 上的最长时间。
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

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

    pub async fn download<F>(
        &self,
        resource: ResolvedResource,
        options: DownloadOptions<F>,
    ) -> Result<DownloadResult, DownloadError>
    where
        F: Fn(DownloadProgress) + Send + Sync,
    {
        let url = reqwest::Url::parse(&resource.url).map_err(|_| DownloadError::InvalidUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(DownloadError::InvalidUrl);
        }
        // 资源链接来自 provider 接口，可能指向内网；请求前必须确认目标是公网地址。
        // 重定向由客户端的 redirect policy 逐跳校验。
        validate_public_url(&url, RESOLVE_TIMEOUT).await?;

        let file_name = build_ready_file_name(options.task_id, "flac")?;
        fs::create_dir_all(&self.dir).await?;

        // 下载完成后先发布为任务专属的隐藏ready文件，打完标签后再改成用户可见名称
        let target = self.dir.join(&file_name);
        if fs::try_exists(&target).await? {
            let size = fs::metadata(&target).await?.len();
            if size == 0 {
                return Err(DownloadError::Empty);
            } else if size > self.max_bytes {
                return Err(DownloadError::TooLarge);
            } else {
                return Ok(DownloadResult { path: target, size });
            }
        }

        // 临时文件
        let temp = self.dir.join(format!(".download-{}.part", options.task_id));
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&temp)
            .await?;

        // 当前已有大小
        let mut downloaded = file.metadata().await?.len();
        if downloaded > self.max_bytes {
            return Err(DownloadError::TooLarge);
        }

        // 创建请求，provider不能设置range，断点位置只能由本地临时文件决定
        let mut request = self.client.get(&resource.url);
        for (key, value) in resource.headers {
            if key.eq_ignore_ascii_case(header::RANGE.as_str()) {
                continue;
            }
            request = request.header(key, value);
        }
        if downloaded > 0 {
            request = request.header(header::RANGE, format!("bytes={downloaded}-"))
        }

        let resp = tokio::select! {
          _ = options.cancellation.cancelled() => {
            return Err(DownloadError::Cancelled);
          }
          resp = request.send() => resp?,
        };
        let status = resp.status();

        if status != StatusCode::OK && status != StatusCode::PARTIAL_CONTENT {
            return Err(DownloadError::Http(status));
        }

        // 提前拒绝常见的接口错误页，最终音频有效性仍由后续处理阶段确认
        if let Some(content_type) = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            && is_non_audio_content_type(content_type)
        {
            return Err(DownloadError::UnexpectedContentType(
                content_type.to_string(),
            ));
        }

        // 如何服务器不支持断点续传(206),从头开始下载
        if downloaded > 0 && status == StatusCode::OK {
            file.set_len(0).await?;
            file.seek(SeekFrom::Start(0)).await?;
            downloaded = 0;
        }

        // 计算总大小
        let mut total = resp.content_length();
        if status == StatusCode::PARTIAL_CONTENT {
            let value = resp
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .ok_or(DownloadError::InvalidRange)?;
            let range = parse_content_range(value)?;
            if range.start != downloaded {
                return Err(DownloadError::InvalidRange);
            }
            // 请求的是 `bytes={downloaded}-`，因此响应范围必须一直覆盖到文件末尾。
            // 同时校验响应体声明长度，避免在流读取完成后才发现范围响应不完整。
            let range_length = range.end - range.start + 1;
            if range.end + 1 != range.total
                || resp
                    .content_length()
                    .is_some_and(|length| length != range_length)
            {
                return Err(DownloadError::InvalidRange);
            }
            total = Some(range.total);
        }

        if total.is_some_and(|total| total > self.max_bytes) {
            return Err(DownloadError::TooLarge);
        }

        if let Some(callback) = &options.progress {
            callback(DownloadProgress { downloaded, total })
        }

        // 流式下载
        let mut stream = resp.bytes_stream();
        loop {
            let chunk = tokio::select! {
              _ = options.cancellation.cancelled() => {
                return Err(DownloadError::Cancelled);
              }
              chunk = stream.next() => chunk,
            };

            let Some(chunk) = chunk else {
                break;
            };
            let bytes = chunk?;

            downloaded += bytes.len() as u64;
            // 限制文件大小
            if downloaded > self.max_bytes {
                return Err(DownloadError::TooLarge);
            }

            file.write_all(&bytes).await?;

            // 回调函数
            if let Some(callback) = &options.progress {
                callback(DownloadProgress { downloaded, total })
            }
        }

        if downloaded == 0 {
            return Err(DownloadError::Empty);
        }

        if let Some(expected) = total
            && downloaded != expected
        {
            return Err(DownloadError::SizeMismatch {
                expected,
                actual: downloaded,
            });
        }

        file.sync_all().await?;
        drop(file);

        // hard_link会在ready文件已存在时原子失败，避免并发任务静默覆盖文件
        fs::hard_link(&temp, &target).await.map_err(|error| {
            if error.kind() == ErrorKind::AlreadyExists {
                DownloadError::TargetExists
            } else {
                DownloadError::Io(error)
            }
        })?;
        if let Err(error) = fs::remove_file(&temp).await {
            // ready文件已经原子发布，清理part链接失败不应该把成功下载报告为失败
            tracing::warn!(path = ?temp, error = %error, "清理下载临时文件失败");
        }

        // ready文件发布成功后，以实际大小结束进度，长度未知的响应也能落库总大小
        if let Some(callback) = &options.progress {
            callback(DownloadProgress {
                downloaded,
                total: Some(downloaded),
            });
        }
        Ok(DownloadResult {
            path: target,
            size: downloaded,
        })
    }
}

async fn remove_if_exists(path: &Path) -> Result<(), DownloadError> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn build_ready_file_name(task_id: i64, format: &str) -> Result<String, DownloadError> {
    let format = format.trim().trim_start_matches(".");
    if format.is_empty() || !format.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(DownloadError::InvalidFileName);
    }

    Ok(format!(".download-{task_id}.ready.{format}"))
}

fn is_non_audio_content_type(content_type: &str) -> bool {
    let media_type = content_type
        .split(";")
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    media_type.starts_with("text/")
        || media_type == "application/json"
        || media_type.ends_with("+json")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentRange {
    start: u64,
    end: u64,
    total: u64,
}

fn parse_content_range(value: &str) -> Result<ContentRange, DownloadError> {
    let value = value
        .strip_prefix("bytes ")
        .ok_or(DownloadError::InvalidRange)?;

    let (range, total) = value.split_once('/').ok_or(DownloadError::InvalidRange)?;
    let (start, end) = range.split_once('-').ok_or(DownloadError::InvalidRange)?;
    let start = start.parse().map_err(|_| DownloadError::InvalidRange)?;
    let end = end.parse().map_err(|_| DownloadError::InvalidRange)?;
    let total = total.parse().map_err(|_| DownloadError::InvalidRange)?;
    if start > end || end >= total {
        return Err(DownloadError::InvalidRange);
    }

    Ok(ContentRange { start, end, total })
}

#[cfg(test)]
mod tests {
    use super::{ContentRange, parse_content_range};

    #[test]
    fn parses_content_range_with_total_size() {
        assert_eq!(
            parse_content_range("bytes 100-999/1000").unwrap(),
            ContentRange {
                start: 100,
                end: 999,
                total: 1000,
            }
        );
    }

    #[test]
    fn rejects_invalid_content_ranges() {
        for value in [
            "bytes 100-99/1000",
            "bytes 100-1000/1000",
            "bytes */1000",
            "items 100-999/1000",
            "bytes 100-999/*",
        ] {
            assert!(parse_content_range(value).is_err(), "{value}");
        }
    }
}
