#[cfg(feature = "server")]
use crate::error::ServerError;
use contracts::{
    download::{Download, DownloadAction, DownloadEvent, DownloadPage},
    library::LibrarySnapshot,
    music::MusicResult,
    request::{CreateDownloadRequest, SearchRequest},
};
use dioxus::fullstack::ServerEvents;
use dioxus::prelude::*;
#[cfg(feature = "server")]
use dioxus::server::axum::Extension;
#[cfg(feature = "server")]
use tokio::sync::broadcast::error::RecvError;

pub const PAGE_SIZE: i64 = 20;

#[server(state: Extension<crate::server::AppState>)]
pub async fn list_providers() -> ServerFnResult<Vec<String>> {
    Ok(state.service.providers())
}

#[server(state: Extension<crate::server::AppState>)]
pub async fn search_music(request: SearchRequest) -> ServerFnResult<MusicResult> {
    state
        .service
        .search(request)
        .await
        .map_err(ServerError::into)
}

#[server(state: Extension<crate::server::AppState>)]
pub async fn create_download(request: CreateDownloadRequest) -> ServerFnResult<String> {
    let created = state
        .service
        .create_download(request)
        .await
        .map_err(ServerError::into)?;

    if let Some(id) = created {
        state.manager.wake().await.map_err(ServerError::into)?;
        dioxus::logger::tracing::info!(task_id = id, "下载任务已创建");
        Ok("已加入下载队列".to_owned())
    } else {
        Ok("下载任务已存在".to_owned())
    }
}

#[server(state: Extension<crate::server::AppState>)]
pub async fn list_downloads(before_id: Option<i64>, limit: i64) -> ServerFnResult<DownloadPage> {
    state
        .service
        .list_downloads(before_id, limit)
        .await
        .map_err(ServerError::into)
}

#[server(state: Extension<crate::server::AppState>)]
pub async fn get_download(id: i64) -> ServerFnResult<Option<Download>> {
    state
        .service
        .get_download(id)
        .await
        .map_err(ServerError::into)
}

#[get("/api/downloads/events", state: Extension<crate::server::AppState>)]
pub async fn download_events() -> ServerFnResult<ServerEvents<DownloadEvent>> {
    let mut receiver = state.download_events.subscribe();
    let shutdown = state.shutdown.clone();

    Ok(ServerEvents::new(move |mut sender| async move {
        loop {
            let received = tokio::select! {
                _ = shutdown.cancelled() => break,
                received = receiver.recv() => received,
            };
            let event = match received {
                Ok(event) => event,
                // 客户端落后时关闭本次流，让它重连并从数据库恢复一致状态。
                Err(RecvError::Lagged(_)) => break,
                Err(RecvError::Closed) => break,
            };
            if sender.send(event).await.is_err() {
                break;
            }
        }
    }))
}

#[server(state: Extension<crate::server::AppState>)]
pub async fn change_download(id: i64, action: DownloadAction) -> ServerFnResult<()> {
    let changed = match action {
        DownloadAction::Pause => state.manager.pause(id).await.map_err(ServerError::into)?,
        DownloadAction::Resume => state.manager.resume(id).await.map_err(ServerError::into)?,
        DownloadAction::Cancel => state.manager.cancel(id).await.map_err(ServerError::into)?,
        DownloadAction::Retry => state.manager.retry(id).await.map_err(ServerError::into)?,
        DownloadAction::Delete => state
            .service
            .delete_download(id)
            .await
            .map_err(ServerError::into)?,
    };

    if !changed {
        return Err(ServerFnError::ServerError {
            message: "任务不存在或当前状态不允许该操作".to_owned(),
            code: 409,
            details: None,
        });
    }

    if matches!(action, DownloadAction::Resume | DownloadAction::Retry) {
        state.manager.wake().await.map_err(ServerError::into)?;
    }

    dioxus::logger::tracing::info!(task_id = id, ?action, "下载任务操作已完成");

    Ok(())
}

#[server(state: Extension<crate::server::AppState>)]
pub async fn load_library(
    query: String,
    before_id: Option<i64>,
    limit: i64,
) -> ServerFnResult<LibrarySnapshot> {
    let query = query.trim();
    let page = state
        .service
        .list_songs(query, before_id, limit)
        .await
        .map_err(ServerError::into)?;
    let count = state
        .service
        .count_songs()
        .await
        .map_err(ServerError::into)?;

    Ok(LibrarySnapshot { page, count })
}

#[server(state: Extension<crate::server::AppState>)]
pub async fn sync_library() -> ServerFnResult<i64> {
    let count = state
        .service
        .sync_library()
        .await
        .map_err(ServerError::into)?;
    dioxus::logger::tracing::info!(song_count = count, "曲库扫描完成");
    Ok(count)
}

#[server(state: Extension<crate::server::AppState>)]
pub async fn delete_song(id: i64) -> ServerFnResult<()> {
    let deleted = state
        .service
        .delete_song(id)
        .await
        .map_err(ServerError::into)?;

    if deleted {
        dioxus::logger::tracing::info!(song_id = id, "曲库歌曲已删除");
        Ok(())
    } else {
        Err(ServerFnError::ServerError {
            message: "歌曲不存在".to_owned(),
            code: 404,
            details: None,
        })
    }
}
