use std::{collections::HashMap, sync::Arc};

use tokio::sync::Semaphore;

use crate::{
    error::ServiceError,
    model::DownloadRequest,
    server::{db::model::Download, download::Downloader, provider::Provider},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Resolving,
    Downloading,
    Tagging,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl Status {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Resolving)
                | (Self::Resolving, Self::Downloading)
                | (Self::Downloading, Self::Tagging)
                | (Self::Tagging, Self::Completed)
                // 暂停后恢复
                | (Self::Paused, Self::Downloading | Self::Pending)
                // 任务中途状态都可以暂停/取消/标记失败
                | (_, Self::Failed | Self::Cancelled | Self::Paused)
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

pub struct Service {
    db: toasty::Db,
    providers: HashMap<String, Box<dyn Provider>>,
    downloader: Downloader,
    semaphore: Arc<Semaphore>,
}

impl Service {
    pub fn provider(&self, name: &str) -> Result<&dyn Provider, ServiceError> {
        self.providers
            .get(name)
            .map(|p| p.as_ref())
            .ok_or(ServiceError::InvalidProvider)
    }

    pub async fn create_download(&self, req: DownloadRequest) -> Result<(), ServiceError> {
        let metadata = serde_json::to_string(&req.metadata)?;
        Download::create()
            .provider(req.provider)
            .track_id(req.track_id)
            .quality(req.quality)
            .metadata(metadata)
            .exec(&mut self.db.clone())
            .await
            .map_err(|err| {
                if is_unique_violation(&err) {
                    ServiceError::AlreadyExists
                } else {
                    ServiceError::Db(err)
                }
            })?;

        Ok(())
    }
}

fn is_unique_violation(err: &toasty::Error) -> bool {
    err.to_string().contains("UNIQUE constraint failed")
}
