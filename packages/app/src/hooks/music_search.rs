use dioxus::prelude::*;
use shared::{music::MusicResult, request::SearchRequest};

use crate::{api, view};

#[derive(Clone, PartialEq, Eq)]
pub enum MusicSearchState {
    Idle,
    Loading,
    Ready(MusicResult),
    Failed(String),
}

pub struct MusicSearch {
    state: Signal<MusicSearchState>,
    loading_more: Signal<bool>,
    load_more_error: Signal<Option<String>>,
    generation: Signal<u64>,
}

#[derive(Clone, Copy)]
pub struct MusicSearchSession {
    pub provider: Signal<Option<String>>,
    pub query: Signal<String>,
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
        matches!(self.state(), MusicSearchState::Loading)
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

    pub fn search(&mut self, request: SearchRequest) {
        let request_generation = self.invalidate_requests();
        self.loading_more.set(false);
        self.load_more_error.set(None);
        self.state.set(MusicSearchState::Loading);
        let mut state = self.state;
        let generation = self.generation;
        spawn(async move {
            let next = match api::search_music(request).await {
                Ok(result) => MusicSearchState::Ready(result),
                Err(error) => MusicSearchState::Failed(view::server_error_message(error)),
            };
            if generation() == request_generation {
                state.set(next);
            }
        });
    }

    pub fn load_more(&mut self) {
        if self.loading_more() {
            return;
        }

        let MusicSearchState::Ready(mut current) = self.state() else {
            return;
        };
        let request = SearchRequest {
            provider: current.provider.clone(),
            query: current.query.clone(),
            page: current.page + 1,
            limit: current.limit,
        };

        self.loading_more.set(true);
        self.load_more_error.set(None);
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

pub fn use_music_search_session() -> MusicSearchSession {
    MusicSearchSession {
        provider: use_signal(|| None),
        query: use_signal(String::new),
        search: use_music_search(),
    }
}
