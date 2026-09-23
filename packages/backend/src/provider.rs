pub mod kuwo;
pub mod netease;

use std::collections::HashMap;

use async_trait::async_trait;
use contracts::music::MusicResult;
use futures_util::StreamExt;
use serde::de::DeserializeOwned;

use crate::error::ProviderError;

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36";
const MAX_PROVIDER_JSON_BYTES: usize = 4 * 1024 * 1024;
pub const DOWNLOAD_QUALITY_ID: &str = "2000kflac";
pub const PREVIEW_QUALITY_ID: &str = "320kmp3";

pub struct ResolvedResource {
    pub provider: &'static str,
    pub track_id: String,
    pub url: String,
    pub headers: HashMap<String, String>,
}

pub(crate) async fn decode_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
    source: &str,
) -> Result<T, ProviderError> {
    let body = read_limited_response(response, source, MAX_PROVIDER_JSON_BYTES).await?;
    Ok(serde_json::from_slice(&body)?)
}

pub(crate) async fn read_limited_response(
    response: reqwest::Response,
    source: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, ProviderError> {
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > max_bytes as u64) {
        return Err(ProviderError::InvalidResponse(format!(
            "{source}响应超过{max_bytes}字节限制"
        )));
    }

    let mut body = Vec::with_capacity(content_length.unwrap_or(0) as usize);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        extend_limited(&mut body, &chunk, source, max_bytes)?;
    }

    Ok(body)
}

fn extend_limited(
    body: &mut Vec<u8>,
    chunk: &[u8],
    source: &str,
    max_bytes: usize,
) -> Result<(), ProviderError> {
    if body.len().saturating_add(chunk.len()) > max_bytes {
        return Err(ProviderError::InvalidResponse(format!(
            "{source}响应超过{max_bytes}字节限制"
        )));
    }
    body.extend_from_slice(chunk);
    Ok(())
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn search(
        &self,
        query: &str,
        page: i64,
        limit: i64,
    ) -> Result<MusicResult, ProviderError>;
    async fn resolve(
        &self,
        track_id: &str,
        quality: &str,
    ) -> Result<ResolvedResource, ProviderError>;
    async fn lyrics(&self, track_id: &str) -> Result<String, ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::extend_limited;
    use crate::error::ProviderError;

    #[test]
    fn rejects_streamed_chunks_when_accumulated_body_exceeds_limit() {
        let mut body = Vec::new();
        extend_limited(&mut body, b"123", "测试接口", 4).unwrap();
        let error = extend_limited(&mut body, b"45", "测试接口", 4).unwrap_err();

        assert!(matches!(
            error,
            ProviderError::InvalidResponse(message) if message.contains("超过4字节限制")
        ));
        assert_eq!(body, b"123");
    }
}
