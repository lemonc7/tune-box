use dioxus::{fullstack::FullstackContext, prelude::*};
use dioxus_icons::lucide::{
    AudioLines, CircleCheck, CircleX, Download, Library, LoaderCircle, Music2, RefreshCw, Search, X,
};

use crate::{
    Route,
    components::{Button, ButtonSize, ButtonVariant},
};

#[derive(Clone, PartialEq, Eq)]
pub struct PreviewTrack {
    pub url: String,
    pub title: String,
    pub artists: String,
    pub cover_url: String,
}

#[component]
pub fn AppLayout() -> Element {
    rsx! {
      div { class: "flex h-dvh min-h-0 flex-col overflow-hidden",
        header { class: "z-30 shrink-0 border-b border-white/70 bg-white/80 shadow-[0_1px_0_rgba(24,24,27,0.04)] backdrop-blur-xl",
          nav {
            class: "mx-auto flex h-14 max-w-7xl items-center px-4 sm:px-6 lg:px-8",
            aria_label: "主导航",
            Link {
              to: Route::Search {},
              class: "mr-3 flex items-center gap-2.5 font-semibold tracking-tight text-zinc-950 no-underline sm:mr-8",
              span {
                class: "flex size-8 items-center justify-center rounded-lg bg-gradient-to-br from-indigo-500 to-violet-600 text-white shadow-lg shadow-indigo-500/20",
                aria_hidden: "true",
                Music2 { size: 17 }
              }
              span { class: "hidden sm:block", "Tune Box" }
            }
            div { class: "flex items-center gap-1 rounded-xl bg-zinc-100/80 p-1",
              Link {
                to: Route::Search {},
                class: "flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm font-medium text-zinc-500 no-underline transition hover:bg-white hover:text-zinc-950",
                active_class: "bg-white text-indigo-600 shadow-sm hover:bg-white hover:text-indigo-600",
                Search { size: 16 }
                span { class: "hidden sm:inline", "搜索" }
              }
              Link {
                to: Route::Downloads {},
                class: "flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm font-medium text-zinc-500 no-underline transition hover:bg-white hover:text-zinc-950",
                active_class: "bg-white text-indigo-600 shadow-sm hover:bg-white hover:text-indigo-600",
                Download { size: 16 }
                span { class: "hidden sm:inline", "下载" }
              }
              Link {
                to: Route::Library {},
                class: "flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm font-medium text-zinc-500 no-underline transition hover:bg-white hover:text-zinc-950",
                active_class: "bg-white text-indigo-600 shadow-sm hover:bg-white hover:text-indigo-600",
                Library { size: 16 }
                span { class: "hidden sm:inline", "曲库" }
              }
            }
          }
        }
        Outlet::<Route> {}
      }
    }
}

#[component]
pub fn ErrorLayout() -> Element {
    let player = use_context::<Signal<Option<PreviewTrack>>>();
    let main_class = if player.read().is_some() {
        "mx-auto flex min-h-0 w-full max-w-7xl flex-1 flex-col overflow-hidden px-4 py-4 pb-28 sm:px-6 sm:py-5 sm:pb-28 lg:px-8"
    } else {
        "mx-auto flex min-h-0 w-full max-w-7xl flex-1 flex-col overflow-hidden px-4 py-4 sm:px-6 sm:py-5 lg:px-8"
    };

    rsx! {
      main { class: main_class,
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
pub fn Player() -> Element {
    let mut player = use_context::<Signal<Option<PreviewTrack>>>();
    let current = player();

    rsx! {
      if let Some(track) = current {
        aside {
          class: "fixed inset-x-0 bottom-0 z-40 px-3 pb-3 sm:px-6 sm:pb-5",
          aria_label: "音乐试听播放器",
          div { class: "mx-auto flex max-w-5xl flex-wrap items-center gap-3 rounded-2xl border border-white/10 bg-zinc-950/95 px-3 py-3 text-white shadow-2xl shadow-zinc-950/25 backdrop-blur-xl sm:flex-nowrap sm:gap-4 sm:px-4",
            if track.cover_url.is_empty() {
              div {
                class: "flex size-11 shrink-0 items-center justify-center rounded-xl bg-white/10 text-indigo-300",
                aria_hidden: "true",
                AudioLines { size: 19 }
              }
            } else {
              img {
                class: "size-11 shrink-0 rounded-xl bg-white/10 object-cover ring-1 ring-white/10",
                src: "{track.cover_url}",
                alt: "",
              }
            }
            div { class: "min-w-0 flex-1 sm:w-52 sm:flex-none",
              p { class: "truncate text-sm font-medium text-white",
                "{track.title}"
              }
              p { class: "mt-0.5 truncate text-xs text-zinc-400", "{track.artists}" }
            }
            audio {
              class: "order-last h-9 min-w-0 basis-full sm:order-none sm:flex-1 sm:basis-auto",
              src: "{track.url}",
              controls: true,
              autoplay: true,
              preload: "none",
            }
            Button {
              class: "shrink-0",
              variant: ButtonVariant::Outline,
              size: ButtonSize::Icon,
              r#type: "button",
              aria_label: "关闭播放器",
              onclick: move |_| player.set(None),
              X { size: 16 }
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
        class: "flex items-start gap-3 rounded-xl border border-red-200/80 bg-red-50 px-4 py-3 text-sm text-red-700 shadow-sm shadow-red-900/5",
        role: "alert",
        CircleX { class: "mt-0.5 shrink-0", size: 17 }
        span { "{message}" }
      }
    }
}

#[component]
pub fn NoticeBanner(message: String) -> Element {
    rsx! {
      div {
        class: "flex items-start gap-3 rounded-xl border border-emerald-200/80 bg-emerald-50 px-4 py-3 text-sm text-emerald-700",
        role: "status",
        CircleCheck { class: "mt-0.5 shrink-0", size: 17 }
        span { "{message}" }
      }
    }
}

#[component]
pub fn EmptyState(message: String) -> Element {
    rsx! {
      div { class: "flex flex-col items-center rounded-2xl border border-dashed border-zinc-200 bg-zinc-50/60 px-8 py-16 text-center",
        span { class: "mb-4 flex size-12 items-center justify-center rounded-2xl bg-white text-zinc-400 shadow-sm ring-1 ring-zinc-200/70",
          Music2 { size: 21 }
        }
        p { class: "text-sm font-medium text-zinc-600", "{message}" }
        p { class: "mt-1 text-xs text-zinc-400",
          "Tune Box 会在这里展示你的音乐内容"
        }
      }
    }
}

#[component]
pub fn InfiniteScrollTrigger(
    loading: bool,
    failed: bool,
    on_load_more: EventHandler<()>,
) -> Element {
    rsx! {
      div {
        class: "flex justify-center",
        onvisible: move |event| {
            if event.is_intersecting().unwrap_or(false) && !loading && !failed {
                on_load_more.call(());
            }
        },
        Button {
          variant: ButtonVariant::Outline,
          r#type: "button",
          disabled: loading,
          onclick: move |_| on_load_more.call(()),
          if loading {
            LoaderCircle { class: "animate-spin", size: 15 }
            "加载中…"
          } else if failed {
            RefreshCw { size: 15 }
            "加载失败，点击重试"
          } else {
            "加载更多"
          }
        }
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
          class: "mt-7 inline-flex h-10 items-center justify-center rounded-lg bg-zinc-900 px-4 text-sm font-medium text-white no-underline shadow-sm transition hover:bg-zinc-800",
          "返回首页"
        }
      }
    }
}
