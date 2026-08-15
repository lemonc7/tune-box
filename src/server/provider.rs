pub mod kuwo;
pub mod netease;

use async_trait::async_trait;

use crate::{error::ProviderError, server::model::ResolvedResource, shared::music::SearchResult};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36";

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn search(
        &self,
        query: &str,
        page: i64,
        limit: i64,
    ) -> Result<SearchResult, ProviderError>;
    async fn resolve(
        &self,
        track_id: &str,
        quality_id: &str,
    ) -> Result<ResolvedResource, ProviderError>;
    async fn lyrics(&self, track_id: &str) -> Result<String, ProviderError>;
}
