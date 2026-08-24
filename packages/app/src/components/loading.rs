use dioxus::prelude::*;
use dioxus_icons::lucide::LoaderCircle;

/// 占满父级剩余空间的页面级加载状态。
#[component]
pub fn Loading(message: String) -> Element {
    rsx! {
      div {
        class: "flex min-h-0 w-full flex-1 items-center justify-center",
        class: "gap-2 text-sm text-muted-foreground",
        role: "status",
        span { class: "inline-flex animate-spin", aria_hidden: true,
          LoaderCircle { size: 18 }
        }
        "{message}"
      }
    }
}
