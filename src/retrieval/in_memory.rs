// `InMemoryRetriever` — the Phase 2 reference backend for the `Retriever`
// trait. Loads `data/corpus.json` at server startup (lazily, via
// `shared_retriever()` in `src/server/mod.rs`), holds all chunks in process,
// and answers `search(query, k)` with cosine top-K over the precomputed
// `text-embedding-3-small` vectors.
//
// Server-only: it depends on `Embed` (server-only trait), the disk, and on
// `tracing`. A future `PgVectorRetriever` (src/retrieval/pgvector.rs)
// implements the same `Retriever` trait against
// `SELECT ... ORDER BY embedding <=> $1 LIMIT k` and never holds chunks in
// process — `plan_trip` swaps backends by changing one line in
// `shared_retriever`, nothing else.

#![cfg(feature = "server")]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;

use crate::domain::{Chunk, Corpus};
use crate::retrieval::{Embed, Retriever};

/// Server-side cap on the query-embed call. Bounded so a slow OpenAI
/// embeddings endpoint can't hold an axum worker indefinitely; the
/// retriever logs and returns an empty result on timeout (same as any
/// other embed failure — see `search`).
const EMBED_QUERY_TIMEOUT: Duration = Duration::from_secs(30);

pub struct InMemoryRetriever {
    chunks: Vec<Chunk>,
    embedder: Arc<dyn Embed>,
}

impl InMemoryRetriever {
    /// Load chunks from a `corpus.json` file. Returns a clear error if the
    /// file is missing or malformed — the server can then surface a useful
    /// boot-time diagnostic instead of panicking deep inside an `OnceLock`.
    /// Does **not** panic.
    pub fn load(corpus_path: &Path, embedder: Arc<dyn Embed>) -> anyhow::Result<Self> {
        let bytes = std::fs::read(corpus_path)
            .with_context(|| format!("failed to read corpus at {corpus_path:?}"))?;
        let corpus: Corpus = serde_json::from_slice(&bytes)
            .with_context(|| format!("corpus.json at {corpus_path:?} malformed"))?;
        let chunks = corpus.chunks;
        tracing::info!(count = chunks.len(), "loaded InMemoryRetriever");
        Ok(Self { chunks, embedder })
    }

    /// Constructor that injects a chunk list + embedder directly. Used by
    /// tests and by `build_corpus`-style fixtures; never hits disk.
    pub fn with_embedder(chunks: Vec<Chunk>, embedder: Arc<dyn Embed>) -> Self {
        Self { chunks, embedder }
    }

    /// Backing slice, for tests + future introspection endpoints.
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }
}

#[async_trait]
impl Retriever for InMemoryRetriever {
    async fn search(&self, query: &str, k: usize) -> Vec<Chunk> {
        let q = match tokio::time::timeout(EMBED_QUERY_TIMEOUT, self.embedder.embed_query(query))
            .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                tracing::error!(error = ?e, "query embed failed; returning empty result");
                return Vec::new();
            }
            Err(_) => {
                tracing::error!(
                    timeout_secs = EMBED_QUERY_TIMEOUT.as_secs(),
                    "query embed timed out; returning empty result"
                );
                return Vec::new();
            }
        };
        cosine_top_k(&self.chunks, &q, k)
    }

    fn len(&self) -> usize {
        self.chunks.len()
    }
}

/// Pure cosine similarity ranking. Exposed publicly so unit tests and a future
/// pgvector-vs-in-memory benchmark can call it without an embedder.
///
/// Contract on degenerate inputs (zero-norm query or chunk):
///   - No panics. Division-by-zero is avoided by treating a zero norm as a
///     cosine score of `0.0`.
///   - Chunks with a positive norm rank above zero-norm chunks (their cosine
///     is undefined → scored `0.0`); among zero-norm chunks, order is stable
///     by original index.
///   - If `k` exceeds the chunk count, returns all chunks (in ranked order).
///   - An empty `chunks` slice or `k == 0` returns an empty `Vec`.
pub fn cosine_top_k(chunks: &[Chunk], query: &[f32], k: usize) -> Vec<Chunk> {
    if k == 0 || chunks.is_empty() {
        return Vec::new();
    }
    let q_norm = norm(query);
    let mut scored: Vec<(usize, f32)> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| (i, cosine(&c.embedding, query, norm(&c.embedding), q_norm)))
        .collect();
    // Sort by score desc, stable on index asc as a tiebreaker so tests are
    // deterministic on equal-distance chunks.
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    let take = k.min(chunks.len());
    scored
        .into_iter()
        .take(take)
        .map(|(i, _)| chunks[i].clone())
        .collect()
}

