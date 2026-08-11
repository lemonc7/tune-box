use std::{collections::HashMap, path::Path};

use sqlx::SqlitePool;

use crate::{
    error::ServiceError,
    server::{db::repository::Repository, download::Downloader, provider::Provider, tag::Embedder},
};

pub struct Service {
    repository: Repository,
    providers: HashMap<String, Box<dyn Provider>>,
    downloader: Downloader,
    embedder: Embedder,
}

impl Service {
    pub fn new(
        repository: Repository,
        providers: HashMap<String, Box<dyn Provider>>,
        downloader: Downloader,
        embedder: Embedder,
    ) -> Self {
        Self {
            repository,
            providers,
            downloader,
            embedder,
        }
    }

    pub fn provider(&self, name: &str) -> Result<&dyn Provider, ServiceError> {
        self.providers
            .get(name)
            .map(|p| p.as_ref())
            .ok_or(ServiceError::InvalidProvider)
    }

    // /// 不经过 Manager 时使用的便捷入口；未领取到任务时直接返回 false。
    // pub async fn process_download(&self, id: i64) -> Result<bool, ServiceError> {
    //     if !self.claim_download(id).await? {
    //         return Ok(false);
    //     }
    //     self.process_claimed_download(id).await?;
    //     Ok(true)
    // }

    // /// 执行已经由 Manager 原子领取、当前处于 Resolving 的任务。
    // pub async fn process_claimed_download(&self, id: i64) -> Result<(), ServiceError> {
    //     let result = self.process_resolving_download(id).await;
    //     if let Err(err) = &result
    //         && !matches!(
    //             err,
    //             ServiceError::InvalidState { .. } | ServiceError::InvalidTransition { .. }
    //         )
    //         && let Err(db_err) = self.mark_failed(id, &err.to_string()).await
    //     {
    //         tracing::error!(task_id = id, error = %db_err, "记录下载任务失败状态时出错");
    //     }
    //     result
    // }

    // async fn process_resolving_download(&self, id: i64) -> Result<(), ServiceError> {
    //     let mut db = self.db.clone();
    //     let task = Download::get_by_id(&mut db, &id).await?;
    //     if task.status != Status::Resolving {
    //         return Err(ServiceError::InvalidState {
    //             expected: Status::Resolving,
    //             actual: task.status,
    //         });
    //     }

    //     let provider = self.provider(&task.provider)?;
    //     let resource = provider.resolve(&task.track_id, &task.quality).await?;
    //     self.start_downloading(id, &resource.url).await?;

    //     let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
    //     let progress_db = self.db.clone();
    //     let progress_task =
    //         tokio::spawn(async move { persist_progress(progress_db, id, progress_rx).await });

    //     let result = self
    //         .downloader
    //         .download(
    //             resource,
    //             DownloadOptions {
    //                 task_id: id,
    //                 // 空名称会使用 provider-track_id，避免曲名中的路径字符影响下载。
    //                 file_name: String::new(),
    //                 progress: Some(move |progress| {
    //                     let _ = progress_tx.send(progress);
    //                 }),
    //             },
    //         )
    //         .await;

    //     // Downloader 返回时回调和 sender 已被释放，确保最终进度已经落库。
    //     progress_task.await??;
    //     let result = result?;
    //     let size = i64::try_from(result.size).map_err(|_| ServiceError::SizeOverflow)?;
    //     let path = path_to_string(&result.path)?;
    //     self.finish_downloading(id, size, &path).await?;

    //     self.transition(id, Status::Downloaded, Status::Tagging)
    //         .await?;
    //     let lyrics = provider.lyrics(&task.track_id).await?;
    //     self.embedder
    //         .embed_flac(&result.path, &task.metadata, &lyrics)
    //         .await?;
    //     self.complete(id, &path).await?;

    //     Ok(())
    // }

    // async fn transition(&self, id: i64, from: Status, to: Status) -> Result<(), ServiceError> {
    //     if !from.can_transition_to(to) {
    //         return Err(ServiceError::InvalidTransition { from, to });
    //     }

    //     Download::filter_by_id(id)
    //         .filter(Download::fields().status().eq(from))
    //         .update()
    //         .status(to)
    //         .exec(&mut self.db.clone())
    //         .await?;
    //     self.ensure_status(id, from, to).await
    // }

