# Phase 1 — Ingest + corpus

**Goal:** Build a one-shot binary `build_corpus` that pulls every post
and page from `visitquangnam.com`'s WordPress REST API, chunks each into
~300-token slices, embeds those slices with OpenAI
`text-embedding-3-small`, and writes the result to `data/corpus.json`.
Run it once now so the corpus is committed; Phase 2 loads this file at
server startup.

**Status:** pending
**Depends on:** Phase 0 (scaffold — done)

## Files to create / edit

- `Cargo.toml` — add a `[[bin]]` target named `build_corpus`. Add
  `tiktoken-rs` (or `tokenizers`) as an **optional** dep behind the
  `server` feature for chunk sizing. Keep `async-openai`, `reqwest`,
  `tokio`, `scraper`, `serde`, `serde_json`, `thiserror`, `anyhow` —
  they're already pulled in by Phase 0.
- `src/domain/mod.rs` — add the `Chunk` struct (see sketch).
- `src/ingest/mod.rs` — re-export `wordpress`, `chunk`, `embedder`.
- `src/ingest/wordpress.rs` — fetch + parse WP REST.
- `src/ingest/chunk.rs` — paragraph chunker.
- `src/ingest/embedder.rs` — OpenAI embeddings client.
- `src/bin/build_corpus.rs` — the xtask entry point.
- `data/corpus.json` — the committed output (do NOT gitignore it).

## Crate additions

```toml
# Cargo.toml (additions)
[[bin]]
name = "build_corpus"
path = "src/bin/build_corpus.rs"

# add to [dependencies]
tiktoken-rs = { version = "0.6", optional = true }   # token-count-aware chunking
anyhow = "1"

# add tiktoken-rs to the server feature:
[features]
server = [
    "dioxus/server",
    "dep:async-openai",
    "dep:reqwest",
    "dep:tokio",
    "dep:scraper",
    "dep:thiserror",
    "dep:tiktoken-rs",
]
```

`anyhow` is fine as a non-optional dep — it compiles to wasm (it's just
a thin error wrapper).

## Domain types

`src/domain/mod.rs` — append below the existing placeholder doc:

```rust
use serde::{Deserialize, Serialize};

/// One slice of a Visit Quang Nam article, ready for embedding and
/// retrieval. Stored verbatim in data/corpus.json.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    /// Stable id: "{post_id}-{chunk_index}".
    pub id: String,
    /// WordPress post id.
    pub post_id: u64,
    /// Source URL on visitquangnam.com (the post's `link` field).
    pub source_url: String,
    /// Article title (rendered, HTML stripped).
    pub title: String,
    /// Best-fit category if the post has one (e.g. "Food", "Culture",
    /// "Nature", "Beaches", "Wellness", "Green travel", "Practical tips",
    /// "Places", "Events"). `None` if uncategorised.
    pub category: Option<String>,
    /// ~300-token cleaned text slice used for the embedding.
    pub text: String,
    /// 1536-dim embedding from text-embedding-3-small.
    pub embedding: Vec<f32>,
}

#[derive(Serialize, Deserialize)]
pub struct Corpus {
    pub model: String,            // "text-embedding-3-small"
    pub generated_at: String,     // ISO 8601
    pub chunks: Vec<Chunk>,
}
```

## Tasks

### 1. WordPress fetch — `src/ingest/wordpress.rs`

- `pub async fn fetch_all(client: &reqwest::Client) -> anyhow::Result<Vec<RawPost>>`
- Hit `https://visitquangnam.com/wp-json/wp/v2/posts?per_page=100&_embed&page={n}`
  in a loop, paginating until the response has fewer items than
  `per_page` (or `X-WP-TotalPages` is exhausted). Repeat for
  `/wp-json/wp/v2/pages` so Practical Tips, Green Travel, FAQ pages are
  captured.
