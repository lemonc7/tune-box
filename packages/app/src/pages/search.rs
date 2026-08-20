use dioxus::prelude::*;
use shared::{music::Track, request::SearchRequest};

use crate::{
    api,
    components::{EmptyState, ErrorBanner, NoticeBanner, PreviewTrack},
    view,
};

#[component]
pub fn SearchPage() -> Element {
    let providers_resource = use_server_future(api::list_providers)?;
    let providers = match providers_resource() {
        Some(Ok(providers)) => providers,
        Some(Err(error)) => {
            return rsx! { ErrorBanner { message: view::server_error_message(error) } };
        }
        None => Vec::new(),
    };

    let initial_provider = providers.first().cloned().unwrap_or_default();
    let mut provider = use_signal(|| initial_provider);
    let mut query = use_signal(String::new);
    let mut searching = use_signal(|| false);
    let mut result = use_signal(|| None::<Result<shared::music::MusicResult, String>>);

    rsx! {
        document::Title { "搜索音乐 - Tune Box" }
        div { class: "space-y-6",
            section { class: "rounded-lg border border-zinc-200 bg-white p-6 shadow-sm sm:p-8",
                header { class: "mb-6",
                    div {
                        h1 { class: "text-2xl font-semibold tracking-tight", "搜索音乐" }
                        p { class: "mt-1 text-sm text-zinc-500", "按歌曲、歌手或专辑搜索，并添加 FLAC 下载任务。" }
                    }
                }

                form {
                    class: "grid grid-cols-1 gap-4 sm:grid-cols-[12rem_minmax(0,1fr)]",
                    onsubmit: move |event| {
                        event.prevent_default();
                        let provider = provider();
                        let query = query().trim().to_owned();
                        if provider.is_empty() || query.is_empty() {
                            result.set(Some(Err("请选择音乐源并输入搜索关键词".to_owned())));
                            return;
                        }

                        searching.set(true);
                        result.set(None);
                        spawn(async move {
                            let response = api::search_music(SearchRequest {
                                provider,
                                query,
                                page: 1,
                                limit: api::PAGE_SIZE,
                            })
                            .await
                            .map_err(view::server_error_message);
                            result.set(Some(response));
                            searching.set(false);
                        });
                    },
                    label { class: "grid gap-2 text-sm font-medium text-zinc-700",
                        span { "音乐源" }
                        select {
                            class: "h-10 w-full rounded-md border border-zinc-200 bg-white px-3 text-sm shadow-sm outline-none transition focus:border-zinc-400 focus:ring-2 focus:ring-zinc-950/10",
                            value: "{provider}",
                            required: true,
                            oninput: move |event| provider.set(event.value()),
                            for name in providers {
                                option { value: "{name}", "{name}" }
                            }
                        }
                    }
                    label { class: "grid min-w-0 gap-2 text-sm font-medium text-zinc-700",
                        span { "关键词" }
                        div { class: "flex gap-2",
                            input {
                                class: "h-10 min-w-0 flex-1 rounded-md border border-zinc-200 bg-white px-3 text-sm font-normal shadow-sm outline-none transition placeholder:text-zinc-400 focus:border-zinc-400 focus:ring-2 focus:ring-zinc-950/10",
                                r#type: "search",
                                value: "{query}",
                                required: true,
                                autocomplete: "off",
                                placeholder: "歌曲、歌手或专辑",
                                oninput: move |event| query.set(event.value()),
                            }
                            button {
                                class: "inline-flex h-10 shrink-0 cursor-pointer items-center justify-center rounded-md bg-zinc-900 px-4 text-sm font-medium text-white shadow-sm transition hover:bg-zinc-800 disabled:pointer-events-none disabled:opacity-50",
                                r#type: "submit",
                                disabled: searching(),
                                if searching() { "搜索中…" } else { "搜索" }
                            }
                        }
                    }
                }
                p { class: "mt-4 text-sm text-zinc-500", "试听使用低码率音频，下载使用 FLAC 无损音质。" }
            }

            section { class: "rounded-lg border border-zinc-200 bg-white p-4 shadow-sm sm:p-6", aria_live: "polite",
                if searching() {
                    div { class: "rounded-lg border border-dashed border-zinc-200 bg-zinc-50/50 px-8 py-14 text-center text-sm text-zinc-500", "正在搜索…" }
                } else {
                    match result() {
                        None => rsx! { EmptyState { message: "请输入关键词开始搜索".to_owned() } },
                        Some(Err(error)) => rsx! { ErrorBanner { message: error } },
                        Some(Ok(search_result)) if search_result.tracks.is_empty() => {
                            rsx! { EmptyState { message: "没有找到符合条件的歌曲".to_owned() } }
                        },
                        Some(Ok(search_result)) => {
                            let has_more = search_result.page * search_result.limit
                                < search_result.total as i64;
                            rsx! {
                                div { class: "mb-2 flex items-baseline justify-between gap-3 border-b border-zinc-100 pb-4",
                                    h2 { class: "text-lg font-semibold tracking-tight", "搜索结果" }
                                    span { class: "rounded-md bg-zinc-100 px-2.5 py-1 text-xs font-medium text-zinc-600", "共 {search_result.total} 条" }
                                }
                                div {
                                    for track in search_result.tracks {
                                        TrackCard {
                                            key: "{track.provider}:{track.id}",
                                            track,
                                        }
                                    }
                                }
                                if has_more {
                                    button {
                                        class: "mx-auto mt-4 flex h-10 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-white px-4 text-sm font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100 disabled:pointer-events-none disabled:opacity-50",
                                        r#type: "button",
                                        disabled: searching(),
                                        onclick: move |_| {
                                            let Some(Ok(current)) = result() else {
                                                return;
                                            };
                                            searching.set(true);
                                            let request = SearchRequest {
                                                provider: current.provider,
                                                query: current.query,
                                                page: current.page + 1,
                                                limit: current.limit,
                                            };
                                            spawn(async move {
                                                match api::search_music(request).await {
                                                    Ok(next) => result.with_mut(|slot| {
                                                        if let Some(Ok(current)) = slot.as_mut() {
                                                            current.page = next.page;
                                                            current.total = next.total;
                                                            current.tracks.extend(next.tracks);
                                                        }
                                                    }),
                                                    Err(error) => {
                                                        result.set(Some(Err(view::server_error_message(error))));
                                                    }
                                                }
                                                searching.set(false);
                                            });
                                        },
                                        "加载更多"
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn TrackCard(track: Track) -> Element {
    let mut player = use_context::<Signal<Option<PreviewTrack>>>();
    let mut submitting = use_signal(|| false);
    let mut feedback = use_signal(|| None::<Result<String, String>>);

    let preview_track = track.clone();
    let download_track = track.clone();
    let artists = view::artists(&track.artists);

    rsx! {
        article { class: "group flex flex-col gap-4 border-b border-zinc-100 py-4 last:border-0 sm:flex-row sm:items-center",
            if track.cover_url.is_empty() {
                div { class: "flex h-14 w-14 shrink-0 items-center justify-center rounded-md bg-zinc-100 text-zinc-400", aria_hidden: "true", "♪" }
            } else {
                img {
                    class: "h-14 w-14 shrink-0 rounded-md bg-zinc-100 object-cover",
                    src: "{track.cover_url}",
                    alt: "",
                    loading: "lazy",
                }
            }
            div { class: "min-w-0 flex-1",
                h3 { class: "truncate text-sm font-medium text-zinc-950", "{track.name}" }
                p { class: "mt-1 flex items-center gap-2 overflow-hidden text-xs text-zinc-500",
                    span { class: "truncate text-zinc-600", "{artists}" }
                    if !track.album.is_empty() {
                        span { aria_hidden: "true", "•" }
                        span { "{track.album}" }
                    }
                    if !track.date.is_empty() {
                        span { aria_hidden: "true", "•" }
                        span { "{track.date}" }
                    }
                }
                div { class: "mt-3 flex flex-wrap items-center gap-2",
                    button {
                        class: "inline-flex h-8 cursor-pointer items-center justify-center gap-1.5 rounded-md border border-zinc-200 bg-white px-3 text-xs font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100",
                        r#type: "button",
                        onclick: move |_| {
                            player.set(Some(PreviewTrack {
                                url: view::preview_url(
                                    &preview_track.provider,
                                    &preview_track.id,
                                ),
                                title: preview_track.name.clone(),
                                artists: view::artists(&preview_track.artists),
                                cover_url: preview_track.cover_url.clone(),
                            }));
                        },
                        "▶ 试听"
                    }
                    button {
                        class: "inline-flex h-8 cursor-pointer items-center justify-center gap-1.5 rounded-md border border-zinc-200 bg-white px-3 text-xs font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100 disabled:pointer-events-none disabled:opacity-50",
                        r#type: "button",
                        disabled: submitting(),
                        onclick: move |_| {
                            submitting.set(true);
                            feedback.set(None);
                            let track = download_track.clone();
                            spawn(async move {
                                let response = api::create_download(track)
                                    .await
                                    .map_err(view::server_error_message);
                                feedback.set(Some(response));
                                submitting.set(false);
                            });
                        },
                        if submitting() { "处理中…" } else { "↓ 下载" }
                    }
                }
            }
            div { class: "shrink-0 sm:max-w-52",
                match feedback() {
                    Some(Ok(message)) => rsx! { NoticeBanner { message } },
                    Some(Err(message)) => rsx! { ErrorBanner { message } },
                    None => rsx! {},
                }
            }
        }
    }
}
