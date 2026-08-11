use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
    time,
};
use tokio_util::sync::CancellationToken;

use crate::{
    error::{ManagerError, ServiceError},
    server::{db::repository::Repository, service::Service},
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
    Cancel {
        id: i64,
        reply: oneshot::Sender<Result<bool, sqlx::Error>>,
    },
    Shutdown,
}

pub struct Manager {
    repository: Repository,
    service: Arc<Service>,
    command_rx: mpsc::Receiver<ManagerCommand>,
    max_concurrent: usize,
    poll_interval: Duration,
}

#[derive(Clone)]
pub struct ManagerHandle {
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
            poll_interval: Duration::from_millis(500),
        };
        let handle = ManagerHandle { command_tx };

        (manager, handle)
    }

    /// 运行单实例任务调度器。
    ///
    /// 数据库保存任务的最终状态，命令通道只负责及时唤醒调度器和停止
    /// 当前进程中的 Worker。
    pub async fn run(mut self) -> Result<(), ServiceError> {
        self.repository.recover_interrupted().await?;

        let mut workers = JoinSet::new();
        let mut running_tokens = HashMap::<i64, CancellationToken>::new();
        let mut ticker = time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        let mut shutting_down = false;

        loop {
            if !shutting_down {
                self.fill_workers(&mut workers, &mut running_tokens).await?;
            }
            if shutting_down && workers.is_empty() {
                break;
            }

            tokio::select! {
                command = self.command_rx.recv(), if !shutting_down => {
                    match command {
                        Some(ManagerCommand::Wake) => {}
                        Some(ManagerCommand::Pause { id, reply }) => {
                            let result = self.repository.pause_download(id).await;
                            if matches!(result, Ok(true))
                                && let Some(token) = running_tokens.get(&id)
                            {
                                token.cancel();
                            }
                            let _ = reply.send(result);
                        }
                        Some(ManagerCommand::Resume { id, reply }) => {
                            let result = self.repository.resume_download(id).await;
                            let _ = reply.send(result);
                        }
                        Some(ManagerCommand::Cancel { id, reply }) => {
                            let result = self.repository.cancel_download(id).await;
                            if matches!(result, Ok(true))
                                && let Some(token) = running_tokens.get(&id)
                            {
                                token.cancel();
                            }
                            let _ = reply.send(result);
                        }
                        Some(ManagerCommand::Shutdown) | None => {
                            shutting_down = true;
                            for token in running_tokens.values() {
                                token.cancel();
                            }
                        }
                    }
                }
                joined = workers.join_next(), if !workers.is_empty() => {
                    match joined {
                        Some(Ok((id, Ok(())))) => {
                            running_tokens.remove(&id);
                        }
                        Some(Ok((id, Err(error)))) => {
                            running_tokens.remove(&id);
                            tracing::error!(
                                task_id = id,
                                error = %error,
                                "任务执行失败"
                            );
                        }
                        Some(Err(error)) => {
                            tracing::error!(error = %error, "Worker 异常退出");
                        }
                        None => {}
                    }
                }
                _ = ticker.tick(), if !shutting_down => {
                    // 回到循环顶部，定期扫描数据库兜底。
                }
            }
        }

        // 正常取消路径会由 Service 回退状态；这里同时恢复 panic 等异常遗留的任务。
        self.repository.recover_interrupted().await?;
        Ok(())
    }

    async fn fill_workers(
        &self,
        workers: &mut JoinSet<(i64, Result<(), ServiceError>)>,
        running_tokens: &mut HashMap<i64, CancellationToken>,
    ) -> Result<(), ServiceError> {
        while workers.len() < self.max_concurrent {
            let Some(task) = self.repository.claim_next().await? else {
                break;
            };

            let id = task.id;
            let service = Arc::clone(&self.service);
            let cancellation = CancellationToken::new();
            running_tokens.insert(id, cancellation.clone());

            workers.spawn(async move {
                let result = service.execute_claimed(task, cancellation).await;
                (id, result)
            });
        }

        Ok(())
    }
}

impl ManagerHandle {
    /// 新任务已经写入数据库时调用，用命令通道立即唤醒 Manager。
    pub async fn wake(&self) -> Result<(), ManagerError> {
        self.command_tx
            .send(ManagerCommand::Wake)
            .await
            .map_err(|_| ManagerError::ManagerStopped)
    }

    pub async fn pause(&self, id: i64) -> Result<bool, ManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::Pause {
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

    pub async fn shutdown(&self) -> Result<(), ManagerError> {
        self.command_tx
            .send(ManagerCommand::Shutdown)
            .await
            .map_err(|_| ManagerError::ManagerStopped)
    }
}