- Deserialise each item into a `RawPost`:
  ```rust
  #[derive(Deserialize)]
  pub struct RawPost {
      pub id: u64,
      pub link: String,
      pub title: Rendered,
      pub content: Rendered,
      pub excerpt: Rendered,
      #[serde(default)]
      pub categories: Vec<u64>,            // term ids
  }
  #[derive(Deserialize)]
  pub struct Rendered {
      #[serde(rename = "rendered")] pub rendered: String,
  }
  ```
- Resolve category IDs to names by also fetching
  `/wp-json/wp/v2/categories` once and building a `HashMap<u64, String>`.
  Skip category ids 1 (Uncategorised) → return as `None`.
- Use `_embed` so the response includes the `_embedded` author and
  term objects — but for the MVP you only need categories; if parsing
  `_embedded` is fiddly, fall back to the separate categories fetch
  above.
- HTML strip: run `content.rendered` through `scraper`'s `Html::parse`
  and concatenate text nodes (skip `<script>`/`<style>`). Same for
  titles. Trim and collapse whitespace.

### 2. Chunker — `src/ingest/chunk.rs`

- `pub fn chunk(text: &str, title: &str) -> Vec<(usize, String)>`
  returning `(chunk_index, text)` pairs.
- Strategy:
  1. Split on blank lines into paragraphs.
  2. Walk paragraphs, accumulating into the current chunk.
  3. After each paragraph, check token count with
     `tiktoken_rs::cl100k_base()?`. If ≥ 256 tokens, close the chunk
     (cap hard at 300 to stay safely under model limits for context
     packing in Phase 3). Start a new chunk.
  4. If a single paragraph exceeds 300 tokens, split it on sentence
     boundaries (`.` `。` `!` `?`) until each piece fits.
- Prepend `# {title}\n\n` to the first chunk of each post so the
  embedding carries the title context.
- Empty input → return `vec![]`. Never emit empty chunks.

### 3. Embedder — `src/ingest/embedder.rs`

- Read `OPENAI_API_KEY` from env at startup. If missing, exit with a
  clear error: `error: OPENAI_API_KEY not set — cannot embed corpus`.
- `pub async fn embed(texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>`
- Use `async_openai::types::CreateEmbeddingRequestArgs`:
  ```rust
  let request = CreateEmbeddingRequestArgs::default()
      .model("text-embedding-3-small")
      .input(texts.to_vec())
      .build()?;
  ```
- Batch in groups of **256 inputs per call** (OpenAI's limit for this
  model). Await all batches concurrently with
  `futures::future::join_all` (add `futures = "0.3"` if not already
  present).
- Return the embeddings in the same order as the inputs.
- Cache resilience: if a batch fails, retry once after 5 s, then surface
  the error to the user — don't silently skip chunks (a missing
  embedding silently breaks retrieval).

### 4. xtask entry — `src/bin/build_corpus.rs`

```rust
use std::env;
use visit_quang_nam_planner::ingest;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    if env::var("OPENAI_API_KEY").is_err() {
        anyhow::bail!("OPENAI_API_KEY not set — cannot embed corpus");
    }

    let client = reqwest::Client::new();
    let posts = ingest::wordpress::fetch_all(&client).await?;
    tracing::info!(count = posts.len(), "fetched posts+pages");

    let mut chunks = Vec::new();
    for post in posts {
        let paragraphs = ingest::chunk::chunk(&post.text, &post.title);
        for (idx, text) in paragraphs {
            chunks.push(Chunk {
                id: format!("{}-{}", post.id, idx),
                post_id: post.id,
                source_url: post.link.clone(),
                title: post.title.clone(),
                category: post.category.clone(),
                text,
                // placeholder; filled in batch after the loop
                embedding: Vec::new(),
            });
        }
    }
    tracing::info!(chunks = chunks.len(), "chunked");

    let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
    let embeddings = ingest::embedder::embed(&texts).await?;
    for (chunk, emb) in chunks.iter_mut().zip(embeddings) {
        chunk.embedding = emb;
    }

    let corpus = Corpus {
        model: "text-embedding-3-small".to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        chunks,
    };
    std::fs::write("data/corpus.json", serde_json::to_string_pretty(&corpus)?)?;
    tracing::info!(path = "data/corpus.json", "wrote corpus");
    Ok(())
}
```

