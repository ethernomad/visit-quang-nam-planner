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
/// Audit #7: every request that hits the cached-error path now emits a
/// `tracing::error!` so an operator who missed the boot log still sees a
/// fresh diagnostic per failed request (the first call's init log is
/// emitted by `InMemoryRetriever::load` / `OpenAiEmbedder::from_env`).
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
        .map_err(|e| {
            tracing::error!(error = %e, "retriever init failed (cached from boot)");
            anyhow::anyhow!("retriever init failed: {e}")
        })?
        .clone())
}

/// Process-wide shared `LlmClient`. Same `OnceLock`-with-cached-error pattern
/// as `shared_retriever()`: if `from_env()` fails (e.g. `OPENCODE_API_KEY`
/// unset), the error is cached for the process lifetime and the operator
/// restarts the server after exporting the key. See `shared_retriever` for the
/// cached-error logging rationale (audit #7).
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
        .map_err(|e| {
            tracing::error!(error = %e, "LLM init failed (cached from boot)");
            anyhow::anyhow!("LLM init failed: {e}")
        })?
        .clone())
}

/// Default cap on simultaneous in-flight `plan_trip` LLM calls. Bounded so a
/// request flood doesn't spawn unbounded work against the Zen endpoint —
/// the N+1th request waits for a permit instead of piling onto axum workers
/// (audit #10). The per-call 60s timeout from `LlmClient` (audit #4) bounds
/// each held permit.
#[cfg(feature = "server")]
const DEFAULT_MAX_CONCURRENCY: usize = 4;

/// Process-wide semaphore capping concurrent `plan_trip` LLM calls (audit
/// #10). Permit count is configured once at first use via
/// `OPENCODE_MAX_CONCURRENCY` (default 4). `plan_trip` `acquire_owned()`s a
/// permit before calling `plan_trip_inner` and drops it on return so the
/// permit is released on both the success and error paths. This caps
/// in-flight LLM traffic; what it does **not** do is cancel an in-flight
/// call on client disconnect — Dioxus/axum drops the future on disconnect,
/// which propagates a cancellation up the `.await` chain, closing that gap.
#[cfg(feature = "server")]
pub fn shared_concurrency_limit() -> Arc<tokio::sync::Semaphore> {
    static SEM: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SEM.get_or_init(|| {
        let permits = std::env::var("OPENCODE_MAX_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_MAX_CONCURRENCY);
        tracing::info!(permits, "LLM concurrency cap");
        Arc::new(tokio::sync::Semaphore::new(permits))
    })
    .clone()
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

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    /// The shared semaphore honours `OPENCODE_MAX_CONCURRENCY` at first
    /// init; once initialised the `OnceLock` caches the permit count for the
    /// process. We don't set the env var here (it would race the OnceLock
    /// across tests), but we DO assert the default falls back to
    /// `DEFAULT_MAX_CONCURRENCY` (permits=4) when unset, and that 4 permits
    /// are acquirable with a 5th blocked — locking the contract the audit
    /// asked for.
    #[tokio::test]
    async fn shared_concurrency_limit_defaults_to_four_permits() {
        let sem = shared_concurrency_limit();
        // Drain the pool: 4 acquires must succeed instantly. `try_acquire_owned`
        // consumes the `Arc<Semaphore>` by value, so clone it per call.
        let p1 = Arc::clone(&sem).try_acquire_owned().expect("permit 1");
        let p2 = Arc::clone(&sem).try_acquire_owned().expect("permit 2");
        let p3 = Arc::clone(&sem).try_acquire_owned().expect("permit 3");
        let p4 = Arc::clone(&sem).try_acquire_owned().expect("permit 4");
        // A 5th concurrent caller would have to wait — `try_acquire` returns
        // `Err(TryAcquireError::NoPermits)` rather than blocking.
        assert!(
            Arc::clone(&sem).try_acquire_owned().is_err(),
            "semaphore should be exhausted after DEFAULT_MAX_CONCURRENCY acquires"
        );
        // Releasing one permit re-opens exactly one slot.
        drop(p1);
        assert!(
            Arc::clone(&sem).try_acquire_owned().is_ok(),
            "releasing a permit frees a slot"
        );
        // Hold the rest until end of test so a later test isn't affected by
        // the cached OnceLock state (the semaphore is process-wide).
        drop(p2);
        drop(p3);
        drop(p4);
    }
}
