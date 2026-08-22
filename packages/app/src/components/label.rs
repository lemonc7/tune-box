use dioxus::prelude::*;
use dioxus_primitives::label::{self, LabelProps};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub fn Label(props: LabelProps) -> Element {
    let base = attributes!(label {
        class: "flex items-center gap-2 text-sm font-medium leading-none text-foreground peer-disabled:cursor-not-allowed peer-disabled:opacity-50"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      label::Label { html_for: props.html_for, attributes: merged, {props.children} }
    }
}
