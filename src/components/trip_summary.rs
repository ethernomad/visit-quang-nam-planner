// `TripSummary` — green-headed card that renders the `TripSummary` struct
// (duration, destinations, budget, sustainability bar) AND the Phase 4
// "More ideas" footer row (flat list of activities from inactive days),
// which replaces the SVG mockup's "AI Recommended For You" sidebar per
// the locked Phase 4 decision.
//
// Why the footer instead of a sidebar: the mockup put the sidebar as a
// sibling of `TripSummary`, but in a single-column responsive layout the
// sidebar duplicates content already present in inactive day cards.
// Surfacing it as a footer row under the summary keeps the same content
// reachable without the extra layout column. (See
// `plans/phase-4-ui.md` §"Suggestions sidebar".)

use dioxus::prelude::*;

use visit_quang_nam_planner::domain::format::category_style;
use visit_quang_nam_planner::domain::{Activity, TripSummary};

#[derive(Props, Clone, PartialEq)]
pub struct TripSummaryProps {
    summary: TripSummary,
    /// Activities from inactive days, flattened. The footer row uses these
    /// to suggest things the user could swap into their active day.
    more_ideas: Vec<Activity>,
}

#[component]
pub fn TripSummary(props: TripSummaryProps) -> Element {
    let s = &props.summary;
    let score = s.sustainability_score.min(100) as u32;
    let destinations = s.destinations.join(" → ");
    let more_ideas = &props.more_ideas;

    // Owned label/value rows for the summary card body. Iterated via `for`
    // so the row markup lives in one place (no `summary_row` helper
    // pseudo-component — Dioxus 0.7 reserves lowercase calls in rsx! for
    // `#[component]`-annotated functions).
    let rows: Vec<(&'static str, String)> = vec![
        ("Duration", s.duration.clone()),
        ("Destinations", destinations),
        ("Budget estimate", s.budget_estimate.clone()),
    ];

    rsx! {
        div { class: "bg-white rounded-2xl shadow-lg border border-[#e8f0eb] overflow-hidden",
            // Header
            div { class: "bg-[#1a4f3a] px-6 py-3",
                h3 { class: "text-base font-bold text-white",
                    "📋 Trip Summary"
                }
            }

            // Body rows
            div { class: "px-6 py-4 space-y-3",
                for (label, value) in rows.into_iter() {
                    div { class: "flex items-center gap-3",
                        span { class: "w-4 h-4 rounded bg-[#a8d5ba] shrink-0" }
                        span { class: "text-sm font-bold text-[#1a2a1e] w-36 shrink-0",
                            "{label}"
                        }
                        span { class: "text-sm text-[#4a6b58]",
                            "{value}"
                        }
                    }
                }

                // Sustainability row with progress bar
                div { class: "flex items-center gap-3",
                    span { class: "w-4 h-4 rounded bg-[#a8d5ba] shrink-0" }
                    span { class: "text-sm font-bold text-[#1a2a1e] w-36 shrink-0",
                        "Sustainability score"
                    }
                    div { class: "flex-1 flex items-center gap-2",
                        div { class: "flex-1 w-full bg-[#e6efe9] rounded-full h-3",
                            div {
                                class: "bg-[#2d7a5e] h-3 rounded-full",
                                style: "width: {score}%",
                            }
                        }
                        span { class: "text-sm text-[#4a6b58] whitespace-nowrap",
                            "🌱 {score}/100"
                        }
                    }
                }
            }

            // "More ideas" footer row
            if !more_ideas.is_empty() {
                div { class: "px-6 py-4 bg-[#1a4f3a]/5 border-t border-[#1a4f3a]/10",
                    h4 { class: "text-sm font-bold text-[#2d7a5e] mb-2",
                        "⭐ More ideas from your other days"
                    }
                    ul { class: "space-y-1.5",
                        for act in more_ideas.iter() {
                            li { class: "text-sm text-[#1a2a1e] flex items-center gap-2",
                                {
                                    let (icon, _label, _classes) = category_style(&act.category);
                                    rsx! { span { "{icon}" } }
                                }
                                span { "{act.title}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
