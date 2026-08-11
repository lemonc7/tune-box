// use std::{sync::Arc, time::Duration};

// use tokio::{task::JoinSet, time};

// use crate::{error::ServiceError, server::service::Service};

// /// 从数据库领取任务，并用 JoinSet 限制和管理运行中的下载任务。
// pub struct DownloadManager {
//     service: Arc<Service>,
//     max_concurrent: usize,
//     poll_interval: Duration,
// }

// impl DownloadManager {
//     pub fn new(service: Arc<Service>, max_concurrent: usize) -> Self {
//         Self {
//             service,
//             max_concurrent: max_concurrent.max(1),
//             poll_interval: Duration::from_millis(500),
//         }
//     }

//     pub async fn run(self) -> Result<(), ServiceError> {
//         let mut running = JoinSet::new();
//         let mut poll = time::interval(self.poll_interval);

//         loop {
//             while running.len() < self.max_concurrent {
//                 let available = self.max_concurrent - running.len();
//                 let ids = self.service.pending_download_ids(available).await?;
//                 if ids.is_empty() {
//                     break;
//                 }

//                 let mut claimed_any = false;
//                 for id in ids {
//                     if !self.service.claim_download(id).await? {
//                         continue;
//                     }

//                     claimed_any = true;
//                     let service = Arc::clone(&self.service);
//                     running.spawn(async move { (id, service.process_claimed_download(id).await) });
//                 }

//                 // 多个 Manager 竞争同一批记录时避免空转，下一轮再重新查询。
//                 if !claimed_any {
//                     break;
//                 }
//             }

//             if running.is_empty() {
//                 poll.tick().await;
//                 continue;
//             }

//             tokio::select! {
//                 joined = running.join_next() => match joined {
//                     Some(Ok((_, Ok(())))) => {}
//                     Some(Ok((id, Err(error)))) => {
//                         tracing::error!(task_id = id, error = %error, "下载任务执行失败");
//                     }
//                     Some(Err(error)) => {
//                         tracing::error!(error = %error, "下载任务异常退出");
//                     }
//                     None => {}
//                 },
//                 _ = poll.tick() => {}
//             }
//         }
//     }
// }
