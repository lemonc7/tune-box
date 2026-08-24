use contracts::download::{Download, DownloadAction, Status};
use dioxus::{CapturedError, prelude::*};
use dioxus_icons::lucide::{EllipsisVertical, LoaderCircle, Music2};

use crate::{api, view};

#[component]
pub fn DownloadItem(
    download: Download,
    progress: Option<(i64, Option<i64>)>,
    refresh: Callback<()>,
) -> Element {
    let mut confirmation = use_signal(|| None::<DownloadAction>);
    let download_id = download.id;
    let mut change_download = use_action(move |action: DownloadAction| async move {
        api::change_download(download_id, action)
            .await
            .map_err(|error| CapturedError::msg(view::server_error_message(error)))?;

        // SSE断开时也能更新页面
        refresh.call(());
        Ok::<_, CapturedError>(())
    });

    // SSE中的进度只覆盖数据库中的字节进度，status和错误信息仍然以数据库记录为准
    let downloaded_bytes = progress
        .map(|(downloaded, _)| downloaded)
        .unwrap_or(download.downloaded_bytes);
    let total_bytes = progress
        .and_then(|(_, total)| total)
        .or(download.total_bytes);

    let completed = download.status == Status::Completed;
    let has_total = total_bytes.is_some_and(|total| total > 0);
    let determinate = has_total || completed;
    let percent = if completed {
        100.0
    } else {
        total_bytes
            .filter(|total| *total > 0)
            .map(|total| {
                let downloaded = downloaded_bytes.max(0) as f64;
                (downloaded / total as f64 * 100.0).clamp(0.0, 100.0)
            })
            .unwrap_or_default()
    };

    let downloaded_text = match total_bytes {
        Some(total) if total > 0 => {
            format!(
                "{} / {}",
                view::format_bytes(downloaded_bytes),
                view::format_bytes(total)
            )
        }
        _ => view::format_bytes(downloaded_bytes),
    };
    let progress_text = if has_total {
        format!("{percent:.0}%")
    } else if matches!(download.status, Status::Resolving | Status::Downloading) {
        "正在下载".to_owned()
    } else {
        String::new()
    };
    let progress_label = if determinate {
        format!("下载进度 {percent:.0}%")
    } else {
        "下载进度未知".to_owned()
    };

    let (status_label, status_class) = view::status_presentation(download.status);
    let artists = view::artists(&download.metadata.artists);
    let details = [
        download.metadata.album.clone(),
        download.metadata.date.clone(),
        download.provider.clone(),
    ]
    .into_iter()
    .filter(|v| !v.is_empty())
    .collect::<Vec<_>>()
    .join(" • ");

    let pending = change_download.pending();
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
    let failed = download.status == Status::Failed;
    let menu_id = format!("download-menu-{download_id}");
    let menu_anchor = format!("--{menu_id}");
    let trigger_style = format!("anchor-name: {menu_anchor};");
    let menu_style = format!(
        "position-anchor: {menu_anchor}; inset: auto; top: anchor(bottom); right: anchor(right);"
    );

    rsx! {
      article { class: "py-5",
        div { class: "flex items-start gap-4",
          // 封面
          if download.metadata.cover_url.is_empty() {
            div {
              class: "flex size-14 shrink-0 items-center justify-center",
              class: "rounded-lg bg-muted text-muted-foreground",
              aria_hidden: true,
              Music2 { size: 20 }
            }
          } else {
            img {
              class: "size-14 shrink-0 rounded-lg bg-muted object-cover",
              src: download.metadata.cover_url.clone(),
              alt: "",
              loading: "lazy",
            }
          }
          // 任务信息
          div { class: "min-w-0 flex-1",
            div { class: "flex min-w-0 flex-wrap items-center gap-2",
              h2 { class: "min-w-0 truncate text-sm font-medium text-foreground",
                "{download.metadata.name}"
              }
              span { class: "{status_class} shrink-0 rounded-full px-2 py-0.5 text-xs font-medium",
                "{status_label}"
              }
            }
            p { class: "mt-1 truncate text-sm text-muted-foreground",
              "{artists}"
            }
            if !details.is_empty() {
              p { class: "mt-1 truncate text-xs text-muted-foreground",
                "{details}"
              }
            }
          }

          // 操作菜单
          if can_pause || can_resume || can_cancel || can_retry || can_delete {
            div { class: "shrink-0",
              button {
                class: "flex size-9 cursor-pointer list-none",
                class: "items-center justify-center rounded-md",
                class: "border border-border text-muted-foreground",
                class: "transition-colors hover:bg-muted",
                r#type: "button",
                style: trigger_style,
                popovertarget: menu_id.clone(),
                popovertargetaction: "toggle",
                aria_label: "任务操作",

                span { class: "inline-flex", aria_hidden: true,
                  if pending {
                    LoaderCircle { size: 17, class: "animate-spin" }
                  } else {
                    EllipsisVertical { size: 17 }
                  }
                }
              }
              div {
                id: menu_id.clone(),
                popover: "auto",
                style: menu_style,
                class: "mt-1 w-36",
                class: "rounded-md border border-border bg-background p-1 shadow-lg",

                if can_pause {
                  DownloadActionButton {
                    popover_id: menu_id.clone(),
                    label: "暂停",
                    action: DownloadAction::Pause,
                    disabled: pending,
                    onselect: move |action| change_download.call(action),
                  }
                }

                if can_resume {
                  DownloadActionButton {
                    popover_id: menu_id.clone(),
                    label: "继续",
                    action: DownloadAction::Resume,
                    disabled: pending,
                    onselect: move |action| change_download.call(action),
                  }
                }

                if can_cancel {
                  DownloadActionButton {
                    popover_id: menu_id.clone(),
                    label: "取消",
                    action: DownloadAction::Cancel,
                    destructive: true,
                    disabled: pending,
                    onselect: move |action| confirmation.set(Some(action)),
                  }
                }

                if can_retry {
                  DownloadActionButton {
                    popover_id: menu_id.clone(),
                    label: "重新下载",
                    action: DownloadAction::Retry,
                    disabled: pending,
                    onselect: move |action| change_download.call(action),
                  }
                }

                if can_delete {
                  DownloadActionButton {
                    popover_id: menu_id.clone(),
                    label: "删除记录",
                    action: DownloadAction::Delete,
                    destructive: true,
                    disabled: pending,
                    onselect: move |action| confirmation.set(Some(action)),
                  }
                }
              }
            }
          }
        }

        // 进度
        div { class: "mt-4",
          progress {
            class: "block h-1.5 w-full overflow-hidden rounded-full accent-primary",
            max: 100.0,
            value: if determinate { percent },
            aria_label: progress_label,
          }
          div {
            class: "mt-2 flex items-center justify-between gap-4",
            class: "text-xs text-muted-foreground",
            span { class: "truncate", "{downloaded_text}" }
            if !progress_text.is_empty() {
              span { class: "shrink-0", "{progress_text}" }
            }
          }
        }

        // 后端任务错误
        if let Some(message) = download.error_message {
          p {
            class: "mt-3 rounded-md px-3 py-2 text-sm",
            class: if failed { "bg-destructive/10 text-destructive" } else { "bg-muted text-muted-foreground" },
            role: if failed { "alert" } else { "status" },
            "{message}"
          }
        }

        // 当前操作错误
        if let Some(Err(error)) = change_download.value() {
          p {
            class: "mt-3 rounded-md bg-destructive/10 px-3 py-2",
            class: "text-sm text-destructive",
            role: "alert",
            "{error}"
          }
        }

        // 取消/删除二次确认
        if let Some(action) = confirmation() {
          ConfirmDownloadAction {
            dialog_id: format!("download-confirm-{download_id}"),
            action,
            pending,
            onconfirm: move |action| {
                confirmation.set(None);
                change_download.call(action)
            },
            oncancel: move |_| confirmation.set(None),
          }
        }
      }
    }
}

