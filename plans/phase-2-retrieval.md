# Phase 2 — Retrieval

**Goal:** Define the `Retriever` trait and implement an
`InMemoryRetriever` that loads `data/corpus.json` at server startup and
answers `search(query, k)` with top-K cosine-similar chunks. The trait
backs Phase 3's `plan_trip` server function. Provide offline unit tests
that exercise the retrieval path without hitting the network.

**Status:** pending
**Depends on:** Phase 0 (scaffold — done). Phase 1's `corpus.json` must
exist to test load behaviour, but the *trait* and *retriever logic* can
be implemented and tested with a hand-authored fixture — don't block on
Phase 1; see "Offline test corpus" below.

## Files to create / edit

- `src/retrieval/mod.rs` — the `Retriever` trait and `Chunk` re-export
  (already declared in `src/domain/mod.rs`). Keep `InMemoryRetriever`
  gated behind `#[cfg(feature = "server")]` since it loads from disk.
- `src/retrieval/in_memory.rs` — `InMemoryRetriever` implementation.
- `src/retrieval/embed.rs` — a thin wrapper around the OpenAI embeddings
  API for query-time embedding (server-only). Could also reuse Phase 1's
  `src/ingest/embedder.rs` if it exposes a `pub async fn embed_query`
  — do NOT duplicate; share.
- `src/lib.rs` — ensure `pub mod retrieval;` is exported (Phase 1 added
  it; verify).
- `src/server/mod.rs` — add a `OnceLock<Arc<dyn Retriever>>` global so
  PlanTrip (Phase 3) can pull a shared handle. Load happens lazily on
  first request.
- `tests/retrieval.rs` (or `src/retrieval/in_memory.rs` `#[cfg(test)]`)
  — the offline test suite.

## The trait

```rust
// src/retrieval/mod.rs
use crate::domain::Chunk;
use std::future::Future;

#[async_trait::async_trait]
pub trait Retriever: Send + Sync {
    /// Return up to `k` chunks most similar to `query`, scored by
    /// cosine similarity between the query embedding and the chunk
    /// embeddings. Implementations are responsible for embedding
    /// `query` themselves; swappable backends embed differently (e.g.
    /// pgvector `embedding <=> $1` from a server-provided vector).
    async fn search(&self, query: &str, k: usize) -> Vec<Chunk>;
}

#[cfg(feature = "server")]
pub mod in_memory;
```

Add `async-trait = { version = "0.1", optional = true }` to `Cargo.toml`
and `dep:async-trait` to the `server` feature list. It's a proc-macro
crate at the call site but the trait needs it wherever defined — keep
it optional so the wasm client (which never references `Retriever`)
doesn't compile it.

> If `async-trait` produces ugly errors because the trait is referenced
> from non-server code, split it: a `RetrieverServer` variant behind
> the feature flag and a plain educational "preview" trait for the
> client. Phase 3 only needs the server variant.

## InMemoryRetriever

```rust
// src/retrieval/in_memory.rs
use crate::domain::{Chunk, Corpus};
use crate::retrieval::Retriever;
use async_trait::async_trait;
use std::path::Path;

pub struct InMemoryRetriever {
    chunks: Vec<Chunk>,
    embedder: crate::ingest::embedder::EmbedderClient,
}

impl InMemoryRetriever {
    /// Load chunks from a corpus.json file. Panics if the file is
    /// missing, since the server cannot function without a corpus.
    pub fn load(corpus_path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(corpus_path)
            .map_err(|e| anyhow::anyhow!("failed to read corpus at {corpus_path:?}: {e}"))?;
        let corpus: Corpus = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("corpus.json malformed: {e}"))?;
        let chunks = corpus.chunks;
        tracing::info!(count = chunks.len(), "loaded InMemoryRetriever");
        Ok(Self {
            chunks,
            embedder: crate::ingest::embedder::EmbedderClient::from_env()?,
        })
    }

    /// Test/fixture constructor that injects a stub embedder.
    #[cfg(test)]
    pub(crate) fn with_embedder(chunks: Vec<Chunk>, embedder: crate::ingest::embedder::EmbedderClient) -> Self {
        Self { chunks, embedder }
    }
}

#[async_trait]
impl Retriever for InMemoryRetriever {
    async fn search(&self, query: &str, k: usize) -> Vec<Chunk> {
        let q = match self.embedder.embed_query(query).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = ?e, "query embed failed; returning empty result");
                return Vec::new();
            }
        };
        cosine_top_k(&self.chunks, &q, k)
    }
}

/// Pure cosine similarity on 1536-dim vectors. Exported for unit tests.
pub fn cosine_top_k(chunks: &[Chunk], query: &[f32], k: usize) -> Vec<Chunk> {
    // 1. score every chunk by (a·b)/(|a||b|)
    // 2. keep top-k by partial selection (avoid full sort; for ~200 chunks
    //    sort is fine — measure in a benchmark before optimising)
    // 3. return the chunks (idealised: Chunk with a `score: f32` field
    //    would let Phase 3 surface relevance — but per the locked types
    //    in docs/domain.md, Chunk has no score. Keep score in a local
    //    (usize, f32) tuple and copy the Chunk out.)
    todo!()
}
```

## Refactor `embedder` so query-time can reuse it

Phase 1's embedder exposes `embed(texts: &[String]) -> Vec<Vec<f32>>`.
Add a `pub struct EmbedderClient { openai: async_openai::Client<OpenAIConfig> }`
with:

- `pub fn from_env() -> anyhow::Result<Self>` (reads `OPENAI_API_KEY`)
- `pub async fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>`
- `pub async fn embed_query(&self, query: &str) -> anyhow::Result<Vec<f32>>`
  (one input, returns the single embedding)

