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

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state.set(MusicSearchState::Failed(message.into()));
    }

    pub fn search(&mut self, request: SearchRequest) {
        self.state.set(MusicSearchState::Loading);
        let mut state = self.state;
        spawn(async move {
            let next = match api::search_music(request).await {
                Ok(result) => MusicSearchState::Ready(result),
                Err(error) => MusicSearchState::Failed(view::server_error_message(error)),
            };
            state.set(next);
        });
    }

    pub fn load_more(&mut self) {
        let MusicSearchState::Ready(mut current) = self.state() else {
            return;
        };
        let request = SearchRequest {
            provider: current.provider.clone(),
            query: current.query.clone(),
            page: current.page + 1,
            limit: current.limit,
        };

        self.state.set(MusicSearchState::Loading);
        let mut state = self.state;
        spawn(async move {
            match api::search_music(request).await {
                Ok(next) => {
                    current.page = next.page;
                    current.total = next.total;
                    current.tracks.extend(next.tracks);
                    state.set(MusicSearchState::Ready(current));
                }
                Err(error) => {
                    state.set(MusicSearchState::Failed(view::server_error_message(error)));
                }
            }
        });
    }
}

pub fn use_music_search() -> MusicSearch {
    MusicSearch {
        state: use_signal(|| MusicSearchState::Idle),
    }
}
