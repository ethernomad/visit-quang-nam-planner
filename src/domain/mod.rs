pub mod format;

use serde::{Deserialize, Serialize};

// Domain types for the Visit Quang Nam AI Trip Planner.
//
// Phase 1: `Chunk` (a slice of an ingested WordPress post, ready for
// embedding) and `Corpus` (the on-disk container written by the
// `build_corpus` xtask and loaded at server startup by
// `InMemoryRetriever` in Phase 2).
//
// Phase 2 adds `Itinerary`, `DayPlan`, `Activity`, `Preferences`, and
// `TripSummary` via the `plan_trip` server function. Those types have
// no server-only deps so they compile cleanly to wasm.

/// One slice of a Visit Quang Nam article, ready for embedding and
/// retrieval. Stored verbatim in `data/corpus.json` and deserialised at
/// server startup to back `InMemoryRetriever`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    /// Stable id: `"{post_id}-{chunk_index}"`.
    pub id: String,
    /// WordPress post id.
    pub post_id: u64,
    /// Source URL on visitquangnam.com (the post's `link` field).
    pub source_url: String,
    /// Article title (rendered, HTML stripped).
    pub title: String,
    /// Best-fit category if the post has one (e.g. "Food", "Culture",
    /// "Nature", "Beaches", "Wellness", "Green travel", "Practical tips",
    /// "Places", "Events"). `None` if uncategorised (WP term id 1).
    pub category: Option<String>,
    /// ~300-token cleaned text slice used for the embedding. The first
    /// chunk of each post is prefixed with `# {title}\n\n` so the
    /// embedding carries the title context.
    pub text: String,
    /// 1536-dim embedding from `text-embedding-3-small`.
    pub embedding: Vec<f32>,
}

/// On-disk container for the prebuilt RAG corpus. Produced by
/// `cargo run --release --bin build_corpus` and committed to
/// `data/corpus.json` so the server boots offline.
#[derive(Serialize, Deserialize)]
pub struct Corpus {
    /// Embedding model that produced `chunks[*].embedding`,
    /// e.g. `"text-embedding-3-small"`.
    pub model: String,
    /// ISO 8601 timestamp the corpus was generated.
    pub generated_at: String,
    /// All chunks, in post-then-chunk order.
    pub chunks: Vec<Chunk>,
}

// ============================================================================
// Phase 3 — planner types. These cross the client/server boundary in
// `Preferences` (client → server) and `Itinerary` (server → client), so every
// type derives `Serialize` + `Deserialize`. They have no server-only deps and
// compile cleanly to wasm.
// ============================================================================

/// A travel interest the user can pick. Maps to WordPress post categories on
/// visitquangnam.com (Food, Beaches, Culture, Nature, Wellness, Green travel).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Interest {
    Food,
    Beaches,
    Culture,
    Nature,
    Wellness,
    GreenTravel,
}

/// Daily activity pace. Drives the per-day activity count in the prompt
/// (Slow ≤3, Moderate 4, Active 5).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Pace {
    Slow,
    Moderate,
    Active,
}

/// Spending tier. Affects which restaurants/hotels the LLM picks from the
/// grounded chunks.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BudgetTier {
    Backpacker,
    Mid,
    Luxury,
}

/// Month of travel. Used by the LLM for weather context and the per-day
/// `date_hint` (e.g. "Monday").
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Month {
    January,
    February,
    March,
    April,
    May,
    June,
    July,
    August,
    September,
    October,
    November,
    December,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Travelers {
    pub adults: u8,
    pub kids: u8,
}

impl Default for Travelers {
    fn default() -> Self {
        Self { adults: 2, kids: 0 }
    }
}

/// Client → server request payload. Posted to `/api/plan-trip` by the form
/// component (Phase 4). `duration_days` is clamped to 1..=14 server-side.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Preferences {
    pub duration_days: u8,
    pub month: Month,
    pub interests: Vec<Interest>,
    pub travelers: Travelers,
    pub pace: Pace,
    pub budget_tier: BudgetTier,
    pub green_travel: bool,
}

impl Default for Preferences {
    /// The defaults the SVG mockup encodes: 5 days in March, interest in
    /// Food + Beaches, 2 adults / 0 kids, Moderate pace, Mid budget, green
    /// travel on. Phase 4's `use_signal(Preferences::default)` initialises the
    /// form with these values so the empty state already matches the mockup.
    fn default() -> Self {
        Self {
            duration_days: 5,
            month: Month::March,
            interests: vec![Interest::Food, Interest::Beaches],
            travelers: Travelers::default(),
            pace: Pace::Moderate,
            budget_tier: BudgetTier::Mid,
            green_travel: true,
        }
    }
}

/// Coarse activity bucket. surfaced as a label/icon in the timeline (Phase 4).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Category {
    Food,
    Nature,
    Culture,
    Beach,
    Wellness,
}

/// One stop on a day plan. `source_url` MUST point back to the
/// visitquangnam.com article the recommendation came from — `post_validate`
/// rejects any itinerary whose activity `source_url` doesn't appear in the
/// retrieved chunk set, so the LLM cannot invent a URL.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Activity {
    /// "10:00 AM"
    pub time: String,
    pub title: String,
    pub description: String,
    pub category: Category,
    pub source_url: String,
    pub estimated_cost_vnd: Option<u32>,
    pub duration_minutes: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WeatherHint {
    /// "Sunny, 28°C"
    pub label: String,
    /// "☀️"
    pub icon: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DayPlan {
    /// 1-based day index, must equal position+1 in `Itinerary::days`.
    pub index: u8,
    /// "Day 1 — Arrival in Da Nang & Hoi An"
    pub title: String,
    /// "Monday" — derived from the month in the prompt.
    pub date_hint: String,
    pub weather: WeatherHint,
    pub activities: Vec<Activity>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TripSummary {
    /// "5 days / 4 nights"
    pub duration: String,
    /// ["Da Nang", "Hoi An", ...]
    pub destinations: Vec<String>,
    /// "$520–680 per person (excl. flights)"
    pub budget_estimate: String,
    /// 0..=100 — the LLM scores this from the green-travel choices it made.
    /// `post_validate` caps at 100; the UI renders a sustainability badge.
    pub sustainability_score: u8,
}

/// Server → client response payload. Returned by `plan_trip`; rendered by the
/// form/day-tabs/timeline/summary components in Phase 4.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Itinerary {
    pub days: Vec<DayPlan>,
    pub summary: TripSummary,
}