    // async fn start_downloading(&self, id: i64, resource_url: &str) -> Result<(), ServiceError> {
    //     Download::filter_by_id(id)
    //         .filter(Download::fields().status().eq(Status::Resolving))
    //         .update()
    //         .status(Status::Downloading)
    //         .resource_url(resource_url)
    //         .error_message(None::<String>)
    //         .exec(&mut self.db.clone())
    //         .await?;
    //     self.ensure_status(id, Status::Resolving, Status::Downloading)
    //         .await
    // }

    // async fn finish_downloading(&self, id: i64, size: i64, path: &str) -> Result<(), ServiceError> {
    //     Download::filter_by_id(id)
    //         .filter(Download::fields().status().eq(Status::Downloading))
    //         .update()
    //         .status(Status::Downloaded)
    //         .downloaded_bytes(size)
    //         .total_bytes(size)
    //         .path(path)
    //         .exec(&mut self.db.clone())
    //         .await?;
    //     self.ensure_status(id, Status::Downloading, Status::Downloaded)
    //         .await
    // }

    // async fn complete(&self, id: i64, path: &str) -> Result<(), ServiceError> {
    //     Download::filter_by_id(id)
    //         .filter(Download::fields().status().eq(Status::Tagging))
    //         .update()
    //         .status(Status::Completed)
    //         .path(path)
    //         .error_message(None::<String>)
    //         .finished_at(Timestamp::now())
    //         .exec(&mut self.db.clone())
    //         .await?;
    //     self.ensure_status(id, Status::Tagging, Status::Completed)
    //         .await
    // }

    // pub(crate) async fn mark_failed(&self, id: i64, message: &str) -> Result<(), ServiceError> {
    //     let mut db = self.db.clone();
    //     let task = Download::get_by_id(&mut db, &id).await?;
    //     if !task.status.can_transition_to(Status::Failed) {
    //         return Ok(());
    //     }

    //     Download::filter_by_id(id)
    //         .filter(Download::fields().status().eq(task.status))
    //         .update()
    //         .status(Status::Failed)
    //         .error_message(message)
    //         .finished_at(Timestamp::now())
    //         .exec(&mut db)
    //         .await?;
    //     Ok(())
    // }

    // async fn ensure_status(
    //     &self,
    //     id: i64,
    //     expected: Status,
    //     next: Status,
    // ) -> Result<(), ServiceError> {
    //     let mut db = self.db.clone();
    //     let task = Download::get_by_id(&mut db, &id).await?;
    //     if task.status == next {
    //         return Ok(());
    //     }
    //     Err(ServiceError::InvalidState {
    //         expected,
    //         actual: task.status,
    //     })
    // }
}

// async fn persist_progress(
//     mut db: toasty::Db,
//     id: i64,
//     mut progress_rx: tokio::sync::mpsc::UnboundedReceiver<DownloadProgress>,
// ) -> Result<(), ServiceError> {
//     let mut ticker = time::interval(Duration::from_millis(500));
//     let mut latest = None;

//     loop {
//         tokio::select! {
//             progress = progress_rx.recv() => match progress {
//                 Some(progress) => latest = Some(progress),
//                 None => {
//                     if let Some(progress) = latest.take() {
//                         write_progress(&mut db, id, progress).await?;
//                     }
//                     return Ok(());
//                 }
//             },
//             _ = ticker.tick() => {
//                 if let Some(progress) = latest.take() {
//                     write_progress(&mut db, id, progress).await?;
//                 }
//             }
//         }
//     }
// }

// async fn write_progress(
//     db: &mut toasty::Db,
//     id: i64,
//     progress: DownloadProgress,
// ) -> Result<(), ServiceError> {
//     let downloaded = i64::try_from(progress.downloaded).map_err(|_| ServiceError::SizeOverflow)?;
//     let update = Download::filter_by_id(id)
//         .filter(Download::fields().status().eq(Status::Downloading))
//         .update()
//         .downloaded_bytes(downloaded);

//     if let Some(total) = progress.total {
//         let total = i64::try_from(total).map_err(|_| ServiceError::SizeOverflow)?;
//         update.total_bytes(total).exec(db).await?;
//     } else {
//         update.exec(db).await?;
//     }
//     Ok(())
// }

fn path_to_string(path: &Path) -> Result<String, ServiceError> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or(ServiceError::Download(
            crate::error::DownloadError::InvalidFileName,
        ))
}

// fn is_unique_violation(err: &toasty::Error) -> bool {
//     err.to_string().contains("UNIQUE constraint failed")
// }
