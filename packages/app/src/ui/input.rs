use dioxus::prelude::*;

const BASE_CLASS: &str = "h-10 w-full min-w-0 rounded-md border border-input bg-background px-3 py-2 text-base text-foreground outline-none transition-colors placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-2 aria-invalid:ring-destructive/20 md:text-sm";

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
) -> Element {
    rsx! {
      input {
        class: BASE_CLASS,
        oninput: move |e| _ = oninput.map(|cb| cb(e)),
        onchange: move |e| _ = onchange.map(|cb| cb(e)),
        oninvalid: move |e| _ = oninvalid.map(|cb| cb(e)),
        onselect: move |e| _ = onselect.map(|cb| cb(e)),
        onselectionchange: move |e| _ = onselectionchange.map(|cb| cb(e)),
        onfocus: move |e| _ = onfocus.map(|cb| cb(e)),
        onblur: move |e| _ = onblur.map(|cb| cb(e)),
        onfocusin: move |e| _ = onfocusin.map(|cb| cb(e)),
        onfocusout: move |e| _ = onfocusout.map(|cb| cb(e)),
        onkeydown: move |e| _ = onkeydown.map(|cb| cb(e)),
        onkeypress: move |e| _ = onkeypress.map(|cb| cb(e)),
        onkeyup: move |e| _ = onkeyup.map(|cb| cb(e)),
        onwheel: move |e| _ = onwheel.map(|cb| cb(e)),
        oncompositionstart: move |e| _ = oncompositionstart.map(|cb| cb(e)),
        oncompositionupdate: move |e| _ = oncompositionupdate.map(|cb| cb(e)),
        oncompositionend: move |e| _ = oncompositionend.map(|cb| cb(e)),
        oncopy: move |e| _ = oncopy.map(|cb| cb(e)),
        onpaste: move |e| _ = onpaste.map(|cb| cb(e)),
        ..attributes,
      }
    }
}
