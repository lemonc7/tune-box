use dioxus::{CapturedError, prelude::*};
use dioxus_icons::lucide::{LoaderCircle, RefreshCw, Search};

use crate::{
    api::{self, PAGE_SIZE},
    components::{Empty, ErrorCard, Loading, SongItem},
    hooks::use_library_state,
    view,
};

#[component]
pub fn LibraryPage() -> Element {
    let mut initial_resource =
        use_server_future(move || api::load_library(String::new(), None, PAGE_SIZE))?;
    let initial_snapshot = match initial_resource() {
        Some(Ok(snapshot)) => snapshot,
        Some(Err(error)) => {
            return rsx! {
              ErrorCard {
                title: "曲库加载失败",
                description: view::server_error_message(error),
                retry: move |_| initial_resource.restart(),
              }
            };
        }
        None => return rsx! { Loading { message: "正在加载曲库…" } },
    };

    let mut library = use_library_state(initial_snapshot);
    let mut sync_library = use_action(move |_: ()| async move {
        let count = api::sync_library()
            .await
            .map_err(|error| CapturedError::msg(view::server_error_message(error)))?;
        library.refresh();
        Ok::<_, CapturedError>(count)
    });

    let snapshot = library.snapshot.read();
    let initial_cursor = snapshot.page.next_before_id;
    let extra_songs = library.pagination.items();
    let songs_empty = snapshot.page.songs.is_empty() && extra_songs.is_empty();
    let pagination_error = library.pagination.error();
    let load_error = library.error();
    let has_more = library.pagination.has_more(initial_cursor);
    let active_query = library.active_query();
    let syncing = sync_library.pending();

    rsx! {
      section { class: "flex h-full min-h-0 flex-1 flex-col",
        header { class: "shrink-0 border-b border-border pb-5",
          div { class: "flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between",
            div {
              h1 { class: "text-2xl font-semibold tracking-tight text-foreground", "本地曲库" }
              p { class: "mt-1 text-sm text-muted-foreground",
                "共 {snapshot.count} 首歌曲"
              }
            }

            div { class: "flex flex-wrap items-center gap-2",
              button {
                class: "inline-flex h-9 items-center justify-center gap-2 rounded-md",
                class: "border border-border bg-background px-3 text-sm font-medium",
                class: "text-foreground transition-colors hover:bg-muted",
                class: "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                class: "disabled:pointer-events-none disabled:opacity-50",
                r#type: "button",
                disabled: library.loading() || syncing,
                onclick: move |_| library.refresh(),
                RefreshCw { size: 15 }
                "刷新列表"
              }
              button {
                class: "inline-flex h-9 items-center justify-center gap-2 rounded-md",
                class: "bg-primary px-3 text-sm font-medium text-primary-foreground",
                class: "transition-colors hover:bg-primary/80",
                class: "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                class: "disabled:pointer-events-none disabled:opacity-50",
                r#type: "button",
                disabled: syncing || library.loading(),
                onclick: move |_| sync_library.call(()),
                if syncing {
                  LoaderCircle { size: 15, class: "animate-spin" }
                  "扫描中…"
                } else {
                  RefreshCw { size: 15 }
                  "扫描曲库"
                }
              }
            }
          }

          form {
            class: "mt-5 flex gap-2",
            onsubmit: move |event| {
                event.prevent_default();
                library.reload(library.draft_query());
            },
            label { class: "sr-only", r#for: "library-query", "搜索曲库" }
            input {
              id: "library-query",
              class: "h-10 min-w-0 flex-1 rounded-md border border-input bg-background px-3",
              class: "text-sm text-foreground outline-none placeholder:text-muted-foreground",
              class: "focus-visible:ring-2 focus-visible:ring-ring/50",
              r#type: "search",
              placeholder: "搜索歌曲或歌手",
              autocomplete: "off",
              value: library.draft_query,
              oninput: move |event| library.draft_query.set(event.value()),
            }
            button {
              class: "inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-md",
              class: "bg-primary px-4 text-sm font-medium text-primary-foreground",
              class: "transition-colors hover:bg-primary/80 focus-visible:outline-none",
              class: "focus-visible:ring-2 focus-visible:ring-ring/50",
              class: "disabled:pointer-events-none disabled:opacity-50",
              r#type: "submit",
              disabled: library.loading() || syncing,
              if library.loading() {
                LoaderCircle { size: 16, class: "animate-spin" }
              } else {
                Search { size: 16 }
              }
              "搜索"
            }
          }

          if let Some(Ok(count)) = sync_library.value() {
            p { class: "mt-3 text-xs text-muted-foreground", role: "status",
              "扫描完成，曲库中共有 {count} 首歌曲。"
            }
          } else if let Some(Err(error)) = sync_library.value() {
            p { class: "mt-3 text-xs text-destructive", role: "alert", "{error}" }
          }
        }

        div { class: "flex min-h-0 flex-1 flex-col pt-5",
          if let Some(message) = load_error.clone() {
            div { class: "shrink-0 pb-4",
              ErrorCard {
                title: "曲库更新失败",
                description: message,
                retry: move |_| library.refresh(),
              }
            }
          }

          if library.loading() {
            Loading { message: "正在读取曲库…" }
          } else if load_error.is_some() {
          } else if songs_empty {
            Empty {
              title: if active_query.is_empty() { "曲库为空" } else { "没有找到歌曲" },
              description: if active_query.is_empty() {
                  "点击“扫描曲库”读取下载目录中的音频文件。"
              } else {
                  "可以尝试更换关键词或清空搜索条件。"
              },
            }
          } else {
            div { class: "min-h-0 flex-1 overflow-y-auto overscroll-contain pr-1",
              div { class: "divide-y divide-border",
                for song in snapshot.page.songs.iter().cloned().chain(extra_songs) {
                  SongItem {
                    key: "{song.id}",
                    song,
                    on_deleted: move |id| library.remove_song(id),
                  }
                }
              }

              if let Some(message) = pagination_error {
                div { class: "py-4",
                  ErrorCard {
                    title: "加载更多失败",
                    description: message,
                    retry: move |_| library.load_more(initial_cursor),
                  }
                }
              } else if has_more {
                div {
                  class: "flex min-h-20 items-center justify-center py-4",
                  onvisible: move |event| {
                      if event.data().is_intersecting().unwrap_or(false) {
                          library.load_more(initial_cursor);
                      }
                  },
                  if library.pagination.loading() {
                    span {
                      class: "inline-flex items-center gap-2 text-sm text-muted-foreground",
                      role: "status",
                      LoaderCircle { size: 16, class: "animate-spin" }
                      "正在加载更多…"
                    }
                  }
                }
              } else {
                p { class: "py-5 text-center text-xs text-muted-foreground", "已经到底了" }
              }
            }
          }
        }
      }
    }
}
