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
    hooks::use_player_provider,
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
    rsx! {
      document::Stylesheet { href: CSS }
      Router::<Route> {}
      // 放到Router外部，路由切换不会卸载播放器
      PlayerBar {}
    }
}
