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
    dioxus::logger::initialize_default();
    server::run().await
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
