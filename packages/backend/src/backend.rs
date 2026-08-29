use std::{collections::HashMap, sync::Arc, time::Duration};

use sqlx::SqlitePool;

use crate::{
    config::Config,
    db::{self, Repository},
    download::Downloader,
    error::BackendError,
    events::DownloadEvents,
    manager::{Manager, ManagerHandle},
    net::public_redirect_policy,
    provider::{Provider, kuwo::KuwoProvider, netease::NeteaseProvider},
    service::Service,
    tag::embed::Embedder,
};

/// 已完成初始化的后端资源，由应用入口负责运行和关闭。
pub struct Backend {
    pub pool: SqlitePool,
    pub service: Arc<Service>,
    pub manager: Manager,
    pub manager_handle: ManagerHandle,
    pub download_events: DownloadEvents,
}

impl Backend {
    pub async fn new(config: &Config) -> Result<Self, BackendError> {
        let api_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.server.http_timeout))
            .build()?;

        // 音频下载不设置总超时，避免大文件在正常下载期间被取消。
        // 下载和试听的目标地址都由 provider 接口返回，重定向必须逐跳校验，
        // 否则请求前的地址校验会被一次 302 绕过。
        let download_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(config.server.download_read_timeout))
            .redirect(public_redirect_policy())
            .build()?;

        let kuwo = KuwoProvider::new(
            api_client.clone(),
            &config.providers.kuwo.search_url,
            &config.providers.kuwo.resolve_url,
            &config.providers.kuwo.lyric_url,
        )?;
        let netease = NeteaseProvider::new(
            api_client.clone(),
            &config.providers.netease.base_url,
            &config.providers.netease.extended_resolve_url,
        )?;

        let mut providers = HashMap::<String, Box<dyn Provider>>::new();
        providers.insert(kuwo.id().to_owned(), Box::new(kuwo));
        providers.insert(netease.id().to_owned(), Box::new(netease));

        // 所有可能失败的网络组件都先初始化，再创建需要显式关闭的连接池。
        let embedder = Embedder::new(Duration::from_secs(config.server.http_timeout))?;
        let pool = db::connect("sqlite://data/tune.db").await?;
        let repository = Repository::new(pool.clone());
        let download_events = repository.download_events();
        let service = Arc::new(Service::new(
            repository.clone(),
            providers,
            Downloader::new(
                download_client.clone(),
                "downloads",
                config.server.max_bytes,
            ),
            download_client,
            embedder,
        ));
        let (manager, manager_handle) = Manager::new(
            repository,
            Arc::clone(&service),
            config.server.max_download_concurrent,
        );

        Ok(Self {
            pool,
            service,
            manager,
            manager_handle,
            download_events,
        })
    }
}
