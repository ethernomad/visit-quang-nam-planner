// Prompt templates for the Visit Quang Nam planner. Kept in one file so
// prompt iteration doesn't touch orchestration (`plan_trip.rs`) — tune the
// wording here, run the gates, ship.
//
// Output contract is enforced two ways:
//   1. The system prompt inlines a TypeScript-shaped schema description the
//      model must mirror, and `response_format: json_object` (Zen's
//      OpenAI-compatible JSON mode) constrains the response to a single JSON
//      object.
//   2. `plan_trip::post_validate` rejects any itinerary whose day count,
//      `source_url`s, or `sustainability_score` don't satisfy the contract —
//      the authoritative guardrail. The prompt is best-effort; the
//      validator is the law.
//
// Per AGENTS.md: do NOT add a `schemars` dependency. The schema string below
// is hand-maintained and version-controlled; if `Itinerary` changes, update
// `SCHEMA_SHAPE` in the same PR.

#![cfg(feature = "server")]

use visit_quang_nam_planner::domain::{Chunk, Preferences};

/// Fixed-role system prompt. `{duration}` and `{month}` are replaced by
/// `plan_trip` per request — kept as placeholders (not `format!`) so the
/// template reads as a string and accidental `{` in the JSON example don't
/// trip `format!`.
pub const SYSTEM_PROMPT: &str = "\
You are the Visit Quang Nam travel planner. Your job is to build a \
personalised day-by-day itinerary from the user's preferences and the \
provided local-knowledge chunks.

Rules:
1. Use ONLY the information in the provided chunks. Do NOT invent \
restaurants, hotels, beaches, or prices. If a preference can't be met from \
the chunks, say so briefly in that day's title or skip the slot rather than \
fabricate.
2. Each activity's `source_url` MUST come from a chunk's `source_url`. \
Quang Nam content you remember from training is NOT allowed as a source — \
only the URLs in the chunks block.
3. Plan exactly {duration} consecutive days (Day 1 through Day {duration}). \
Each day should have 3-5 activities from morning to evening.
4. Respect the pace (Slow <=3 activities/day, Moderate 4, Active 5).
5. Respect the budget tier and traveller ages (kids change pace and picks).
6. When `green_travel` is true, prefer chunks tagged Green travel, \
sustainability, or community-based tourism, and raise the \
sustainability_score accordingly.
7. Weather hints should reflect Quang Nam's climate in {month}.
8. Compute `sustainability_score` 0-100 from how Green Travel-aligned the \
chosen activities are. 100 = every activity is community/eco-aligned; \
0 = none are.

Return ONE JSON object matching this TypeScript shape (no markdown fences, \
no commentary, nothing outside the JSON):

{
  \"days\": [
    {
      \"index\": 1,
      \"title\": \"Day 1 - Arrival ...\",
      \"date_hint\": \"Monday\",
      \"weather\": { \"label\": \"Sunny, 28C\", \"icon\": \"sun\" },
      \"activities\": [
        {
          \"time\": \"10:00 AM\",
          \"title\": \"...\",
          \"description\": \"...\",
          \"category\": \"Food\" | \"Nature\" | \"Culture\" | \"Beach\" | \"Wellness\",
          \"source_url\": \"https://visitquangnam.com/...\",
          \"estimated_cost_vnd\": 120000,
          \"duration_minutes\": 60
        }
      ]
    }
  ],
  \"summary\": {
    \"duration\": \"5 days / 4 nights\",
    \"destinations\": [\"Da Nang\", \"Hoi An\"],
    \"budget_estimate\": \"$520-680 per person (excl. flights)\",
    \"sustainability_score\": 82
  }
}

The `estimated_cost_vnd` and `duration_minutes` fields are optional; you may \
omit them when unknown. Every other field is required. Do not add fields \
outside this shape.";

