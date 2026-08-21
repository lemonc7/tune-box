use dioxus::prelude::*;

struct CursorPage<T> {
    extra_items: Vec<T>,
    next_cursor: Option<i64>,
    loaded: bool,
    loading: bool,
    error: Option<String>,
}

impl<T> Default for CursorPage<T> {
    fn default() -> Self {
        Self {
            extra_items: Vec::new(),
            next_cursor: None,
            loaded: false,
            loading: false,
            error: None,
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
        self.state.set(CursorPage::default());
    }

    pub fn start_loading(&mut self) {
        let mut state = self.state.write();
        state.loading = true;
        state.error = None;
    }

    pub fn append(&mut self, items: impl IntoIterator<Item = T>, next_cursor: Option<i64>) {
        let mut state = self.state.write();
        state.extra_items.extend(items);
        state.next_cursor = next_cursor;
        state.loaded = true;
        state.loading = false;
    }

    pub fn fail(&mut self, message: String) {
        let mut state = self.state.write();
        state.loading = false;
        state.error = Some(message);
    }
}

pub fn use_cursor_pagination<T: 'static>() -> CursorPagination<T> {
    CursorPagination {
        state: use_signal(CursorPage::default),
    }
}
