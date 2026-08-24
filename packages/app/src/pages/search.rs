use contracts::{
    music::Track,
    request::{CreateDownloadRequest, SearchRequest},
};
use dioxus::html::geometry::PixelsVector2D;
use dioxus::{CapturedError, prelude::*};
use dioxus_icons::lucide::{ChevronDown, Download, LoaderCircle, Music2, Play, Search};

use crate::{
    api::{self, PAGE_SIZE},
    components::{Empty, ErrorCard, Loading},
    hooks::{MusicSearchSession, MusicSearchState, use_music_search_session, use_player},
    view,
};

#[component]
pub fn SearchPage() -> Element {
    let mut provider_resource = use_server_future(api::list_providers)?;
    let MusicSearchSession {
        mut provider,
        mut query,
        mut list_scroll_top,
        mut search,
    } = use_music_search_session();

    let providers = match provider_resource() {
        Some(Ok(providers)) => providers,
        Some(Err(error)) => {
            return rsx! {
              ErrorCard {
                title: "搜索服务加载失败",
                description: view::server_error_message(error),
                retry: move |_| provider_resource.restart(),
              }
            };
        }
        None => {
            return rsx! {
              Loading { message: "正在加载搜索服务…" }
            };
        }
    };

    // 不在渲染期间写Signal；已选来源失效时也自动回退到当前第一个来源。
    let selected_provider = provider()
        .filter(|selected| providers.contains(selected))
        .or_else(|| providers.first().cloned());
    let submit_provider = selected_provider.clone();
    let retry_provider = selected_provider.clone();
    let providers_empty = providers.is_empty();
    let search_state = search.state();

    rsx! {
      section { class: "flex h-full min-h-0 flex-1 flex-col",
        header { class: "shrink-0 border-b border-border pb-5",
          h1 { class: "text-2xl font-semibold tracking-tight text-foreground",
            "搜索音乐"
          }
          p { class: "mt-1 text-sm text-muted-foreground",
            "搜索歌曲并添加到下载队列。"
          }

          form {
            class: "mt-5 flex flex-col gap-3 sm:flex-row",
            onsubmit: move |event| {
                event.prevent_default();
                let Some(provider) = submit_provider.clone() else {
                    search.fail("请选择音乐来源");
                    return;
                };
                let query = query().trim().to_owned();
                if query.is_empty() {
                    search.fail("请输入搜索内容");
                    return;
                }
                list_scroll_top.set(0.0);
                search
                    .search(SearchRequest {
                        provider,
                        query,
                        page: 1,
                        limit: PAGE_SIZE,
                    });
            },

            label { class: "sr-only", r#for: "search-provider", "音乐来源" }
            div { class: "relative w-full shrink-0 sm:w-auto",
              select {
                id: "search-provider",
                class: "h-10 w-full min-w-32 appearance-none rounded-md border border-input",
                class: "bg-background py-0 pl-3 pr-9 text-sm text-foreground outline-none",
                class: "focus-visible:ring-2 focus-visible:ring-ring/50",
                value: selected_provider.unwrap_or_default(),
                disabled: providers_empty || search.loading(),
                onchange: move |event| provider.set(Some(event.value())),
                for provider in providers {
                  option { value: provider.clone(), "{provider}" }
                }
              }
              span {
                class: "pointer-events-none absolute right-3 top-1/2 inline-flex -translate-y-1/2",
                class: "text-muted-foreground",
                aria_hidden: true,
                ChevronDown { size: 16 }
              }
            }

            label { class: "sr-only", r#for: "search-query", "搜索歌曲" }
            input {
              id: "search-query",
              class: "h-10 min-w-0 flex-1 rounded-md border border-input bg-background px-3",
              class: "text-sm text-foreground outline-none placeholder:text-muted-foreground",
              class: "focus-visible:ring-2 focus-visible:ring-ring/50",
              r#type: "search",
              name: "query",
              placeholder: "输入歌曲名、歌手或专辑",
              autocomplete: "off",
              value: query,
              disabled: search.loading(),
              oninput: move |event| query.set(event.value()),
            }

            button {
              class: "inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-md",
              class: "bg-primary px-4 text-sm font-medium text-primary-foreground transition-colors",
              class: "hover:bg-primary/80 focus-visible:outline-none focus-visible:ring-2",
              class: "focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50",
              r#type: "submit",
              disabled: providers_empty || search.loading(),
              if search.loading() {
                span {
                  class: "inline-flex animate-spin",
                  aria_hidden: true,
                  LoaderCircle { size: 16 }
                }
                "搜索中…"
              } else {
                span { class: "inline-flex", aria_hidden: true,
                  Search { size: 16 }
                }
                "搜索"
              }
            }
          }
        }

        div { class: "flex min-h-0 flex-1 flex-col pt-5",
          match search_state {
              MusicSearchState::Idle => rsx! {
                Empty {
                  title: "开始搜索",
                  description: "输入歌曲名、歌手或专辑，搜索结果会显示在这里。",
                }
              },
              MusicSearchState::Loading => rsx! {
                Loading { message: "正在搜索音乐…" }
              },
              MusicSearchState::Failed(message) => rsx! {
                ErrorCard {
                  title: "搜索失败",
                  description: message,
                  retry: move |_| {
                      let Some(provider) = retry_provider.clone() else { return };
                      let query = query().trim().to_owned();
                      if query.is_empty() {
                          return;
                      }
                      search
                          .search(SearchRequest {
                              provider,
                              query,
                              page: 1,
                              limit: PAGE_SIZE,
                          });
                  },
                }
              },
              MusicSearchState::Ready(result) if result.tracks.is_empty() => rsx! {
                Empty {
                  title: "没有找到结果",
                  description: "可以尝试更换关键词或音乐来源。",
                }
              },
              MusicSearchState::Ready(result) => rsx! {
                div { class: "flex min-h-0 flex-1 flex-col",
                  div { class: "flex shrink-0 items-center justify-between gap-4 pb-2",
                    h2 { class: "text-sm font-medium text-foreground", "搜索结果" }
                    p { class: "text-xs text-muted-foreground", "共 {result.total} 首" }
                  }

                  // 搜索结果是页面中唯一可以滚动的区域。
                  div {
                    class: "min-h-0 flex-1 overflow-y-auto overscroll-contain pr-1",
                    onmounted: move |event| {
                        let offset = list_scroll_top();
                        spawn(async move {
                            let _ = event
                                .data()
                                .scroll(PixelsVector2D::new(0.0, offset), ScrollBehavior::Instant)
                                .await;
                        });
                    },
                    onscroll: move |event| {
                        list_scroll_top.set(event.data().scroll_top());
                    },
                    div { class: "divide-y divide-border",
                      for track in result.tracks.iter().cloned() {
                        TrackItem { key: "{track.provider}:{track.id}", track }
                      }
                    }

                    // 加载下一页失败时保留现有结果，只显示局部错误。
                    if let Some(message) = search.load_more_error() {
                      div { class: "py-4",
                        ErrorCard {
                          title: "加载更多失败",
                          description: message,
                          retry: move |_| search.load_more(),
                        }
                      }
                    } else if search.has_more() {
                      div {
                        class: "flex min-h-20 items-center justify-center py-4",
                        // 哨兵进入列表可视区域后加载下一页。
                        onvisible: move |event| {
                            if event.data().is_intersecting().unwrap_or(false) {
                                search.load_more();
                            }
                        },
                        if search.loading_more() {
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
                      p { class: "py-5 text-center text-xs text-muted-foreground", "已经到底了" }
                    }
                  }
                }
              },
          }
        }
      }
    }
}

#[component]
fn TrackItem(track: Track) -> Element {
    let mut player = use_player();
    let mut create_download = use_action(move |track: Track| async move {
        api::create_download(CreateDownloadRequest {
            provider: track.provider,
            track_id: track.id,
            name: track.name,
            artists: track.artists,
            album: track.album,
            album_artist: track.album_artist,
            date: track.date,
            cover_url: track.cover_url,
        })
        .await
        .map_err(|error| CapturedError::msg(view::server_error_message(error)))
    });

    let preview_track = track.clone();
    let download_track = track.clone();

    let artists = view::artists(&track.artists);
    let details = [artists, track.album.clone(), track.date.clone()]
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join(" • ");
    let downloading = create_download.pending();

    rsx! {
      article {
        class: "flex flex-col gap-4 py-4",
        class: "sm:flex-row sm:items-center",

        // 封面
        if track.cover_url.is_empty() {
          div {
            class: "flex size-14 shrink-0 items-center justify-center",
            class: "rounded-lg bg-muted text-muted-foreground",
            aria_hidden: true,
            Music2 { size: 20 }
          }
        } else {
          img {
            class: "size-14 shrink-0 rounded-lg bg-muted object-cover",
            src: track.cover_url.clone(),
            alt: "",
            loading: "lazy",
          }
        }

        // 歌曲信息和操作
        div { class: "min-w-0 flex-1",
          // 歌名
          h3 { class: "truncate text-sm font-medium text-foreground", "{track.name}" }

          // 细节信息
          if !details.is_empty() {
            p { class: "mt-1 truncate text-xs text-muted-foreground",
              "{details}"
            }
          }

          div { class: "mt-3 flex flex-wrap items-center gap-2",
            // 试听
            button {
              class: "inline-flex h-8 items-center justify-center gap-1.5",
              class: "rounded-md border border-border bg-background",
              class: "px-3 text-xs font-medium text-foreground",
              class: "shadow-sm transition-colors hover:bg-muted",
              class: "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
              r#type: "button",
              onclick: move |_| player.play(preview_track.clone()),
              span {
                class: "inline-flex shrink-0",
                aria_hidden: true,
                Play { size: 14 }
              }
              "试听"
            }
            // 下载
            button {
              class: "inline-flex h-8 items-center justify-center gap-1.5",
              class: "rounded-md bg-primary px-3 text-xs font-medium",
              class: "text-primary-foreground shadow-sm transition-colors",
              class: "hover:bg-primary/80 focus-visible:outline-none",
              class: "focus-visible:ring-2 focus-visible:ring-ring/50",
              class: "disabled:pointer-events-none disabled:opacity-50",
              r#type: "button",
              disabled: downloading,
              onclick: move |_| create_download.call(download_track.clone()),
              if downloading {
                span {
                  class: "inline-flex shrink-0 animate-spin",
                  aria_hidden: true,
                  LoaderCircle { size: 14 }
                }
                "处理中…"
              } else {
                span {
                  class: "inline-flex shrink-0",
                  aria_hidden: true,
                  Download { size: 14 }
                }
                "下载"
              }
            }
          }

          // 下载操作结果
          match create_download.value() {
              Some(Ok(message)) => rsx! {
                p { class: "mt-3 text-xs text-muted-foreground", role: "status", {message()} }
              },
              Some(Err(error)) => rsx! {
                p { class: "mt-3 text-xs text-destructive", role: "alert", "{error}" }
              },
              None => rsx! {},
          }
        }
      }

    }
}
