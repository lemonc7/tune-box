use contracts::download::DownloadEvent;
use tokio::sync::broadcast;

const DOWNLOAD_EVENT_CAPACITY: usize = 64;

// 进程内下载事件总线。发送端没有订阅者时丢弃事件，不影响后台任务。
#[derive(Debug, Clone)]
pub struct DownloadEvents {
    sender: broadcast::Sender<DownloadEvent>,
}

impl Default for DownloadEvents {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(DOWNLOAD_EVENT_CAPACITY);
        Self { sender }
    }
}

impl DownloadEvents {
    pub fn subscribe(&self) -> broadcast::Receiver<DownloadEvent> {
        self.sender.subscribe()
    }

    pub(crate) fn progress(&self, id: i64, downloaded_bytes: i64, total_bytes: Option<i64>) {
        // 无页面订阅时send会返回错误；状态已经持久化到数据库，可以安全忽略。
        let _ = self.sender.send(DownloadEvent::Progress {
            id,
            downloaded_bytes,
            total_bytes,
        });
    }

    pub(crate) fn changed(&self, id: i64) {
        let _ = self.sender.send(DownloadEvent::Changed { id });
    }
}
