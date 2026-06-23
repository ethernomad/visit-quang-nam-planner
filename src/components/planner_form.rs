// `PlannerForm` — the input panel. Lives in the header card (per the SVG
// mockup). Lifts its state into `Signal<Preferences>` (parent-owned,
// passed in as props) and flips `Signal<bool> submitted` on submit.
//
// The submit handler also bumps a `submit_nonce: Signal<u32>` so the
// parent's `use_resource` re-runs even when the user re-submits the same
// preferences (otherwise Dioxus caches the identical closure result).

use dioxus::prelude::*;

use visit_quang_nam_planner::domain::{BudgetTier, Interest, Month, Pace, Preferences, Travelers};

use crate::components::preference_chip::PreferenceChip;

#[derive(Props, Clone, PartialEq)]
pub struct PlannerFormProps {
    prefs: Signal<Preferences>,
    submitted: Signal<bool>,
    submit_nonce: Signal<u32>,
}

const MONTHS: [Month; 12] = [
    Month::January,
    Month::February,
    Month::March,
    Month::April,
    Month::May,
    Month::June,
    Month::July,
    Month::August,
    Month::September,
    Month::October,
    Month::November,
    Month::December,
];

const INTERESTS: [(Interest, &str); 6] = [
    (Interest::Food, "🍴 Food"),
    (Interest::Beaches, "🏖 Beaches"),
    (Interest::Culture, "🏛 Culture"),
    (Interest::Nature, "🌿 Nature"),
    (Interest::Wellness, "🧘 Wellness"),
    (Interest::GreenTravel, "🌱 Green Travel"),
];

const PACES: [(Pace, &str); 3] = [
    (Pace::Slow, "Slow"),
    (Pace::Moderate, "Moderate"),
    (Pace::Active, "Active"),
];

const BUDGETS: [(BudgetTier, &str); 3] = [
    (BudgetTier::Backpacker, "Backpacker"),
    (BudgetTier::Mid, "Mid"),
    (BudgetTier::Luxury, "Luxury"),
];

