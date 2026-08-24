pub mod download;
pub mod library;
pub mod music;

use crate::{
    db::Repository, download::Downloader, error::ServiceError, provider::Provider,
    tag::embed::Embedder,
};
use std::collections::HashMap;
use tokio::sync::Mutex;

pub struct Service {
    repository: Repository,
    providers: HashMap<String, Box<dyn Provider>>,
    downloader: Downloader,
    preview_client: reqwest::Client,
    embedder: Embedder,
    // 串行化曲库扫描，歌曲发布和歌曲删除，保证文件系统和数据库视图一致
    library_lock: Mutex<()>,
}

impl Service {
    pub fn new(
        repository: Repository,
        providers: HashMap<String, Box<dyn Provider>>,
        downloader: Downloader,
        preview_client: reqwest::Client,
        embedder: Embedder,
    ) -> Self {
        Self {
            repository,
            providers,
            downloader,
            preview_client,
            embedder,
            library_lock: Mutex::new(()),
        }
    }

    fn provider(&self, name: &str) -> Result<&dyn Provider, ServiceError> {
        self.providers
            .get(name)
            .map(|v| v.as_ref())
            .ok_or(ServiceError::InvalidProvider)
    }

    pub fn providers(&self) -> Vec<String> {
        let mut providers = self
            .providers
            .values()
            .map(|v| v.id().to_owned())
            .collect::<Vec<_>>();
        providers.sort_unstable();
        providers
    }
}
