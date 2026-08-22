use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{self, ScrollAreaProps};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub fn ScrollArea(props: ScrollAreaProps) -> Element {
    let base = attributes!(div {
        class: "relative overscroll-contain rounded-[inherit] outline-none [scrollbar-color:var(--color-border)_transparent] [scrollbar-width:thin] focus-visible:ring-[3px] focus-visible:ring-ring/50"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      scroll_area::ScrollArea {
        direction: props.direction,
        always_show_scrollbars: props.always_show_scrollbars,
        scroll_type: props.scroll_type,
        attributes: merged,
        {props.children}
      }
    }
}
