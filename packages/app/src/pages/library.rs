use dioxus::prelude::*;
use shared::library::Song;

use crate::{
    api,
    components::{EmptyState, ErrorBanner, NoticeBanner},
    view,
};

#[component]
pub fn LibraryPage() -> Element {
    let mut draft_query = use_signal(String::new);
    let mut query = use_signal(String::new);
    let mut revision = use_signal(|| 0_u64);
    let mut extra_songs = use_signal(Vec::<Song>::new);
    let mut loaded_more = use_signal(|| false);
    let mut next_before_id = use_signal(|| None::<i64>);
    let mut loading_more = use_signal(|| false);
    let mut syncing = use_signal(|| false);
    let mut feedback = use_signal(|| None::<Result<String, String>>);

    let library_resource = use_server_future(move || {
        let query = query();
        let _revision = revision();
        api::load_library(query, None, api::PAGE_SIZE)
    })?;

    let snapshot = match library_resource() {
        Some(Ok(snapshot)) => snapshot,
        Some(Err(error)) => {
            return rsx! { ErrorBanner { message: view::server_error_message(error) } };
        }
        None => shared::library::LibrarySnapshot {
            page: shared::library::SongPage {
                songs: Vec::new(),
                next_before_id: None,
            },
            count: 0,
        },
    };

    let next_cursor = if loaded_more() {
        next_before_id()
    } else {
        snapshot.page.next_before_id
    };
    let has_songs = !snapshot.page.songs.is_empty() || !extra_songs.read().is_empty();

    rsx! {
        document::Title { "本地曲库 - Tune Box" }
        section { class: "space-y-6",
            header { class: "flex flex-wrap items-start justify-between gap-3",
                div {
                    h1 { class: "text-2xl font-semibold tracking-tight", "本地曲库" }
                    p { class: "mt-1 text-sm text-zinc-500", "共 {snapshot.count} 首歌曲" }
                }
                div { class: "flex items-center gap-2",
                    button {
                        class: "inline-flex h-9 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-white px-3 text-sm font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100 disabled:pointer-events-none disabled:opacity-50",
                        r#type: "button",
                        disabled: syncing(),
                        onclick: move |_| {
                            syncing.set(true);
                            feedback.set(None);
                            spawn(async move {
                                match api::sync_library().await {
                                    Ok(count) => {
                                        feedback.set(Some(Ok(format!("已同步 {count} 首歌曲"))));
                                        extra_songs.write().clear();
                                        loaded_more.set(false);
                                        next_before_id.set(None);
                                        *revision.write() += 1;
                                    }
                                    Err(error) => feedback.set(Some(Err(
                                        view::server_error_message(error),
                                    ))),
                                }
                                syncing.set(false);
                            });
                        },
                        if syncing() { "同步中…" } else { "同步曲库" }
                    }
                }
            }

            match feedback() {
                Some(Ok(message)) => rsx! { NoticeBanner { message } },
                Some(Err(message)) => rsx! { ErrorBanner { message } },
                None => rsx! {},
            }

            div { class: "overflow-hidden rounded-lg border border-zinc-200 bg-white shadow-sm",
                form {
                    class: "flex gap-2 border-b border-zinc-100 p-4",
                    onsubmit: move |event| {
                        event.prevent_default();
                        query.set(draft_query().trim().to_owned());
                        extra_songs.write().clear();
                        loaded_more.set(false);
                        next_before_id.set(None);
                        *revision.write() += 1;
                    },
                    input {
                        class: "h-10 min-w-0 flex-1 rounded-md border border-zinc-200 bg-white px-3 text-sm shadow-sm outline-none transition placeholder:text-zinc-400 focus:border-zinc-400 focus:ring-2 focus:ring-zinc-950/10",
                        r#type: "search",
                        value: "{draft_query}",
                        autocomplete: "off",
                        placeholder: "搜索歌曲或歌手",
                        aria_label: "搜索本地曲库",
                        oninput: move |event| draft_query.set(event.value()),
                    }
                    button {
                        class: "inline-flex h-10 cursor-pointer items-center justify-center rounded-md bg-zinc-900 px-4 text-sm font-medium text-white shadow-sm transition hover:bg-zinc-800",
                        r#type: "submit",
                        "搜索"
                    }
                }

                div { class: "divide-y divide-zinc-100", aria_live: "polite",
                    if !has_songs {
                        EmptyState { message: "没有找到符合条件的歌曲".to_owned() }
                    } else {
                        for song in snapshot.page.songs {
                            SongCard {
                                key: "{song.id}",
                                song,
                                on_deleted: move |_| {
                                    extra_songs.write().clear();
                                    loaded_more.set(false);
                                    next_before_id.set(None);
                                    feedback.set(Some(Ok("歌曲已删除".to_owned())));
                                    *revision.write() += 1;
                                },
                            }
                        }
                        for song in extra_songs() {
                            SongCard {
                                key: "extra-{song.id}",
                                song,
                                on_deleted: move |_| {
                                    extra_songs.write().clear();
                                    loaded_more.set(false);
                                    next_before_id.set(None);
                                    feedback.set(Some(Ok("歌曲已删除".to_owned())));
                                    *revision.write() += 1;
                                },
                            }
                        }
                    }
                }
            }

            if let Some(cursor) = next_cursor {
                button {
                    class: "mx-auto flex h-10 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-white px-4 text-sm font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100 disabled:pointer-events-none disabled:opacity-50",
                    r#type: "button",
                    disabled: loading_more(),
                    onclick: move |_| {
                        loading_more.set(true);
                        let current_query = query();
                        spawn(async move {
                            match api::load_library(
                                current_query,
                                Some(cursor),
                                api::PAGE_SIZE,
                            )
                            .await
                            {
                                Ok(next) => {
                                    extra_songs.write().extend(next.page.songs);
                                    next_before_id.set(next.page.next_before_id);
                                    loaded_more.set(true);
                                }
                                Err(error) => feedback.set(Some(Err(
                                    view::server_error_message(error),
                                ))),
                            }
                            loading_more.set(false);
                        });
                    },
                    if loading_more() { "加载中…" } else { "加载更多" }
                }
            } else if has_songs {
                p { class: "text-center text-xs text-zinc-400", "已经到底了" }
            }
        }
    }
}

