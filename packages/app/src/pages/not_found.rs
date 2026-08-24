use dioxus::{fullstack::FullstackContext, prelude::*};

use crate::components::ErrorPage;

#[component]
pub fn NotFoundPage(segments: Vec<String>) -> Element {
    let _ = segments;

    // SSR请求时返回真正的HTTP 404
    // 客户端路由跳转时没有FullstackContext，该操作不会修改HTTP响应
    FullstackContext::commit_http_status(StatusCode::NOT_FOUND, Some("页面不存在".to_owned()));

    rsx! {
      ErrorPage {
        code: 404,
        title: "页面不存在",
        message: "你访问的页面可能已被移动或删除。",
      }
    }
}
