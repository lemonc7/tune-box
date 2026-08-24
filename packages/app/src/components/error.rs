use dioxus::prelude::*;
use dioxus_icons::lucide::{CircleAlert, House, RefreshCw, TriangleAlert};

use crate::Route;

#[component]
pub fn ErrorCard(
    title: String,
    description: String,
    #[props(default)] retry: Option<EventHandler<()>>,
) -> Element {
    rsx! {
      div {
        class: "flex w-full items-start gap-3 rounded-lg",
        class: "border border-destructive/30 bg-destructive/5 p-4",
        role: "alert",
        div {
          class: "flex size-9 shrink-0 items-center justify-center",
          class: "rounded-md bg-destructive/10 text-destructive",
          aria_hidden: true,
          TriangleAlert { size: 18 }
        }
        div { class: "min-w-0 flex-1",
          div { class: "font-medium text-foreground", "{title}" }
          p { class: "mt-1 text-pretty text-sm leading-relaxed text-muted-foreground",
            "{description}"
          }
          if let Some(retry) = retry {
            button {
              class: "mt-3 inline-flex h-8 items-center justify-center",
              class: "gap-2 rounded-md border border-border",
              class: "bg-background px-3 text-sm font-medium",
              class: "text-foreground transition-colors",
              class: "hover:bg-muted focus-visible:outline-none",
              class: "focus-visible:ring-2 focus-visible:ring-ring/50",
              r#type: "button",
              onclick: move |_| retry.call(()),
              RefreshCw { size: 14 }
              "重试"
            }
          }
        }
      }
    }
}

#[component]
pub fn ErrorPage(code: u16, title: String, message: String) -> Element {
    rsx! {
      section {
        class: "flex min-h-64 w-full flex-1 flex-col",
        class: "items-center justify-center px-6 py-12 text-center",
        aria_labelledby: "error-page-title",
        div {
          class: "flex size-12 items-center justify-center",
          class: "rounded-xl bg-destructive/10 text-destructive",
          CircleAlert { size: 24 }
        }
        div { class: "mt-6 text-sm font-medium text-destructive", "错误 {code}" }
        h1 {
          id: "error-page-title",
          class: "mt-2 text-2xl font-semibold tracking-tight",
          class: "text-foreground sm:text-3xl",
          "{title}"
        }
        p {
          class: "mt-3 max-w-md text-balance text-sm",
          class: "text-muted-foreground sm:text-base",
          "{message}"
        }

        Link {
          to: Route::Search {},
          class: "\
              mt-6 inline-flex h-9 items-center justify-center gap-2 \
              rounded-md bg-primary px-4 text-sm font-medium \
              text-primary-foreground no-underline \
              transition-colors hover:bg-primary/80",
          House { size: 16 }
          "返回首页"
        }
      }
    }
}
