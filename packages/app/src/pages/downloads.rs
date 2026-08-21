use dioxus::prelude::*;
use shared::download::{Download, DownloadAction, Status};

use crate::{
    api,
    components::{EmptyState, ErrorBanner},
    hooks::{use_cursor_pagination, use_download_events},
    view,
};

#[component]
pub fn DownloadsPage() -> Element {
    let mut pagination = use_cursor_pagination::<Download>();
    let mut downloads_resource = use_server_future(|| api::list_downloads(None, api::PAGE_SIZE))?;
    let refresh_downloads = use_callback(move |()| downloads_resource.restart());
    let download_events = use_download_events(refresh_downloads);
    let mut progress_updates = download_events.progress;
    let events_error = download_events.error;

    let page = match downloads_resource() {
        Some(Ok(page)) => page,
        Some(Err(error)) => {
            return rsx! { ErrorBanner { message: view::server_error_message(error) } };
        }
        None => shared::download::DownloadPage {
            downloads: Vec::new(),
            next_before_id: None,
        },
    };

    let next_cursor = pagination.next_cursor(page.next_before_id);
    let has_downloads = !page.downloads.is_empty() || !pagination.is_empty();

    rsx! {
        document::Title { "下载任务 - Tune Box" }
        section { class: "space-y-6",
            header { class: "flex flex-wrap items-start justify-between gap-3",
                div {
                    h1 { class: "text-2xl font-semibold tracking-tight", "下载任务" }
                    p { class: "mt-1 text-sm text-zinc-500", "查看实时进度，并管理下载和标签写入任务。" }
                }
                button {
                    class: "inline-flex h-9 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-white px-3 text-sm font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100",
                    r#type: "button",
                    onclick: move |_| {
                        progress_updates.write().clear();
                        pagination.reset();
                        downloads_resource.restart();
                    },
                    "刷新"
                }
            }

            div { class: "divide-y divide-zinc-100 overflow-visible rounded-lg border border-zinc-200 bg-white shadow-sm", aria_live: "polite",
                if !has_downloads {
                    EmptyState { message: "暂无下载任务".to_owned() }
                } else {
                    for download in page.downloads {
                        DownloadCard {
                            key: "{download.id}",
                            progress: progress_updates.read().get(&download.id).copied(),
                            download,
                            on_changed: move |_| {
                                progress_updates.write().clear();
                                pagination.reset();
                                downloads_resource.restart();
                            },
                        }
                    }
                    for download in pagination.items() {
                        DownloadCard {
                            key: "extra-{download.id}",
                            progress: progress_updates.read().get(&download.id).copied(),
                            download,
                            on_changed: move |_| {
                                progress_updates.write().clear();
                                pagination.reset();
                                downloads_resource.restart();
                            },
                        }
                    }
                }
            }

            if let Some(error) = pagination.error() {
                ErrorBanner { message: error }
            }
            if let Some(error) = events_error() {
                ErrorBanner { message: error }
            }
            if let Some(cursor) = next_cursor {
                button {
                    class: "mx-auto flex h-10 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-white px-4 text-sm font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100 disabled:pointer-events-none disabled:opacity-50",
                    r#type: "button",
                    disabled: pagination.loading(),
                    onclick: move |_| {
                        pagination.start_loading();
                        spawn(async move {
                            match api::list_downloads(Some(cursor), api::PAGE_SIZE).await {
                                Ok(next) => {
                                    pagination.append(next.downloads, next.next_before_id);
                                }
                                Err(error) => {
                                    pagination.fail(view::server_error_message(error));
                                }
                            }
                        });
                    },
                    if pagination.loading() { "加载中…" } else { "加载更多" }
                }
            } else if has_downloads {
                p { class: "text-center text-xs text-zinc-400", "已经到底了" }
            }
        }
    }
}

