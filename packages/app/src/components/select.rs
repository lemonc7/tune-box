use dioxus::prelude::*;
use dioxus_icons::lucide::{Check, ChevronDown};
use dioxus_primitives::select::{
    self, SelectGroupLabelProps, SelectMultiProps, SelectOptionProps, SelectProps,
};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub fn Select<T: Clone + PartialEq + 'static>(props: SelectProps<T>) -> Element {
    let base = attributes!(div {
        class: "group/select relative w-full"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      select::Select {
        value: props.value,
        default_value: props.default_value,
        on_value_change: props.on_value_change,
        disabled: props.disabled,
        open: props.open,
        default_open: props.default_open,
        on_open_change: props.on_open_change,
        name: props.name,
        roving_loop: props.roving_loop,
        typeahead_timeout: props.typeahead_timeout,
        attributes: merged,
        select::SelectTrigger { class: "flex h-9 w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-input bg-transparent px-3 py-2 text-sm text-foreground shadow-xs outline-none transition-[color,box-shadow] hover:bg-accent/50 focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 group-data-[disabled=true]/select:pointer-events-none group-data-[disabled=true]/select:cursor-not-allowed group-data-[disabled=true]/select:opacity-50 group-data-[state=open]/select:pointer-events-none [&_[data-placeholder=true]]:text-muted-foreground",
          select::SelectValue {}
          ChevronDown { class: "size-4 shrink-0 text-muted-foreground opacity-50" }
        }
        select::SelectList { class: "absolute left-0 top-full z-50 mt-1 max-h-64 min-w-full origin-top overflow-y-auto rounded-md border border-border bg-popover p-1 text-popover-foreground opacity-0 shadow-md transition duration-150 data-[state=open]:pointer-events-auto data-[state=open]:translate-y-0 data-[state=open]:scale-100 data-[state=open]:opacity-100 data-[state=closed]:pointer-events-none data-[state=closed]:-translate-y-1 data-[state=closed]:scale-95 data-[state=closed]:opacity-0",
          {props.children}
        }
      }
    }
}

#[component]
pub fn SelectMulti<T: Clone + PartialEq + 'static>(props: SelectMultiProps<T>) -> Element {
    let base = attributes!(div {
        class: "group/select relative w-full"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      select::SelectMulti {
        values: props.values,
        default_values: props.default_values,
        on_values_change: props.on_values_change,
        disabled: props.disabled,
        open: props.open,
        default_open: props.default_open,
        on_open_change: props.on_open_change,
        name: props.name,
        roving_loop: props.roving_loop,
        typeahead_timeout: props.typeahead_timeout,
        attributes: merged,
        select::SelectTrigger { class: "flex h-9 w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-input bg-transparent px-3 py-2 text-sm text-foreground shadow-xs outline-none transition-[color,box-shadow] hover:bg-accent/50 focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 group-data-[disabled=true]/select:pointer-events-none group-data-[disabled=true]/select:cursor-not-allowed group-data-[disabled=true]/select:opacity-50 group-data-[state=open]/select:pointer-events-none [&_[data-placeholder=true]]:text-muted-foreground",
          select::SelectValue {}
          ChevronDown { class: "size-4 shrink-0 text-muted-foreground opacity-50" }
        }
        select::SelectList { class: "absolute left-0 top-full z-50 mt-1 max-h-64 min-w-full origin-top overflow-y-auto rounded-md border border-border bg-popover p-1 text-popover-foreground opacity-0 shadow-md transition duration-150 data-[state=open]:pointer-events-auto data-[state=open]:translate-y-0 data-[state=open]:scale-100 data-[state=open]:opacity-100 data-[state=closed]:pointer-events-none data-[state=closed]:-translate-y-1 data-[state=closed]:scale-95 data-[state=closed]:opacity-0",
          {props.children}
        }
      }
    }
}

#[component]
pub fn SelectGroupLabel(props: SelectGroupLabelProps) -> Element {
    let base = attributes!(div {
        class: "px-2 py-1.5 text-xs font-medium text-muted-foreground"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      select::SelectGroupLabel { id: props.id, attributes: merged, {props.children} }
    }
}

#[component]
pub fn SelectOption<T: Clone + PartialEq + 'static>(props: SelectOptionProps<T>) -> Element {
    let base = attributes!(div {
        class: "relative flex cursor-default select-none items-center justify-between gap-2 rounded-sm px-2 py-1.5 text-sm outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      select::SelectOption::<T> {
        value: props.value,
        text_value: props.text_value,
        disabled: props.disabled,
        id: props.id,
        index: props.index,
        aria_label: props.aria_label,
        aria_roledescription: props.aria_roledescription,
        attributes: merged,
        {props.children}
        select::SelectItemIndicator {
          Check { class: "size-4 text-primary" }
        }
      }
    }
}
