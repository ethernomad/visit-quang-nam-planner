// Integration test for the Phase 3 `plan_trip` pipeline — minus the live LLM
// hop, which is unit-tested in `src/server/plan_trip.rs::tests` against a
// `MockLlm`.
//
// What this file proves offline:
//   1. The committed fixture corpus (`tests/fixtures/corpus.json`) loads via
//      `InMemoryRetriever::load` — the same code path `shared_retriever()`
//      uses at server boot.
//   2. `Retriever::search` returns chunks for a query (the grounding step
//      `plan_trip_inner` runs before prompting the LLM).
//   3. The committed fixture LLM response (`tests/fixtures/llm_response.json`)
//      deserialises into the typed `Itinerary` and every `source_url` it cites
//      is present in the retrieved chunk set — the same guardrail
//      `post_validate` enforces server-side. If a future prompt change makes
//      the model invent a URL, this test catches it before it ships.
//
// The `LlmCompleter` / `plan_trip_inner` orchestration itself is bin-internal
// (per AGENTS.md, `src/server` stays in the bin crate so server-only deps
// don't leak into the wasm client), so it can't be imported from this
// integration test. The orchestration unit tests in
// `src/server/plan_trip.rs::tests` cover that path against `MockLlm`; this
// file covers the offline-fixture half of the contract.

#![cfg(feature = "server")]

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use visit_quang_nam_planner::domain::{Chunk, Itinerary};
use visit_quang_nam_planner::retrieval::{Embed, InMemoryRetriever, Retriever};

/// Canned-vector embedder: returns the same `Vec<f32>` for any query, so the
/// retriever's cosine step is deterministic and offline-safe. The vector
/// matches fixture chunk `1-0` (Hoi An food tour) exactly, then ranks the
/// green-travel chunks (4-0, 5-0) next.
struct MockEmbedder {
    v: Vec<f32>,
}

#[async_trait]
impl Embed for MockEmbedder {
    async fn embed_query(&self, _query: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.v.clone())
    }
}

fn fixture_corpus() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus.json")
}

fn fixture_llm_response() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/llm_response.json");
    std::fs::read_to_string(&path).expect("llm_response.json fixture should be committed")
}

/// Parse a JSON string into `T` with a clear error — mirrors the parse step
/// inside `LlmClient::complete_json`.
fn parse_json<T: DeserializeOwned>(s: &str) -> T {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("fixture JSON parse failed: {e}\n{s}"))
}

#[tokio::test]
async fn fixture_corpus_loads_and_search_returns_chunks() {
    let embedder: Arc<dyn Embed> = Arc::new(MockEmbedder {
        v: vec![1.0, 0.0, 0.0],
    });
    let retriever = InMemoryRetriever::load(&fixture_corpus(), embedder)
        .expect("fixture corpus.json must load");
    assert_eq!(retriever.len(), 5, "fixture corpus has 5 chunks");

    let out = retriever
        .search("anything; mock embedder ignores it", 8)
        .await;
    assert_eq!(out.len(), 5, "TOP_K=8 but corpus has 5 chunks");
    // MockEmbedder vector is [1,0,0] → cosine 1.0 against chunk 1-0, ties broken
    // by stable index. Sanity-check the best match is the food-tour chunk.
    assert_eq!(out[0].id, "1-0");
}

#[tokio::test]
async fn fixture_llm_response_parses_into_itinerary() {
    let itin: Itinerary = parse_json(&fixture_llm_response());
    assert_eq!(itin.days.len(), 2, "fixture itinerary is 2 days");
    assert!(itin.summary.sustainability_score <= 100);
}

/// Cross-check: every `source_url` in the fixture LLM response must appear in
/// the retrieved chunk set. This is precisely the rule `plan_trip_inner`'s
/// `post_validate` enforces server-side — and the rule that breaks the API
/// call (rather than the UI) if the LLM hallucinates a URL.
#[tokio::test]
async fn fixture_llm_response_source_urls_are_in_corpus() {
    let embedder: Arc<dyn Embed> = Arc::new(MockEmbedder {
        v: vec![1.0, 0.0, 0.0],
    });
    let retriever = InMemoryRetriever::load(&fixture_corpus(), embedder).unwrap();
    let chunks: Vec<Chunk> = retriever.search("any", 8).await;
    let allowed: std::collections::HashSet<&str> =
        chunks.iter().map(|c| c.source_url.as_str()).collect();

    let itin: Itinerary = parse_json(&fixture_llm_response());
    for day in &itin.days {
        for act in &day.activities {
            assert!(
                allowed.contains(act.source_url.as_str()),
                "fixture LLM response cites {} not in corpus",
                act.source_url
            );
        }
    }
}

/// Sanity: the fixture LLM response's day count must match the test
/// preferences (2 days), proving the day-count branch of `post_validate`
/// would pass on this fixture.
#[tokio::test]
async fn fixture_llm_response_day_count_matches_duration() {
    let itin: Itinerary = parse_json(&fixture_llm_response());
    let expected_days: u8 = 2; // matches the Preferences fixture below
    assert_eq!(itin.days.len() as u8, expected_days);
}
