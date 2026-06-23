// `PreferenceChip` — small pill used both as decorative chips in the
// header (showing the active selection summary) and as the toggleable
// interest chips inside the planner form.

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PreferenceChipProps {
    label: String,
    /// When true the chip is rendered in the "selected" green palette used
    /// by the form's interest toggles. Decorative header chips pass `None`
    /// for the translucent white look from the SVG mockup.
    active: Option<bool>,
    /// Optional on-click handler. Chips without a handler are non-
    /// interactive (the header summary row).
    on_click: Option<EventHandler<()>>,
}

#[component]
pub fn PreferenceChip(props: PreferenceChipProps) -> Element {
    let base = match props.active {
        Some(true) => "bg-[#1a4f3a] text-white",
        Some(false) => "bg-white/15 text-white/80 hover:bg-white/25",
        None => "bg-white/20 text-white",
    };
    let cursor = if props.on_click.is_some() {
        "cursor-pointer"
    } else {
        ""
    };
    rsx! {
        button {
            class: "text-xs px-3 py-1 rounded-full transition {base} {cursor}",
            disabled: props.on_click.is_none(),
            onclick: move |_| {
                if let Some(handler) = props.on_click.as_ref() {
                    handler.call(());
                }
            },
            "{props.label}"
        }
    }
}
