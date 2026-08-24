use std::collections::HashMap;

#[cfg(feature = "web")]
use contracts::download::DownloadEvent;
use dioxus::prelude::*;

#[cfg(feature = "web")]
use crate::{api, view};

type DownloadProgress = HashMap<i64, (i64, Option<i64>)>;

#[derive(Clone, Copy)]
pub struct DownloadEvents {
    progress: Signal<DownloadProgress>,
    error: Signal<Option<String>>,
}

impl DownloadEvents {
    pub fn progress(&self, id: i64) -> Option<(i64, Option<i64>)> {
        self.progress.read().get(&id).copied()
    }

    pub fn error(&self) -> Option<String> {
        self.error.read().clone()
    }

    pub fn clear_progress(&mut self) {
        self.progress.write().clear();
    }
}

// 订阅下载进度事件。数据库中的下载记录是最终状态，事件只用于实时更新界面。
pub fn use_download_events(refresh: Callback<()>) -> DownloadEvents {
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
        let mut retry_ms = INITIAL_RETRY_MS;
        #[cfg(feature = "web")]
        loop {
            match api::download_events().await {
                // 连接成功
                Ok(mut events) => {
                    // 重置重连退避
                    retry_ms = INITIAL_RETRY_MS;
                    error_task.set(None);
                    // 重连后丢弃可能过期的进度，并从数据库恢复最终状态。
                    progress_task.write().clear();
                    // 从数据库重新获取可信状态
                    refresh.call(());

                    // 读取事件流
                    while let Some(event) = events.recv().await {
                        match event {
                            Ok(DownloadEvent::Progress {
                                id,
                                downloaded_bytes,
                                total_bytes,
                            }) => {
                                // 更新进度
                                progress_task
                                    .write()
                                    .insert(id, (downloaded_bytes, total_bytes));
                            }
                            Ok(DownloadEvent::Changed { id }) => {
                                progress_task.write().remove(&id);
                                // 事件只负责使缓存失效，数据库中的记录才是最终状态。
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

            retry_delay(retry_ms).await;
            retry_ms = retry_ms.saturating_mul(2).min(MAX_RETRY_MS);
        }

        #[cfg(not(feature = "web"))]
        std::future::pending::<()>().await;
    });

    DownloadEvents { progress, error }
}

// 平台相关的重连等待集中在这里；添加 desktop/mobile 时不需要修改 Hook 接口。
#[cfg(feature = "web")]
const INITIAL_RETRY_MS: u32 = 1_000;
#[cfg(feature = "web")]
const MAX_RETRY_MS: u32 = 30_000;

#[cfg(feature = "web")]
async fn retry_delay(milliseconds: u32) {
    gloo_timers::future::TimeoutFuture::new(milliseconds).await;
}
