use contracts::download::{Download, DownloadCategory, DownloadPage};
use dioxus::prelude::*;
use dioxus_icons::lucide::{LoaderCircle, RefreshCw};

use crate::{
    api::{self, PAGE_SIZE},
    components::{DownloadItem, Empty, ErrorCard, Loading},
    hooks::{CursorPagination, use_cursor_pagination, use_download_events},
    view,
};

const DOWNLOAD_TABS: [(DownloadCategory, &str); 3] = [
    (DownloadCategory::Active, "进行中"),
    (DownloadCategory::Attention, "待处理"),
    (DownloadCategory::History, "历史记录"),
];

#[component]
pub fn DownloadsPage() -> Element {
    let initial_resource =
        use_server_future(move || api::list_downloads(DownloadCategory::Active, None, PAGE_SIZE))?;
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
    let mut page = use_signal(move || initial_page);
    let mut category = use_signal(|| DownloadCategory::Active);
    // 加载更多获得的额外数据
    let mut pagination = use_cursor_pagination::<Download>();
    // 后台刷新状态
    let mut refreshing = use_signal(|| false);
    let mut refresh_error = use_signal(|| None::<String>);
    // 防止旧刷新请求覆盖新请求
    let mut refresh_generation = use_signal(|| 0_u64);
    let reload = Callback::new(move |requested_category: DownloadCategory| {
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
        let category = category;

        spawn(async move {
            let result = api::list_downloads(requested_category, None, PAGE_SIZE).await;

            // 只允许当前分类的最后一次刷新修改状态
            if generation() != request_generation || category() != requested_category {
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
    let refresh = Callback::new(move |_: ()| reload.call(category()));
    let select_category = Callback::new(move |next_category: DownloadCategory| {
        if category() == next_category {
            return;
        }

        category.set(next_category);
        pagination.reset();
        let attention_count = page.read().attention_count;
        page.set(DownloadPage {
            downloads: Vec::new(),
            next_before_id: None,
            attention_count,
        });
        reload.call(next_category);
    });

    let sync_download = Callback::new(move |_: i64| refresh.call(()));

    // progress 事件直接更新内存；低频的 changed 事件刷新当前分类，跨分类状态自然保持一致。
    let download_events = use_download_events(refresh, sync_download);
    let sync_if_disconnected = Callback::new(move |id| {
        // 正常连接时服务端必定发送Changed；断线时由操作完成回调兜底同步。
        if !download_events.connected() {
            sync_download.call(id);
        }
    });
    let current_page = page.read();
    let current_category = category();
    let attention_count = current_page.attention_count;
    let initial_cursor = current_page.next_before_id;
    let extra_downloads = pagination.items();
    let downloads_empty = current_page.downloads.is_empty() && extra_downloads.is_empty();
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

          nav {
            class: "mt-5 inline-flex w-fit rounded-lg bg-muted p-1",
            role: "tablist",
            aria_label: "下载任务分类",
            for (tab_category, label) in DOWNLOAD_TABS {
              button {
                class: "inline-flex h-8 items-center gap-2 rounded-md px-3",
                class: "text-sm font-medium transition-colors",
                class: if current_category == tab_category {
                    "bg-background text-foreground shadow-sm"
                } else {
                    "text-muted-foreground hover:text-foreground"
                },
                r#type: "button",
                role: "tab",
                aria_selected: current_category == tab_category,
                onclick: move |_| select_category.call(tab_category),
                "{label}"
                if tab_category == DownloadCategory::Attention && attention_count > 0 {
                  span {
                    class: "inline-flex h-5 min-w-5 items-center justify-center rounded-full",
                    class: "bg-destructive px-1.5 text-[11px] font-semibold leading-none text-white",
                    "{attention_badge(attention_count)}"
                  }
                }
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
                title: "更新下载任务失败",
                description: message,
                retry: move |_| refresh.call(()),
              }
            }
          }

          if downloads_empty {
            if refreshing() {
              Loading { message: "正在加载下载任务..." }
            } else {
              Empty {
                title: empty_title(current_category),
                description: empty_description(current_category),
              }
            }
          } else {
            // 只有任务列表滚动
            div { class: "min-h-0 flex-1 overflow-y-auto overscroll-contain pr-1",
              div { class: "divide-y divide-border",
                for download in current_page.downloads.iter().cloned().chain(extra_downloads) {
                  DownloadItem {
                    key: "{download.id}",
                    progress: download_events.progress(download.id),
                    download,
                    on_changed: sync_if_disconnected,
                  }
                }
              }

              // 加载更多失败时不再渲染onvisible哨兵，避免哨兵持续可见导致自动无限重试
              if let Some(message) = pagination_error {
                div { class: "py-4",
                  ErrorCard {
                    title: "加载更多失败",
                    description: message,
                    retry: move |_| {
                        load_more(current_category, pagination, initial_cursor)
                    },
                  }
                }
              } else if has_more {
                div {
                  class: "flex min-h-20 items-center justify-center py-4",
                  onvisible: move |event| {
                      if event.data().is_intersecting().unwrap_or(false) {
                          load_more(current_category, pagination, initial_cursor);
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
fn load_more(
    category: DownloadCategory,
    mut pagination: CursorPagination<Download>,
    initial_cursor: Option<i64>,
) {
    let Some(request) = pagination.try_start_loading(initial_cursor) else {
        return;
    };

    spawn(async move {
        match api::list_downloads(category, Some(request.cursor()), PAGE_SIZE).await {
            Ok(next_page) => {
                pagination.append(request, next_page.downloads, next_page.next_before_id);
            }
            Err(error) => {
                pagination.fail(request, view::server_error_message(error));
            }
        }
    });
}

fn empty_title(category: DownloadCategory) -> &'static str {
    match category {
        DownloadCategory::Active => "暂无进行中的任务",
        DownloadCategory::Attention => "暂无待处理任务",
        DownloadCategory::History => "暂无历史记录",
    }
}

fn empty_description(category: DownloadCategory) -> &'static str {
    match category {
        DownloadCategory::Active => "从搜索页面添加歌曲后，任务会显示在这里。",
        DownloadCategory::Attention => "失败或暂停的任务会显示在这里。",
        DownloadCategory::History => "已完成或已取消的任务会显示在这里。",
    }
}

fn attention_badge(count: i64) -> String {
    if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    }
}
