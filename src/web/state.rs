use std::sync::Arc;

use crate::server::{manager::ManagerHandle, service::Service};

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<Service>,
    pub manager: ManagerHandle,
}
