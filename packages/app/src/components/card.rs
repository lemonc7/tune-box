use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub fn Card(
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "flex flex-col gap-6 rounded-xl border border-border bg-card py-6 text-card-foreground shadow-sm"
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
      div { "data-slot": "card", ..merged, {children} }
    }
}

#[component]
pub fn CardHeader(
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "grid auto-rows-min grid-rows-[auto_auto] items-start gap-1.5 px-6 [&:has([data-slot=card-action])]:grid-cols-[1fr_auto]"
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
      div { "data-slot": "card-header", ..merged, {children} }
    }
}

#[component]
pub fn CardTitle(
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "font-semibold leading-none"
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
      div { "data-slot": "card-title", ..merged, {children} }
    }
}

#[component]
pub fn CardDescription(
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "text-sm text-muted-foreground"
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
      div { "data-slot": "card-description", ..merged, {children} }
    }
}

#[component]
pub fn CardAction(
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "col-start-2 row-span-2 row-start-1 place-self-start justify-self-end"
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
      div { "data-slot": "card-action", ..merged, {children} }
    }
}

#[component]
pub fn CardContent(
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div { class: "px-6" });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
      div { "data-slot": "card-content", ..merged, {children} }
    }
}

#[component]
pub fn CardFooter(
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "flex items-center px-6"
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
      div { "data-slot": "card-footer", ..merged, {children} }
    }
}
