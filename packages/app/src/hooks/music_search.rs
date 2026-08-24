use std::{collections::HashSet, sync::Arc};

use contracts::{
    music::{MusicResult, Track},
    request::SearchRequest,
};
use dioxus::prelude::*;

use crate::{api, view};

#[derive(Clone, PartialEq, Eq)]
pub enum MusicSearchState {
    // 页面刚打开，还没开始搜索
    Idle,
    Loading,
    Ready(Arc<MusicResult>),
    Failed(String),
}

pub struct MusicSearch {
    // 首次加载页面状态
    state: Signal<MusicSearchState>,
    // 加载更多时，页面状态
    loading_more: Signal<bool>,
    load_more_error: Signal<Option<String>>,
    exhausted: Signal<bool>,
    generation: Signal<u64>,
}

#[derive(Clone, Copy)]
pub struct MusicSearchSession {
    pub provider: Signal<Option<String>>,
    pub query: Signal<String>,
    pub list_scroll_top: Signal<f64>,
    pub search: MusicSearch,
}

impl Clone for MusicSearch {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for MusicSearch {}

impl MusicSearch {
    pub fn state(&self) -> MusicSearchState {
        self.state.read().clone()
    }

    pub fn loading(&self) -> bool {
        matches!(&*self.state.read(), MusicSearchState::Loading)
    }

    pub fn has_more(&self) -> bool {
        match &*self.state.read() {
            MusicSearchState::Ready(result) => {
                !(self.exhausted)() && result.tracks.len() < result.total
            }
            _ => false,
        }
    }

    pub fn loading_more(&self) -> bool {
        (self.loading_more)()
    }

    pub fn load_more_error(&self) -> Option<String> {
        self.load_more_error.read().clone()
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.invalidate_requests();
        self.loading_more.set(false);
        self.load_more_error.set(None);
        self.exhausted.set(true);
        self.state.set(MusicSearchState::Failed(message.into()));
    }

    // 首次搜索
    pub fn search(&mut self, request: SearchRequest) {
        // 使之前的请求失败
        let request_generation = self.invalidate_requests();
        // 取消 加载更多 状态
        self.loading_more.set(false);
        self.load_more_error.set(None);
        self.exhausted.set(false);
        // 设置页面为loading状态
        self.state.set(MusicSearchState::Loading);
        // 发起异步请求
        let mut state = self.state;
        let generation = self.generation;
        spawn(async move {
            let next = match api::search_music(request).await {
                Ok(mut result) => {
                    retain_new_tracks(&[], &mut result.tracks);
                    result.total = result.total.max(result.tracks.len());
                    MusicSearchState::Ready(Arc::new(result))
                }
                Err(error) => MusicSearchState::Failed(view::server_error_message(error)),
            };
            // 版本号对应上，才更新数据
            if generation() == request_generation {
                state.set(next);
            }
        });
    }

    // 加载更多
    pub fn load_more(&mut self) {
        // 拒绝非法调用和重复请求
        if self.loading_more() || !self.has_more() {
            return;
        }

        // 从当前搜索结果中构造下一页请求
        let MusicSearchState::Ready(current) = self.state() else {
            return;
        };
        let mut current = (*current).clone();
        let request = SearchRequest {
            provider: current.provider.clone(),
            query: current.query.clone(),
            page: current.page + 1,
            limit: current.limit,
        };

        // 更新状态
        self.loading_more.set(true);
        self.load_more_error.set(None);
        // 发起异步请求
        let request_generation = self.generation();
        let mut state = self.state;
        let mut loading_more = self.loading_more;
        let mut load_more_error = self.load_more_error;
        let mut exhausted = self.exhausted;
        let generation = self.generation;
        spawn(async move {
            let result = api::search_music(request).await;
            if generation() != request_generation {
                return;
            }

            match result {
                Ok(mut next) => {
                    let added = retain_new_tracks(&current.tracks, &mut next.tracks);
                    // 第三方源的total可能不准确；空页、重复页或不前进的页码都应停止分页。
                    if next.page <= current.page || added == 0 {
                        exhausted.set(true);
                    }
                    current.page = current.page.max(next.page);
                    current.tracks.extend(next.tracks);
                    current.total = next.total.max(current.tracks.len());
                    state.set(MusicSearchState::Ready(Arc::new(current)));
                }
                Err(error) => {
                    load_more_error.set(Some(view::server_error_message(error)));
                }
            }
            loading_more.set(false);
        });
    }

    fn generation(&self) -> u64 {
        (self.generation)()
    }

    // 使之前的请求失败
    fn invalidate_requests(&mut self) -> u64 {
        let generation = self.generation().wrapping_add(1);
        self.generation.set(generation);
        generation
    }
}

pub fn use_music_search() -> MusicSearch {
    MusicSearch {
        state: use_signal(|| MusicSearchState::Idle),
        loading_more: use_signal(|| false),
        load_more_error: use_signal(|| None),
        exhausted: use_signal(|| false),
        generation: use_signal(|| 0),
    }
}

fn retain_new_tracks(existing: &[Track], incoming: &mut Vec<Track>) -> usize {
    let mut seen = existing
        .iter()
        .map(|track| (track.provider.clone(), track.id.clone()))
        .collect::<HashSet<_>>();
    incoming.retain(|track| seen.insert((track.provider.clone(), track.id.clone())));
    incoming.len()
}

pub fn use_music_search_provider() -> MusicSearchSession {
    let session = MusicSearchSession {
        provider: use_signal(|| None),
        query: use_signal(String::new),
        list_scroll_top: use_signal(|| 0.0),
        search: use_music_search(),
    };
    use_context_provider(|| session);
    session
}

pub fn use_music_search_session() -> MusicSearchSession {
    use_context::<MusicSearchSession>()
}

#[cfg(test)]
mod tests {
    use contracts::music::Track;

    use super::retain_new_tracks;

    fn track(id: &str) -> Track {
        Track {
            id: id.to_owned(),
            provider: "provider".to_owned(),
            name: id.to_owned(),
            artists: Vec::new(),
            album: String::new(),
            album_artist: String::new(),
            date: String::new(),
            cover_url: String::new(),
        }
    }

    #[test]
    fn removes_tracks_already_loaded_and_duplicates_in_the_same_page() {
        let existing = vec![track("1")];
        let mut incoming = vec![track("1"), track("2"), track("2"), track("3")];

        assert_eq!(retain_new_tracks(&existing, &mut incoming), 2);
        assert_eq!(
            incoming
                .into_iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            ["2", "3"]
        );
    }
}