#[component]
fn DownloadActionButton(
    popover_id: String,
    label: String,
    action: DownloadAction,
    #[props(default)] destructive: bool,
    disabled: bool,
    onselect: EventHandler<DownloadAction>,
) -> Element {
    rsx! {
      button {
        class: "block w-full rounded-sm px-2.5 py-2",
        class: "text-left text-sm transition-colors",
        class: if destructive { "text-destructive hover:bg-destructive/10" } else { "text-foreground hover:bg-muted" },
        class: "disabled:pointer-events-none disabled:opacity-50",
        r#type: "button",
        popovertarget: popover_id,
        popovertargetaction: "hide",
        disabled,
        onclick: move |_| onselect.call(action),
        "{label}"
      }
    }
}

#[component]
fn ConfirmDownloadAction(
    dialog_id: String,
    action: DownloadAction,
    pending: bool,
    onconfirm: EventHandler<DownloadAction>,
    oncancel: EventHandler<()>,
) -> Element {
    let (title, description, confirm_label) = match action {
        DownloadAction::Cancel => (
            "确认取消任务？",
            "当前下载会停止，之后仍然可以重新下载。",
            "确认取消",
        ),
        DownloadAction::Delete => (
            "确认删除记录？",
            "这条下载记录将从任务列表中移除",
            "确认删除",
        ),
        _ => return rsx! {},
    };
    let title_id = format!("{dialog_id}-title");
    let description_id = format!("{dialog_id}-description");
    #[cfg(feature = "web")]
    let mounted_dialog_id = dialog_id.clone();

    rsx! {
      dialog {
        id: dialog_id,
        class: "m-auto w-[min(28rem,calc(100%-2rem))] rounded-xl",
        class: "border border-border bg-background p-0 text-foreground shadow-xl",
        class: "backdrop:bg-zinc-950/40 backdrop:backdrop-blur-sm",
        aria_labelledby: title_id.clone(),
        aria_describedby: description_id.clone(),
        onmounted: move |_| {
            #[cfg(feature = "web")]
            {
                let _ = document::eval(
                    &format!("document.getElementById({mounted_dialog_id:?})?.showModal();"),
                );
            }
        },
        oncancel: move |_| oncancel.call(()),

        form { method: "dialog", class: "p-6",
          h2 {
            id: title_id,
            class: "text-base font-semibold text-foreground",
            "{title}"
          }
          p {
            id: description_id,
            class: "mt-2 text-sm leading-6 text-muted-foreground",
            "{description}"
          }
          div { class: "mt-6 flex justify-end gap-2",
            button {
              class: "inline-flex h-9 items-center justify-center rounded-md",
              class: "border border-border bg-background px-4 text-sm font-medium",
              class: "transition-colors hover:bg-muted",
              r#type: "submit",
              value: "cancel",
              autofocus: true,
              disabled: pending,
              onclick: move |_| oncancel.call(()),
              "返回"
            }
            button {
              class: "inline-flex h-9 items-center justify-center rounded-md",
              class: "bg-destructive px-4 text-sm font-medium text-white",
              class: "transition-opacity hover:opacity-90",
              class: "disabled:pointer-events-none disabled:opacity-50",
              r#type: "submit",
              value: "confirm",
              disabled: pending,
              onclick: move |_| onconfirm.call(action),
              if pending {
                "处理中…"
              } else {
                "{confirm_label}"
              }
            }
          }
        }
      }
    }
}