/// Build the per-request user prompt: preferences as YAML + the retrieved
/// chunks (id, title, category, text, source_url) + a final instruction.
/// `serde_yaml` is intentionally NOT pulled in just for this — we hand-format
/// the preferences block. It's small and stable, and avoids a new dep.
pub fn build_user_prompt(prefs: &Preferences, chunks: &[Chunk]) -> String {
    let mut s = String::with_capacity(8 * 1024);
    s.push_str("# Preferences\n\n");
    s.push_str(&format_preferences(prefs));
    s.push_str("\n\n# Local knowledge chunks\n\n");
    for (i, c) in chunks.iter().enumerate() {
        s.push_str(&format!(
            "## [{}] {} ({})\n{}\nURL: {}\n\n",
            i + 1,
            c.title,
            c.category.as_deref().unwrap_or("General"),
            c.text,
            c.source_url,
        ));
    }
    s.push_str(
        "Now return the itinerary JSON. Use only the chunk URLs above as \
`source_url` values. Do not include any text before or after the JSON.",
    );
    s
}

/// Hand-rolled preferences formatter. Single source of truth for what the
/// model sees about the user's request — kept here so à la carte prompt
/// tweaks don't sprawl across `plan_trip.rs`.
fn format_preferences(p: &Preferences) -> String {
    let mut s = String::new();
    s.push_str(&format!("duration_days: {}\n", p.duration_days));
    s.push_str(&format!("month: {:?}\n", p.month));
    s.push_str(&format!("pace: {:?}\n", p.pace));
    s.push_str(&format!("budget_tier: {:?}\n", p.budget_tier));
    s.push_str(&format!("green_travel: {}\n", p.green_travel));
    s.push_str(&format!(
        "travelers: {{ adults: {}, kids: {} }}\n",
        p.travelers.adults, p.travelers.kids
    ));
    s.push_str("interests:");
    if p.interests.is_empty() {
        s.push_str(" []\n");
    } else {
        for i in &p.interests {
            s.push_str(&format!("\n  - {:?}", i));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use visit_quang_nam_planner::domain::{
        BudgetTier, Interest, Month, Pace, Preferences, Travelers,
    };

    fn sample_prefs() -> Preferences {
        Preferences {
            duration_days: 3,
            month: Month::March,
            interests: vec![Interest::Food, Interest::Beaches],
            travelers: Travelers { adults: 2, kids: 0 },
            pace: Pace::Slow,
            budget_tier: BudgetTier::Mid,
            green_travel: true,
        }
    }

    #[test]
    fn system_prompt_has_required_placeholders() {
        assert!(SYSTEM_PROMPT.contains("{duration}"));
        assert!(SYSTEM_PROMPT.contains("{month}"));
    }

    #[test]
    fn replace_placeholders_substitutes_duration_and_month() {
        let p = SYSTEM_PROMPT
            .replace("{duration}", "3")
            .replace("{month}", "March");
        assert!(p.contains("Plan exactly 3 consecutive days"));
        assert!(p.contains("climate in March"));
        assert!(!p.contains("{duration}"));
        assert!(!p.contains("{month}"));
    }

    #[test]
    fn build_user_prompt_includes_chunks_and_prefs() {
        let chunk = Chunk {
            id: "1-0".into(),
            post_id: 1,
            source_url: "https://visitquangnam.com/post-1".into(),
            title: "Hoi An food tour".into(),
            category: Some("Food".into()),
            text: "Try cao lau and banh mi in the old town.".into(),
            embedding: Vec::new(),
        };
        let out = build_user_prompt(&sample_prefs(), std::slice::from_ref(&chunk));
        // Preferences block
        assert!(out.contains("duration_days: 3"));
        assert!(out.contains("month: March"));
        assert!(out.contains("green_travel: true"));
        assert!(out.contains("Food"));
        // Chunk surfaced with URL
        assert!(out.contains("Hoi An food tour"));
        assert!(out.contains("https://visitquangnam.com/post-1"));
        // Final instruction
        assert!(out.contains("return the itinerary JSON"));
    }

    #[test]
    fn format_preferences_handles_empty_interests() {
        let mut p = sample_prefs();
        p.interests = Vec::new();
        let s = format_preferences(&p);
        assert!(s.contains("interests: []"));
    }
}
