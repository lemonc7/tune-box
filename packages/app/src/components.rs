use dioxus::{fullstack::FullstackContext, prelude::*};

use crate::Route;

#[derive(Clone, PartialEq, Eq)]
pub struct PreviewTrack {
    pub url: String,
    pub title: String,
    pub artists: String,
    pub cover_url: String,
}

#[component]
pub fn AppLayout() -> Element {
    let player = use_signal(|| None::<PreviewTrack>);
    use_context_provider(|| player);

    rsx! {
      header { class: "sticky top-0 z-30 border-b border-zinc-200 bg-white/90 backdrop-blur-md",
        nav {
          class: "mx-auto flex h-14 max-w-6xl items-center px-4 sm:px-6",
          aria_label: "主导航",
          Link {
            to: Route::Search {},
            class: "mr-3 flex items-center gap-2 font-semibold tracking-tight text-zinc-950 no-underline sm:mr-6",
            span {
              class: "flex h-7 w-7 items-center justify-center rounded-md bg-zinc-900 text-sm text-white",
              aria_hidden: "true",
              "♪"
            }
            span { class: "hidden sm:inline", "Tune Box" }
          }
          div { class: "flex items-center gap-1",
            Link {
              to: Route::Search {},
              class: "rounded-md px-3 py-2 text-sm font-medium text-zinc-600 no-underline transition-colors hover:bg-zinc-100 hover:text-zinc-950",
              active_class: "bg-zinc-900 text-white hover:bg-zinc-800 hover:text-white",
              "搜索"
            }
            Link {
              to: Route::Downloads {},
              class: "rounded-md px-3 py-2 text-sm font-medium text-zinc-600 no-underline transition-colors hover:bg-zinc-100 hover:text-zinc-950",
              active_class: "bg-zinc-900 text-white hover:bg-zinc-800 hover:text-white",
              "下载"
            }
            Link {
              to: Route::Library {},
              class: "rounded-md px-3 py-2 text-sm font-medium text-zinc-600 no-underline transition-colors hover:bg-zinc-100 hover:text-zinc-950",
              active_class: "bg-zinc-900 text-white hover:bg-zinc-800 hover:text-white",
              "曲库"
            }
          }
        }
      }
      Outlet::<Route> {}
      Player {}
    }
}

#[component]
pub fn ErrorLayout() -> Element {
    rsx! {
      main { class: "mx-auto max-w-6xl px-4 py-8 pb-28 sm:px-6",
        ErrorBoundary {
          handle_error: move |context: ErrorContext| {
              let http_error = FullstackContext::commit_error_status(context.error().unwrap());
              match http_error.status {
                  StatusCode::NOT_FOUND => rsx! {
                    ErrorPage {
                      code: "404".to_owned(),
                      title: "页面不存在".to_owned(),
                      message: "你访问的页面可能已被移动或删除。".to_owned(),
                    }
                  },
                  _ => rsx! {
                    ErrorPage {
                      code: "500".to_owned(),
                      title: "页面加载失败".to_owned(),
                      message: "服务器暂时无法完成请求，请稍后重试。".to_owned(),
                    }
                  },
              }
          },
          Outlet::<Route> {}
        }
      }
    }
}

#[component]
fn Player() -> Element {
    let mut player = use_context::<Signal<Option<PreviewTrack>>>();
    let current = player();

    rsx! {
      if let Some(track) = current {
        aside {
          class: "fixed inset-x-0 bottom-0 z-40 border-t border-zinc-200 bg-white/95 shadow-[0_-4px_20px_rgba(0,0,0,0.05)] backdrop-blur-md",
          aria_label: "音乐试听播放器",
          div { class: "mx-auto flex max-w-6xl flex-wrap items-center gap-3 px-4 py-3 sm:flex-nowrap sm:gap-4 sm:px-6",
            if track.cover_url.is_empty() {
              div {
                class: "flex h-11 w-11 shrink-0 items-center justify-center rounded-md bg-zinc-100 text-zinc-400",
                aria_hidden: "true",
                "♪"
              }
            } else {
              img {
                class: "h-11 w-11 shrink-0 rounded-md bg-zinc-100 object-cover",
                src: "{track.cover_url}",
                alt: "",
              }
            }
            div { class: "min-w-0 flex-1 sm:w-52 sm:flex-none",
              p { class: "truncate text-sm font-medium text-zinc-950",
                "{track.title}"
              }
              p { class: "mt-0.5 truncate text-xs text-zinc-500", "{track.artists}" }
            }
            audio {
              class: "order-last h-9 min-w-0 basis-full sm:order-none sm:flex-1 sm:basis-auto",
              src: "{track.url}",
              controls: true,
              autoplay: true,
              preload: "none",
            }
            button {
              class: "inline-flex h-9 w-9 shrink-0 cursor-pointer items-center justify-center rounded-md border-0 bg-transparent text-xl text-zinc-500 transition hover:bg-zinc-100 hover:text-zinc-950",
              r#type: "button",
              aria_label: "关闭播放器",
              onclick: move |_| player.set(None),
              "×"
            }
          }
        }
      }
    }
}

#[component]
pub fn ErrorBanner(message: String) -> Element {
    rsx! {
      div {
        class: "rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 shadow-sm",
        role: "alert",
        "{message}"
      }
    }
}

#[component]
pub fn NoticeBanner(message: String) -> Element {
    rsx! {
      div {
        class: "rounded-md border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-700",
        role: "status",
        "{message}"
      }
    }
}

#[component]
pub fn EmptyState(message: String) -> Element {
    rsx! {
      div { class: "rounded-lg border border-dashed border-zinc-200 bg-zinc-50/50 px-8 py-14 text-center text-sm text-zinc-500",
        "{message}"
      }
    }
}

#[component]
fn ErrorPage(code: String, title: String, message: String) -> Element {
    rsx! {
      document::Title { "{title} - Tune Box" }
      section { class: "mx-auto flex min-h-[55vh] max-w-xl flex-col items-center justify-center text-center",
        p { class: "text-sm font-semibold tracking-[0.2em] text-zinc-400",
          "{code}"
        }
        h1 { class: "mt-3 text-3xl font-semibold tracking-tight text-zinc-950",
          "{title}"
        }
        p { class: "mt-3 text-sm leading-6 text-zinc-500", "{message}" }
        Link {
          to: Route::Search {},
          class: "mt-7 inline-flex h-10 items-center justify-center rounded-md bg-zinc-900 px-4 text-sm font-medium text-white no-underline shadow-sm transition hover:bg-zinc-800",
          "返回首页"
        }
      }
    }
}
