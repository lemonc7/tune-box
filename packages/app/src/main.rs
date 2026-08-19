pub mod server;

use dioxus::prelude::*;

static CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
      document::Stylesheet { href: CSS }
      div { "Hello World" }
    }
}
