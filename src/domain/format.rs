//! Pure presentation helpers for the planner UI (Phase 4).
//!
//! These functions live in the library (not `src/components`) so they are
//! reachable from `tests/*.rs` integration tests — `components` is
//! bin-internal per AGENTS.md, so anything that needs cross-crate test
//! coverage must be `pub` here and re-exported via `domain::format`.
//!
//! Keeping them as free functions also lets the components stay thin: an
//! `ActivityRow` just calls `format_price(act.estimated_cost_vnd)` instead
//! of inlining the formatting logic where it can't be exercised without
//! rendering the component (Dioxus 0.7 component render tests are fiddly;
//! the Phase 4 plan accepts pure-helper unit tests as the equivalent
//! coverage).

use crate::domain::Category;

/// Format an estimated cost in Vietnamese đồng as the mockup's price pill
/// does. Suffixes with ` VND`, comma-grouped. Returns the empty string when
/// the LLM left `estimated_cost_vnd` unset so the UI can drop the tag.
pub fn format_price(vnd: Option<u32>) -> String {
    match vnd {
        Some(0) => "Free".to_string(),
        Some(n) => {
            // Group thousands with commas — VND uses no decimals.
            let s = n
                .to_string()
                .as_bytes()
                .rchunks(3)
                .rev()
                .map(std::str::from_utf8)
                .collect::<Result<Vec<&str>, _>>()
                .unwrap_or_default()
                .join(",");
            format!("💵 ~{s} VND")
        }
        None => String::new(),
    }
}

/// Format an activity duration (minutes) as the mockup's `⏱ 45 min` /
/// `⏱ 2 h` pill. Returns empty string when unset.
pub fn format_duration(min: Option<u32>) -> String {
    match min {
        Some(0) | None => String::new(),
        Some(m) if m < 60 => format!("⏱ {m} min"),
        Some(m) => {
            let hours = m / 60;
            let rem = m % 60;
            if rem == 0 {
                format!("⏱ {hours} h")
            } else {
                format!("⏱ {hours}.{rem} h")
            }
        }
    }
}

/// `(emoji icon, category label, Tailwind classes)` for the category tag.
/// Drives both the icon and the bg/text colour so the chip styling in
/// `activity_row.rs` is a one-liner. Mirrors the table in
/// `plans/phase-4-ui.md` §"Activity row".
pub fn category_style(cat: &Category) -> (&'static str, &'static str, &'static str) {
    match cat {
        Category::Food => ("🍴", "Food & Drink", "bg-[#fce4ec] text-[#c62828]"),
        Category::Nature => ("🌿", "Nature", "bg-[#e8f5e9] text-[#2e7d32]"),
        Category::Culture => ("🏛", "Culture", "bg-[#f3e5f5] text-[#6a1b9a]"),
        Category::Beach => ("🏖", "Beach", "bg-[#e3f2fd] text-[#1565c0]"),
        Category::Wellness => ("🧘", "Wellness", "bg-[#fff3e0] text-[#e65100]"),
    }
}

/// Per-day header gradient classes (Tailwind arbitrary value). Mirrors the
/// 5 day-grads in the SVG mockup; cycles after day 5 so longer itineraries
/// still get a coloured strip.
pub fn day_header_gradient(idx_1_based: u8) -> &'static str {
    match ((idx_1_based - 1) % 5) + 1 {
        1 => "from-[#e8f5e9] to-[#f1f8e9]",
        2 => "from-[#fff3e0] to-[#fff8e1]",
        3 => "from-[#e3f2fd] to-[#e8eaf6]",
        4 => "from-[#fce4ec] to-[#f3e5f5]",
        5 => "from-[#e0f7fa] to-[#e8f5e9]",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_price_some_formats_with_commas() {
        assert_eq!(format_price(Some(50_000)), "💵 ~50,000 VND");
        assert_eq!(format_price(Some(1_200_000)), "💵 ~1,200,000 VND");
        assert_eq!(format_price(Some(1)), "💵 ~1 VND");
    }

    #[test]
    fn format_price_zero_is_free() {
        assert_eq!(format_price(Some(0)), "Free");
    }

    #[test]
    fn format_price_none_is_empty() {
        assert!(format_price(None).is_empty());
    }

    #[test]
    fn format_duration_minutes_uses_min() {
        assert_eq!(format_duration(Some(45)), "⏱ 45 min");
        assert_eq!(format_duration(Some(1)), "⏱ 1 min");
    }

    #[test]
    fn format_duration_hours_uses_h() {
        assert_eq!(format_duration(Some(60)), "⏱ 1 h");
        assert_eq!(format_duration(Some(120)), "⏱ 2 h");
    }

    #[test]
    fn format_duration_mixed_hours_and_minutes() {
        assert_eq!(format_duration(Some(90)), "⏱ 1.30 h");
    }

    #[test]
    fn format_duration_none_or_zero_empty() {
        assert!(format_duration(None).is_empty());
        assert!(format_duration(Some(0)).is_empty());
    }

    #[test]
    fn category_style_returns_icon_label_classes() {
        let (icon, label, classes) = category_style(&Category::Food);
        assert_eq!(icon, "🍴");
        assert_eq!(label, "Food & Drink");
        assert!(classes.contains("bg-[#fce4ec]"));
        assert!(classes.contains("text-[#c62828]"));
    }

    #[test]
    fn category_style_covers_each_category() {
        for cat in [
            Category::Food,
            Category::Nature,
            Category::Culture,
            Category::Beach,
            Category::Wellness,
        ] {
            let (_, label, classes) = category_style(&cat);
            assert!(!label.is_empty());
            assert!(classes.contains("bg-[#") && classes.contains("text-[#"));
        }
    }

    #[test]
    fn day_header_gradient_cycles_every_five_days() {
        assert_eq!(day_header_gradient(1), "from-[#e8f5e9] to-[#f1f8e9]");
        assert_eq!(day_header_gradient(5), "from-[#e0f7fa] to-[#e8f5e9]");
        // Day 6 wraps to day 1's gradient.
        assert_eq!(day_header_gradient(6), day_header_gradient(1));
        assert_eq!(day_header_gradient(11), day_header_gradient(1));
        assert_eq!(day_header_gradient(13), day_header_gradient(3));
    }
}
