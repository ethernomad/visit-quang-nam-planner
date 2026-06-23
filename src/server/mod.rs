// Server functions / singletons. Phase 3: `plan_trip` orchestrates
// retrieve → prompt → LLM → typed `Itinerary`. Phase 4 calls `plan_trip`
// from the wasm client via the `#[post]`-generated client stub, so the
// `plan_trip` symbol must be visible under both `web`-only and `server`
// builds — but the helpers it uses (`shared_retriever`, `shared_llm`,
// `llm::LlmCompleter`, `prompts`) touch server-only deps and stay gated
// behind `#[cfg(feature = "server")]`.

#[cfg(feature = "server")]
use std::path::PathBuf;
#[cfg(feature = "server")]
use std::sync::{Arc, OnceLock};

#[cfg(feature = "server")]
use dioxus::prelude::*;

#[cfg(feature = "server")]
use visit_quang_nam_planner::ingest::embedder::OpenAiEmbedder;
#[cfg(feature = "server")]
use visit_quang_nam_planner::retrieval::{InMemoryRetriever, Retriever};

pub mod llm;
pub mod plan_trip;
pub mod prompts;

/// Returns a process-wide shared retriever handle. Initialisation runs once
/// per process on first call; if it fails, the error is **cached** and
/// subsequent calls return the same error without retrying — an MVP choice
/// matching AGENTS.md ("operator restarts the server to re-index").
///
/// Corpus path is configurable via `CORPUS_PATH` (default
/// `data/corpus.json`). A future `PgVectorRetriever` swap is a one-line
/// change inside this function; `plan_trip` only ever calls
/// `shared_retriever()` and stays backend-agnostic.
#[cfg(feature = "server")]
pub fn shared_retriever() -> anyhow::Result<Arc<dyn Retriever>> {
    static RETRIEVER: OnceLock<anyhow::Result<Arc<dyn Retriever>>> = OnceLock::new();
    Ok(RETRIEVER
        .get_or_init(|| {
            let path = std::env::var("CORPUS_PATH").unwrap_or("data/corpus.json".into());
            let path = PathBuf::from(path);
            let embedder: Arc<dyn visit_quang_nam_planner::retrieval::Embed> =
                Arc::new(OpenAiEmbedder::from_env()?);
            InMemoryRetriever::load(&path, embedder).map(|r| Arc::new(r) as Arc<dyn Retriever>)
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("retriever init failed: {e}"))?
        .clone())
}

/// Process-wide shared `LlmClient`. Same `OnceLock`-with-cached-error pattern
/// as `shared_retriever()`: if `from_env()` fails (e.g. `OPENCODE_API_KEY`
/// unset), the error is cached for the process lifetime and the operator
/// restarts the server after exporting the key.
///
/// `plan_trip` calls this via the `#[post]` wrapper. Tests bypass it by
/// calling `plan_trip_inner` with their own `MockLlm`.
#[cfg(feature = "server")]
pub fn shared_llm() -> anyhow::Result<Arc<dyn llm::LlmCompleter>> {
    static LLM: OnceLock<anyhow::Result<Arc<dyn llm::LlmCompleter>>> = OnceLock::new();
    Ok(LLM
        .get_or_init(|| {
            let client = llm::LlmClient::from_env()?;
            Ok(Arc::new(client) as Arc<dyn llm::LlmCompleter>)
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("LLM init failed: {e}"))?
        .clone())
}

/// Smoke endpoint: returns the corpus chunk count. Fails if the corpus file
/// is missing/malformed or `OPENAI_API_KEY` isn't set (the latter only
/// matters at first query embedding; loading the corpus itself needs no
/// key). Used by integration checks to confirm `shared_retriever` boots.
#[cfg(feature = "server")]
#[server]
pub async fn retriever_smoke() -> Result<usize, ServerFnError> {
    let r = shared_retriever().map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(r.len())
}
