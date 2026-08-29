use std::time::Duration;

use contracts::{music::MusicResult, request::SearchRequest};
use garde::Validate;

use crate::{
    error::{DownloadError, ServiceError},
    net::validate_public_url,
    provider::PREVIEW_QUALITY_ID,
};

use super::Service;

// 校验试听目标时允许花在 DNS 上的最长时间。
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

impl Service {
    pub async fn search(&self, request: SearchRequest) -> Result<MusicResult, ServiceError> {
        request
            .validate()
            .map_err(|error| ServiceError::InvalidRequest(error.to_string()))?;
        let provider = self.provider(&request.provider)?;
        provider
            .search(&request.query, request.page, request.limit)
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

        // 资源链接来自 provider 或第三方解析服务，不做校验就请求会把这台机器变成
        // 可以被外部操纵的代理。响应体会原样回显给调用方，所以这里比下载路径更关键。
        let url = reqwest::Url::parse(&resource.url).map_err(|_| DownloadError::InvalidUrl)?;
        validate_public_url(&url, RESOLVE_TIMEOUT)
            .await
            .map_err(DownloadError::from)?;

        let mut request = self.preview_client.get(url);
        for (name, value) in resource.headers {
            // 不允许上游通过 Host 头把请求指到别的地方。
            if name.eq_ignore_ascii_case(reqwest::header::HOST.as_str()) {
                continue;
            }
            request = request.header(name, value);
        }
        if let Some(range) = range {
            request = request.header(reqwest::header::RANGE, range);
        }
        Ok(request.send().await.map_err(DownloadError::from)?)
    }
}
