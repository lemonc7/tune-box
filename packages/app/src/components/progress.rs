use dioxus::prelude::*;
use dioxus_primitives::progress::{self, ProgressProps};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub fn Progress(props: ProgressProps) -> Element {
    let base = attributes!(div {
        class: "group/progress relative h-2 w-full min-w-0 overflow-hidden rounded-full bg-primary/20"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      progress::Progress { value: props.value, max: props.max, attributes: merged,
        progress::ProgressIndicator { class: "h-full w-[var(--progress-value,0%)] bg-primary transition-[width] duration-300 group-data-[state=indeterminate]/progress:w-1/2 group-data-[state=indeterminate]/progress:animate-pulse" }
      }
    }
}
