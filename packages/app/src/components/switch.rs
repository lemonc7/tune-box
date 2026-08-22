use dioxus::prelude::*;
use dioxus_primitives::switch::{self, SwitchProps};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub fn Switch(props: SwitchProps) -> Element {
    let base = attributes!(button {
        class: "group/switch inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border border-transparent bg-input shadow-xs outline-none transition-colors data-[state=checked]:bg-primary data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50 focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      switch::Switch {
        checked: props.checked,
        default_checked: props.default_checked,
        disabled: props.disabled,
        required: props.required,
        name: props.name,
        value: props.value,
        on_checked_change: props.on_checked_change,
        attributes: merged,
        switch::SwitchThumb { class: "pointer-events-none block size-4 translate-x-0.5 rounded-full bg-background shadow-sm transition-transform group-data-[state=checked]/switch:translate-x-[1.125rem]" }
      }
    }
}
