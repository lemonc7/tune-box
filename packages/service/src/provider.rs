pub mod kuwo;

use std::collections::HashMap;

use async_trait::async_trait;
use shared::music::MusicResult;

use crate::error::ProviderError;

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36";

pub struct ResolvedResource {
    pub provider: &'static str,
    pub track_id: String,
    pub url: String,
    pub headers: HashMap<String, String>,
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
