use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::{
    sync::{mpsc, oneshot},
    task::{Id, JoinSet},
    time,
};
use tokio_util::sync::CancellationToken;

use crate::{
    db::Repository,
    error::{ManagerError, ServiceError},
    service::Service,
};

enum ManagerCommand {
    Wake,
    Pause {
        id: i64,
        reply: oneshot::Sender<Result<bool, sqlx::Error>>,
    },
    Resume {
        id: i64,
        reply: oneshot::Sender<Result<bool, sqlx::Error>>,
    },
    Retry {
        id: i64,
        reply: oneshot::Sender<Result<bool, sqlx::Error>>,
    },
    Cancel {
        id: i64,
        reply: oneshot::Sender<Result<bool, sqlx::Error>>,
    },
    Shutdown,
}

pub struct Manager {
    repository: Repository,
    service: Arc<Service>,
    // 接受命令的通道
    command_rx: mpsc::Receiver<ManagerCommand>,
    // 最多同时运行的任务数量
    max_concurrent: usize,
    // 定期扫描数据库
    poll_interval: Duration,
}

#[derive(Clone)]
pub struct ManagerHandle {
    // 发送命令的通道
    command_tx: mpsc::Sender<ManagerCommand>,
}

impl Manager {
    pub fn new(
        repository: Repository,
        service: Arc<Service>,
        max_concurrent: usize,
    ) -> (Self, ManagerHandle) {
        let (command_tx, command_rx) = mpsc::channel(64);
        let manager = Self {
            repository,
            service,
            command_rx,
            max_concurrent: max_concurrent.max(1),
            poll_interval: Duration::from_secs(5),
        };
        let handle = ManagerHandle { command_tx };

        (manager, handle)
    }

