use std::sync::Arc;

use rmcp::{
    ServerHandler, handler::server::wrapper::Parameters, schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;

use crate::{
    server::{manager::ManagerHandle, service::Service},
    shared::request::SearchRequest,
};

#[derive(Clone)]
pub struct MusicMcp {
    service: Arc<Service>,
    manager: ManagerHandle,
}

impl MusicMcp {
    pub fn new(service: Arc<Service>, manager: ManagerHandle) -> Self {
        Self { service, manager }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchMusicArgs {
    provider: String,
    query: String,
    lossless: bool,
}

impl From<SearchMusicArgs> for SearchRequest {
    fn from(value: SearchMusicArgs) -> Self {
        Self {
            provider: value.provider,
            query: value.query,
            page: 1,
            limit: 20,
            lossless: value.lossless,
        }
    }
}

#[tool_router]
impl MusicMcp {
    #[tool(description = "按照关键词搜索歌曲。该操作只读取数据，不会创建下载任务。")]
    async fn search_music(
        &self,
        Parameters(args): Parameters<SearchMusicArgs>,
    ) -> Result<String, String> {
        let result = self
            .service
            .search(args.into())
            .await
            .map_err(|error| error.to_string())?;

        serde_json::to_string(&result).map_err(|error| error.to_string())
    }
}

#[tool_handler(
    name = "tune-box",
    instructions = "用于搜索音乐、创建下载任务以及管理下载状态。"
)]
impl ServerHandler for MusicMcp {}