Note: `main.rs` currently pulls every module via `mod app; mod components;`
etc. — you'll need to either move the bin into a separate target that
shares the crate via `lib.rs`, OR make the bin file not require it.
Simplest: convert the crate to expose a small `lib.rs` that re-exports
the ingest/domain modules the bin needs, then have both `main.rs`
(client/server launcher) and `src/bin/build_corpus.rs` depend on it. See
the "Refactor to lib+bin" section below.

### 5. Refactor to lib + bin

- Create `src/lib.rs` containing:
  ```rust
  pub mod domain;
  pub mod ingest;
  pub mod retrieval;
  ```
- In `main.rs`, drop `mod domain; mod ingest; mod retrieval;` and replace
  with `use visit_quang_nam_planner::{domain, ingest, retrieval};`. Keep
  `mod app; mod components; mod server;` as binary-internal modules.
- In `src/bin/build_corpus.rs`, `use visit_quang_nam_planner::{domain::{Chunk, Corpus}, ingest};`
- Add `[lib]` to `Cargo.toml` (it's optional; cargo infers `lib.rs`
  automatically, but making it explicit avoids surprises).
- Verify the wasm client target still builds — the bin should not pull
  in any server-only deps at compile time. Server-only crates are only
  referenced inside `#[cfg(feature = "server")]` code and the bin target
  opts into the `server` feature implicitly via `default = ["web", "server"]`.

## Acceptance criteria

- [ ] `cargo run --release --bin build_corpus` succeeds with
      `OPENAI_API_KEY` set, prints a chunk count log line, and writes
      `data/corpus.json`.
- [ ] `data/corpus.json` exists and contains >100 chunks with non-empty
      `embedding` arrays of length 1536.
- [ ] If `OPENAI_API_KEY` is unset, the bin exits with a clear
      human-readable error (no panic).
- [ ] The wasm client target still compiles: `cargo check --target
      wasm32-unknown-unknown --no-default-features --features web`
      passes.
- [ ] If OpenAI is unreachable (kill it mid-run in a test), the bin
      retries once then surfaces the underlying error — no silent skip.
- [ ] All four CI gates pass (fmt, clippy `-D warnings`, test, wasm
      check). Add at least one unit test for `chunk.rs`: input with
      obvious paragraph boundaries, assert chunk count and that no chunk
      exceeds 300 tokens.

## Open decisions for the user

- **Run embedder during the demo:** the corpus `build_corpus` produces is
  committed, so Phase 2 onwards never needs the OpenAI key. The user
  will need to run `OPENAI_API_KEY=... cargo run --release --bin
  build_corpus` **once** after the phase is implemented, then commit
  `data/corpus.json`. Flag this clearly in the PR/commit message.
- **Pages vs posts:** if `wp/v2/pages` returns mostly empty navigation
  chrome, drop it and document the decision in `ingest/wordpress.rs`.

## Notes for the agent

- Don't add `reqwest`'s `rustls` feature unless the default native-tls
  build fails on your platform. Default features are fine.
- `async-openai` 0.27 changed `Client::new` to take `OpenAIConfig`. Read
  the version pinned in `Cargo.toml` (0.27) before reaching for
  examples on the web — older snippets will not compile.
- If `tiktoken-rs` fails to fetch its tokenizer file on first use (it
  downloads `cl100k_base.tiktoken` from openaipublic.blob.core.windows.net)
  fall back to a char-count heuristic (≈4 chars/token) so the bin runs
  offline. Note this clearly in a module doc comment.
- Do NOT gitignore `data/corpus.json`. The whole point of Phase 1 is to
  commit it so Phase 2 boots offline.