use dioxus::prelude::*;

struct CursorPage<T> {
    // 保存通过加载更多获取的数据
    extra_items: Vec<T>,
    // hook最近一次加载成功后，服务器返回的下一页游标
    next_cursor: Option<i64>,
    // 是否成功加载过至少一页数据
    has_loaded: bool,
    // 是否正在加载下一页
    loading: bool,
    error: Option<String>,
    // 请求版本号，防止旧异步请求覆盖新状态
    request_id: u64,
}

impl<T> Default for CursorPage<T> {
    fn default() -> Self {
        Self {
            extra_items: Vec::new(),
            next_cursor: None,
            has_loaded: false,
            loading: false,
            error: None,
            request_id: 0,
        }
    }
}

/// 一次游标分页请求的快照，用于确保只有当前请求能修改分页状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorRequest {
    cursor: i64,
    request_id: u64,
}

impl CursorRequest {
    pub fn cursor(self) -> i64 {
        self.cursor
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
        if state.has_loaded {
            state.next_cursor
        } else {
            initial
        }
    }

    pub fn has_more(&self, initial: Option<i64>) -> bool {
        self.next_cursor(initial).is_some()
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
        let mut state = self.state.write();
        let request_id = state.request_id.wrapping_add(1);
        *state = CursorPage {
            request_id,
            ..CursorPage::default()
        };
    }

    // 尝试开始加载下一页。
    // 正在加载或没有下一页时返回 `None`；否则返回包含游标和请求版本的令牌。
    pub fn try_start_loading(&mut self, initial: Option<i64>) -> Option<CursorRequest> {
        let mut state = self.state.write();

        // 已经在加载
        if state.loading {
            return None;
        }

        // 如果已经到底，返回None(?)
        let cursor = if state.has_loaded {
            state.next_cursor
        } else {
            initial
        }?;

        // 可以加载，更新状态
        state.request_id = state.request_id.wrapping_add(1);
        state.loading = true;
        state.error = None;

        // 返回请求令牌
        Some(CursorRequest {
            cursor,
            request_id: state.request_id,
        })
    }

    // 请求成功时，追加列表
    pub fn append(
        &mut self,
        request: CursorRequest,
        items: impl IntoIterator<Item = T>,
        next_cursor: Option<i64>,
    ) {
        let mut state = self.state.write();
        if state.request_id != request.request_id || !state.loading {
            return;
        }

        state.extra_items.extend(items);
        state.next_cursor = next_cursor;
        state.has_loaded = true;
        state.loading = false;
    }

    // 请求失败，更新状态
    pub fn fail(&mut self, request: CursorRequest, message: String) {
        let mut state = self.state.write();
        if state.request_id != request.request_id || !state.loading {
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
