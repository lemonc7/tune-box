use contracts::download::Download;
use dioxus::prelude::*;
use dioxus_icons::lucide::{LoaderCircle, RefreshCw};

use crate::{
    api::{self, PAGE_SIZE},
    components::{DownloadItem, Empty, ErrorCard, Loading},
    hooks::{CursorPagination, use_cursor_pagination, use_download_events},
    view,
};

#[component]
pub fn DownloadsPage() -> Element {
    let initial_resource = use_server_future(move || api::list_downloads(None, PAGE_SIZE))?;
    let initial_page = match initial_resource() {
        Some(Ok(page)) => page,
        Some(Err(error)) => {
            let mut initial_resource = initial_resource;
            return rsx! {
              ErrorCard {
                title: "下载任务加载失败",
                description: view::server_error_message(error),
                retry: move |_| initial_resource.restart(),
              }
            };
        }
        None => {
            return rsx! {
              Loading { message: "正在加载下载任务..." }
            };
        }
    };

    // 当前第一页，刷新时直接替换
    let page = use_signal(move || initial_page);
    // 加载更多获得的额外数据
    let pagination = use_cursor_pagination::<Download>();
    // 后台刷新状态
    let mut refreshing = use_signal(|| false);
    let mut refresh_error = use_signal(|| None::<String>);
    // 防止旧刷新请求覆盖新请求
    let mut refresh_generation = use_signal(|| 0_u64);
    let refresh = Callback::new(move |_: ()| {
        let request_generation = refresh_generation.with_mut(|generation| {
            *generation = generation.wrapping_add(1);
            *generation
        });
        refreshing.set(true);
        refresh_error.set(None);

        let generation = refresh_generation;
        let mut page = page;
        let mut pagination = pagination;
        let mut refreshing = refreshing;
        let mut refresh_error = refresh_error;

        spawn(async move {
            let result = api::list_downloads(None, PAGE_SIZE).await;

            // 只允许最后一次刷新修改状态
            if generation() != request_generation {
                return;
            }

            match result {
                Ok(next_page) => {
                    // 第一页变化后，之前加载的额外分页可能已经失效，因此成功刷新时一起重置分页
                    pagination.reset();
                    page.set(next_page);
                }
                Err(error) => refresh_error.set(Some(view::server_error_message(error))),
            }
            refreshing.set(false);
        });
    });

    // progress事件直接更新内存进度，changed事件触发后台refresh，从数据库读取最终状态
    let download_events = use_download_events(refresh);
    let current_page = page();
    let initial_cursor = current_page.next_before_id;
    let mut downloads = current_page.downloads;
    downloads.extend(pagination.items());
    let pagination_error = pagination.error();
    let has_more = pagination.has_more(initial_cursor);

    rsx! {
      section { class: "flex h-full min-h-0 flex-1 flex-col",
        header { class: "shrink-0 border-b border-border pb-5",
          div { class: "flex items-start justify-between gap-4",
            div {
              h1 { class: "text-2xl font-semibold tracking-tight text-foreground",
                "下载任务"
              }
              p { class: "mt-1 text-sm text-muted-foreground",
                "查看下载进度并管理任务"
              }
            }
            button {
              class: "inline-flex h-9 shrink-0 items-center",
              class: "justify-center gap-2 rounded-md",
              class: "border border-border bg-background px-3",
              class: "text-sm font-medium text-foreground",
              class: "transition-colors hover:bg-muted",
              class: "focus-visible:outline-none focus-visible:ring-2",
              class: "focus-visible:ring-ring/50",
              class: "disabled:pointer-events-none disabled:opacity-50",
              r#type: "button",
              disabled: refreshing(),
              onclick: move |_| refresh.call(()),
              if refreshing() {
                span {
                  class: "inline-flex animate-spin",
                  aria_hidden: true,
                  LoaderCircle { size: 15 }
                }
                "刷新中..."
              } else {
                span { class: "inline-flex", aria_hidden: true,
                  RefreshCw { size: 15 }
                }
                "刷新"
              }
            }
          }

          // SSE断开不是页面级错误，显示非阻塞状态即可
          if let Some(message) = download_events.error() {
            p {
              class: "mt-3 text-xs text-muted-foreground",
              role: "status",
              "{message}"
            }
          }
        }

        // 页面剩余区域
        div { class: "flex min-h-0 flex-1 flex-col pt-5",
          if let Some(message) = refresh_error() {
            div { class: "shrink-0 pb-4",
              ErrorCard {
                title: "刷新下载任务失败",
                description: message,
                retry: move |_| refresh.call(()),
              }
            }
          }

          if downloads.is_empty() {
            Empty {
              title: "暂无下载任务",
              description: "从搜索页面添加歌曲后，任务会显示在这里。",
            }
          } else {
            // 只有任务列表滚动
            div { class: "min-h-0 flex-1 overflow-y-auto overscroll-contain pr-1",
              div { class: "divide-y divide-border",
                for download in downloads {
                  DownloadItem {
                    key: "{download.id}",
                    progress: download_events.progress(download.id),
                    download,
                    refresh,
                  }
                }
              }

              // 加载更多失败时不再渲染onvisible哨兵，避免哨兵持续可见导致自动无限重试
              if let Some(message) = pagination_error {
                div { class: "py-4",
                  ErrorCard {
                    title: "加载更多失败",
                    description: message,
                    retry: move |_| load_more(pagination, initial_cursor),
                  }
                }
              } else if has_more {
                div {
                  class: "flex min-h-20 items-center justify-center py-4",
                  onvisible: move |event| {
                      if event.data().is_intersecting().unwrap_or(false) {
                          load_more(pagination, initial_cursor);
                      }
                  },
                  if pagination.loading() {
                    span {
                      class: "inline-flex items-center gap-2 text-sm text-muted-foreground",
                      role: "status",
                      span {
                        class: "inline-flex animate-spin",
                        aria_hidden: true,
                        LoaderCircle { size: 16 }
                      }
                      "正在加载更多…"
                    }
                  }
                }
              } else {
                p { class: "py-5 text-center text-xs text-muted-foreground",
                  "已经到底了"
                }
              }
            }
          }
        }
      }
    }
}

/// 请求下一页。`CursorPagination` 负责阻止重复请求，并忽略刷新前发出的旧请求。
fn load_more(mut pagination: CursorPagination<Download>, initial_cursor: Option<i64>) {
    let Some(request) = pagination.try_start_loading(initial_cursor) else {
        return;
    };

    spawn(async move {
        match api::list_downloads(Some(request.cursor()), PAGE_SIZE).await {
            Ok(next_page) => {
                pagination.append(request, next_page.downloads, next_page.next_before_id);
            }
            Err(error) => {
                pagination.fail(request, view::server_error_message(error));
            }
        }
    });
}
