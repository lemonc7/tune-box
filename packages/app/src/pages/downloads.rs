use dioxus::prelude::*;
use shared::download::{Download, DownloadAction, Status};

use crate::{
    api,
    components::{EmptyState, ErrorBanner},
    view,
};

#[component]
pub fn DownloadsPage() -> Element {
    let mut revision = use_signal(|| 0_u64);
    let mut extra_downloads = use_signal(Vec::<Download>::new);
    let mut loaded_more = use_signal(|| false);
    let mut next_before_id = use_signal(|| None::<i64>);
    let mut loading_more = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);

    let downloads_resource = use_server_future(move || {
        let _revision = revision();
        api::list_downloads(None, api::PAGE_SIZE)
    })?;

    let _active = use_memo(move || {
        let base_active = downloads_resource()
            .and_then(Result::ok)
            .is_some_and(|page| {
                page.downloads
                    .iter()
                    .any(|download| view::is_active(download.status))
            });
        base_active
            || extra_downloads
                .read()
                .iter()
                .any(|download| view::is_active(download.status))
    });

    let _polling = use_future(move || async move {
        #[cfg(feature = "web")]
        loop {
            gloo_timers::future::TimeoutFuture::new(2_000).await;
            if _active() {
                *revision.write() += 1;
            }
        }

        #[cfg(not(feature = "web"))]
        std::future::pending::<()>().await;
    });

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

    let next_cursor = if loaded_more() {
        next_before_id()
    } else {
        page.next_before_id
    };
    let has_downloads = !page.downloads.is_empty() || !extra_downloads.read().is_empty();

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
                    onclick: move |_| *revision.write() += 1,
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
                            download,
                            on_changed: move |_| {
                                extra_downloads.write().clear();
                                loaded_more.set(false);
                                next_before_id.set(None);
                                *revision.write() += 1;
                            },
                        }
                    }
                    for download in extra_downloads() {
                        DownloadCard {
                            key: "extra-{download.id}",
                            download,
                            on_changed: move |_| {
                                extra_downloads.write().clear();
                                loaded_more.set(false);
                                next_before_id.set(None);
                                *revision.write() += 1;
                            },
                        }
                    }
                }
            }

            if let Some(error) = load_error() {
                ErrorBanner { message: error }
            }
            if let Some(cursor) = next_cursor {
                button {
                    class: "mx-auto flex h-10 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-white px-4 text-sm font-medium text-zinc-900 shadow-sm transition hover:bg-zinc-100 disabled:pointer-events-none disabled:opacity-50",
                    r#type: "button",
                    disabled: loading_more(),
                    onclick: move |_| {
                        loading_more.set(true);
                        load_error.set(None);
                        spawn(async move {
                            match api::list_downloads(Some(cursor), api::PAGE_SIZE).await {
                                Ok(next) => {
                                    extra_downloads.write().extend(next.downloads);
                                    next_before_id.set(next.next_before_id);
                                    loaded_more.set(true);
                                }
                                Err(error) => {
                                    load_error.set(Some(view::server_error_message(error)));
                                }
                            }
                            loading_more.set(false);
                        });
                    },
                    if loading_more() { "加载中…" } else { "加载更多" }
                }
            } else if has_downloads {
                p { class: "text-center text-xs text-zinc-400", "已经到底了" }
            }
        }
    }
}

#[component]
fn DownloadCard(download: Download, on_changed: EventHandler<()>) -> Element {
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
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    rsx! {
        button {
            class: if dangerous {
                "block w-full rounded-sm px-2.5 py-2 text-left text-sm text-red-600 hover:bg-red-50 disabled:opacity-50"
            } else {
                "block w-full rounded-sm px-2.5 py-2 text-left text-sm text-zinc-700 hover:bg-zinc-100 disabled:opacity-50"
            },
            r#type: "button",
            disabled: busy(),
            onclick: move |_| {
                if dangerous && !armed() {
                    armed.set(true);
                    return;
                }

                busy.set(true);
                error.set(None);
                spawn(async move {
                    match api::change_download(id, action).await {
                        Ok(()) => on_changed.call(()),
                        Err(response_error) => {
                            error.set(Some(view::server_error_message(response_error)));
                            armed.set(false);
                        }
                    }
                    busy.set(false);
                });
            },
            if busy() {
                "处理中…"
            } else if dangerous && armed() {
                "再次点击确认"
            } else {
                "{label}"
            }
        }
        if let Some(message) = error() {
            span { class: "block px-2.5 py-1 text-xs text-red-600", "{message}" }
        }
    }
}