#[component]
pub fn PlannerForm(props: PlannerFormProps) -> Element {
    let mut prefs = props.prefs;
    let mut submitted = props.submitted;
    let mut nonce = props.submit_nonce;

    let mut set_travelers = move |adults_delta: i32, kids_delta: i32| {
        let mut p = prefs();
        let new_adults = (p.travelers.adults as i32 + adults_delta).clamp(1, 20) as u8;
        let new_kids = (p.travelers.kids as i32 + kids_delta).clamp(0, 20) as u8;
        p.travelers = Travelers {
            adults: new_adults,
            kids: new_kids,
        };
        prefs.set(p);
    };

    let on_submit = move |_| {
        submitted.set(true);
        nonce.set(nonce() + 1);
    };

    let current = prefs();

    rsx! {
        div { class: "space-y-4",
            // Row 1: duration + month
            div { class: "grid sm:grid-cols-2 gap-4",
                div {
                    label { class: "block text-xs font-semibold text-[#a8d5ba] mb-1",
                        "Duration: {current.duration_days} days"
                    }
                    input {
                        r#type: "range",
                        min: 1,
                        max: 14,
                        step: 1,
                        value: "{current.duration_days}",
                        class: "w-full accent-[#a8d5ba]",
                        oninput: move |e| {
                            let mut p = prefs();
                            if let Ok(v) = e.value().parse::<u8>() {
                                p.duration_days = v;
                                prefs.set(p);
                            }
                        },
                    }
                }
                div {
                    label { class: "block text-xs font-semibold text-[#a8d5ba] mb-1",
                        "Month"
                    }
                    select {
                        class: "w-full bg-white text-[#1a4f3a] text-sm rounded-md px-3 py-2 border border-[#a8d5ba]/40",
                        value: format!("{:?}", current.month),
                        onchange: move |e| {
                            let mut p = prefs();
                            let parsed = MONTHS
                                .iter()
                                .find(|m| format!("{m:?}") == e.value())
                                .copied();
                            if let Some(m) = parsed {
                                p.month = m;
                                prefs.set(p);
                            }
                        },
                        for m in MONTHS.iter() {
                            option { value: format!("{m:?}"), "{m:?}" }
                        }
                    }
                }
            }

            // Row 2: interest chips
            div {
                label { class: "block text-xs font-semibold text-[#a8d5ba] mb-1.5",
                    "Interests"
                }
                div { class: "flex flex-wrap gap-2",
                    for (interest, label) in INTERESTS.iter().copied() {
                        PreferenceChip {
                            key: "{label}",
                            label: label.to_string(),
                            active: Some(current.interests.contains(&interest)),
                            on_click: Some(EventHandler::new(move |_| {
                                let mut p = prefs();
                                if let Some(pos) =
                                    p.interests.iter().position(|i| *i == interest)
                                {
                                    p.interests.remove(pos);
                                } else {
                                    p.interests.push(interest);
                                }
                                prefs.set(p);
                            })),
                        }
                    }
                }
            }

            // Row 3: travelers + pace + budget + green
            div { class: "grid sm:grid-cols-2 lg:grid-cols-4 gap-4",
                // Travelers steppers
                div {
                    label { class: "block text-xs font-semibold text-[#a8d5ba] mb-1.5",
                        "Travelers"
                    }
                    div { class: "flex gap-2",
                        Stepper {
                            label: "Adults".to_string(),
                            value: current.travelers.adults,
                            on_minus: EventHandler::new(move |_| set_travelers(-1, 0)),
                            on_plus: EventHandler::new(move |_| set_travelers(1, 0)),
                        }
                        Stepper {
                            label: "Kids".to_string(),
                            value: current.travelers.kids,
                            on_minus: EventHandler::new(move |_| set_travelers(0, -1)),
                            on_plus: EventHandler::new(move |_| set_travelers(0, 1)),
                        }
                    }
                }

                // Pace
                div {
                    label { class: "block text-xs font-semibold text-[#a8d5ba] mb-1.5",
                        "Pace"
                    }
                    div { class: "flex flex-wrap gap-1.5",
                        for (pace, label) in PACES.iter().copied() {
                            button {
                                key: "{label}",
                                class: if current.pace == pace {
                                    "text-xs px-2.5 py-1 rounded-full transition bg-[#1a4f3a] text-white"
                                } else {
                                    "text-xs px-2.5 py-1 rounded-full transition bg-white/15 text-white/80 hover:bg-white/25"
                                },
                                onclick: move |_| {
                                    let mut p = prefs();
                                    p.pace = pace;
                                    prefs.set(p);
                                },
                                "{label}"
                            }
                        }
                    }
                }

                // Budget
                div {
                    label { class: "block text-xs font-semibold text-[#a8d5ba] mb-1.5",
                        "Budget"
                    }
                    div { class: "flex flex-wrap gap-1.5",
                        for (tier, label) in BUDGETS.iter().copied() {
                            button {
                                key: "{label}",
                                class: if current.budget_tier == tier {
                                    "text-xs px-2.5 py-1 rounded-full transition bg-[#1a4f3a] text-white"
                                } else {
                                    "text-xs px-2.5 py-1 rounded-full transition bg-white/15 text-white/80 hover:bg-white/25"
                                },
                                onclick: move |_| {
                                    let mut p = prefs();
                                    p.budget_tier = tier;
                                    prefs.set(p);
                                },
                                "{label}"
                            }
                        }
                    }
                }

                // Green travel
                div {
                    label { class: "block text-xs font-semibold text-[#a8d5ba] mb-1.5",
                        "Sustainability"
                    }
                    label { class: "flex items-center gap-2 cursor-pointer text-white text-sm",
                        input {
                            r#type: "checkbox",
                            checked: current.green_travel,
                            class: "accent-[#a8d5ba] w-4 h-4",
                            onchange: move |e| {
                                let mut p = prefs();
                                p.green_travel = e.checked();
                                prefs.set(p);
                            },
                        }
                        "🌱 Green travel"
                    }
                }
            }

            // Submit button
            div { class: "flex justify-end",
                button {
                    class: "bg-[#a8d5ba] text-[#1a4f3a] font-bold text-sm px-6 py-3 rounded-full hover:bg-[#9bc8ab] transition shadow-md",
                    onclick: on_submit,
                    "✨ Plan My Trip"
                }
            }
        }
    }
}

// Adults / kids counter pill. Inlined component (no separate file) since
// it's a small primitive reused twice in this form and nowhere else.
#[derive(Props, Clone, PartialEq)]
struct StepperProps {
    label: String,
    value: u8,
    on_minus: EventHandler<()>,
    on_plus: EventHandler<()>,
}

#[component]
fn Stepper(props: StepperProps) -> Element {
    rsx! {
        div { class: "flex items-center gap-1.5 bg-white/10 rounded-full px-2 py-1",
            button {
                class: "w-5 h-5 rounded-full bg-[#1a4f3a] text-white text-xs leading-none",
                onclick: move |_| props.on_minus.call(()),
                "−"
            }
            span { class: "text-white text-xs min-w-12 text-center",
                "{props.label}: {props.value}"
            }
            button {
                class: "w-5 h-5 rounded-full bg-[#1a4f3a] text-white text-xs leading-none",
                onclick: move |_| props.on_plus.call(()),
                "+"
            }
        }
    }
}
