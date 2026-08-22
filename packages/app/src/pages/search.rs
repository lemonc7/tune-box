use dioxus::prelude::*;
use dioxus_icons::lucide::{CloudDownload, LoaderCircle, Music2, Play, Search};
use shared::{
    music::Track,
    request::{CreateDownloadRequest, SearchRequest},
};

use crate::{
    api,
    components::*,
    hooks::{MusicSearchSession, MusicSearchState},
    view,
};

#[component]
pub fn SearchPage() -> Element {
    let providers_resource = use_server_future(api::list_providers)?;
    let providers = match providers_resource() {
        Some(Ok(providers)) => providers,
        Some(Err(error)) => {
            return rsx! {
              ErrorBanner { message: view::server_error_message(error) }
            };
        }
        None => Vec::new(),
    };

    let session = use_context::<MusicSearchSession>();
    let mut provider = session.provider;
    let mut query = session.query;
    let mut search = session.search;
    let mut provider_initializer = provider;
    use_effect(move || {
        if provider_initializer.peek().is_none()
            && let Some(Ok(providers)) = providers_resource()
        {
            provider_initializer.set(providers.first().cloned());
        }
    });
    let run_search = use_callback(move |()| {
        let Some(provider) = provider() else {
            search.fail("当前没有可用的音乐源");
            return;
        };
        let query = query().trim().to_owned();
        if query.is_empty() {
            search.fail("请输入搜索关键词");
            return;
        }

        search.search(SearchRequest {
            provider,
            query,
            page: 1,
            limit: api::PAGE_SIZE,
        });
    });

    rsx! {
      document::Title { "搜索音乐 - Tune Box" }
      div { class: "flex h-full min-h-0 flex-col gap-4",
        section { class: "relative z-10 shrink-0 overflow-visible rounded-2xl border border-white/70 bg-white/90 p-4 shadow-lg shadow-zinc-900/5 backdrop-blur sm:p-5",
          div { class: "flex items-center gap-3",
            span { class: "flex size-9 shrink-0 items-center justify-center rounded-xl bg-indigo-50 text-indigo-600 ring-1 ring-indigo-100",
              Search { size: 17 }
            }
            div { class: "min-w-0",
              h1 { class: "text-xl font-semibold tracking-tight text-zinc-950", "搜索音乐" }
              p { class: "truncate text-xs text-zinc-500 sm:text-sm", "聚合音乐源搜索歌曲、歌手和专辑" }
            }
          }
          form {
              class: "mt-4 grid gap-3 sm:grid-cols-[13rem_minmax(0,1fr)]",
              onsubmit: move |event| {
                  event.prevent_default();
                  run_search.call(());
              },
                label { class: "grid min-w-0 gap-1.5",
                  span { class: "text-xs font-medium text-zinc-500",
                    "音乐源"
                  }
                  Select::<String> {
                    class: "w-full",
                    value: Some(provider.into()),
                    disabled: providers.is_empty(),
                    on_value_change: move |value| provider.set(value),
                    for (index, name) in providers.iter().enumerate() {
                      SelectOption::<String> {
                        key: "{name}",
                        index,
                        value: name.clone(),
                        text_value: name.clone(),
                        "{name}"
                      }
                    }
                  }
                }
                label { class: "grid min-w-0 gap-1.5",
                  span { class: "text-xs font-medium text-zinc-500",
                    "关键词"
                  }
                  div { class: "flex gap-2",
                    div { class: "relative min-w-0 flex-1",
                      Search {
                        class: "pointer-events-none absolute left-3 top-1/2 z-10 -translate-y-1/2 text-zinc-400",
                        size: 17,
                      }
                      Input {
                        class: "h-9 w-full pl-10",
                        r#type: "search",
                        value: "{query}",
                        required: true,
                        autocomplete: "off",
                        placeholder: "歌曲、歌手或专辑",
                        oninput: move |event: FormEvent| query.set(event.value()),
                      }
                    }
                    Button {
                      class: "shrink-0",
                      size: ButtonSize::Default,
                      r#type: "button",
                      disabled: search.loading() || providers.is_empty(),
                      onclick: move |_| run_search.call(()),
                      if search.loading() {
                        LoaderCircle {
                          class: "animate-spin",
                          size: 16,
                        }
                        "搜索中"
                      } else {
                        Search { size: 16 }
                        "搜索"
                      }
                    }
                  }
                }
          }
        }

        section {
          class: "flex min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-zinc-200/80 bg-white shadow-lg shadow-zinc-900/[0.035]",
          aria_live: "polite",
          match search.state() {
              MusicSearchState::Idle => rsx! {
                div { class: "flex min-h-0 flex-1 items-center justify-center p-4",
                  EmptyState { message: "请输入关键词开始搜索".to_owned() }
                }
              },
              MusicSearchState::Loading => rsx! {
                div { class: "flex min-h-0 flex-1 flex-col items-center justify-center px-8 text-sm text-zinc-500",
                  LoaderCircle { class: "mb-4 animate-spin text-indigo-500", size: 24 }
                  "正在搜索音乐…"
                }
              },
              MusicSearchState::Failed(error) => rsx! {
                div { class: "p-4", ErrorBanner { message: error } }
              },
              MusicSearchState::Ready(search_result) if search_result.tracks.is_empty() => {
                  rsx! {
                    div { class: "flex min-h-0 flex-1 items-center justify-center p-4",
                      EmptyState { message: "没有找到符合条件的歌曲".to_owned() }
                    }
                  }
              }
              MusicSearchState::Ready(search_result) => {
                  let has_more = search_result.page * search_result.limit
                      < search_result.total as i64;
                  rsx! {
                    div { class: "flex items-center justify-between gap-3 border-b border-zinc-100 px-5 py-4 sm:px-6",
                      div { class: "flex items-center gap-3",
                        span { class: "flex size-9 items-center justify-center rounded-xl bg-indigo-50 text-indigo-600",
                          Music2 { size: 17 }
                        }
                        div {
                          h2 { class: "font-semibold tracking-tight text-zinc-950", "搜索结果" }
                          p { class: "text-xs text-zinc-400", "选择试听或加入下载队列" }
                        }
                      }
                      span { class: "rounded-full bg-zinc-100 px-2.5 py-1 text-xs font-medium text-zinc-500",
                        "共 {search_result.total} 条"
                      }
                    }
                    ScrollArea {
                      class: "min-h-0 flex-1",
                      tabindex: "0",
                      aria_label: "搜索结果列表",
                      div { class: "space-y-3 p-4 sm:p-5",
                        for track in search_result.tracks {
                          TrackCard { key: "{track.provider}:{track.id}", track }
                        }
                        if let Some(error) = search.load_more_error() {
                          ErrorBanner { message: error }
                        }
                        if has_more {
                          div { class: "py-2",
                            InfiniteScrollTrigger {
                              key: "search-load-more-{search_result.page}",
                              loading: search.loading_more(),
                              failed: search.load_more_error().is_some(),
                              on_load_more: move |_| search.load_more(),
                            }
                          }
                        } else {
                          p { class: "py-2 text-center text-xs text-muted-foreground", "已经到底了" }
                        }
                      }
                    }
                  }
              }
          }
        }
      }
    }
}