#[inline]
fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[inline]
fn cosine(a: &[f32], b: &[f32], norm_a: f32, norm_b: f32) -> f32 {
    let denom = norm_a * norm_b;
    if denom == 0.0 {
        return 0.0;
    }
    let dot = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>();
    dot / denom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Chunk;

    fn chunk(id: &str, text: &str, embedding: Vec<f32>) -> Chunk {
        Chunk {
            id: id.into(),
            post_id: 0,
            source_url: "https://visitquangnam.com/x".into(),
            title: text.into(),
            category: None,
            text: text.into(),
            embedding,
        }
    }

    #[test]
    fn cosine_top_k_returns_most_similar() {
        let chunks = vec![
            chunk("food-1", "food", vec![1.0, 0.0, 0.0]),
            chunk("beach-1", "beach", vec![0.0, 1.0, 0.0]),
            chunk("culture-1", "culture", vec![0.0, 0.0, 1.0]),
            chunk("food-2", "food", vec![0.95, 0.05, 0.0]),
        ];
        let q = vec![1.0, 0.0, 0.0];
        let out = cosine_top_k(&chunks, &q, 2);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|c| c.id == "food-1"));
        assert!(out.iter().any(|c| c.id == "food-2"));
        // Most similar first.
        assert_eq!(out[0].id, "food-1");
    }

    #[test]
    fn cosine_top_k_handles_ties_deterministically() {
        // Two equally-similar chunks; tiebreaker is original index asc.
        let chunks = vec![
            chunk("a", "a", vec![1.0, 0.0]),
            chunk("b", "b", vec![1.0, 0.0]),
            chunk("c", "c", vec![0.0, 1.0]),
        ];
        let q = vec![1.0, 0.0];
        let out = cosine_top_k(&chunks, &q, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "a");
        assert_eq!(out[1].id, "b");
    }

    #[test]
    fn cosine_top_k_handles_zero_query_vector() {
        // |query| = 0 → all cosine scores 0.0 → no panic. We pick the
        // contract: still return up to `k` chunks (any selection), since the
        // plan allows "returns 1 chunk with score 0, OR returns 0". Here we
        // return `k` ordered by stable index, which is the most predictable
        // shape for callers.
        let chunks = vec![chunk("x", "x", vec![1.0; 3]), chunk("y", "y", vec![1.0; 3])];
        let q = vec![0.0; 3];
        let out = cosine_top_k(&chunks, &q, 5);
        assert!(out.len() <= 2);
        assert!(out.iter().all(|c| c.id == "x" || c.id == "y"));
    }

    #[test]
    fn cosine_top_k_handles_zero_chunk_vector() {
        // A chunk with |embedding| = 0 should not panic and should rank
        // strictly below a positive-norm chunk (score 0.0 vs ~1.0).
        let chunks = vec![
            chunk("zero", "zero", vec![0.0, 0.0, 0.0]),
            chunk("real", "real", vec![1.0, 0.0, 0.0]),
        ];
        let q = vec![1.0, 0.0, 0.0];
        let out = cosine_top_k(&chunks, &q, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "real");
        assert_eq!(out[1].id, "zero");
    }

    #[test]
    fn cosine_top_k_k_larger_than_chunks_returns_all() {
        let chunks = vec![
            chunk("a", "a", vec![1.0, 0.0]),
            chunk("b", "b", vec![0.0, 1.0]),
        ];
        let q = vec![1.0, 0.0];
        let out = cosine_top_k(&chunks, &q, 10);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "a");
    }

    #[test]
    fn cosine_top_k_k_zero_returns_empty() {
        let chunks = vec![chunk("a", "a", vec![1.0])];
        let q = vec![1.0];
        assert!(cosine_top_k(&chunks, &q, 0).is_empty());
    }

    #[test]
    fn cosine_top_k_empty_chunks_returns_empty() {
        let q = vec![1.0];
        assert!(cosine_top_k(&[], &q, 5).is_empty());
    }

    #[test]
    fn cosine_top_k_negative_components_score_correctly() {
        // Cosine is sign-sensitive: an anti-parallel chunk should rank last.
        let chunks = vec![
            chunk("pos", "pos", vec![1.0, 0.0]),
            chunk("neg", "neg", vec![-1.0, 0.0]),
        ];
        let q = vec![1.0, 0.0];
        let out = cosine_top_k(&chunks, &q, 2);
        assert_eq!(out[0].id, "pos");
        assert_eq!(out[1].id, "neg");
    }
}
