use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub fn Input(
    oninput: Option<EventHandler<FormEvent>>,
    onchange: Option<EventHandler<FormEvent>>,
    oninvalid: Option<EventHandler<FormEvent>>,
    onselect: Option<EventHandler<SelectionEvent>>,
    onselectionchange: Option<EventHandler<SelectionEvent>>,
    onfocus: Option<EventHandler<FocusEvent>>,
    onblur: Option<EventHandler<FocusEvent>>,
    onfocusin: Option<EventHandler<FocusEvent>>,
    onfocusout: Option<EventHandler<FocusEvent>>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    onkeypress: Option<EventHandler<KeyboardEvent>>,
    onkeyup: Option<EventHandler<KeyboardEvent>>,
    onwheel: Option<EventHandler<WheelEvent>>,
    oncompositionstart: Option<EventHandler<CompositionEvent>>,
    oncompositionupdate: Option<EventHandler<CompositionEvent>>,
    oncompositionend: Option<EventHandler<CompositionEvent>>,
    oncopy: Option<EventHandler<ClipboardEvent>>,
    oncut: Option<EventHandler<ClipboardEvent>>,
    onpaste: Option<EventHandler<ClipboardEvent>>,
    #[props(extends=GlobalAttributes)]
    #[props(extends=input)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(input {
        class: "flex h-9 w-full min-w-0 rounded-md border border-input bg-transparent px-3 py-1 text-base text-foreground shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground selection:bg-primary selection:text-primary-foreground disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-invalid:border-destructive aria-invalid:ring-[3px] aria-invalid:ring-destructive/20 sm:text-sm"
    });
    let merged = merge_attributes(vec![base, attributes]);

    rsx! {
      input {
        oninput: move |e| _ = oninput.map(|callback| callback(e)),
        onchange: move |e| _ = onchange.map(|callback| callback(e)),
        oninvalid: move |e| _ = oninvalid.map(|callback| callback(e)),
        onselect: move |e| _ = onselect.map(|callback| callback(e)),
        onselectionchange: move |e| _ = onselectionchange.map(|callback| callback(e)),
        onfocus: move |e| _ = onfocus.map(|callback| callback(e)),
        onblur: move |e| _ = onblur.map(|callback| callback(e)),
        onfocusin: move |e| _ = onfocusin.map(|callback| callback(e)),
        onfocusout: move |e| _ = onfocusout.map(|callback| callback(e)),
        onkeydown: move |e| _ = onkeydown.map(|callback| callback(e)),
        onkeypress: move |e| _ = onkeypress.map(|callback| callback(e)),
        onkeyup: move |e| _ = onkeyup.map(|callback| callback(e)),
        onwheel: move |e| _ = onwheel.map(|callback| callback(e)),
        oncompositionstart: move |e| _ = oncompositionstart.map(|callback| callback(e)),
        oncompositionupdate: move |e| _ = oncompositionupdate.map(|callback| callback(e)),
        oncompositionend: move |e| _ = oncompositionend.map(|callback| callback(e)),
        oncopy: move |e| _ = oncopy.map(|callback| callback(e)),
        oncut: move |e| _ = oncut.map(|callback| callback(e)),
        onpaste: move |e| _ = onpaste.map(|callback| callback(e)),
        ..merged,
        {children}
      }
    }
}