#[component]
fn TrackCard(track: Track) -> Element {
    let mut player = use_context::<Signal<Option<PreviewTrack>>>();
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
        .map_err(|error| std::io::Error::other(view::server_error_message(error)))
    });

    let preview_track = track.clone();
    let download_track = track.clone();
    let artists = view::artists(&track.artists);

    rsx! {
      Card { class: "group transition-colors hover:border-indigo-200 hover:bg-indigo-50/20 hover:shadow-md",
        CardContent { class: "flex flex-col gap-4 sm:flex-row sm:items-center",
          if track.cover_url.is_empty() {
            div {
              class: "flex size-14 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-zinc-100 to-zinc-200 text-zinc-400 ring-1 ring-zinc-200",
              aria_hidden: "true",
              Music2 { size: 20 }
            }
          } else {
            img {
              class: "size-14 shrink-0 rounded-xl bg-zinc-100 object-cover shadow-sm ring-1 ring-zinc-200/80",
              src: "{track.cover_url}",
              alt: "",
              loading: "lazy",
            }
          }
          div { class: "min-w-0 flex-1",
            CardTitle { class: "truncate text-sm text-zinc-950", "{track.name}" }
            CardDescription { class: "mt-1 flex items-center gap-2 overflow-hidden text-xs",
              span { class: "truncate text-zinc-600", "{artists}" }
              if !track.album.is_empty() {
                span { aria_hidden: "true", "•" }
                span { class: "truncate", "{track.album}" }
              }
              if !track.date.is_empty() {
                span { aria_hidden: "true", "•" }
                span { "{track.date}" }
              }
            }
            div { class: "mt-3 flex flex-wrap items-center gap-2 sm:opacity-70 sm:transition sm:group-hover:opacity-100",
              Button {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                r#type: "button",
                onclick: move |_| {
                    player
                        .set(
                            Some(PreviewTrack {
                                url: view::preview_url(&preview_track.provider, &preview_track.id),
                                title: preview_track.name.clone(),
                                artists: view::artists(&preview_track.artists),
                                cover_url: preview_track.cover_url.clone(),
                            }),
                        );
                },
                Play { size: 14 }
                "试听"
              }
              Button {
                variant: ButtonVariant::Secondary,
                size: ButtonSize::Sm,
                r#type: "button",
                disabled: create_download.pending(),
                onclick: move |_| {
                    create_download.call(download_track.clone());
                },
                if create_download.pending() {
                  LoaderCircle { class: "animate-spin", size: 14 }
                  "处理中"
                } else {
                  CloudDownload { size: 14 }
                  "下载"
                }
              }
            }
          }
          div { class: "shrink-0 sm:max-w-52",
            match create_download.value() {
                Some(Ok(message)) => rsx! {
                  NoticeBanner { message: message() }
                },
                Some(Err(error)) => rsx! {
                  ErrorBanner { message: error.to_string() }
                },
                None => rsx! {},
            }
          }
        }
      }
    }
}
