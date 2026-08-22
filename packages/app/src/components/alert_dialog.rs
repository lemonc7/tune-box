use dioxus::prelude::*;
use dioxus_primitives::alert_dialog::{
    self, AlertDialogActionProps, AlertDialogActionsProps, AlertDialogCancelProps,
    AlertDialogDescriptionProps, AlertDialogRootProps, AlertDialogTitleProps,
};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub fn AlertDialog(props: AlertDialogRootProps) -> Element {
    let base = attributes!(div {
        class: "group/alert fixed inset-0 z-50 bg-black/50 opacity-0 backdrop-blur-[1px] transition-opacity duration-150 data-[state=open]:opacity-100 data-[state=closed]:pointer-events-none data-[state=closed]:opacity-0"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      alert_dialog::AlertDialogRoot {
        id: props.id,
        default_open: props.default_open,
        open: props.open,
        on_open_change: props.on_open_change,
        attributes: merged,
        alert_dialog::AlertDialogContent { class: "fixed left-1/2 top-1/2 z-50 grid w-[calc(100%-2rem)] max-w-lg -translate-x-1/2 -translate-y-1/2 scale-95 gap-4 rounded-lg border border-border bg-background p-6 text-foreground opacity-0 shadow-lg transition duration-150 group-data-[state=open]/alert:scale-100 group-data-[state=open]/alert:opacity-100",
          {props.children}
        }
      }
    }
}

#[component]
pub fn AlertDialogTitle(props: AlertDialogTitleProps) -> Element {
    let base = attributes!(div {
        class: "text-lg font-semibold"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      alert_dialog::AlertDialogTitle { attributes: merged, {props.children} }
    }
}

#[component]
pub fn AlertDialogDescription(props: AlertDialogDescriptionProps) -> Element {
    let base = attributes!(div {
        class: "text-sm leading-6 text-muted-foreground"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      alert_dialog::AlertDialogDescription { attributes: merged, {props.children} }
    }
}

#[component]
pub fn AlertDialogActions(props: AlertDialogActionsProps) -> Element {
    let base = attributes!(div {
        class: "flex flex-col-reverse gap-2 sm:flex-row sm:justify-end"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      alert_dialog::AlertDialogActions { attributes: merged, {props.children} }
    }
}

#[component]
pub fn AlertDialogCancel(props: AlertDialogCancelProps) -> Element {
    let base = attributes!(button {
        class: "inline-flex h-9 cursor-pointer items-center justify-center rounded-md border border-input bg-background px-4 py-2 text-sm font-medium text-foreground shadow-xs outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      alert_dialog::AlertDialogCancel { on_click: props.on_click, attributes: merged, {props.children} }
    }
}

#[component]
pub fn AlertDialogAction(props: AlertDialogActionProps) -> Element {
    let base = attributes!(button {
        class: "inline-flex h-9 cursor-pointer items-center justify-center rounded-md bg-destructive px-4 py-2 text-sm font-medium text-white shadow-xs outline-none transition-colors hover:bg-destructive/90 focus-visible:ring-[3px] focus-visible:ring-destructive/20"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      alert_dialog::AlertDialogAction { on_click: props.on_click, attributes: merged, {props.children} }
    }
}
