use std::sync::Arc;

use contracts::request::{CreateDownloadRequest, SearchRequest};
use rmcp::{
    ServerHandler, handler::server::wrapper::Parameters, schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;

use crate::{manager::ManagerHandle, service::Service};

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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TaskArgs {
    id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListArgs {
    query: Option<String>,
    before_id: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateDownloadArgs {
    provider: String,
    track_id: String,
    name: String,
    artists: Vec<String>,
    album: Option<String>,
    album_artist: Option<String>,
    date: Option<String>,
    cover_url: Option<String>,
}

impl From<SearchMusicArgs> for SearchRequest {
    fn from(value: SearchMusicArgs) -> Self {
        Self {
            provider: value.provider,
            query: value.query,
            page: 1,
            limit: 20,
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

    #[tool(description = "创建下载任务，并立即唤醒下载管理器。")]
    async fn create_download(
        &self,
        Parameters(args): Parameters<CreateDownloadArgs>,
    ) -> Result<String, String> {
        let id = self
            .service
            .create_download(CreateDownloadRequest {
                provider: args.provider,
                track_id: args.track_id,
                name: args.name,
                artists: args.artists,
                album: args.album.unwrap_or_default(),
                album_artist: args.album_artist.unwrap_or_default(),
                date: args.date.unwrap_or_default(),
                cover_url: args.cover_url.unwrap_or_default(),
            })
            .await
            .map_err(|error| error.to_string())?;
        if let Some(id) = id {
            self.manager
                .wake()
                .await
                .map_err(|error| error.to_string())?;
            tracing::info!(task_id = id, "MCP 已创建下载任务");
            Ok(format!("已创建下载任务 {id}"))
        } else {
            Ok("下载任务已存在".to_owned())
        }
    }

    #[tool(description = "分页查看下载任务。")]
    async fn list_downloads(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<String, String> {
        let page = self
            .service
            .list_downloads(args.before_id, args.limit.unwrap_or(20))
            .await
            .map_err(|error| error.to_string())?;
        let downloads = page
            .downloads
            .into_iter()
            .map(|download| {
                serde_json::json!({
                    "id": download.id,
                    "provider": download.provider,
                    "metadata": download.metadata,
                    "status": download.status,
                    "downloaded_bytes": download.downloaded_bytes,
                    "total_bytes": download.total_bytes,
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&serde_json::json!({
            "downloads": downloads,
            "next_before_id": page.next_before_id,
        }))
        .map_err(|error| error.to_string())
    }

    #[tool(description = "搜索本地曲库。")]
    async fn search_library(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<String, String> {
        let page = self
            .service
            .list_songs(
                args.query.as_deref().unwrap_or_default(),
                args.before_id,
                args.limit.unwrap_or(20),
            )
            .await
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&serde_json::json!({
            "songs": page.songs,
            "next_before_id": page.next_before_id,
        }))
        .map_err(|error| error.to_string())
    }

    #[tool(description = "暂停正在排队、解析或下载的任务。")]
    async fn pause_download(
        &self,
        Parameters(args): Parameters<TaskArgs>,
    ) -> Result<String, String> {
        task_result(self.manager.pause(args.id).await, "已暂停")
    }

    #[tool(description = "继续已暂停的下载任务。")]
    async fn resume_download(
        &self,
        Parameters(args): Parameters<TaskArgs>,
    ) -> Result<String, String> {
        let result = self.manager.resume(args.id).await;
        if matches!(result, Ok(true)) {
            self.manager
                .wake()
                .await
                .map_err(|error| error.to_string())?;
        }
        task_result(result, "已继续")
    }

    #[tool(description = "取消下载任务并清理临时文件。")]
    async fn cancel_download(
        &self,
        Parameters(args): Parameters<TaskArgs>,
    ) -> Result<String, String> {
        task_result(self.manager.cancel(args.id).await, "已取消")
    }

    #[tool(description = "重新执行失败或已取消的下载任务。")]
    async fn retry_download(
        &self,
        Parameters(args): Parameters<TaskArgs>,
    ) -> Result<String, String> {
        let result = self.manager.retry(args.id).await;
        if matches!(result, Ok(true)) {
            self.manager
                .wake()
                .await
                .map_err(|error| error.to_string())?;
        }
        task_result(result, "已重新排队")
    }

    #[tool(description = "同步下载目录与本地曲库数据库。")]
    async fn sync_library(&self) -> Result<String, String> {
        let count = self
            .service
            .sync_library()
            .await
            .map_err(|error| error.to_string())?;
        tracing::info!(song_count = count, "MCP 曲库扫描完成");
        Ok(format!("同步完成，共 {count} 首歌曲"))
    }

    #[tool(description = "只删除终态下载历史，不删除音乐文件。")]
    async fn delete_download(
        &self,
        Parameters(args): Parameters<TaskArgs>,
    ) -> Result<String, String> {
        let deleted = self
            .service
            .delete_download(args.id)
            .await
            .map_err(|error| error.to_string())?;
        if deleted {
            tracing::info!(task_id = args.id, "MCP 已删除下载历史");
        }
        Ok(if deleted {
            "已删除下载历史"
        } else {
            "任务不存在或当前状态不能删除"
        }
        .to_owned())
    }

    #[tool(description = "删除曲库歌曲、本地文件及关联下载记录。")]
    async fn delete_song(&self, Parameters(args): Parameters<TaskArgs>) -> Result<String, String> {
        let deleted = self
            .service
            .delete_song(args.id)
            .await
            .map_err(|error| error.to_string())?;
        if deleted {
            tracing::info!(song_id = args.id, "MCP 已删除曲库歌曲");
        }
        Ok(if deleted {
            "已删除歌曲"
        } else {
            "歌曲不存在"
        }
        .to_owned())
    }
}

fn task_result(
    result: Result<bool, crate::error::ManagerError>,
    success: &str,
) -> Result<String, String> {
    result
        .map(|changed| {
            if changed {
                success.to_owned()
            } else {
                "任务不存在或当前状态不允许该操作".to_owned()
            }
        })
        .map_err(|error| error.to_string())
}

#[tool_handler(
    name = "tune-box",
    instructions = "用于搜索音乐、创建下载任务以及管理下载状态。"
)]
impl ServerHandler for MusicMcp {}
