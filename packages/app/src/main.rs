mod api;
pub mod components;
#[cfg(feature = "server")]
mod error;
mod hooks;
mod pages;
#[cfg(feature = "server")]
mod server;
mod view;
use dioxus::prelude::*;
use pages::{DownloadsPage as Downloads, LibraryPage as Library, SearchPage as Search};

use crate::{components::*, hooks::use_music_search_session};

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

#[derive(Routable, Clone, PartialEq, Debug)]
enum Route {
    #[layout(AppLayout)]
    #[layout(ErrorLayout)]
    #[route("/")]
    Search {},
    #[route("/downloads")]
    Downloads {},
    #[route("/library")]
    Library {},
}

#[component]
fn App() -> Element {
    let player = use_signal(|| None::<PreviewTrack>);
    use_context_provider(|| player);
    let search_session = use_music_search_session();
    use_context_provider(|| search_session);

    rsx! {
      document::Stylesheet { href: CSS }
      Router::<Route> {}
      Player {}
    }
}
