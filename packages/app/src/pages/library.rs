use dioxus::prelude::*;
use dioxus_icons::lucide::{HardDrive, Library, LoaderCircle, Music2, RefreshCw, Search, Trash2};
use shared::library::Song;

use crate::{api, components::*, hooks::use_cursor_pagination, view};

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
    let load_more = use_callback(move |cursor: i64| {
        if pagination.loading() {
            return;
        }

        let generation = pagination.start_loading();
        let current_query = query();
        spawn(async move {
            match api::load_library(current_query, Some(cursor), api::PAGE_SIZE).await {
                Ok(next) => {
                    pagination.append(generation, next.page.songs, next.page.next_before_id);
                }
                Err(error) => {
                    pagination.fail(generation, view::server_error_message(error));
                }
            }
        });
    });

    rsx! {
        document::Title { "本地曲库 - Tune Box" }
        section { class: "flex h-full min-h-0 flex-col gap-4",
            header { class: "flex shrink-0 flex-wrap items-center justify-between gap-3",
                div { class: "flex min-w-0 items-center gap-3",
                    span { class: "flex size-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-cyan-500 to-blue-600 text-white shadow-md shadow-cyan-500/20",
                        Library { size: 17 }
                    }
                    div { class: "min-w-0",
                        h1 { class: "text-xl font-semibold tracking-tight text-zinc-950", "本地曲库" }
                        p { class: "truncate text-xs text-zinc-500 sm:text-sm", "已整理 {snapshot.count} 首本地歌曲" }
                    }
                }
                div { class: "flex items-center gap-2",
                    Button {
                        variant: ButtonVariant::Outline,
                        r#type: "button",
                        disabled: sync_library.pending(),
                        onclick: move |_| { sync_library.call(()); },
                        if sync_library.pending() {
                            LoaderCircle { class: "animate-spin", size: 15 }
                            "同步中"
                        } else {
                            RefreshCw { size: 15 }
                            "同步曲库"
                        }
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

            div { class: "flex min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-zinc-200/80 bg-white shadow-lg shadow-zinc-900/[0.035]",
                form {
                    class: "flex shrink-0 gap-2 border-b border-zinc-100 bg-zinc-50/50 p-3 sm:p-4",
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
                    div { class: "relative min-w-0 flex-1",
                        Search { class: "pointer-events-none absolute left-3 top-1/2 z-10 -translate-y-1/2 text-zinc-400", size: 17 }
                        Input {
                            class: "pl-10",
                            r#type: "search",
                            value: "{draft_query}",
                            autocomplete: "off",
                            placeholder: "搜索歌曲或歌手",
                            aria_label: "搜索本地曲库",
                            oninput: move |event: FormEvent| draft_query.set(event.value()),
                        }
                    }
                    Button {
                        size: ButtonSize::Lg,
                        r#type: "submit",
                        Search { size: 15 }
                        "搜索"
                    }
                }

                ScrollArea {
                    class: "min-h-0 flex-1",
                    tabindex: "0",
                    aria_label: "本地曲库歌曲列表",
                    div { class: "space-y-3 p-4 sm:p-5", aria_live: "polite",
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
                        if let Some(cursor) = next_cursor {
                            div { class: "py-2",
                                InfiniteScrollTrigger {
                                    key: "library-load-more-{cursor}",
                                    loading: pagination.loading(),
                                    failed: pagination.error().is_some(),
                                    on_load_more: move |_| load_more.call(cursor),
                                }
                            }
                        } else if has_songs {
                            p { class: "py-2 text-center text-xs text-muted-foreground", "已经到底了" }
                        }
                    }
                }
            }

            if let Some(error) = pagination.error() {
                ErrorBanner { message: error }
            }

        }
    }
}

#[component]
fn SongCard(song: Song, on_deleted: EventHandler<()>) -> Element {
    let mut confirm_open = use_signal(|| false);
    let mut delete_song = use_action(move |id: i64| async move {
        match api::delete_song(id).await {
            Ok(()) => {
                on_deleted.call(());
                Ok(())
            }
            Err(error) => Err(std::io::Error::other(view::server_error_message(error))),
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
        Card { class: "group transition-colors hover:border-cyan-200 hover:bg-cyan-50/20 hover:shadow-md",
          CardContent { class: "flex flex-wrap items-center gap-4 sm:flex-nowrap",
            div { class: "flex size-11 shrink-0 items-center justify-center rounded-xl bg-cyan-50 text-cyan-600 ring-1 ring-cyan-100", aria_hidden: "true",
                Music2 { size: 18 }
            }
            div { class: "min-w-0 flex-1",
                CardTitle { class: "truncate text-sm text-zinc-950", "{song.name}" }
                CardDescription { class: "mt-1 truncate", "{artists}" }
                div { class: "mt-2 flex items-center gap-2",
                    span { class: "rounded px-1.5 py-0.5 text-[0.65rem] font-semibold uppercase {extension_class}", "{extension}" }
                    span { class: "inline-flex items-center gap-1 text-xs text-zinc-400",
                        HardDrive { size: 12 }
                        "{size}"
                    }
                }
            }
            div { class: "grid w-full justify-items-stretch gap-1 sm:w-auto sm:justify-items-end",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    r#type: "button",
                    aria_label: "删除 {song.name}",
                    disabled: delete_song.pending(),
                    onclick: move |_| confirm_open.set(true),
                    if delete_song.pending() {
                        LoaderCircle { class: "animate-spin text-zinc-400", size: 16 }
                    } else {
                        Trash2 { class: "text-red-500", size: 16 }
                    }
                }
                if let Some(Err(error)) = delete_song.value() {
                    span { class: "text-xs text-red-600", "{error}" }
                }
            }
          }
            AlertDialog {
                open: confirm_open(),
                on_open_change: move |open| confirm_open.set(open),
                AlertDialogTitle { "从曲库中删除？" }
                AlertDialogDescription {
                    "将删除《{song.name}》的本地文件和曲库记录。此操作无法撤销。"
                }
                AlertDialogActions {
                    AlertDialogCancel { "取消" }
                    AlertDialogAction {
                        on_click: move |_| delete_song.call(song.id),
                        "确认删除"
                    }
                }
            }
        }
    }
}
