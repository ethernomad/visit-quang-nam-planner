mod app;
mod components;
mod copies;
mod server;
mod util;

fn main() {
    #[cfg(feature = "server")]
    {
        // Load ./.env if present. Process-supplied env vars win (dotenvy
        // never overrides an already-set var), so this is a no-op in prod
        // containers that inject keys via the orchestrator. Kept server
        // only — keys never ship to wasm.
        let _ = dotenvy::dotenv();
        // Runtime tracing is initialised once per process so server-side
        // `tracing` macros emit boot logs, query-embed errors, duplicate
        // activity warnings, etc. `try_init` is safe to call repeatedly
        // (defensive — `build_corpus` is a separate binary, but dx serve /
        // hot reload spawn the server once per process).
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
        // Eagerly warm the singletons so the first request doesn't pay
        // the corpus.json load + parse cost on its critical path. Failures
        // are cached in `OnceLock` per the existing contract — the first
        // real request will return the same error.
        let _ = server::shared_retriever();
        let _ = server::shared_llm();
    }
    dioxus::launch(app::App);
}
