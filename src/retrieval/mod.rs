// Retrieval. Phase 2: `Retriever` trait + `InMemoryRetriever` (cosine over
// Vec<f32>). Behind a trait so pgvector is a drop-in swap later.
//
// Both traits here are server-only: the wasm client never retrieves (the
// `plan_trip` server function does, and ships typed `Itinerary` back). Keeping
// `Retriever` + `Embed` gated behind `#[cfg(feature = "server")]` plus the
// `async-trait` proc-macro also being server-only keeps the client build clean
// (see AGENTS.md: "Any code touching server-only deps must be gated").

pub use crate::domain::Chunk;

#[cfg(feature = "server")]
pub mod in_memory;

#[cfg(feature = "server")]
pub use in_memory::InMemoryRetriever;

#[cfg(feature = "server")]
pub use in_memory::cosine_top_k;

#[cfg(feature = "server")]
mod traits {
    use async_trait::async_trait;

    use crate::domain::Chunk;

    /// Retrieve up to `k` chunks most similar to a natural-language query.
    ///
    /// Implementations own their embedding strategy: the in-memory backend
    /// embeds the query with OpenAI and scores by cosine similarity; a
    /// future pgvector backend would push the same query vector into SQL
    /// (`ORDER BY embedding <=> $1 LIMIT k`) and never hold chunks in
    /// process. `plan_trip` (Phase 3) only ever calls this trait — never a
    /// concrete struct — so swapping the backend changes nothing in the
    /// orchestrator.
    #[async_trait]
    pub trait Retriever: Send + Sync {
        /// Return up to `k` chunks most similar to `query`, scored by cosine
        /// similarity between the query embedding and chunk embeddings.
        /// Implementations are responsible for embedding `query` themselves.
        async fn search(&self, query: &str, k: usize) -> Vec<Chunk>;

        /// Number of chunks currently searchable. The in-memory backend
        /// returns its held slice length; a future pgvector backend would
        /// run `SELECT count(*)`. Used by `/api/retriever-smoke` and any
        /// future admin introspection endpoint.
        fn len(&self) -> usize;

        /// Convenience predicate. Default impl derive from `len`; backends
        /// may override (e.g. an empty pgvector table can `SELECT EXISTS`).
        fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }

    /// Query-time embedding seam. The real server runs an `OpenAiEmbedder`
    /// against `text-embedding-3-small`; tests inject a `MockEmbedder` that
    /// returns a canned vector so the retriever can be exercised without HTTP.
    /// Lives in `retrieval` (not `ingest`) so `ingest` depends on `retrieval`
    /// without forcing a circular import.
    #[async_trait]
    pub trait Embed: Send + Sync {
        /// Embed a single query string into the same 1536-dim space as the
        /// corpus chunks. Errors should surface a clear cause (missing key,
        /// network, API error) — the caller decides whether to fail hard or
        /// return an empty result set.
        async fn embed_query(&self, query: &str) -> anyhow::Result<Vec<f32>>;
    }
}

#[cfg(feature = "server")]
pub use traits::{Embed, Retriever};