#[component]
fn SongCard(song: Song, on_deleted: EventHandler<()>) -> Element {
    let mut armed = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let artists = view::artists(&song.artists);
    let size = view::format_bytes(song.size);
    let extension = if song.extension.is_empty() {
        "audio".to_owned()
    } else {
        song.extension.clone()
    };
    let extension_class = view::extension_class(&song.extension);

    rsx! {
        article { class: "group flex flex-wrap items-center gap-4 px-4 py-3.5 transition-colors hover:bg-zinc-50 sm:flex-nowrap",
            div { class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-zinc-100 text-sm font-semibold text-zinc-500", aria_hidden: "true", "♪" }
            div { class: "min-w-0 flex-1",
                h2 { class: "truncate text-sm font-medium text-zinc-950", "{song.name}" }
                p { class: "mt-0.5 truncate text-sm text-zinc-500", "{artists}" }
                div { class: "mt-2 flex items-center gap-2",
                    span { class: "rounded px-1.5 py-0.5 text-[0.65rem] font-semibold uppercase {extension_class}", "{extension}" }
                    span { class: "text-xs text-zinc-400", "{size}" }
                }
            }
            div { class: "grid w-full justify-items-stretch gap-1 sm:w-auto sm:justify-items-end",
                button {
                    class: "inline-flex h-9 cursor-pointer items-center justify-center rounded-md border border-red-200 bg-white px-3 text-sm font-medium text-red-600 shadow-sm transition hover:bg-red-50 disabled:pointer-events-none disabled:opacity-50",
                    r#type: "button",
                    disabled: deleting(),
                    onclick: move |_| {
                        if !armed() {
                            armed.set(true);
                            return;
                        }

                        deleting.set(true);
                        error.set(None);
                        spawn(async move {
                            match api::delete_song(song.id).await {
                                Ok(()) => on_deleted.call(()),
                                Err(response_error) => {
                                    error.set(Some(view::server_error_message(response_error)));
                                    armed.set(false);
                                }
                            }
                            deleting.set(false);
                        });
                    },
                    if deleting() {
                        "删除中…"
                    } else if armed() {
                        "再次点击确认"
                    } else {
                        "删除"
                    }
                }
                if let Some(message) = error() {
                    span { class: "text-xs text-red-600", "{message}" }
                }
            }
        }
    }
}
