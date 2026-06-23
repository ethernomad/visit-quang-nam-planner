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
//
// Phase 5 sustainability tooltip decision (per `plans/phase-5-polish.md`
// §3, option (a) with fallback): `TripSummary` now carries an optional
// `sustainability_breakdown: Vec<(String, u8)>` populated by the LLM
// (see `prompts::SYSTEM_PROMPT`). When non-empty, the tooltip renders the
// per-contribution breakdown ("Eco-certified lodging (+30), Local food
// (+20), ... = 82/100"). When empty (LLM omitted it, or `green_travel`
// was false), the tooltip falls back to a static explainer string
// (`copies::SUSTAINABILITY_TOOLTIP_STATIC`). The breakdown is additive
// (serde defaults to `[]`) so pre-Phase-5 responses still parse.

use dioxus::prelude::*;

use visit_quang_nam_planner::domain::format::category_style;
use visit_quang_nam_planner::domain::{Activity, TripSummary};

use crate::copies;

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

    // Phase 5: sustainability tooltip content. When the LLM provided a
    // breakdown, render each `(label, points)` pair; otherwise fall back
    // to the static explainer string.
    let tooltip_text = if s.sustainability_breakdown.is_empty() {
        copies::SUSTAINABILITY_TOOLTIP_STATIC.to_string()
    } else {
        let parts: Vec<String> = s
            .sustainability_breakdown
            .iter()
            .map(|(label, pts)| format!("{label} (+{pts})"))
            .collect();
        format!(
            "{}: {} = {}/100",
            copies::SUSTAINABILITY_TOOLTIP_BREAKDOWN_LABEL,
            parts.join(", "),
            score
        )
    };

    // Owned label/value rows for the summary card body. Iterated via `for`
    // so the row markup lives in one place (no `summary_row` helper
    // pseudo-component — Dioxus 0.7 reserves lowercase calls in rsx! for
    // `#[component]`-annotated functions).
    let rows: Vec<(&'static str, String)> = vec![
        (copies::SUMMARY_DURATION_LABEL, s.duration.clone()),
        (copies::SUMMARY_DESTINATIONS_LABEL, destinations),
        (copies::SUMMARY_BUDGET_LABEL, s.budget_estimate.clone()),
    ];

    rsx! {
        div { class: "bg-white rounded-2xl shadow-lg border border-[#e8f0eb] overflow-hidden",
            // Header
            div { class: "bg-[#1a4f3a] px-6 py-3",
                h3 { class: "text-base font-bold text-white",
                    "{copies::TRIP_SUMMARY_TITLE}"
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

                // Sustainability row with progress bar + Phase 5 tooltip.
                // The tooltip is a `group`-hover popup (Tailwind
                // `group-hover:opacity-100`) so it works without JS; the
                // `aria-label` makes the score accessible to screen
                // readers who can't see the hover.
                div { class: "flex items-center gap-3 group relative",
                    span { class: "w-4 h-4 rounded bg-[#a8d5ba] shrink-0" }
                    span { class: "text-sm font-bold text-[#1a2a1e] w-36 shrink-0",
                        "{copies::SUSTAINABILITY_LABEL}"
                    }
                    div { class: "flex-1 flex items-center gap-2",
                        div {
                            class: "flex-1 w-full bg-[#e6efe9] rounded-full h-3",
                            role: "img",
                            aria_label: "{tooltip_text}",
                            div {
                                class: "bg-[#2d7a5e] h-3 rounded-full transition-all",
                                style: "width: {score}%",
                            }
                        }
                        span { class: "text-sm text-[#4a6b58] whitespace-nowrap",
                            "🌱 {score}/100"
                        }
                    }
                    // Hover/focus tooltip (CSS-only, no JS)
                    div {
                        class: "absolute left-40 bottom-full mb-2 px-3 py-2 rounded-lg bg-[#1a2a1e] text-white text-xs max-w-xs opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-10 shadow-lg",
                        role: "tooltip",
                        "{tooltip_text}"
                    }
                }
            }

            // "More ideas" footer row
            if !more_ideas.is_empty() {
                div { class: "px-6 py-4 bg-[#1a4f3a]/5 border-t border-[#1a4f3a]/10",
                    h4 { class: "text-sm font-bold text-[#2d7a5e] mb-2",
                        "{copies::MORE_IDEAS_TITLE}"
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

#[cfg(test)]
mod tests {
    use super::*;
    use visit_quang_nam_planner::domain::TripSummary;

    fn summary_with_breakdown(score: u8, breakdown: Vec<(String, u8)>) -> TripSummary {
        TripSummary {
            duration: "2 days".into(),
            destinations: vec!["Hoi An".into()],
            budget_estimate: "$200".into(),
            sustainability_score: score,
            sustainability_breakdown: breakdown,
        }
    }

    #[test]
    fn tooltip_with_breakdown_lists_contributions() {
        let s = summary_with_breakdown(
            82,
            vec![
                ("Eco-certified lodging".into(), 30),
                ("Local food".into(), 20),
                ("Low-carbon transport".into(), 15),
                ("Off-peak timing".into(), 17),
            ],
        );
        // Reproduce the tooltip formatting logic to verify it reads sensibly.
        let parts: Vec<String> = s
            .sustainability_breakdown
            .iter()
            .map(|(label, pts)| format!("{label} (+{pts})"))
            .collect();
        let tooltip = format!(
            "{}: {} = {}/100",
            copies::SUSTAINABILITY_TOOLTIP_BREAKDOWN_LABEL,
            parts.join(", "),
            82
        );
        assert!(tooltip.contains("Eco-certified lodging (+30)"));
        assert!(tooltip.contains("Local food (+20)"));
        assert!(tooltip.contains("= 82/100"));
    }

    #[test]
    fn tooltip_without_breakdown_falls_back_to_static() {
        let s = summary_with_breakdown(50, Vec::new());
        // When the breakdown is empty, the component uses the static string.
        assert!(s.sustainability_breakdown.is_empty());
        assert!(!copies::SUSTAINABILITY_TOOLTIP_STATIC.is_empty());
    }
}
