use contracts::{music::MusicResult, request::SearchRequest};
use dioxus::prelude::*;

use crate::{api, view};

#[derive(Clone, PartialEq, Eq)]
pub enum MusicSearchState {
    // 页面刚打开，还没开始搜索
    Idle,
    Loading,
    Ready(MusicResult),
    Failed(String),
}

pub struct MusicSearch {
    // 首次加载页面状态
    state: Signal<MusicSearchState>,
    // 加载更多时，页面状态
    loading_more: Signal<bool>,
    load_more_error: Signal<Option<String>>,
    generation: Signal<u64>,
}

#[derive(Clone, Copy)]
pub struct MusicSearchSession {
    pub provider: Signal<Option<String>>,
    pub query: Signal<String>,
    pub list_scroll_top: Signal<f64>,
    pub search: MusicSearch,
}

// 由持久的路由宿主创建，搜索页只消费它，避免每次进入搜索页重新触发 Suspense。
#[derive(Clone, Copy)]
pub struct MusicProviders(pub Resource<ServerFnResult<Vec<String>>>);

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
            MusicSearchState::Ready(result) => result.tracks.len() < result.total,
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
        self.state.set(MusicSearchState::Failed(message.into()));
    }

    // 首次搜索
    pub fn search(&mut self, request: SearchRequest) {
        // 使之前的请求失败
        let request_generation = self.invalidate_requests();
        // 取消 加载更多 状态
        self.loading_more.set(false);
        self.load_more_error.set(None);
        // 设置页面为loading状态
        self.state.set(MusicSearchState::Loading);
        // 发起异步请求
        let mut state = self.state;
        let generation = self.generation;
        spawn(async move {
            let next = match api::search_music(request).await {
                Ok(result) => MusicSearchState::Ready(result),
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
        let MusicSearchState::Ready(mut current) = self.state() else {
            return;
        };
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
        let generation = self.generation;
        spawn(async move {
            let result = api::search_music(request).await;
            if generation() != request_generation {
                return;
            }

            match result {
                Ok(next) => {
                    current.page = next.page;
                    current.total = next.total;
                    current.tracks.extend(next.tracks);
                    state.set(MusicSearchState::Ready(current));
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
        generation: use_signal(|| 0),
    }
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
