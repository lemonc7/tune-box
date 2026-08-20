use dioxus::prelude::*;
#[cfg(feature = "server")]
use dioxus::server::axum::Extension;
use shared::{
    download::{DownloadAction, DownloadPage},
    library::LibrarySnapshot,
    music::{MusicResult, Track},
    request::SearchRequest,
};

#[cfg(feature = "server")]
use shared::request::CreateDownloadRequest;

pub const PAGE_SIZE: i64 = 20;

#[server(state: Extension<crate::state::AppState>)]
pub async fn list_providers() -> ServerFnResult<Vec<String>> {
    Ok(state.service.providers())
}

#[server(state: Extension<crate::state::AppState>)]
pub async fn search_music(request: SearchRequest) -> ServerFnResult<MusicResult> {
    state
        .service
        .search(request)
        .await
        .map_err(ServerFnError::new)
}

#[server(state: Extension<crate::state::AppState>)]
pub async fn create_download(track: Track) -> ServerFnResult<String> {
    let created = state
        .service
        .create_download(CreateDownloadRequest {
            provider: track.provider,
            track_id: track.id,
            name: track.name,
            artists: track.artists,
            album: track.album,
            album_artist: track.album_artist,
            date: track.date,
            cover_url: track.cover_url,
        })
        .await
        .map_err(ServerFnError::new)?;

    if created.is_some() {
        state.manager.wake().await.map_err(ServerFnError::new)?;
        Ok("已加入下载队列".to_owned())
    } else {
        Ok("下载任务已存在".to_owned())
    }
}

#[server(state: Extension<crate::state::AppState>)]
pub async fn list_downloads(before_id: Option<i64>, limit: i64) -> ServerFnResult<DownloadPage> {
    state
        .service
        .list_downloads(before_id, limit)
        .await
        .map_err(ServerFnError::new)
}

#[server(state: Extension<crate::state::AppState>)]
pub async fn change_download(id: i64, action: DownloadAction) -> ServerFnResult<()> {
    let changed = match action {
        DownloadAction::Pause => state.manager.pause(id).await.map_err(ServerFnError::new)?,
        DownloadAction::Resume => state.manager.resume(id).await.map_err(ServerFnError::new)?,
        DownloadAction::Cancel => state.manager.cancel(id).await.map_err(ServerFnError::new)?,
        DownloadAction::Retry => state.manager.retry(id).await.map_err(ServerFnError::new)?,
        DownloadAction::Delete => state
            .service
            .delete_download(id)
            .await
            .map_err(ServerFnError::new)?,
    };

    if !changed {
        return Err(ServerFnError::new("任务不存在或当前状态不允许该操作"));
    }

    if matches!(action, DownloadAction::Resume | DownloadAction::Retry) {
        state.manager.wake().await.map_err(ServerFnError::new)?;
    }

    Ok(())
}

#[server(state: Extension<crate::state::AppState>)]
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
        .map_err(ServerFnError::new)?;
    let count = state
        .service
        .count_songs()
        .await
        .map_err(ServerFnError::new)?;

    Ok(LibrarySnapshot { page, count })
}

#[server(state: Extension<crate::state::AppState>)]
pub async fn sync_library() -> ServerFnResult<i64> {
    state
        .service
        .sync_library()
        .await
        .map_err(ServerFnError::new)
}

#[server(state: Extension<crate::state::AppState>)]
pub async fn delete_song(id: i64) -> ServerFnResult<()> {
    let deleted = state
        .service
        .delete_song(id)
        .await
        .map_err(ServerFnError::new)?;

    if deleted {
        Ok(())
    } else {
        Err(ServerFnError::new("歌曲不存在"))
    }
}
