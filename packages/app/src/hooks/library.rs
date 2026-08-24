use contracts::library::{LibrarySnapshot, Song};
use dioxus::prelude::*;

use crate::{
    api::{self, PAGE_SIZE},
    hooks::CursorPagination,
    view,
};

#[derive(Clone, Copy)]
pub struct LibraryState {
    pub draft_query: Signal<String>,
    pub active_query: Signal<String>,
    pub snapshot: Signal<LibrarySnapshot>,
    pub pagination: CursorPagination<Song>,
    pub loading: Signal<bool>,
    pub error: Signal<Option<String>>,
    generation: Signal<u64>,
}

impl LibraryState {
    pub fn draft_query(&self) -> String {
        (self.draft_query)()
    }

    pub fn active_query(&self) -> String {
        (self.active_query)()
    }

    pub fn loading(&self) -> bool {
        (self.loading)()
    }

    pub fn error(&self) -> Option<String> {
        self.error.read().clone()
    }

    pub fn reload(&mut self, query: String) {
        let query = query.trim().to_owned();
        let request_generation = self.generation.with_mut(|generation| {
            *generation = generation.wrapping_add(1);
            *generation
        });
        self.active_query.set(query.clone());
        self.pagination.reset();
        self.loading.set(true);
        self.error.set(None);

        let generation = self.generation;
        let mut snapshot = self.snapshot;
        let mut loading = self.loading;
        let mut error = self.error;
        spawn(async move {
            let result = api::load_library(query, None, PAGE_SIZE).await;
            if generation() != request_generation {
                return;
            }
            match result {
                Ok(next) => snapshot.set(next),
                Err(load_error) => {
                    error.set(Some(view::server_error_message(load_error)));
                }
            }
            loading.set(false);
        });
    }

    pub fn refresh(&mut self) {
        self.reload(self.active_query());
    }

    pub fn load_more(&mut self, initial_cursor: Option<i64>) {
        let Some(request) = self.pagination.try_start_loading(initial_cursor) else {
            return;
        };
        let query = self.active_query();
        let mut pagination = self.pagination;
        spawn(async move {
            match api::load_library(query, Some(request.cursor()), PAGE_SIZE).await {
                Ok(next) => pagination.append(request, next.page.songs, next.page.next_before_id),
                Err(load_error) => {
                    pagination.fail(request, view::server_error_message(load_error));
                }
            }
        });
    }

    pub fn remove_song(&mut self, id: i64) {
        let removed_from_first = self.snapshot.with_mut(|snapshot| {
            let original_len = snapshot.page.songs.len();
            snapshot.page.songs.retain(|song| song.id != id);
            snapshot.page.songs.len() != original_len
        });
        let removed_from_extra = self.pagination.remove_where(|song| song.id == id);
        if removed_from_first || removed_from_extra {
            self.snapshot
                .with_mut(|snapshot| snapshot.count = snapshot.count.saturating_sub(1));
        }
    }
}

pub fn use_library_state(initial_snapshot: LibrarySnapshot) -> LibraryState {
    LibraryState {
        draft_query: use_signal(String::new),
        active_query: use_signal(String::new),
        snapshot: use_signal(move || initial_snapshot),
        pagination: crate::hooks::use_cursor_pagination(),
        loading: use_signal(|| false),
        error: use_signal(|| None),
        generation: use_signal(|| 0),
    }
}
