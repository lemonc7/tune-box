use dioxus::prelude::*;
use dioxus_primitives::toast::{
    self, Toast, ToastCloseButtonProps, ToastContentProps, ToastDescriptionProps, ToastProps,
    ToastTitleProps,
};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use std::time::Duration;

#[component]
fn StyledToast(props: ToastProps) -> Element {
    let base = attributes!(div {
        class: "pointer-events-auto relative flex w-full items-start justify-between gap-3 overflow-hidden rounded-lg border border-border bg-background p-4 pr-10 text-foreground shadow-lg transition-all data-[type=success]:border-emerald-200 data-[type=success]:bg-emerald-50 data-[type=error]:border-red-200 data-[type=error]:bg-red-50 data-[type=warning]:border-amber-200 data-[type=warning]:bg-amber-50 data-[type=info]:border-blue-200 data-[type=info]:bg-blue-50"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
      Toast {
        id: props.id,
        index: props.index,
        title: props.title,
        description: props.description,
        toast_type: props.toast_type,
        on_close: props.on_close,
        permanent: props.permanent,
        duration: props.duration,
        attributes: merged,
        ToastContent {
          ToastTitle {}
          ToastDescription {}
        }
        ToastCloseButton {}
      }
    }
}

#[component]
fn ToastContent(props: ToastContentProps) -> Element {
    let base = attributes!(div {
        class: "grid flex-1 gap-1"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      toast::ToastContent { attributes: merged, {props.children} }
    }
}

#[component]
fn ToastTitle(props: ToastTitleProps) -> Element {
    let base = attributes!(div {
        class: "text-sm font-semibold text-foreground"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      toast::ToastTitle { attributes: merged, children: props.children }
    }
}

#[component]
fn ToastDescription(props: ToastDescriptionProps) -> Element {
    let base = attributes!(div {
        class: "text-sm text-muted-foreground"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      toast::ToastDescription { attributes: merged, children: props.children }
    }
}

#[component]
fn ToastCloseButton(props: ToastCloseButtonProps) -> Element {
    let base = attributes!(button {
        class: "absolute right-2 top-2 inline-flex size-6 cursor-pointer items-center justify-center rounded-md border-0 bg-transparent text-lg leading-none text-muted-foreground opacity-70 outline-none transition hover:bg-black/5 hover:text-foreground hover:opacity-100 focus-visible:ring-2 focus-visible:ring-ring"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
      toast::ToastCloseButton { attributes: merged, children: props.children }
    }
}

#[component]
pub fn ToastProvider(
    #[props(default = ReadSignal::new(Signal::new(Some(Duration::from_secs(5)))))]
    default_duration: ReadSignal<Option<Duration>>,
    #[props(default = ReadSignal::new(Signal::new(10)))] max_toasts: ReadSignal<usize>,
    #[props(default)] render_toast: Option<Callback<toast::ToastPropsWithOwner, Element>>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let render_toast = render_toast.unwrap_or_else(|| {
        Callback::new(|p: toast::ToastPropsWithOwner| {
            rsx! {
              StyledToast { ..p }
            }
        })
    });

    let base = attributes!(div {
        class: "fixed right-4 top-4 z-[100] flex max-h-screen w-[calc(100%-2rem)] max-w-sm flex-col [&_ol]:m-0 [&_ol]:flex [&_ol]:list-none [&_ol]:flex-col [&_ol]:gap-3 [&_ol]:p-0 [&_li]:flex"
    });
    let merged = merge_attributes(vec![base, attributes]);

    rsx! {
      toast::ToastProvider {
        default_duration,
        max_toasts,
        render_toast,
        attributes: merged,
        {children}
      }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus_primitives::toast::{ToastOptions, use_toast};

    #[component]
    fn TriggerToast() -> Element {
        let toast_api = use_toast();
        use_hook(move || {
            toast_api.success(
                "Saved".to_string(),
                ToastOptions::new()
                    .description("Everything synced")
                    .permanent(true),
            );
        });

        rsx! {}
    }
}
