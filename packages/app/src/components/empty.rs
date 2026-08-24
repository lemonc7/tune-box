use dioxus::prelude::*;
use dioxus_icons::lucide::Inbox;

#[component]
pub fn Empty(title: String, description: String) -> Element {
    rsx! {
      div {
        class: "flex min-h-64 w-full min-w-0 flex-col",
        class: "items-center justify-center gap-4",
        class: "rounded-xl border border-dashed border-border",
        class: "p-6 text-center",
        role: "status",
        div {
          class: "flex size-10 shrink-0 items-center justify-center",
          class: "rounded-lg bg-muted text-muted-foreground",
          aria_hidden: true,
          Inbox { size: 20 }
        }
        div { class: "flex max-w-sm flex-col items-center gap-2",
          div { class: "text-lg font-medium tracking-tight text-foreground",
            "{title}"
          }
          p { class: "text-balance text-sm text-muted-foreground", "{description}" }
        }
      }
    }
}
