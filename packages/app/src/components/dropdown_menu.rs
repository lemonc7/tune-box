use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::dropdown_menu::{
    self, DropdownMenuContentProps, DropdownMenuItemProps, DropdownMenuProps,
    DropdownMenuTriggerProps,
};
use dioxus_primitives::merge_attributes;

#[component]
pub fn DropdownMenu(props: DropdownMenuProps) -> Element {
    let base = attributes!(div {
        class: "relative inline-block",
    });
    let merged = merge_attributes(vec![base, props.attributes.clone()]);

    rsx! {
        dropdown_menu::DropdownMenu {
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            disabled: props.disabled,
            roving_loop: props.roving_loop,
            attributes: merged,
            {props.children}
        }
    }
}

#[component]
pub fn DropdownMenuTrigger(props: DropdownMenuTriggerProps) -> Element {
    let base = attributes!(button {
        class: "inline-flex size-9 cursor-pointer items-center justify-center rounded-md border border-input bg-background p-0 text-sm text-foreground shadow-xs outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50",
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        dropdown_menu::DropdownMenuTrigger { r#as: props.r#as, attributes: merged, {props.children} }
    }
}

#[component]
pub fn DropdownMenuContent(props: DropdownMenuContentProps) -> Element {
    let base = attributes!(div {
        class: "absolute right-0 top-full z-50 mt-1 min-w-44 origin-top-right overflow-hidden rounded-md border border-border bg-popover p-1 text-popover-foreground opacity-0 shadow-md transition duration-150 data-[state=open]:pointer-events-auto data-[state=open]:translate-y-0 data-[state=open]:scale-100 data-[state=open]:opacity-100 data-[state=closed]:pointer-events-none data-[state=closed]:-translate-y-1 data-[state=closed]:scale-95 data-[state=closed]:opacity-0",
    });
    let merged = merge_attributes(vec![base, props.attributes.clone()]);

    rsx! {
        dropdown_menu::DropdownMenuContent { id: props.id, attributes: merged, {props.children} }
    }
}

#[component]
pub fn DropdownMenuItem<T: Clone + PartialEq + 'static>(
    props: DropdownMenuItemProps<T>,
) -> Element {
    let base = attributes!(div {
        class: "relative flex cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-popover-foreground outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
    });
    let merged = merge_attributes(vec![base, props.attributes.clone()]);

    rsx! {
        dropdown_menu::DropdownMenuItem {
            disabled: props.disabled,
            value: props.value,
            index: props.index,
            on_select: props.on_select,
            attributes: merged,
            {props.children}
        }
    }
}
