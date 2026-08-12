use async_trait::async_trait;

use crate::{error::ProviderError, server::model::ResolvedResource, shared::music::SearchResult};

pub mod kuwo;

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    async fn search(
        &self,
        query: &str,
        page: u32,
        limit: u32,
    ) -> Result<SearchResult, ProviderError>;
    async fn resolve(
        &self,
        track_id: &str,
        quality_id: &str,
    ) -> Result<ResolvedResource, ProviderError>;
    async fn lyrics(&self, track_id: &str) -> Result<String, ProviderError>;
}
