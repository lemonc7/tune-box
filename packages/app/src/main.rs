mod api;
mod components;
mod pages;
mod view;

#[cfg(feature = "server")]
mod server;
#[cfg(feature = "server")]
mod state;

use dioxus::prelude::*;

use components::ErrorLayout;
use pages::{DownloadsPage as Downloads, LibraryPage as Library, SearchPage as Search};

static CSS: Asset = asset!("/assets/tailwind.css");

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

#[cfg(feature = "server")]
#[tokio::main]
async fn main() -> Result<(), server::BoxError> {
    dioxus::logger::initialize_default();
    server::run().await
}

#[derive(Routable, Clone, PartialEq, Debug)]
enum Route {
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
