use std::{collections::HashMap, sync::Arc};

use tokio::sync::Semaphore;

use crate::{
    error::ServiceError,
    model::DownloadRequest,
    server::{
        db::{Download, Status},
        download::Downloader,
        provider::Provider,
    },
};

impl Status {
    pub fn can_transition_to(self, next: Self) -> bool {
        self != next
            && !self.is_terminal()
            && matches!(
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
        Download::create()
            .provider(req.provider)
            .track_id(req.track_id)
            .quality(req.quality)
            .metadata(req.metadata)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses_cannot_transition() {
        for status in [Status::Completed, Status::Failed, Status::Cancelled] {
            for next in [
                Status::Pending,
                Status::Resolving,
                Status::Downloading,
                Status::Tagging,
                Status::Paused,
                Status::Completed,
                Status::Failed,
                Status::Cancelled,
            ] {
                assert!(!status.can_transition_to(next));
            }
        }
    }

    #[test]
    fn valid_download_flow_can_transition() {
        assert!(Status::Pending.can_transition_to(Status::Resolving));
        assert!(Status::Resolving.can_transition_to(Status::Downloading));
        assert!(Status::Downloading.can_transition_to(Status::Tagging));
        assert!(Status::Tagging.can_transition_to(Status::Completed));
        assert!(Status::Downloading.can_transition_to(Status::Paused));
        assert!(Status::Paused.can_transition_to(Status::Downloading));
        assert!(!Status::Downloading.can_transition_to(Status::Downloading));
    }
}