    /// 数据库保存任务的最终状态，命令通道只负责及时唤醒调度器和停止当前进程中的worker
    pub async fn run(mut self) -> Result<(), ServiceError> {
        // 恢复意外中断的任务
        self.repository.recover_interrupted().await?;

        // 创建workers
        let mut workers = JoinSet::new();
        // 取消令牌，对应任务id
        let mut running_tokens = HashMap::<i64, CancellationToken>::new();
        // worker任务表，如果worker正常返回，可以返回任务id+version；否则只能拿到task id
        let mut worker_tasks = HashMap::<Id, (i64, i64)>::new();
        // 定时器，如果遗漏了，直接忽略
        let mut ticker = time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        // 是否已经开始关闭
        let mut shutting_down = false;
        // 是否发生了导致整个manager不能继续工作的错误
        let mut fatal_error = None;

        loop {
            // 如果没有进入关闭状态，调用fill_workers出现失败
            if !shutting_down
                && let Err(error) = self
                    .fill_workers(&mut workers, &mut running_tokens, &mut worker_tasks)
                    .await
            {
                fatal_error = Some(error);
                shutting_down = true;
                // 通知所有运行任务取消
                cancel_running(&running_tokens);
            }
            // 只有已经准备关闭和所有worker已经退出，才退出循环
            if shutting_down && workers.is_empty() {
                break;
            }

            tokio::select! {
                // 等待命令；如果进入关闭状态，不再接受命令
                command = self.command_rx.recv(), if !shutting_down => {
                    match command {
                        Some(ManagerCommand::Wake) => {
                          // 自身不需要执行任务，只是让等待的select立即返回，然后进行下一轮fill_workers
                        }
                        Some(ManagerCommand::Pause { id, reply }) => {
                            // 先修改数据库状态
                            let result = self.repository.pause_download(id).await;
                            // 如果数据库修改成功，获取对应令牌，通知当前worker退出
                            if matches!(result, Ok(true))
                                && let Some(token) = running_tokens.get(&id)
                            {
                                token.cancel();
                            }
                            // 把数据库处理结果返回调用方，忽略错误，因为调用方可能已经断开
                            let _ = reply.send(result);
                        }
                        Some(ManagerCommand::Resume { id, reply }) => {
                            // 恢复下载，不用做其它动作，下一轮循环会处理
                            let result = self.repository.resume_download(id).await;
                            let _ = reply.send(result);
                        }
                        Some(ManagerCommand::Retry { id, reply }) => {
                            let result = self.repository.retry_download(id).await;
                            let _ = reply.send(result);
                        }
                        Some(ManagerCommand::Cancel { id, reply }) => {
                            let result = self.repository.cancel_download(id).await;
                            if matches!(result, Ok(true))
                                && let Some(token) = running_tokens.get(&id)
                            {
                                token.cancel();
                            }
                            if matches!(result, Ok(true))
                                && let Err(error) = self.service.cleanup_download_files(id).await
                            {
                                tracing::warn!(task_id = id, %error, "清理已取消任务的临时文件失败");
                            }
                            let _ = reply.send(result);
                        }
                        Some(ManagerCommand::Shutdown) | None => {
                            // 收到shutdown命令或所有handle都被销毁，导致通道关闭(recv() -> None)
                            // 进入关闭状态，通知任务取消
                            shutting_down = true;
                            cancel_running(&running_tokens);
                        }
                    }
                }
                // worker；只有当前worker存在，才启用这个分支
                joined = workers.join_next_with_id(), if !workers.is_empty() => {
                    // Option<Result<(tokio_task_id, returned_value), JoinError>>
                    match joined {
                        // worker成功完成
                        Some(Ok((task_id, (id, Ok(()))))) => {
                            // 清理两张内存表: 任务表和取消令牌表
                            worker_tasks.remove(&task_id);
                            running_tokens.remove(&id);
                        }
                        // worker正常返回，但是service返回错误
                        Some(Ok((task_id, (id, Err(error))))) => {
                            // 移除取消令牌
                            running_tokens.remove(&id);
                            tracing::error!(
                                task_id = id,
                                error = %error,
                                "任务执行失败"
                            );
                            // 移除当前任务，然后重试任务
                            if let Some((_, version)) = worker_tasks.remove(&task_id)
                                && let Err(database_error) = self.repository
                                    .recover_worker(id, version, &error.to_string())
                                    .await
                            {
                                // 重试任务失败，记录错误，进入关闭状态
                                fatal_error = Some(database_error.into());
                                shutting_down = true;
                                cancel_running(&running_tokens);
                            }
                        }
                        // worker异常终止
                        Some(Err(error)) => {
                            let task_id = error.id();
                            // 移除任务，并通过task_id查对应的download_id+version
                            if let Some((id, version)) = worker_tasks.remove(&task_id) {
                                // 移除取消令牌
                                running_tokens.remove(&id);
                                tracing::error!(task_id = id, error = %error, "Worker 异常退出");
                                // 重试任务
                                if let Err(database_error) = self.repository
                                    .recover_worker(id, version, &format!("Worker 异常退出: {error}"))
                                    .await
                                {
                                    fatal_error = Some(database_error.into());
                                    shutting_down = true;
                                    cancel_running(&running_tokens);
                                }
                            } else {
                                // 查不到，记录错误
                                tracing::error!(error = %error, "未知 Worker 异常退出");
                            }
                        }
                        None => {}
                    }
                }
                // 定时器
                _ = ticker.tick(), if !shutting_down => {
                    // 回到循环顶部，定期扫描数据库兜底。
                }
            }
        }

        // 如果是致命错误导致错误关闭，返回错误
        if let Some(error) = fatal_error {
            return Err(error);
        }

        // 正常取消路径会由service回退状态；这里兜底恢复异常遗留的中间状态
        self.repository.recover_interrupted().await?;
        Ok(())
    }

    // 给workers分配任务，控制并发数量
    async fn fill_workers(
        &self,
        workers: &mut JoinSet<(i64, Result<(), ServiceError>)>,
        running_tokens: &mut HashMap<i64, CancellationToken>,
        worker_tasks: &mut HashMap<Id, (i64, i64)>,
    ) -> Result<(), ServiceError> {
        // 检查并发槽位
        while workers.len() < self.max_concurrent {
            // 从数据库领取任务
            let running_ids = running_tokens.keys().copied().collect::<Vec<_>>();
            let Some(task) = self.repository.claim_next(&running_ids).await? else {
                break;
            };

            let id = task.id;
            let version = task.version;
            let service = Arc::clone(&self.service);
            // 创建取消令牌
            let cancellation = CancellationToken::new();
            running_tokens.insert(id, cancellation.clone());

            // 启动worker
            let abort_handle = workers.spawn(async move {
                let result = service.execute_claimed(task, cancellation).await;
                (id, result)
            });
            worker_tasks.insert(abort_handle.id(), (id, version));
        }

        Ok(())
    }
}

