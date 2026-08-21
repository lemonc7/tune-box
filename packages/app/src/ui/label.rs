use dioxus::prelude::*;

const BASE_CLASS: &str = "flex items-center gap-2 text-sm font-medium leading-none text-foreground";

#[component]
pub fn Label(
    html_for: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
      label { class: BASE_CLASS, r#for: html_for, ..attributes, {children} }
    }
}
