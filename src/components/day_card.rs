// `DayCard` — renders a single `DayPlan`: gradient header strip with day
// number badge, title, date hint + weather chip, then a timeline of
// `ActivityRow`s joined by the dashed-line rail.
//
// Phase 5: the zero-activities message is hoisted to `copies.rs` as
// `NO_ACTIVITIES_HINT` ("No activities planned for this day. Try adjusting
// your interests.") and extracted into a pure helper `empty_day_message()`
// so the rule has a unit test that doesn't need a DOM.

use dioxus::prelude::*;

use visit_quang_nam_planner::domain::DayPlan;
use visit_quang_nam_planner::domain::format::day_header_gradient;

use crate::components::activity_row::ActivityRow;
use crate::copies;

#[derive(Props, Clone, PartialEq)]
pub struct DayCardProps {
    day: DayPlan,
}

#[component]
pub fn DayCard(props: DayCardProps) -> Element {
    let day = &props.day;
    let gradient = day_header_gradient(day.index);
    let last_idx = day.activities.len().saturating_sub(1);

    rsx! {
        div { class: "bg-white rounded-2xl shadow-lg border border-[#e8f0eb] overflow-hidden",
            // Header strip
            div { class: "bg-gradient-to-br {gradient} px-6 py-4",
                div { class: "flex items-center gap-3 flex-wrap",
                    div { class: "w-9 h-9 rounded-lg bg-[#2d7a5e] text-white font-bold flex items-center justify-center",
                        "{day.index}"
                    }
                    div { class: "flex-1 min-w-0",
                        h3 { class: "text-base font-bold text-[#1a4f3a] truncate",
                            "{day.title}"
                        }
                        p { class: "text-xs text-[#6b8a78]",
                            "{day.date_hint}"
                        }
                    }
                    span {
                        class: "text-xs px-3 py-1 rounded-full bg-[#fff8e1] border border-[#f0e0b8] text-[#b8860b]",
                        "{day.weather.icon} {day.weather.label}"
                    }
                }
            }

            // Timeline body
            div { class: "px-6 py-5",
                if day.activities.is_empty() {
                    p { class: "text-sm text-[#6b8a78] italic",
                        "{empty_day_message()}"
                    }
                } else {
                    for (i, act) in day.activities.iter().enumerate() {
                        ActivityRow {
                            key: "{i}",
                            activity: act.clone(),
                            is_last: i == last_idx,
                        }
                    }
                }
            }
        }
    }
}

/// The message shown when a day has no activities. Extracted from the
/// component so the rule has a unit test (Phase 5 §"Empty / edge cases").
pub fn empty_day_message() -> &'static str {
    copies::NO_ACTIVITIES_HINT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_day_message_matches_plan_spec() {
        let msg = empty_day_message();
        assert!(msg.contains("No activities planned"));
        assert!(msg.contains("adjusting your interests"));
    }
}
