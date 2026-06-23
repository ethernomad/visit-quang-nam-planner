// `ItineraryView` — day tabs + active-day card + trip summary + "More
// ideas" footer. Receives the readiness-ready `Itinerary` from the parent
// (which has already discriminated `Ready(Ok(itin))`) and an
// `active_day: Signal<usize>` index into `itin.days`.
//
// Phase 4 decision per `plans/phase-4-ui.md` §"Suggestions sidebar": the
// mockup's "AI Recommended For You" sidebar is dropped. Its content (a
// flat list of activities from inactive days) is surfaced as a "More
// ideas" footer row inside `TripSummary` instead, so the same content is
// reachable without a two-column layout. `AGENTS.md` is updated to drop
// the `suggestions` entry from the project layout accordingly.

use dioxus::prelude::*;

use visit_quang_nam_planner::domain::{Activity, Itinerary};

use crate::components::day_card::DayCard;
use crate::components::trip_summary::TripSummary;

#[derive(Props, Clone, PartialEq)]
pub struct ItineraryViewProps {
    itinerary: Itinerary,
    active_day: Signal<usize>,
}

#[component]
pub fn ItineraryView(props: ItineraryViewProps) -> Element {
    let itin = &props.itinerary;
    let mut active_day = props.active_day;
    let n = itin.days.len();

    // Clamp in case the previous itinerary was longer than this one.
    if active_day() >= n {
        active_day.set(n.saturating_sub(1));
    }
    let active = active_day().min(n.saturating_sub(1));
    let day = itin.days[active].clone();

    // Activities from non-active days, flattened, capped at 6 so the
    // footer doesn't run forever for long trips. Each `Activity` is
    // cloned (cheap; they hold `String`s but typically a few hundred
    // bytes each).
    let more_ideas: Vec<Activity> = itin
        .days
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != active)
        .flat_map(|(_, d)| d.activities.clone())
        .take(6)
        .collect();

    let summary = itin.summary.clone();

    rsx! {
        div { class: "space-y-6",
            // Day tabs
            div { class: "flex flex-wrap gap-2",
                for (i, d) in itin.days.iter().enumerate() {
                    button {
                        key: "{i}",
                        class: if i == active {
                            "text-sm font-bold px-4 py-2.5 rounded-lg transition bg-[#1a4f3a] text-white"
                        } else {
                            "text-sm font-bold px-4 py-2.5 rounded-lg transition bg-[#e8f0eb]/70 text-[#4a6b58] hover:bg-[#e8f0eb]"
                        },
                        onclick: move |_| active_day.set(i),
                        // `d.title` already starts with "Day N — …" (see
                        // `domain::DayPlan`); no need to prefix again.
                        "{d.title}"
                    }
                }
            }

            // Active day card
            DayCard { key: "{active}", day: day }

            // Trip summary + more ideas footer
            TripSummary { summary: summary, more_ideas: more_ideas }
        }
    }
}
