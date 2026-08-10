use async_trait::async_trait;

use crate::{
    error::ProviderError,
    model::{Resource, SearchResult},
};

pub mod kuwo;

#[async_trait]
pub trait Provider {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    async fn search(
        &self,
        query: &str,
        page: u32,
        limit: u32,
    ) -> Result<SearchResult, ProviderError>;
    async fn resolve(&self, track_id: &str, quality_id: &str) -> Result<Resource, ProviderError>;
    async fn lyrics(&self, track_id: &str) -> Result<String, ProviderError>;
}
