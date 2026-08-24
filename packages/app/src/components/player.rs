use dioxus::prelude::*;
use dioxus_icons::lucide::{Music2, X};

use crate::{hooks::use_player, view::preview_url};

#[component]
pub fn PlayerBar() -> Element {
    let mut player = use_player();
    let state = player.snapshot();

    let Some(track) = state.track else {
        return rsx! {};
    };

    let source = preview_url(&track.provider, &track.id);
    let artists = track.artists.join(" / ");
    let revision = state.revision;

    rsx! {
      aside {
        class: "fixed inset-x-0 bottom-0 z-50",
        class: "border-t border-border bg-background/95",
        class: "shadow-[0_-4px_20px_rgba(0,0,0,0.06)] backdrop-blur-xl",
        aria_label: "音乐播放器",
        div {
          class: "mx-auto flex min-h-20 w-full max-w-7xl",
          class: "items-center gap-4 px-4 py-3 sm:px-6 lg:px-8",

          if track.cover_url.is_empty() {
            div {
              class: "flex size-12 shrink-0 items-center justify-center",
              class: "rounded-lg bg-muted text-muted-foreground",
              aria_hidden: true,
              Music2 { size: 20 }
            }
          } else {
            img {
              class: "size-12 shrink-0 rounded-lg object-cover",
              src: track.cover_url,
              alt: "",
            }
          }
          div { class: "hidden min-w-0 w-48 shrink-0 sm:block",
            div { class: "truncate text-sm font-medium text-foreground",
              "{track.name}"
            }
            div { class: "truncate text-xs text-muted-foreground", "{artists}" }
          }

          div { class: "min-w-0 flex-1",
            audio {
              key: "{revision}",
              class: "h-10 w-full",
              src: source,
              controls: true,
              autoplay: true,
              preload: "metadata",
              onerror: move |_| player.set_error("试听资源加载失败"),
            }

            if let Some(error) = state.error {
              p { class: "mt-1 text-xs text-destructive", "{error}" }
            }
          }

          button {
            class: "flex size-8 shrink-0 items-center justify-center",
            class: "rounded-md text-muted-foreground",
            class: "bg-muted hover:text-foreground",
            r#type: "button",
            aria_label: "关闭播放器",
            onclick: move |_| player.close(),
            X { size: 18 }
          }
        }
      }
    }
}