Phase 1's `pub async fn embed(...)` free function should be rewritten as
a method on `EmbedderClient` — or add the struct as a thin wrapper
around the existing function and have both call sites use it. Pick one
shape; don't keep two implementations.

## Register the retriever in `src/server/mod.rs`

```rust
#[cfg(feature = "server")]
pub fn shared_retriever() -> anyhow::Result<std::sync::Arc<dyn retrieval::Retriever>> {
    static RETRIEVER: std::sync::OnceLock<anyhow::Result<std::sync::Arc<dyn retrieval::Retriever>>> =
        std::sync::OnceLock::new();
    Ok(RETRIEVER
        .get_or_init(|| {
            let path = std::env::var("CORPUS_PATH").unwrap_or("data/corpus.json".into());
            retrieval::InMemoryRetriever::load(std::path::Path::new(&path))
                .map(|r| std::sync::Arc::new(r) as std::sync::Arc<dyn retrieval::Retriever>)
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("retriever init failed: {e}"))?
        .clone())
}
```

`OnceLock` is fine for a stateless MVP — the corpus is immutable once
booted. To re-index, restart the server.

When a future `PgVectorRetriever` is added:

```rust
pub mod pgvector;            // src/retrieval/pgvector.rs
// implements Retriever against SELECT ... ORDER BY embedding <=> $1 LIMIT k
```

`plan_trip` only ever calls `shared_retriever()` — no backend branch.

## Offline test corpus

`src/retrieval/in_memory.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn cosine_top_k_handles_zero_vectors() {
        // |query| = 0 should not panic; returns empty or arbitrary chunks
        let chunks = vec![chunk("x", "x", vec![1.0; 3])];
        let q = vec![0.0; 3];
        let out = cosine_top_k(&chunks, &q, 5);
        // pick the contract: returns 1 chunk with score 0, OR returns 0
        // Either is fine — but never divide-by-zero panic. Assert the
        // choice in code.
        assert!(out.len() <= 1);
    }
}
```

Add a `tests/load_corpus.rs` integration test that **skips** if
`data/corpus.json` is absent (Phase 1 not yet run) and otherwise asserts
that loading succeeds and `cosine_top_k` returns sensible results on a
sample query with deterministic embeddings (use a sentinel query string
whose embedding you've stubbed via `EmbedderClient`'s test seam — see
below). Skip-on-missing is fine; tests on the public-facing trait should
not depend on the internet.

## Mockable embedder (for tests)

Give `EmbedderClient` a test-only seam so unit tests can inject a
query embedding without calling OpenAI:

```rust
#[cfg(test)]
impl EmbedderClient {
    pub fn with_query_response(embedding: Vec<f32>) -> Self { /* ... */ }
}
```

OR model it as `trait Embed { async fn embed_query(&self) -> ... }`
and inject a `MockEmbedder`. Move the trait into `src/retrieval`
behind `#[cfg(feature = "server")]` to keep the wasm client clean.
Pick whichever path compiles cleanest — but **do** make the retriever
testable without making HTTP calls.

## Tasks

1. Add `async-trait` to `Cargo.toml` (optional, in `server` feature).
2. Define `Retriever` in `src/retrieval/mod.rs`.
3. Implement `InMemoryRetriever` in `src/retrieval/in_memory.rs`
   (server-only module).
4. Refactor `src/ingest/embedder.rs` so query-time embedding is reusable
   without inheriting Phase 1's batch utils.
5. Add the `OnceLock` `shared_retriever()` to `src/server/mod.rs`.
6. Add the `cosine_top_k` pure function with unit tests.
7. Wire `mod retrieval` into `src/lib.rs`.
8. Ensure no new code is pulled into the wasm build — `cargo check
   --target wasm32-unknown-unknown --no-default-features --features web`
   must still succeed.

## Acceptance criteria

- [ ] `Retriever` trait + `InMemoryRetriever` exist, compiles cleanly
      on the server target.
- [ ] `cosine_top_k` has unit tests covering the typical case, ties
      (equal distances), zero-vector inputs, and `k` larger than the
      chunk count.
- [ ] `InMemoryRetriever::load` returns a clear error if the corpus
      file is missing or malformed; it does **not** panic.
- [ ] `shared_retriever()` is callable from a `#[server]` fn (smoke
      test: a tiny `#[get("/api/retriever-smoke")] async fn smoke() ->
      Result<usize>` that returns the chunk count).
- [ ] All four CI gates pass. Add a `#[test]` that the wasm client
      build does not break (`cargo check --target
      wasm32-unknown-unknown --no-default-features --features web`).
- [ ] `cargo test --features server` passes end-to-end without the
      OpenAI key (tests use the mock embedder seam).

## Notes for the agent

- `OnceLock::get_or_init`'s closure runs once per process. If init
  fails, the error is **cached** too — `get_or_init` will not retry.
  For an MVP that's acceptable (operator restarts the server). Document
  it in the doc comment so future you doesn't waste a day on it.
- Don't parallelise `cosine_top_k` with `rayon` for ~200 chunks; the
  overhead dominates. Add a benchmark only if Phase 3 latency warrants.
- `Chunk` is declared in `domain/mod.rs` by Phase 1 — do not duplicate
  it. Re-export from `retrieval/mod.rs` via `pub use crate::domain::Chunk;`
  so `plan_trip` can import from one place.
- Do not add a `score` field to `Chunk` — keep `domain::Chunk` stable
  (Phase 1 wrote it to disk). Track scores in a local `(usize, f32)`
  tuple during selection. Phase 3 will surface score in the UI purely
  on the `Activity` type if needed.