use dioxus::{fullstack::FullstackContext, prelude::*};
use dioxus_icons::lucide::{Download, Library, Music2, Search};

use crate::{Route, components::ErrorPage, hooks::use_player};

#[component]
pub fn AppLayout() -> Element {
    rsx! {
      div {
        class: "flex h-dvh min-h-0 flex-col",
        class: "overflow-hidden bg-background text-foreground",
        header {
          class: "z-30 shrink-0 border-b border-border",
          class: "bg-background/80 shadow-sm backdrop-blur-xl",
          nav {
            class: "mx-auto flex h-14 w-full max-w-7xl",
            class: "items-center px-4 sm:px-6 lg:px-8",
            aria_label: "主导航",
            Link {
              to: Route::Search {},
              class: "\
                  mr-3 flex items-center gap-2.5 \
                  font-semibold tracking-tight \
                  text-foreground no-underline sm:mr-8",
              aria_label: "Tune Box 首页",

              span {
                class: "flex size-8 items-center justify-center",
                class: "rounded-lg bg-primary text-primary-foreground shadow-sm",
                aria_hidden: true,
                Music2 { size: 17 }
              }
              span { class: "hidden sm:block", "Tune Box" }
            }

            div { class: "flex items-center gap-1 rounded-xl bg-muted/80 p-1",
              Link {
                to: Route::Search {},
                class: "\
                    flex items-center gap-2 rounded-lg \
                    px-3 py-1.5 text-sm font-medium \
                    text-muted-foreground no-underline \
                    transition-colors hover:bg-background hover:text-foreground",
                active_class: "\
                    bg-background text-primary shadow-sm \
                    hover:bg-background hover:text-primary",
                aria_label: "搜索",
                Search { size: 16 }
                span { class: "hidden sm:inline", "搜索" }
              }
              Link {
                to: Route::Downloads {},
                class: "\
                    flex items-center gap-2 rounded-lg \
                    px-3 py-1.5 text-sm font-medium \
                    text-muted-foreground no-underline \
                    transition-colors hover:bg-background hover:text-foreground",
                active_class: "\
                    bg-background text-primary shadow-sm \
                    hover:bg-background hover:text-primary",
                aria_label: "下载",
                Download { size: 16 }
                span { class: "hidden sm:inline", "下载" }
              }
              Link {
                to: Route::Library {},
                class: "\
                    flex items-center gap-2 rounded-lg \
                    px-3 py-1.5 text-sm font-medium \
                    text-muted-foreground no-underline \
                    transition-colors hover:bg-background hover:text-foreground",
                active_class: "\
                    bg-background text-primary shadow-sm \
                    hover:bg-background hover:text-primary",
                aria_label: "曲库",
                Library { size: 16 }
                span { class: "hidden sm:inline", "曲库" }
              }
            }
          }
        }
        // 渲染ErrorLayout
        Outlet::<Route> {}
      }
    }
}

#[component]
pub fn ErrorLayout() -> Element {
    let player = use_player();
    rsx! {
      main {
        class: "mx-auto flex min-h-0 w-full max-w-7xl",
        class: "flex-1 flex-col overflow-y-auto",
        class: "px-4 py-4 sm:px-6 sm:py-5 lg:px-8",
        class: if player.is_active() { "pb-28 sm:pb-28" },
        ErrorBoundary {
          handle_error: move |context: ErrorContext| {
              let error = context.error().expect("ErrorBoundary中必须存在错误");
              let http_error = FullstackContext::commit_error_status(error);
              match http_error.status {
                  StatusCode::NOT_FOUND => rsx! {
                    ErrorPage {
                      code: 404,
                      title: "页面不存在",
                      message: "你访问的页面可能已被移动或删除。",
                    }
                  },
                  status => rsx! {
                    ErrorPage {
                      code: status.as_u16(),
                      title: "页面加载失败",
                      message: "服务器暂时无法完成请求，请稍后重试。",
                    }
                  },
              }
          },
          // 渲染Search/Download/Library页面
          Outlet::<Route> {}
        }
      }
    }
}
