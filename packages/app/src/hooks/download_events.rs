use std::collections::HashMap;

use dioxus::prelude::*;
#[cfg(feature = "web")]
use shared::download::DownloadEvent;

#[cfg(feature = "web")]
use crate::{api, view};

pub type DownloadProgress = HashMap<i64, (i64, Option<i64>)>;

#[derive(Clone, Copy)]
pub struct DownloadEventsState {
    pub progress: Signal<DownloadProgress>,
    pub error: Signal<Option<String>>,
}

// 订阅下载进度事件。数据库中的下载记录是最终状态，事件只用于实时更新界面。
pub fn use_download_events(refresh: Callback<()>) -> DownloadEventsState {
    let progress = use_signal(DownloadProgress::new);
    let error = use_signal(|| None::<String>);

    #[cfg(feature = "web")]
    let mut progress_task = progress;
    #[cfg(feature = "web")]
    let mut error_task = error;
    #[cfg(not(feature = "web"))]
    let _ = refresh;

    let _events_task = use_future(move || async move {
        #[cfg(feature = "web")]
        loop {
            match api::download_events().await {
                Ok(mut events) => {
                    error_task.set(None);
                    // 重连后丢弃可能过期的进度，并从数据库恢复最终状态。
                    progress_task.write().clear();
                    refresh.call(());

                    while let Some(event) = events.recv().await {
                        match event {
                            Ok(DownloadEvent::Progress {
                                id,
                                downloaded_bytes,
                                total_bytes,
                            }) => {
                                progress_task
                                    .write()
                                    .insert(id, (downloaded_bytes, total_bytes));
                            }
                            Ok(DownloadEvent::Downloaded { id }) => {
                                progress_task.write().remove(&id);
                                // 音频已经落盘，此时刷新可读到 downloaded/tagging。
                                refresh.call(());
                            }
                            Err(error) => {
                                error_task.set(Some(format!("实时连接中断：{error}")));
                                break;
                            }
                        }
                    }

                    if error_task().is_none() {
                        error_task.set(Some("实时连接已断开，正在重连…".to_owned()));
                    }
                }
                Err(error) => {
                    error_task.set(Some(format!(
                        "无法建立实时连接：{}",
                        view::server_error_message(error)
                    )));
                }
            }

            retry_delay().await;
        }

        #[cfg(not(feature = "web"))]
        std::future::pending::<()>().await;
    });

    DownloadEventsState { progress, error }
}

// 平台相关的重连等待集中在这里；添加 desktop/mobile 时不需要修改 Hook 接口。
#[cfg(feature = "web")]
async fn retry_delay() {
    gloo_timers::future::TimeoutFuture::new(2_000).await;
}
