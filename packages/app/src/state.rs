use std::sync::Arc;

use service::{events::DownloadEvents, manager::ManagerHandle, service::Service};

// 服务端组合状态，由Axum Extension注入SSR、Server Function和自定义路由。
#[derive(Clone)]
pub(crate) struct AppState {
    pub service: Arc<Service>,
    pub manager: ManagerHandle,
    pub download_events: DownloadEvents,
}

impl AppState {
    pub(crate) fn new(
        service: Arc<Service>,
        manager: ManagerHandle,
        download_events: DownloadEvents,
    ) -> Self {
        Self {
            service,
            manager,
            download_events,
        }
    }
}
