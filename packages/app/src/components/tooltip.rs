use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;
use dioxus_primitives::tooltip::{self, TooltipContentProps, TooltipProps, TooltipTriggerProps};
use dioxus_primitives::{ContentAlign, ContentSide};

#[component]
pub fn Tooltip(props: TooltipProps) -> Element {
    let base = attributes!(div {
        class: "relative inline-flex",
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      tooltip::Tooltip {
        disabled: props.disabled,
        open: props.open,
        default_open: props.default_open,
        on_open_change: props.on_open_change,
        attributes: merged,
        {props.children}
      }
    }
}

#[component]
pub fn TooltipTrigger(props: TooltipTriggerProps) -> Element {
    let base = attributes!(button {
        class: "inline-flex outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50",
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      tooltip::TooltipTrigger { id: props.id, r#as: props.r#as, attributes: merged, {props.children} }
    }
}

#[component]
pub fn TooltipContent(props: TooltipContentProps) -> Element {
    let side_class = match props.side {
        ContentSide::Top => "bottom-full mb-2",
        ContentSide::Right => "left-full ml-2",
        ContentSide::Bottom => "top-full mt-2",
        ContentSide::Left => "right-full mr-2",
    };
    let align_class = match (props.side, props.align) {
        (ContentSide::Top | ContentSide::Bottom, ContentAlign::Start) => "left-0",
        (ContentSide::Top | ContentSide::Bottom, ContentAlign::Center) => {
            "left-1/2 -translate-x-1/2"
        }
        (ContentSide::Top | ContentSide::Bottom, ContentAlign::End) => "right-0",
        (ContentSide::Left | ContentSide::Right, ContentAlign::Start) => "top-0",
        (ContentSide::Left | ContentSide::Right, ContentAlign::Center) => {
            "top-1/2 -translate-y-1/2"
        }
        (ContentSide::Left | ContentSide::Right, ContentAlign::End) => "bottom-0",
    };
    let class = format!(
        "absolute z-50 max-w-64 rounded-md bg-primary px-3 py-1.5 text-xs leading-5 text-primary-foreground opacity-0 shadow-md transition-opacity duration-150 data-[state=open]:opacity-100 data-[state=closed]:pointer-events-none {side_class} {align_class}"
    );
    let base = attributes!(div { class });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      tooltip::TooltipContent {
        id: props.id,
        side: props.side,
        align: props.align,
        attributes: merged,
        {props.children}
      }
    }
}
