use dioxus::prelude::*;

use super::attributes::merge_attributes;

const BASE_CLASS: &str = "inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap rounded-md border text-sm font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0";

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Destructive,
    Outline,
    Ghost,
    Link,
}

impl ButtonVariant {
    fn class(self) -> &'static str {
        match self {
            ButtonVariant::Primary => {
                "border-transparent bg-primary text-primary-foreground hover:bg-primary/80"
            }
            ButtonVariant::Outline => "border-border bg-background text-foreground hover:bg-muted",
            ButtonVariant::Secondary => {
                "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80"
            }
            ButtonVariant::Ghost => "border-transparent text-foreground hover:bg-muted",
            ButtonVariant::Destructive => {
                "border-transparent bg-destructive/10 text-destructive hover:bg-destructive/20 focus-visible:ring-destructive/20"
            }
            ButtonVariant::Link => {
                "border-transparent text-primary underline-offset-4 hover:underline"
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ButtonSize {
    Xs,
    Sm,
    #[default]
    Default,
    Lg,
    Icon,
    IconXs,
    IconSm,
    IconLg,
}

impl ButtonSize {
    fn class(self) -> &'static str {
        match self {
            ButtonSize::Default => "h-9 gap-2 px-4 [&_svg]:size-4",
            ButtonSize::Xs => "h-6 gap-1 px-2 text-xs [&_svg]:size-3",
            ButtonSize::Sm => "h-8 gap-1.5 px-3 [&_svg]:size-3.5",
            ButtonSize::Lg => "h-10 gap-2 px-6 [&_svg]:size-4",
            ButtonSize::Icon => "size-9 p-0 [&_svg]:size-4",
            ButtonSize::IconXs => "size-6 p-0 [&_svg]:size-3",
            ButtonSize::IconSm => "size-8 p-0 [&_svg]:size-3.5",
            ButtonSize::IconLg => "size-10 p-0 [&_svg]:size-4",
        }
    }
}

#[component]
pub fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(extends=GlobalAttributes)]
    #[props(extends=button)]
    attributes: Vec<Attribute>,
    onclick: Option<EventHandler<MouseEvent>>,
    onmousedown: Option<EventHandler<MouseEvent>>,
    onmouseup: Option<EventHandler<MouseEvent>>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    children: Element,
) -> Element {
    let base = vec![Attribute::new(
        "class",
        format!("{BASE_CLASS} {} {}", variant.class(), size.class()),
        None,
        false,
    )];
    let merged = merge_attributes(vec![base, attributes]);

    rsx! {
      button {
        onclick: move |event| {
            if let Some(handler) = &onclick {
                handler.call(event);
            }
        },
        onmousedown: move |event| {
            if let Some(handler) = &onmousedown {
                handler.call(event);
            }
        },
        onmouseup: move |event| {
            if let Some(handler) = &onmouseup {
                handler.call(event);
            }
        },
        onkeydown: move |event| {
            if let Some(handler) = &onkeydown {
                handler.call(event);
            }
        },
        ..merged,
        {children}
      }
    }
}
