use dioxus::prelude::*;
use shared::library::Song;

use crate::{
    api,
    components::{EmptyState, ErrorBanner, NoticeBanner},
    hooks::use_cursor_pagination,
    view,
};

#[component]
pub fn LibraryPage() -> Element {
    let mut draft_query = use_signal(String::new);
    let mut query = use_signal(String::new);
    let mut notice = use_signal(|| None::<String>);
    let mut pagination = use_cursor_pagination::<Song>();

    let mut library_resource = use_server_future(move || {
        let query = query();
        api::load_library(query, None, api::PAGE_SIZE)
    })?;
    let mut sync_library = use_action(move |(): ()| async move {
        let count = api::sync_library()
            .await
            .map_err(|error| std::io::Error::other(view::server_error_message(error)))?;
        pagination.reset();
        library_resource.restart();
        Ok::<_, std::io::Error>(count)
    });

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

    let next_cursor = pagination.next_cursor(snapshot.page.next_before_id);
    let has_songs = !snapshot.page.songs.is_empty() || !pagination.is_empty();

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
                        disabled: sync_library.pending(),
                        onclick: move |_| { sync_library.call(()); },
                        if sync_library.pending() { "同步中…" } else { "同步曲库" }
                    }
                }
            }

            if let Some(message) = notice() {
                NoticeBanner { message }
            }
            match sync_library.value() {
                Some(Ok(count)) => rsx! { NoticeBanner { message: format!("已同步 {} 首歌曲", count()) } },
                Some(Err(error)) => rsx! { ErrorBanner { message: error.to_string() } },
                None => rsx! {},
            }

            div { class: "overflow-hidden rounded-lg border border-zinc-200 bg-white shadow-sm",
                form {
                    class: "flex gap-2 border-b border-zinc-100 p-4",
                    onsubmit: move |event| {
                        event.prevent_default();
                        let next_query = draft_query().trim().to_owned();
                        pagination.reset();
                        notice.set(None);
                        if query() == next_query {
                            library_resource.restart();
                        } else {
                            query.set(next_query);
                        }
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
                                    pagination.reset();
                                    notice.set(Some("歌曲已删除".to_owned()));
                                    library_resource.restart();
                                },
                            }
                        }
                        for song in pagination.items() {
                            SongCard {
                                key: "extra-{song.id}",
                                song,
                                on_deleted: move |_| {
                                    pagination.reset();
                                    notice.set(Some("歌曲已删除".to_owned()));
                                    library_resource.restart();
                                },
                            }
                        }
                    }
                }
            }

            if let Some(error) = pagination.error() {
                ErrorBanner { message: error }
            }

            if let Some(cursor) = next_cursor {
                button {
                    class: "mx-auto flex h-10 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-white px-4 text-sm font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100 disabled:pointer-events-none disabled:opacity-50",
                    r#type: "button",
                    disabled: pagination.loading(),
                    onclick: move |_| {
                        pagination.start_loading();
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
                                    pagination.append(
                                        next.page.songs,
                                        next.page.next_before_id,
                                    );
                                }
                                Err(error) => {
                                    pagination.fail(view::server_error_message(error));
                                }
                            }
                        });
                    },
                    if pagination.loading() { "加载中…" } else { "加载更多" }
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
    let mut armed_task = armed;
    let mut delete_song = use_action(move |id: i64| async move {
        match api::delete_song(id).await {
            Ok(()) => {
                on_deleted.call(());
                Ok(())
            }
            Err(error) => {
                armed_task.set(false);
                Err(std::io::Error::other(view::server_error_message(error)))
            }
        }
    });
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
                    disabled: delete_song.pending(),
                    onclick: move |_| {
                        if !armed() {
                            armed.set(true);
                            return;
                        }

                        delete_song.call(song.id);
                    },
                    if delete_song.pending() {
                        "删除中…"
                    } else if armed() {
                        "再次点击确认"
                    } else {
                        "删除"
                    }
                }
                if let Some(Err(error)) = delete_song.value() {
                    span { class: "text-xs text-red-600", "{error}" }
                }
            }
        }
    }
}
