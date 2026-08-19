use shared::{music::MusicResult, request::SearchRequest};

use crate::{
    error::{DownloadError, ServiceError},
    provider::PREVIEW_QUALITY_ID,
};

use super::Service;

impl Service {
    pub async fn search(&self, request: SearchRequest) -> Result<MusicResult, ServiceError> {
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
        let mut request = self.preview_client.get(resource.url);
        for (name, value) in resource.headers {
            request = request.header(name, value);
        }
        if let Some(range) = range {
            request = request.header(reqwest::header::RANGE, range);
        }
        Ok(request.send().await.map_err(DownloadError::from)?)
    }
}
