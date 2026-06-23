// Integration test for `InMemoryRetriever` load + cosine search.
//
// Skips when `data/corpus.json` is absent (Phase 1 not yet run) so this test
// is offline-safe: pure-path unit tests in `src/retrieval/in_memory.rs` run
// unconditionally and cover the cosine math, while this file exercises the
// disk-load path against the committed corpus when it exists.

#![cfg(feature = "server")]

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use visit_quang_nam_planner::retrieval::Embed;
use visit_quang_nam_planner::retrieval::InMemoryRetriever;
use visit_quang_nam_planner::retrieval::Retriever;
use visit_quang_nam_planner::retrieval::cosine_top_k;

/// A canned-vector embedder. Returns the same `Vec<f32>` for every query,
/// so tests can drive `Retriever::search` against real chunk embeddings
/// without hitting OpenAI. The vector's dimensionality must match
/// `text-embedding-3-small` (1536) when used with the committed corpus.
struct MockEmbedder {
    v: Vec<f32>,
}

#[async_trait]
impl Embed for MockEmbedder {
    async fn embed_query(&self, _query: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.v.clone())
    }
}

#[tokio::test]
async fn load_corpus_and_search() {
    let path = Path::new("data/corpus.json");
    if !path.exists() {
        eprintln!("skipping: {path:?} absent (Phase 1 not yet run)");
        return;
    }

    // Load with a stub embedder — loading reads no API.
    let embedder: Arc<dyn Embed> = Arc::new(MockEmbedder { v: vec![1.0; 1536] });
    let retriever =
        InMemoryRetriever::load(path, embedder).expect("committed corpus.json should load");
    assert!(retriever.len() > 0, "corpus should contain chunks");

    // Pure cosine on real chunks with a unit query vector: returns k chunks
    // without panicking and without touching the embedder.
    let unit_q = vec![1.0; 1536];
    let out = cosine_top_k(retriever.chunks(), &unit_q, 5);
    assert!(out.len() <= 5);
    assert!(!out.is_empty(), "cosine_top_k should return results");
    // Each returned Chunk has a 1536-dim embedding (sanity: corpus invariant).
    for c in &out {
        assert_eq!(c.embedding.len(), 1536, "chunk {} dim mismatch", c.id);
    }

    // Retriever::search path uses the mock embedder (canned vector) then runs
    // cosine_top_k — same result as the direct call above.
    let via_search = retriever.search("any query; mock ignores it", 5).await;
    assert_eq!(via_search.len(), out.len());
    assert_eq!(
        via_search.first().map(|c| c.id.clone()),
        out.first().map(|c| c.id.clone())
    );
}
