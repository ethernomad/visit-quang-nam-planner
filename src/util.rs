//! Small reusable pure helpers for the planner UI (Phase 5).
//!
//! Lives in the bin crate (not `src/lib.rs`) because the canonical formatters
//! `format_price` / `format_duration` live in the library at
//! `domain::format` — that's where cross-crate integration tests can reach
//! them. This module re-exports them under the Phase 5 names (`format_vnd`,
//! `format_duration_minutes`) so component code has one import path, and
//! adds the new `weather_label_for_month` helper that has no library home
//! (it's a UI-only concern).
//!
//! Everything here is pure and unit-tested without a DOM.
//!
//! Re-exports are `#[allow(unused_imports)]`-gated because not every binary
//! target uses every re-export (e.g. the `build_corpus` xtask doesn't touch
//! them), but they're part of the single import surface component code
//! relies on.

#![allow(unused_imports)]

pub use visit_quang_nam_planner::domain::format::{
    category_style, day_header_gradient, format_duration as format_duration_minutes,
    format_price as format_vnd,
};

use visit_quang_nam_planner::domain::Month;

/// Quang Nam climate hint for a travel month, used under the form's month
/// dropdown so the user can plan around the rainy season without leaving the
/// form. Coarse-grained by design — the LLM's per-day `weather` hint is the
/// authoritative in-itinerary forecast; this is just a planning aid.
///
/// Source: Vietnam National Administration of Tourism climate summaries for
/// the central coast (Da Nang/Hoi An/Quang Nam). Months grouped:
///  - Dec–Apr: dry season, pleasant 22–30°C
///  - May–Aug: hot dry season, 28–35°C
///  - Sep–Nov: rainy season, 24–30°C with afternoon downpours
pub fn weather_label_for_month(m: Month) -> &'static str {
    match m {
        Month::December | Month::January | Month::February => {
            "Dry season — cool and pleasant, 22–28°C. Best months to visit."
        }
        Month::March | Month::April => "Dry season warming up, 25–32°C. Good for beaches.",
        Month::May | Month::June => "Hot dry season begins, 28–35°C. Bring sun protection.",
        Month::July | Month::August => "Peak heat, 30–35°C. Coastal breeze helps; book A/C.",
        Month::September | Month::October | Month::November => {
            "Rainy season — afternoon downpours, 24–30°C. Pack a light rain shell."
        }
    }
}

/// `true` when an `Activity` carries a usable `source_url` (non-empty). The
/// `activity_row` component uses this to decide whether to render the
/// "Read more" link. Extracted from the component so the rule has a unit
/// test that doesn't need a DOM (Phase 5 §"Empty / edge cases": "add a quick
/// unit test to lock it in").
pub fn has_read_more(source_url: &str) -> bool {
    !source_url.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_label_covers_every_month() {
        for m in [
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
        ] {
            let s = weather_label_for_month(m);
            assert!(!s.is_empty());
            assert!(s.contains("°C"), "label for {m:?} should mention °C");
        }
    }

    #[test]
    fn weather_label_distinguishes_dry_and_rainy() {
        assert!(weather_label_for_month(Month::January).contains("Dry"));
        assert!(weather_label_for_month(Month::October).contains("Rainy"));
    }

    #[test]
    fn format_vnd_reexport_matches_domain_helper() {
        // Re-exports must point at the same impl — sanity check the wiring.
        assert_eq!(format_vnd(Some(50_000)), "💵 ~50,000 VND");
        assert_eq!(format_vnd(Some(0)), "Free");
        assert!(format_vnd(None).is_empty());
    }

    #[test]
    fn format_duration_minutes_reexport_matches_domain_helper() {
        assert_eq!(format_duration_minutes(Some(45)), "⏱ 45 min");
        assert_eq!(format_duration_minutes(Some(120)), "⏱ 2 h");
        assert!(format_duration_minutes(None).is_empty());
    }

    #[test]
    fn has_read_more_true_for_nonempty_url() {
        assert!(has_read_more("https://visitquangnam.com/x"));
        assert!(has_read_more("x"));
    }

    #[test]
    fn has_read_more_false_for_empty_url() {
        assert!(!has_read_more(""));
    }
}
