// Ingest pipeline. Phase 1: `html` (scrape rendered article pages from
// the fixed section indexes of visitquangnam.com — the site's WP REST
// API is down, returning 404 for `/wp-json/wp/v2/*`; see `html.rs`),
// `chunk` (paragraph chunker), `embedder` (OpenAI text-embedding-3-small).
// Run as an xtask via `cargo run --release --bin build_corpus`.
//
// `html` and `embedder` reference server-only deps (reqwest, scraper,
// async-openai) and are gated behind `#[cfg(feature = "server")]`.
// `chunk` has no server-only deps — it compiles to wasm so its unit
// tests run on the host target used by `cargo test --all`.

pub mod chunk;

#[cfg(feature = "server")]
pub mod embedder;

#[cfg(feature = "server")]
pub mod html;
