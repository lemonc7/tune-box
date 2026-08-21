mod api;
mod components;
mod hooks;
mod pages;
pub mod ui;
mod view;

#[cfg(feature = "server")]
mod error;
#[cfg(feature = "server")]
mod server;

use dioxus::prelude::*;

use components::{AppLayout, ErrorLayout};
use pages::{DownloadsPage as Downloads, LibraryPage as Library, SearchPage as Search};

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
    rsx! {
      document::Stylesheet { href: CSS }
      Router::<Route> {}
    }
}
