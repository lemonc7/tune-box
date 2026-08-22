use dioxus::prelude::*;
use dioxus_icons::lucide::{
    CircleCheck, CloudDownload, Ellipsis, LoaderCircle, Pause, Play, RefreshCw, RotateCw, Trash2, X,
};
use shared::download::{Download, DownloadAction, Status};

use crate::{
    api,
    components::*,
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
    let load_more = use_callback(move |cursor: i64| {
        if pagination.loading() {
            return;
        }

        let generation = pagination.start_loading();
        spawn(async move {
            match api::list_downloads(Some(cursor), api::PAGE_SIZE).await {
                Ok(next) => {
                    pagination.append(generation, next.downloads, next.next_before_id);
                }
                Err(error) => {
                    pagination.fail(generation, view::server_error_message(error));
                }
            }
        });
    });

    rsx! {
        document::Title { "下载任务 - Tune Box" }
        section { class: "flex h-full min-h-0 flex-col gap-4",
            header { class: "flex shrink-0 flex-wrap items-center justify-between gap-3",
                div { class: "flex min-w-0 items-center gap-3",
                    span { class: "flex size-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-indigo-500 to-violet-600 text-white shadow-md shadow-indigo-500/20",
                        CloudDownload { size: 17 }
                    }
                    div { class: "min-w-0",
                        h1 { class: "text-xl font-semibold tracking-tight text-zinc-950", "下载任务" }
                        p { class: "truncate text-xs text-zinc-500 sm:text-sm", "实时追踪下载进度和标签写入状态" }
                    }
                }
                Button {
                    variant: ButtonVariant::Outline,
                    r#type: "button",
                    onclick: move |_| {
                        progress_updates.write().clear();
                        pagination.reset();
                        downloads_resource.restart();
                    },
                    RefreshCw { size: 15 }
                    "刷新"
                }
            }

            ScrollArea {
                class: "min-h-0 flex-1",
                tabindex: "0",
                aria_label: "下载任务列表",
                div { class: "space-y-3 pr-2", aria_live: "polite",
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
                    if let Some(cursor) = next_cursor {
                        div { class: "py-2",
                            InfiniteScrollTrigger {
                                key: "downloads-load-more-{cursor}",
                                loading: pagination.loading(),
                                failed: pagination.error().is_some(),
                                on_load_more: move |_| load_more.call(cursor),
                            }
                        }
                    } else if has_downloads {
                        p { class: "py-2 text-center text-xs text-muted-foreground", "已经到底了" }
                    }
                }
            }

            if let Some(error) = pagination.error() {
                ErrorBanner { message: error }
            }
            if let Some(error) = events_error() {
                ErrorBanner { message: error }
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
    let mut actions = Vec::new();
    if can_pause {
        actions.push((DownloadAction::Pause, "暂停下载", false));
    }
    if can_resume {
        actions.push((DownloadAction::Resume, "继续下载", false));
    }
    if can_cancel {
        actions.push((DownloadAction::Cancel, "取消任务", true));
    }
    if can_retry {
        actions.push((DownloadAction::Retry, "重新下载", false));
    }
    if can_delete {
        actions.push((DownloadAction::Delete, "删除记录", true));
    }
    let has_actions = !actions.is_empty();
    let mut confirm_action = use_signal(|| None::<DownloadAction>);
    let mut confirm_action_task = confirm_action;
    let mut change_download = use_action(move |(id, action): (i64, DownloadAction)| async move {
        match api::change_download(id, action).await {
            Ok(()) => {
                on_changed.call(());
                Ok(())
            }
            Err(error) => {
                confirm_action_task.set(None);
                Err(std::io::Error::other(view::server_error_message(error)))
            }
        }
    });

    rsx! {
        Card { class: "group relative transition-colors hover:border-indigo-200 hover:bg-indigo-50/20 hover:shadow-md",
          CardContent {
            div { class: "flex items-start gap-4",
                span { class: if download.status == Status::Completed {
                        "flex size-11 shrink-0 items-center justify-center rounded-xl bg-emerald-50 text-emerald-600 ring-1 ring-emerald-100"
                    } else {
                        "flex size-11 shrink-0 items-center justify-center rounded-xl bg-indigo-50 text-indigo-600 ring-1 ring-indigo-100"
                    },
                    if download.status == Status::Completed {
                        CircleCheck { size: 19 }
                    } else if download.status == Status::Downloading {
                        CloudDownload { size: 19 }
                    } else if download.status == Status::Paused {
                        Pause { size: 19 }
                    } else if matches!(download.status, Status::Failed | Status::Cancelled) {
                        X { size: 19 }
                    } else {
                        LoaderCircle { class: "animate-spin", size: 19 }
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "flex flex-wrap items-center gap-2",
                        CardTitle { class: "truncate text-zinc-950", "{download.metadata.name}" }
                        span { class: "rounded-full px-2 py-0.5 text-xs font-semibold {status_class}", "{status_label}" }
                    }
                    CardDescription { class: "mt-1 truncate text-zinc-600", "{artists}" }
                    p { class: "mt-1 truncate text-xs text-zinc-400", "{details}" }
                }
                if has_actions {
                    DropdownMenu { class: "shrink-0",
                        DropdownMenuTrigger { aria_label: "任务操作",
                            Ellipsis { size: 17 }
                        }
                        DropdownMenuContent {
                            for (index, (action, label, dangerous)) in actions.into_iter().enumerate() {
                                DropdownMenuItem::<DownloadAction> {
                                    class: if dangerous { "text-red-600" } else { "" },
                                    index,
                                    value: action,
                                    disabled: change_download.pending(),
                                    on_select: move |selected| {
                                        if dangerous {
                                            confirm_action.set(Some(selected));
                                        } else {
                                            change_download.call((download.id, selected));
                                        }
                                    },
                                    match action {
                                        DownloadAction::Pause => rsx! { Pause { size: 15 } },
                                        DownloadAction::Resume => rsx! { Play { size: 15 } },
                                        DownloadAction::Cancel => rsx! { X { size: 15 } },
                                        DownloadAction::Retry => rsx! { RotateCw { size: 15 } },
                                        DownloadAction::Delete => rsx! { Trash2 { size: 15 } },
                                    }
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "mt-4",
                Progress {
                    value: progress as f64,
                    aria_label: "下载进度 {progress}%",
                }
                div { class: "mt-2 flex justify-between text-xs text-zinc-400",
                    span { "{downloaded_text}" }
                    span { "{progress_text}" }
                }
            }
            if let Some(error) = download.error_message {
                p { class: "mt-3 rounded-lg bg-red-50 px-3 py-2 text-sm text-red-700", "{error}" }
            }
            if let Some(Err(error)) = change_download.value() {
                p { class: "mt-3 text-xs text-red-600", "{error}" }
            }
          }
            if let Some(action) = confirm_action() {
                AlertDialog {
                    open: true,
                    on_open_change: move |open: bool| {
                        if !open {
                            confirm_action.set(None);
                        }
                    },
                    AlertDialogTitle {
                        if action == DownloadAction::Delete {
                            "删除下载记录？"
                        } else {
                            "取消这个下载任务？"
                        }
                    }
                    AlertDialogDescription {
                        if action == DownloadAction::Delete {
                            "下载记录将从列表中移除，此操作无法撤销。"
                        } else {
                            "当前任务将被取消，之后仍可以重新创建下载。"
                        }
                    }
                    AlertDialogActions {
                        AlertDialogCancel { "返回" }
                        AlertDialogAction {
                            on_click: move |_| change_download.call((download.id, action)),
                            if action == DownloadAction::Delete { "确认删除" } else { "确认取消" }
                        }
                    }
                }
            }
        }
    }
}
