mod api;
#[cfg(feature = "server")]
mod error;
mod hooks;
#[cfg(feature = "server")]
mod server;
mod view;
use dioxus::prelude::*;

use crate::hooks::use_music_search_session;

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

#[component]
fn App() -> Element {
    rsx! {
        document::Stylesheet { href: CSS }
        div { class: "flex" }
    }
}
