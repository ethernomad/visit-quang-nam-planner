// `ActivityRow` — one stop on a `DayPlan`'s timeline. Renders the SVG
// mockup's row shape: green dot + dashed connecting line on the left,
// time pill, title, description, category tag, optional price/duration
// tags, and a "Read more" link to the visitquangnam.com article the
// recommendation was grounded in.
//
// The link is suppressed (not rendered) when `activity.source_url` is
// empty, per `plans/phase-4-ui.md` §"Notes for the agent". `post_validate`
// should already reject empty URLs server-side but the UI stays defensive.

use dioxus::prelude::*;

use visit_quang_nam_planner::domain::Activity;
use visit_quang_nam_planner::domain::format::{category_style, format_duration, format_price};

/// `is_last` controls whether the dashed connector line below the dot is
/// drawn — the bottom row of a day has no successor to connect to.
#[derive(Props, Clone, PartialEq)]
pub struct ActivityRowProps {
    activity: Activity,
    is_last: bool,
}

#[component]
pub fn ActivityRow(props: ActivityRowProps) -> Element {
    let act = &props.activity;
    let (icon, label, cat_classes) = category_style(&act.category);
    let price = format_price(act.estimated_cost_vnd);
    let duration = format_duration(act.duration_minutes);
    let has_link = !act.source_url.is_empty();

    rsx! {
        div { class: "flex gap-3 pb-6 last:pb-0",
            // Dot + dashed connector line
            div { class: "flex flex-col items-center pt-1",
                div { class: "w-4 h-4 rounded-full bg-[#2d7a5e] shrink-0" }
                if !props.is_last {
                    div { class: "flex-1 w-px border-l-2 border-dashed border-[#c8dcd0] mt-1" }
                }
            }

            // Body
            div { class: "flex-1",
                // Time pill
                span {
                    class: "inline-block text-xs font-bold text-[#2d7a5e] bg-[#e8f5e9] rounded-md px-3 py-1 mb-2",
                    "{act.time}"
                }

                // Title + description
                h4 { class: "text-base font-bold text-[#1a2a1e]",
                    "{act.title}"
                }
                p { class: "text-sm text-[#6b8a78] mt-1 mb-2",
                    "{act.description}"
                }

                // Tags row
                div { class: "flex flex-wrap gap-2 items-center",
                    span {
                        class: "text-xs px-2.5 py-1 rounded-md {cat_classes}",
                        "{icon} {label}"
                    }
                    if !price.is_empty() {
                        span {
                            class: "text-xs px-2.5 py-1 rounded-md bg-[#fff3e0] text-[#e65100]",
                            "{price}"
                        }
                    }
                    if !duration.is_empty() {
                        span {
                            class: "text-xs px-2.5 py-1 rounded-md bg-[#e8f5e9] text-[#2e7d32]",
                            "{duration}"
                        }
                    }
                }

                // Read more link to the grounded article
                if has_link {
                    a {
                        class: "inline-block mt-2 text-xs text-[#2d7a5e] hover:text-[#1a4f3a] underline",
                        href: act.source_url.clone(),
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "↳ Read more on visitquangnam.com"
                    }
                }
            }
        }
    }
}
