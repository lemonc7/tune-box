mod api;
mod components;
#[cfg(feature = "server")]
mod error;
mod hooks;
mod pages;
#[cfg(feature = "server")]
mod server;
mod view;
use crate::{
    components::{AppLayout, ErrorLayout, PlayerBar},
    hooks::{use_music_search_provider, use_player_provider},
    pages::*,
};
use dioxus::prelude::*;

static CSS: Asset = asset!("/assets/tailwind.css");

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

#[cfg(feature = "server")]
#[tokio::main]
async fn main() -> dioxus::Result<()> {
    init_tracing();
    server::run().await
}

#[cfg(feature = "server")]
fn init_tracing() {
    use dioxus::logger::tracing::{Level, subscriber::set_global_default};
    use tracing_subscriber::{EnvFilter, fmt, fmt::time::ChronoLocal};

    let level = if cfg!(debug_assertions) {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy()
        .add_directive("hyper_util=warn".parse().expect("有效的日志过滤规则"));
    let subscriber = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S".to_owned()))
        // Docker和dx serve都会捕获标准输出，自动终端检测会误判为不支持颜色。
        // 日志平台不支持ANSI时可通过NO_COLOR=1关闭。
        .with_ansi(std::env::var_os("NO_COLOR").is_none());

    let result = if std::env::var_os("DIOXUS_CLI_ENABLED").is_some() {
        // dx serve自身已经输出时间，避免重复时间戳。
        set_global_default(subscriber.without_time().finish())
    } else {
        set_global_default(subscriber.finish())
    };
    result.expect("日志系统只能初始化一次");
}

#[derive(Routable, Clone)]
enum Route {
    #[layout(AppLayout)]
    #[layout(ErrorLayout)]
    #[route("/", SearchPage)]
    Search {},
    #[route("/downloads", DownloadsPage)]
    Downloads {},
    #[route("/library", LibraryPage)]
    Library {},
    // 兜底404路由
    #[route("/:..segments", NotFoundPage)]
    NotFound { segments: Vec<String> },
}

#[component]
fn App() -> Element {
    use_player_provider();
    use_music_search_provider();
    rsx! {
      document::Stylesheet { href: CSS }
      PlayerBar {}
      // 路由数据暂停时不能冒泡到Dioxus默认根边界，否则根边界会暂时卸载播放器。
      SuspenseBoundary { fallback: |_| rsx! {}, Router::<Route> {} }
    }
}
