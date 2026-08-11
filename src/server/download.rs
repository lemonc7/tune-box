use futures_util::StreamExt;
use reqwest::{
    Url,
    header::{CONTENT_TYPE, RANGE},
};
use std::{
    io::{ErrorKind, SeekFrom},
    path::{Path, PathBuf},
};
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
    F: Fn(DownloadProgress) + Send + Sync,
{
    pub task_id: i64,
    pub file_name: String,
    pub progress: Option<F>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub downloaded: u64,
    /// `None` 表示服务器没有提供 Content-Length/Content-Range。
    pub total: Option<u64>,
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
        F: Fn(DownloadProgress) + Send + Sync,
    {
        let url = Url::parse(&resource.url).map_err(|_| DownloadError::InvalidUrl)?;
        let schema = url.scheme();
        if !matches!(schema, "http" | "https") {
            return Err(DownloadError::InvalidUrl);
        }

        let file_name = build_file_name(
            &options.file_name,
            resource.provider,
            &resource.track_id,
            resource.quality.format,
        )?;
        // 创建目录
        fs::create_dir_all(&self.dir).await?;

        // 最终文件名只能位于下载目录内，避免绝对路径和../路径穿越。
        let target = self.dir.join(&file_name);
        if fs::try_exists(&target).await? {
            return Err(DownloadError::TargetExists);
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

        // 创建请求。Provider 不能设置 Range，断点位置只能由本地临时文件决定。
        let mut request = self.client.get(&resource.url);
        for (key, value) in resource.headers {
            if key.eq_ignore_ascii_case(RANGE.as_str()) {
                continue;
            }
            request = request.header(key, value);
        }
        if downloaded > 0 {
            request = request.header(RANGE, format!("bytes={downloaded}-"));
        }

        let resp = request.send().await?;
        let status = resp.status();

        if status != reqwest::StatusCode::OK && status != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(DownloadError::Http(status));
        }

        // 提前拒绝常见的接口错误页；最终音频有效性仍由后续处理阶段确认。
        if let Some(content_type) = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            && is_non_audio_content_type(content_type)
        {
            return Err(DownloadError::UnexpectedContentType(
                content_type.to_string(),
            ));
        }

        // 如果服务器不支持端点续传(206)，从头开始下载
        if downloaded > 0 && status == reqwest::StatusCode::OK {
            file.set_len(0).await?;
            file.seek(SeekFrom::Start(0)).await?;
            downloaded = 0;
        }

        // 计算总大小
        let mut total = resp.content_length();
        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            let value = resp
                .headers()
                .get("content-range")
                .and_then(|value| value.to_str().ok())
                .ok_or(DownloadError::InvalidRange)?;
            let (start, range_total) = parse_content_range(value)?;
            if start != downloaded {
                return Err(DownloadError::InvalidRange);
            }
            total = Some(range_total);
        }

        if total.is_some_and(|total| total > self.max_bytes) {
            return Err(DownloadError::TooLarge);
        }

        if let Some(callback) = &options.progress {
            callback(DownloadProgress { downloaded, total })
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
                callback(DownloadProgress { downloaded, total });
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

        // hard_link 会在目标已存在时原子失败，避免并发任务静默覆盖文件。
        fs::hard_link(&temp, &target).await.map_err(|err| {
            if err.kind() == ErrorKind::AlreadyExists {
                DownloadError::TargetExists
            } else {
                DownloadError::Io(err)
            }
        })?;
        if let Err(err) = fs::remove_file(&temp).await {
            // 正式文件已经原子发布，清理隐藏临时链接失败不应把成功下载报告为失败。
            tracing::warn!(path = ?temp, error = %err, "清理下载临时文件失败");
        }

        // 正式文件发布成功后，以实际大小结束进度；长度未知的响应也能落库总大小。
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

fn validate_file_name(file_name: &str) -> Result<&Path, DownloadError> {
    let path = Path::new(file_name);
    if path.file_name() != Some(path.as_os_str()) {
        return Err(DownloadError::InvalidFileName);
    }
    Ok(path)
}

fn build_file_name(
    requested: &str,
    provider: &str,
    track_id: &str,
    format: &str,
) -> Result<String, DownloadError> {
    let format = format.trim().trim_start_matches('.');
    if format.is_empty() || !format.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(DownloadError::InvalidFileName);
    }

    let mut file_name = requested.trim().to_string();
    if file_name.is_empty() {
        file_name = format!("{provider}-{track_id}");
    }
    validate_file_name(&file_name)?;

    let has_expected_extension = Path::new(&file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(format));
    if !has_expected_extension {
        file_name.push('.');
        file_name.push_str(format);
    }

    validate_file_name(&file_name)?;
    Ok(file_name)
}

fn is_non_audio_content_type(content_type: &str) -> bool {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    media_type.starts_with("text/")
        || media_type == "application/json"
        || media_type.ends_with("+json")
}

fn parse_content_range(value: &str) -> Result<(u64, u64), DownloadError> {
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
    Ok((start, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_file_name() {
        assert_eq!(
            validate_file_name("track.flac").unwrap(),
            Path::new("track.flac")
        );
    }

    #[test]
    fn rejects_paths_as_file_names() {
        for name in [
            "",
            ".",
            "..",
            "../track.flac",
            "dir/track.flac",
            "/tmp/track.flac",
        ] {
            assert!(matches!(
                validate_file_name(name),
                Err(DownloadError::InvalidFileName)
            ));
        }
    }

    #[test]
    fn builds_file_name_and_expected_extension() {
        assert_eq!(
            build_file_name("", "kuwo", "123", "flac").unwrap(),
            "kuwo-123.flac"
        );
        assert_eq!(
            build_file_name("track.FLAC", "kuwo", "123", "flac").unwrap(),
            "track.FLAC"
        );
        assert_eq!(
            build_file_name("track.mp3", "kuwo", "123", "flac").unwrap(),
            "track.mp3.flac"
        );
        assert!(build_file_name("../track", "kuwo", "123", "flac").is_err());
    }

    #[test]
    fn rejects_obvious_non_audio_content_types() {
        assert!(is_non_audio_content_type("text/html; charset=utf-8"));
        assert!(is_non_audio_content_type("application/json"));
        assert!(is_non_audio_content_type("application/problem+json"));
        assert!(!is_non_audio_content_type("audio/flac"));
        assert!(!is_non_audio_content_type("application/octet-stream"));
    }

    #[test]
    fn parses_content_range() {
        assert_eq!(parse_content_range("bytes 10-19/20").unwrap(), (10, 20));
        for value in [
            "bytes 20-19/20",
            "bytes 10-20/20",
            "items 10-19/20",
            "bytes */20",
        ] {
            assert!(matches!(
                parse_content_range(value),
                Err(DownloadError::InvalidRange)
            ));
        }
    }
}
