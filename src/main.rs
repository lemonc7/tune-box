pub mod error;
pub mod server;
pub mod shared;
pub mod web;

use std::{collections::HashMap, error::Error, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::State,
    http::{self, HeaderName, Request},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::{net::TcpListener, signal};
use tokio_util::sync::CancellationToken;
use tower::{ServiceBuilder, limit::GlobalConcurrencyLimitLayer};
use tower_http::{
    LatencyUnit,
    compression::CompressionLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::{Level, info_span};
use tracing_subscriber::{EnvFilter, fmt::time::ChronoLocal};

use crate::{
    server::{
        config::{Config, Server, load_config},
        db::{self, Repository},
        download::Downloader,
        manager::Manager,
        mcp::MusicMcp,
        provider::{Provider, kuwo::KuwoProvider},
        service::Service,
        tag::Embedder,
    },
    web::{
        assets::assets,
        handler::{
            cancel_download, create_download, delete_download, delete_song, download_card,
            downloads, health_check, home, library, library_items, more_downloads,
            more_search_results, pause_download, resume_download, retry_download, search,
            stream_preview, sync_library,
        },
        state::AppState,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 初始化日志系统
    init_tracing();

    let config = load_config("./config/config.toml")?;
    run(config).await
}

async fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let pool = db::connect("sqlite://data/tune.db").await?;
    let repository = Repository::new(pool);

    // 搜索，解析，歌词和封面等短时间使用总超时
    let api_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.server.http_timeout))
        .build()?;

    // 音频下载不设置总超时，避免大文件被中途取消
    let download_client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(config.server.download_read_timeout))
        .build()?;

    let kw = KuwoProvider::new(
        api_client.clone(),
        config.providers.kuwo.search_url,
        config.providers.kuwo.resolve_url,
        config.providers.kuwo.lyric_url,
    );

    let mut providers: HashMap<String, Box<dyn Provider>> = HashMap::new();
    providers.insert(kw.id().to_owned(), Box::new(kw));

    let downloader = Downloader::new(
        download_client.clone(),
        "downloads",
        config.server.max_bytes,
    );

    let service = Arc::new(Service::new(
        repository.clone(),
        providers,
        downloader,
        download_client,
        Embedder::new(Duration::from_secs(config.server.http_timeout))?,
    ));

    let (manager, manager_handle) = Manager::new(
        repository,
        Arc::clone(&service),
        config.server.max_download_concurrent,
    );

    let state = AppState::new(service, manager_handle.clone());
    let app = build_router(&config.server, state);

    let address = format!("0.0.0.0:{}", config.server.port);
    let listener = TcpListener::bind(&address).await?;

    tracing::info!(%address, "服务器已启动");

    // 关闭信号，异步共享
    let shutdown = CancellationToken::new();
    let server_shutdown = shutdown.clone();

    // 启动http服务器任务
    let mut server_task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(server_shutdown.cancelled_owned())
            .await
    });

    // 启动manager任务
    let mut manager_task = tokio::spawn(manager.run());

    tokio::select! {
      // 收到关闭信号
      _ = shutdown_signal() => {
        tracing::info!("收到关闭信号");

        // 先关闭http服务器
        shutdown.cancel();
        // 发送关闭命令给manger
        let _ = manager_handle.shutdown().await;

        // 同时等待两个任务完成
        let (server_result, manager_result) = tokio::join!(&mut server_task, &mut manager_task);

        server_result??;
        manager_result??;
      }
      // http服务先停止
      server_result = &mut server_task => {
        tracing::warn!("HTTP 服务已经停止");

        shutdown.cancel();
        let _ = manager_handle.shutdown().await;

        server_result??;
        manager_task.await??;
      }
      // manager先停止
      manager_result = &mut manager_task => {
        tracing::error!("下载管理器意外终止");

        // 通知http服务完成优雅关闭
        shutdown.cancel();
        server_task.await??;

        manager_result??;
        return Err("下载管理器意外终止".into());
      }
    }

    tracing::info!("服务器已关闭");
    Ok(())
}

