use dioxus::prelude::*;

struct CursorPage<T> {
    extra_items: Vec<T>,
    next_cursor: Option<i64>,
    loaded: bool,
    loading: bool,
    error: Option<String>,
    generation: u64,
}

impl<T> Default for CursorPage<T> {
    fn default() -> Self {
        Self {
            extra_items: Vec::new(),
            next_cursor: None,
            loaded: false,
            loading: false,
            error: None,
            generation: 0,
        }
    }
}

pub struct CursorPagination<T: 'static> {
    state: Signal<CursorPage<T>>,
}

impl<T> Clone for CursorPagination<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for CursorPagination<T> {}

impl<T: 'static> CursorPagination<T> {
    pub fn items(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.state.read().extra_items.clone()
    }

    pub fn next_cursor(&self, initial: Option<i64>) -> Option<i64> {
        let state = self.state.read();
        if state.loaded {
            state.next_cursor
        } else {
            initial
        }
    }

    pub fn loading(&self) -> bool {
        self.state.read().loading
    }

    pub fn error(&self) -> Option<String> {
        self.state.read().error.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.state.read().extra_items.is_empty()
    }

    pub fn reset(&mut self) {
        let generation = self.state.read().generation.wrapping_add(1);
        self.state.set(CursorPage {
            generation,
            ..CursorPage::default()
        });
    }

    pub fn start_loading(&mut self) -> u64 {
        let mut state = self.state.write();
        state.loading = true;
        state.error = None;
        state.generation
    }

    pub fn append(
        &mut self,
        generation: u64,
        items: impl IntoIterator<Item = T>,
        next_cursor: Option<i64>,
    ) {
        let mut state = self.state.write();
        if state.generation != generation {
            return;
        }

        state.extra_items.extend(items);
        state.next_cursor = next_cursor;
        state.loaded = true;
        state.loading = false;
    }

    pub fn fail(&mut self, generation: u64, message: String) {
        let mut state = self.state.write();
        if state.generation != generation {
            return;
        }

        state.loading = false;
        state.error = Some(message);
    }
}

pub fn use_cursor_pagination<T: 'static>() -> CursorPagination<T> {
    CursorPagination {
        state: use_signal(CursorPage::default),
    }
}