#[component]
fn DownloadCard(
    download: Download,
    progress: Option<(i64, Option<i64>)>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut download = download;
    if let Some((downloaded_bytes, total_bytes)) = progress {
        download.downloaded_bytes = downloaded_bytes;
        if total_bytes.is_some() {
            download.total_bytes = total_bytes;
        }
    }

    let (status_label, status_class) = view::status_presentation(download.status);
    let artists = view::artists(&download.metadata.artists);
    let progress = view::progress_percent(&download);
    let downloaded_text = view::format_bytes(download.downloaded_bytes);
    let progress_text = download
        .total_bytes
        .filter(|total| *total > 0)
        .map(|total| format!("{progress}% · {}", view::format_bytes(total)))
        .unwrap_or_else(|| "总大小未知".to_owned());
    let details = if download.metadata.album.is_empty() {
        download.provider.clone()
    } else {
        format!("{} · {}", download.provider, download.metadata.album)
    };

    let can_pause = matches!(
        download.status,
        Status::Pending | Status::Resolving | Status::Downloading
    );
    let can_resume = download.status == Status::Paused;
    let can_cancel = !matches!(
        download.status,
        Status::Completed | Status::Failed | Status::Cancelled
    );
    let can_retry = matches!(download.status, Status::Failed | Status::Cancelled);
    let can_delete = matches!(
        download.status,
        Status::Completed | Status::Failed | Status::Cancelled
    );
    let has_actions = can_pause || can_resume || can_cancel || can_retry || can_delete;

    rsx! {
        article { class: "relative p-5",
            div { class: "flex items-start gap-4",
                div { class: "min-w-0 flex-1",
                    div { class: "flex flex-wrap items-center gap-2",
                        h2 { class: "truncate font-medium text-zinc-950", "{download.metadata.name}" }
                        span { class: "rounded-full px-2 py-0.5 text-xs font-semibold {status_class}", "{status_label}" }
                    }
                    p { class: "mt-1 truncate text-sm text-zinc-600", "{artists}" }
                    p { class: "mt-1 truncate text-xs text-zinc-400", "{details}" }
                }
                if has_actions {
                    details { class: "relative shrink-0",
                        summary { class: "flex h-9 w-9 cursor-pointer list-none items-center justify-center rounded-md border border-zinc-200 text-sm text-zinc-500 shadow-xs hover:bg-zinc-100", aria_label: "任务操作", "•••" }
                        div { class: "absolute right-0 z-20 mt-1 w-40 overflow-hidden rounded-md border border-zinc-200 bg-white p-1 shadow-lg",
                            if can_pause {
                                DownloadActionButton {
                                    id: download.id,
                                    action: DownloadAction::Pause,
                                    label: "暂停".to_owned(),
                                    dangerous: false,
                                    on_changed,
                                }
                            }
                            if can_resume {
                                DownloadActionButton {
                                    id: download.id,
                                    action: DownloadAction::Resume,
                                    label: "继续".to_owned(),
                                    dangerous: false,
                                    on_changed,
                                }
                            }
                            if can_cancel {
                                DownloadActionButton {
                                    id: download.id,
                                    action: DownloadAction::Cancel,
                                    label: "取消".to_owned(),
                                    dangerous: true,
                                    on_changed,
                                }
                            }
                            if can_retry {
                                DownloadActionButton {
                                    id: download.id,
                                    action: DownloadAction::Retry,
                                    label: "重新下载".to_owned(),
                                    dangerous: false,
                                    on_changed,
                                }
                            }
                            if can_delete {
                                DownloadActionButton {
                                    id: download.id,
                                    action: DownloadAction::Delete,
                                    label: "删除记录".to_owned(),
                                    dangerous: true,
                                    on_changed,
                                }
                            }
                        }
                    }
                }
            }
            div { class: "mt-4",
                progress {
                    class: "block h-1.5 w-full overflow-hidden rounded-full accent-zinc-900",
                    value: "{progress}",
                    max: "100",
                    aria_label: "下载进度 {progress}%",
                }
                div { class: "mt-2 flex justify-between text-xs text-zinc-400",
                    span { "{downloaded_text}" }
                    span { "{progress_text}" }
                }
            }
            if let Some(error) = download.error_message {
                p { class: "mt-3 rounded-md bg-red-50 px-3 py-2 text-sm text-red-700", "{error}" }
            }
        }
    }
}

#[component]
fn DownloadActionButton(
    id: i64,
    action: DownloadAction,
    label: String,
    dangerous: bool,
    on_changed: EventHandler<()>,
) -> Element {
    let mut armed = use_signal(|| false);
    let mut armed_task = armed;
    let mut change_download = use_action(move |(id, action): (i64, DownloadAction)| async move {
        match api::change_download(id, action).await {
            Ok(()) => {
                on_changed.call(());
                Ok(())
            }
            Err(error) => {
                armed_task.set(false);
                Err(std::io::Error::other(view::server_error_message(error)))
            }
        }
    });

    rsx! {
        button {
            class: if dangerous {
                "block w-full rounded-sm px-2.5 py-2 text-left text-sm text-red-600 hover:bg-red-50 disabled:opacity-50"
            } else {
                "block w-full rounded-sm px-2.5 py-2 text-left text-sm text-zinc-700 hover:bg-zinc-100 disabled:opacity-50"
            },
            r#type: "button",
            disabled: change_download.pending(),
            onclick: move |_| {
                if dangerous && !armed() {
                    armed.set(true);
                    return;
                }

                change_download.call((id, action));
            },
            if change_download.pending() {
                "处理中…"
            } else if dangerous && armed() {
                "再次点击确认"
            } else {
                "{label}"
            }
        }
        if let Some(Err(error)) = change_download.value() {
            span { class: "block px-2.5 py-1 text-xs text-red-600", "{error}" }
        }
    }
}