fn cancel_running(running_tokens: &HashMap<i64, CancellationToken>) {
    for token in running_tokens.values() {
        token.cancel();
    }
}

impl ManagerHandle {
    // 新任务已经写入数据库时调用，用命令通道立即唤醒 Manager。
    pub async fn wake(&self) -> Result<(), ManagerError> {
        self.command_tx
            .send(ManagerCommand::Wake)
            .await
            .map_err(|_| ManagerError::ManagerStopped)
    }

    pub async fn pause(&self, id: i64) -> Result<bool, ManagerError> {
        // 创建回复通道，tx给manager，rx接受返回值
        let (reply_tx, reply_rx) = oneshot::channel();
        // 给manager发送暂停命令
        self.command_tx
            .send(ManagerCommand::Pause {
                id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ManagerError::ManagerStopped)?;

        // 等待处理结果
        reply_rx
            .await
            .map_err(|_| ManagerError::ManagerStopped)?
            .map_err(Into::into)
    }

    pub async fn resume(&self, id: i64) -> Result<bool, ManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::Resume {
                id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ManagerError::ManagerStopped)?;

        reply_rx
            .await
            .map_err(|_| ManagerError::ManagerStopped)?
            .map_err(Into::into)
    }

    pub async fn cancel(&self, id: i64) -> Result<bool, ManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::Cancel {
                id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ManagerError::ManagerStopped)?;

        reply_rx
            .await
            .map_err(|_| ManagerError::ManagerStopped)?
            .map_err(Into::into)
    }

    pub async fn retry(&self, id: i64) -> Result<bool, ManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::Retry {
                id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ManagerError::ManagerStopped)?;

        reply_rx
            .await
            .map_err(|_| ManagerError::ManagerStopped)?
            .map_err(Into::into)
    }

    pub async fn shutdown(&self) -> Result<(), ManagerError> {
        self.command_tx
            .send(ManagerCommand::Shutdown)
            .await
            .map_err(|_| ManagerError::ManagerStopped)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc, time::Duration};

    use contracts::music::Metadata;
    use tokio::task::JoinSet;
    use tokio_util::sync::CancellationToken;

    use super::Manager;
    use crate::{
        db, download::Downloader, error::ServiceError, service::Service, tag::embed::Embedder,
    };

    #[tokio::test]
    async fn fill_workers_does_not_spawn_a_second_worker_for_the_same_task() {
        let pool = db::connect_memory().await.unwrap();
        let repository = db::Repository::new(pool);
        let metadata = Metadata {
            name: "song".to_owned(),
            artists: vec!["artist".to_owned()],
            album: String::new(),
            album_artist: String::new(),
            date: String::new(),
            cover_url: String::new(),
            track_number: String::new(),
            disc_number: String::new(),
        };
        let running_id = repository
            .add_download("kuwo", "1", &metadata)
            .await
            .unwrap()
            .unwrap();
        let next_id = repository
            .add_download("kuwo", "2", &metadata)
            .await
            .unwrap()
            .unwrap();

        let client = reqwest::Client::new();
        let service = Arc::new(Service::new(
            repository.clone(),
            HashMap::new(),
            Downloader::new(client, std::env::temp_dir(), 1024),
            reqwest::Client::new(),
            Embedder::new(Duration::from_secs(30)).unwrap(),
        ));
        let (manager, _handle) = Manager::new(repository.clone(), service, 2);

        let mut workers = JoinSet::<(i64, Result<(), ServiceError>)>::new();
        let old_worker = workers.spawn(async move { std::future::pending().await });
        let mut running_tokens = HashMap::from([(running_id, CancellationToken::new())]);
        let mut worker_tasks = HashMap::from([(old_worker.id(), (running_id, 0))]);

        manager
            .fill_workers(&mut workers, &mut running_tokens, &mut worker_tasks)
            .await
            .unwrap();

        assert!(running_tokens.contains_key(&running_id));
        assert!(running_tokens.contains_key(&next_id));

        workers.abort_all();
        while workers.join_next().await.is_some() {}
    }
}
