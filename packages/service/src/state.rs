use std::sync::Arc;

use crate::{manager::ManagerHandle, service::Service};

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<Service>,
    pub manager: ManagerHandle,
}

impl AppState {
    pub fn new(service: Arc<Service>, manager: ManagerHandle) -> Self {
        Self { service, manager }
    }
}
