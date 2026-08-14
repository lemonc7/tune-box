use garde::Validate;

use crate::{
    error::{DownloadError, ServiceError},
    server::model::PREVIEW_QUALITY_ID,
    shared::{music::SearchResult, request::SearchRequest},
};

use super::Service;

impl Service {
    pub async fn search(&self, request: SearchRequest) -> Result<SearchResult, ServiceError> {
        let request = request.normalize();
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