async fn shutdown_signal() {
    cfg_select! {
        unix => {
            use tokio::signal::unix::{SignalKind, signal as unix_signal};

            let mut terminate = unix_signal(SignalKind::terminate())
                .expect("注册SIGTERM处理器失败");

            tokio::select! {
                result = signal::ctrl_c() => {
                    if let Err(error) = result {
                        tracing::error!(%error, "监听Ctrl+C失败");
                    }
                }
                _ = terminate.recv() => {}
            }
        }
        _ => {
            if let Err(error) = signal::ctrl_c().await {
                tracing::error!(%error, "监听Ctrl+C失败");
            }
        }
    }
}

fn build_router(config: &Server, state: AppState) -> Router {
    let mcp = MusicMcp::new(Arc::clone(&state.service), state.manager.clone());
    let mcp_service: StreamableHttpService<MusicMcp, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(mcp.clone()),
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_json_response(true)
                .disable_allowed_hosts(),
        );
    let mcp_routes = Router::new()
        .nest_service("/mcp", mcp_service)
        .layer(from_fn_with_state(config.mcp_token.clone(), authorize_mcp));

    // 需要调用handler/service/provider或数据库
    let dynamic_routes = Router::new()
        .route("/", get(home))
        .route("/search", get(search))
        .route("/search/more", get(more_search_results))
        .route("/preview", get(stream_preview))
        .route("/downloads", get(downloads).post(create_download))
        .route("/downloads/more", get(more_downloads))
        .route("/downloads/{id}/card", get(download_card))
        .route("/downloads/{id}/pause", post(pause_download))
        .route("/downloads/{id}/resume", post(resume_download))
        .route("/downloads/{id}/cancel", post(cancel_download))
        .route("/downloads/{id}/retry", post(retry_download))
        .route("/downloads/{id}/delete", post(delete_download))
        .route("/library", get(library))
        .route("/library/items", get(library_items))
        .route("/library/sync", post(sync_library))
        .route("/library/{id}/delete", post(delete_song))
        .layer(
            ServiceBuilder::new()
                // 请求体最大64KB
                .layer(RequestBodyLimitLayer::new(64 * 1024))
                // 请求超时
                .layer(TimeoutLayer::with_status_code(
                    http::StatusCode::GATEWAY_TIMEOUT,
                    Duration::from_secs(config.request_timeout),
                ))
                // 限制http请求数量
                .layer(GlobalConcurrencyLimitLayer::new(config.max_http_concurrent))
                // 压缩传输
                .layer(CompressionLayer::new()),
        );

    // 静态资源文件
    let static_routes = Router::new().route("/assets/{*path}", get(assets));

    let request_id = HeaderName::from_static("x-request-id");
    let trace_request_id = request_id.clone();

    Router::new()
        .merge(dynamic_routes)
        .merge(static_routes)
        .merge(mcp_routes)
        .route("/health", get(health_check))
        .layer(
            ServiceBuilder::new()
                // 自动生成请求id
                .layer(SetRequestIdLayer::new(request_id.clone(), MakeRequestUuid))
                .layer(
                    // 为请求创建日志span: request_id+method+uri
                    TraceLayer::new_for_http()
                        .make_span_with(move |request: &Request<_>| {
                            let request_id = request
                                .headers()
                                .get(&trace_request_id)
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("-");
                            info_span!(
                              "http_request",
                              request_id,
                              method = %request.method(),
                              uri = %request.uri(),
                            )
                        })
                        .on_response(
                            DefaultOnResponse::new()
                                .level(Level::INFO)
                                .latency_unit(LatencyUnit::Millis),
                        ),
                )
                // 将ID加进响应头，传给浏览器
                .layer(PropagateRequestIdLayer::new(request_id)),
        )
        .with_state(state)
}

async fn authorize_mcp(
    State(token): State<String>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"));
    if !authorized {
        return http::StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

fn init_tracing() {
    // 默认只显示 info 及以上日志，可通过 RUST_LOG 临时调整。
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("tune_box=info,tower_http=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_timer(ChronoLocal::rfc_3339())
        // 不显示模块路径
        .with_target(false)
        // 紧凑日志
        .compact()
        // 注册为全局日志处理器
        .init()
}
