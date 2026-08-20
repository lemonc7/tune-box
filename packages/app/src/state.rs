use std::sync::Arc;

use service::{manager::ManagerHandle, service::Service};

/// 服务端组合状态，由 Axum Extension 注入 SSR、Server Function 和自定义路由。
#[derive(Clone)]
pub(crate) struct AppState {
    pub service: Arc<Service>,
    pub manager: ManagerHandle,
}

impl AppState {
    pub(crate) fn new(service: Arc<Service>, manager: ManagerHandle) -> Self {
        Self { service, manager }
    }
}
