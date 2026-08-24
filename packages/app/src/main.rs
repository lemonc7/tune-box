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
    hooks::{MusicProviders, use_music_search_provider, use_player_provider},
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
      RouterHost {}
    }
}

// Provider 数据在这个持久组件中只加载一次，不跟随具体页面卸载。
#[component]
fn RouterHost() -> Element {
    let providers = use_server_future(api::list_providers)?;
    use_context_provider(|| MusicProviders(providers));

    rsx! {
      Router::<Route> {}
    }
}
