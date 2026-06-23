// Ingest pipeline. Phase 1: `wordpress` (fetch /wp-json/wp/v2/{posts,pages}),
// `chunk` (paragraph chunker), `embedder` (OpenAI text-embedding-3-small).
// Run as an xtask via `cargo run --release --bin build_corpus`.
//
// `wordpress` and `embedder` reference server-only deps (reqwest,
// async-openai) and are gated behind `#[cfg(feature = "server")]`. `chunk`
// has no server-only deps — it compiles to wasm so its unit tests run on
// the host target used by `cargo test --all`.

pub mod chunk;

#[cfg(feature = "server")]
pub mod embedder;

#[cfg(feature = "server")]
pub mod wordpress;
