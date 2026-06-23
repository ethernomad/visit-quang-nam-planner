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
