// UI components. Phase 4 implements the planner UI:
//   - `preference_chip`   — small pill (decorative header chip + form toggle)
//   - `activity_row`      — one timeline stop with "Read more" link
//   - `day_card`          — gradient header + dot-and-line timeline
//   - `trip_summary`      — `TripSummary` card + sustainability bar + the
//                           "More ideas" footer row that replaces the SVG
//                           mockup's "AI Recommended For You" sidebar
//                           (Phase 4 decision; AGENTS.md layout was updated
//                           to drop the separate `suggestions` entry)
//   - `planner_form`      — the input panel (duration, month, interests,
//                           travelers, pace, budget, green), calls `plan_trip`
//                           via the parent's `use_resource`
//   - `itinerary_view`    — day tabs + active day card + summary footer
//
// `suggestions` is intentionally NOT here — see `trip_summary` for the
// "More ideas" footer decision.

pub mod activity_row;
pub mod day_card;
pub mod itinerary_view;
pub mod planner_form;
pub mod preference_chip;
pub mod trip_summary;
