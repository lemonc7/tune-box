use dioxus::prelude::*;
use shared::{music::Track, request::SearchRequest};

use crate::{
    api,
    components::{EmptyState, ErrorBanner, NoticeBanner, PreviewTrack},
    hooks::{MusicSearchState, use_music_search},
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
    let mut search = use_music_search();

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
                            search.fail("请选择音乐源并输入搜索关键词");
                            return;
                        }

                        search.search(SearchRequest {
                            provider,
                            query,
                            page: 1,
                            limit: api::PAGE_SIZE,
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
                                disabled: search.loading(),
                                if search.loading() { "搜索中…" } else { "搜索" }
                            }
                        }
                    }
                }
                p { class: "mt-4 text-sm text-zinc-500", "试听使用低码率音频，下载使用 FLAC 无损音质。" }
            }

            section { class: "rounded-lg border border-zinc-200 bg-white p-4 shadow-sm sm:p-6", aria_live: "polite",
                match search.state() {
                    MusicSearchState::Idle => rsx! { EmptyState { message: "请输入关键词开始搜索".to_owned() } },
                    MusicSearchState::Loading => rsx! {
                        div { class: "rounded-lg border border-dashed border-zinc-200 bg-zinc-50/50 px-8 py-14 text-center text-sm text-zinc-500", "正在搜索…" }
                    },
                    MusicSearchState::Failed(error) => rsx! { ErrorBanner { message: error } },
                    MusicSearchState::Ready(search_result) if search_result.tracks.is_empty() => {
                        rsx! { EmptyState { message: "没有找到符合条件的歌曲".to_owned() } }
                    },
                    MusicSearchState::Ready(search_result) => {
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
                                    onclick: move |_| search.load_more(),
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

#[component]
fn TrackCard(track: Track) -> Element {
    let mut player = use_context::<Signal<Option<PreviewTrack>>>();
    let mut create_download = use_action(move |track: Track| async move {
        api::create_download(track)
            .await
            .map_err(|error| std::io::Error::other(view::server_error_message(error)))
    });

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
                        disabled: create_download.pending(),
                        onclick: move |_| {
                            create_download.call(download_track.clone());
                        },
                        if create_download.pending() { "处理中…" } else { "↓ 下载" }
                    }
                }
            }
            div { class: "shrink-0 sm:max-w-52",
                match create_download.value() {
                    Some(Ok(message)) => rsx! { NoticeBanner { message: message() } },
                    Some(Err(error)) => rsx! { ErrorBanner { message: error.to_string() } },
                    None => rsx! {},
                }
            }
        }
    }
}
