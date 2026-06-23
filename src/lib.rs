pub mod domain;
pub mod ingest;
pub mod retrieval;

// Bin-internal modules are declared in `main.rs`, not the library. The
// library surface is intentionally minimal: only the modules shared by the
// wasm client, the axum server, and the `build_corpus` xtask are re-exported
// here. `app`, `components`, and `server` (plan_trip) stay binary-internal so
// server-only deps they reference are never pulled into the wasm client.
