use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

const BUTTON_BASE: &str = "inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium outline-none transition-colors disabled:pointer-events-none disabled:opacity-50 focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-invalid:border-destructive aria-invalid:ring-[3px] aria-invalid:ring-destructive/20 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4";

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
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
    pub fn class(&self) -> &'static str {
        match self {
            ButtonVariant::Primary => {
                "bg-primary text-primary-foreground shadow-xs hover:bg-primary/90"
            }
            ButtonVariant::Secondary => {
                "bg-secondary text-secondary-foreground shadow-xs hover:bg-secondary/80"
            }
            ButtonVariant::Destructive => {
                "bg-destructive text-white shadow-xs hover:bg-destructive/90 focus-visible:ring-destructive/20"
            }
            ButtonVariant::Outline => {
                "border border-input bg-background shadow-xs hover:bg-accent hover:text-accent-foreground"
            }
            ButtonVariant::Ghost => "hover:bg-accent hover:text-accent-foreground",
            ButtonVariant::Link => "text-primary underline-offset-4 hover:underline",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
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
    pub fn class(&self) -> &'static str {
        match self {
            ButtonSize::Xs => {
                "h-6 gap-1 rounded-md px-2 text-xs [&_svg:not([class*='size-'])]:size-3"
            }
            ButtonSize::Sm => "h-8 gap-1.5 px-3",
            ButtonSize::Default => "h-9 px-4 py-2",
            ButtonSize::Lg => "h-10 rounded-md px-6",
            ButtonSize::Icon => "size-9",
            ButtonSize::IconXs => "size-6 rounded-md [&_svg:not([class*='size-'])]:size-3",
            ButtonSize::IconSm => "size-8",
            ButtonSize::IconLg => "size-10",
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
    let class = format!("{BUTTON_BASE} {} {}", variant.class(), size.class());
    let base = attributes!(button { class });
    let merged = merge_attributes(vec![base, attributes]);

    rsx! {
      button {
        onclick: move |event| {
            if let Some(f) = &onclick {
                f.call(event);
            }
        },
        onmousedown: move |event| {
            if let Some(f) = &onmousedown {
                f.call(event);
            }
        },
        onmouseup: move |event| {
            if let Some(f) = &onmouseup {
                f.call(event);
            }
        },
        onkeydown: move |event| {
            if let Some(f) = &onkeydown {
                f.call(event);
            }
        },
        ..merged,
        {children}
      }
    }
}
