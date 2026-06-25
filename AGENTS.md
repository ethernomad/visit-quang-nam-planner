# Visit Quang Nam AI Trip Planner

This repo builds the Visit Quang Nam AI Personalized Trip Planner as a
standalone Dioxus 0.7 fullstack web app.

The assistant working in this repo MUST follow these rules.

> The system's architecture (module layout, request lifecycle, the
> retrieval/LLM seams, feature gating, persistence model) is documented
> in [`ARCHITECTURE.md`](./ARCHITECTURE.md). That file **MUST be kept
> up-to-date** with the code — any change to one of the concerns it
> covers requires a matching edit in the same PR. Treat `AGENTS.md` as
> the contract and `ARCHITECTURE.md` as the map.

## Tech stack (locked)

- **Framework:** Dioxus 0.7 fullstack — single crate, wasm client + axum
  server, `#[get]`/`#[post]`/`#[server]` server functions.
- **Target:** Web only (`dx serve --web`).
- **LLM:** OpenCode Zen's `opencode/big-pickle` via
  `async-openai` pointed at `https://opencode.ai/zen/v1/chat/completions`
  using `OPENCODE_API_KEY` + `OPENCODE_BASE_URL` (server-side only).
  (`gpt-4o-mini` was the original plan; Zen's `big-pickle` is free
  during its stealth period and OpenAI-chat-compatible.)
- **Embeddings:** OpenAI `text-embedding-3-small` (1536-dim), real
  OpenAI only — Zen has no `/embeddings` endpoint.
- **Retrieval:** In-memory cosine now, behind a `trait Retriever` so a
  future `PgVectorRetriever` is a drop-in swap.
- **Content source:** `visitquangnam.com` article pages, discovered by
  crawling a fixed list of section indexes (`/`, `/places/`,
  `/experiences/<sub>/`, `/events/`, `/practical-tips/`, `/green-travel/`)
  and scraping each linked article's rendered HTML. The site's WP REST
  API (`/wp-json/wp/v2/*`) went dark in mid-2026 (returns 404 / silent
  homepage fallback), so it ingests by scraping the Uncode theme
  markup (`<body class="postid-N">`, `<article class="category-NAME">`,
  `<h1>`, `<div class="post-content">`) instead of REST JSON.
- **Persistence:** Stateless MVP — no DB, no auth.
- **i18n:** English only for MVP.
- **Styling:** Tailwind v4 (`@import "tailwindcss"`), compiled with
  `npx @tailwindcss/cli` to `assets/tailwind.css`.

## Repo notes

- Dioxus 0.7 changes every API. Do NOT carry over `cx`/`Scope`/`use_state`.
  Use `use_signal`, `use_memo`, `use_resource`, `use_server_future`.
- Server functions use `#[get("/api/...")]` / `#[post("/api/...")]` macros
  (or the anonymous `#[server]` macro). They must be `async`, return
  `Result<T>` (anyhow via the prelude) or `Result<T, ServerFnError>`.
- Server-only deps (`async-openai`, `reqwest`, `tokio`, `scraper`,
  `thiserror`, `tiktoken-rs`) are optional in `Cargo.toml` and enabled
  only via the `server` feature, so they are excluded from the wasm
  client build.
- Any code touching server-only deps must be gated with
  `#[cfg(feature = "server")]` or live inside a `#[server]` function body.
- The crate is a lib + bin: `src/lib.rs` re-exports `domain`, `ingest`,
  `retriever` (shared by the wasm client, the axum server, and the
  `build_corpus` xtask); `src/main.rs` holds the bin-internal `app`,
  `components`, `server` modules and launches Dioxus. The `build_corpus`
  xtask lives at `src/bin/build_corpus.rs` and `use`s the library.
- Server-function orchestration (`plan_trip`) keeps LLM keys in env
  (`OPENCODE_API_KEY`, `OPENCODE_BASE_URL=https://opencode.ai/zen/v1`,
  model `opencode/big-pickle`) for chat, and the OpenAI
  embeddings key (`OPENAI_API_KEY`) is used only by the one-time
  `build_corpus` run. Neither key ever ships to wasm.
- The `Retriever` trait lives in `src/retrieval/mod.rs`. Any new backend
  (pgvector etc.) implements the same trait — do not branch on backend
  inside `plan_trip`.

## Commands

Run these before considering any task complete. Treat failures as
blocking.

```sh
# Format
cargo fmt --check

# Lint
cargo clippy --all-targets -- -D warnings

# Unit tests
cargo test --all

# Fullstack dev server (web client + axum server, hot-reload)
dx serve --web

# Production bundle
dx bundle --release --platform web

# Tailwind (run in a separate terminal during UI work)
npx @tailwindcss/cli -i ./input.css -o ./assets/tailwind.css --watch

# Rebuild the RAG corpus from visitquangnam.com HTML
cargo run --release --bin build_corpus
```

Notes:
- `cargo build` alone is the server target (default features =
  `["web","server"]`). DX splits the build internally; do not invoke
  `--no-default-features` for daily work.
- For wasm-specific checks: `cargo check --target wasm32-unknown-unknown --no-default-features --features web`.
- The `data/corpus.json` file is committed so the server boots offline;
  re-run `build_corpus` to refresh.
- No database, no migrations. A `PgVectorRetriever` swap does not change
  any test in `src/retrieval`.

## Project layout

```
src/
├── main.rs              # dioxus::launch(App)
├── app.rs               # root component, Tailwind shell, brand chrome
├── components/          # form, day tabs, timeline, summary, "More
│                        # ideas" footer (the SVG mockup's
│                        # "AI Recommended For You" sidebar was folded into
│                        # `trip_summary.rs` — no separate suggestions.rs)
├── domain/              # Chunk, Corpus; Itinerary, DayPlan...
├── ingest/              # HTML scraper + chunker + embedder
├── retrieval/           # Retriever trait + InMemoryRetriever
├── server/              # plan_trip server function, LLM client
└── bin/
    └── build_corpus.rs  # xtask: scrape HTML → chunks → embeddings → corpus.json
data/
└── corpus.json          # committed, prebuilt chunks + embeddings cache
assets/
└── tailwind.css         # generated from ../input.css (gitignored)
```
