# Architecture

> **This document is the canonical reference for the system's architecture.
> It MUST be kept up-to-date with the code.** Any change to the module
> structure, the request lifecycle, the server/client split, the retrieval
> backend seam, the LLM client seam, or the persistence model requires a
> matching edit here. Stale architecture docs are worse than no docs — if
> you read this and the code disagrees, fix one of them in the same PR.
>
> Cross-references: [`AGENTS.md`](./AGENTS.md) (locked tech stack, commands,
> and contributor rules). `AGENTS.md` is the contract; this file is the
> map.

## 1. Overview

The Visit Quang Nam AI Trip Planner is a **Dioxus 0.7 fullstack** web app:
a single Rust crate that compiles to both a wasm client and an axum
server from the same source tree. The user fills in a preferences form in
the browser; the wasm client posts to a Dioxus `#[post]` server function
(`/api/plan-trip`); the server runs a **retrieve → prompt → LLM →
validate** pipeline and returns a typed `Itinerary` that the client
renders as day tabs + a timeline + a summary card.

Grounding comes from a RAG corpus built offline by **scraping rendered
article HTML** from `visitquangnam.com`'s fixed section indexes (the
site's WordPress REST API `/wp-json/wp/v2/*` went dark in mid-2026).
The corpus (`Chunk`s with precomputed 1536-dim embeddings) is
committed to `data/corpus.json` so the server boots offline; query-time
embeddings (one per request) still go to real OpenAI
`text-embedding-3-small`. Chat completions go to OpenCode Zen's
`mimo-v2.5-free` (an OpenAI-chat-compatible endpoint) during Zen's
free stealth period.

There is **no database, no auth, no per-user state**. Statelessness is an
explicit MVP choice (see [`AGENTS.md`](./AGENTS.md) "Persistence").

## 2. High-level diagram

```
┌─────────────────────────────── browser ──────────────────────────────┐
│  wasm client  (feature = "web")                                       │
│   app.rs ─ App root component, use_resource(plan_trip) state machine   │
│   components/ ─ PlannerForm, ItineraryView, DayCard, TripSummary,      │
│                ErrorBox, PreferenceChip, ActivityRow                   │
│       │                                                               │
│       │  plan_trip(prefs) ── auto-generated typed client stub         │
└───────┼───────────────────────────────────────────────────────────────┘
        │  HTTP POST /api/plan-trip  (Preferences JSON)
┌───────▼────────────────────────────── axum server (feature = "server")─┐
│  server/plan_trip.rs ─ #[post("/api/plan-trip")]                       │
│   validate_prefs(prefs)                                                │
│   build_retrieval_query(prefs)                                         │
│   shared_retriever().search(query, TOP_K=5) ── chunks ──┐              │
│   prompts::build_user_prompt(prefs, &chunks)             │              │
│   shared_llm().complete_itinerary(system, user)          │              │
│   post_validate(itin, prefs, &chunks) ◀── enforces URLs ┘              │
│   → Itinerary (typed, serde)                                           │
└────────────────────────────────────────────────────────────────────────┘
        │
        │  cosine search                       chat completion (json_object)
        ▼                                       ▼
┌──────────────────────────┐        ┌────────────────────────────────────┐
│ InMemoryRetriever       │        │ LlmClient (async-openai)           │
│  holds Vec<Chunk> +     │        │  OPENCODE_API_KEY                   │
│  Arc<dyn Embed>         │        │  OPENCODE_BASE_URL (Zen v1)         │
│  loads data/corpus.json │        │  model = mimo-v2.5-free        │
│  cosine top-K           │        └────────────────────────────────────┘
└──────────────────────────┘
        │
        │  embed_query(query) → Vec<f32>  (1536-dim)
        ▼
┌────────────────────────────────────┐
│ OpenAiEmbedder                     │
│  OPENAI_API_KEY                    │
│  text-embedding-3-small            │
└────────────────────────────────────┘
```

The corpus itself is built offline by a separate xtask:

```
cargo run --release --bin build_corpus      # one-time, server feature
  visitquangnam.com article pages (scraped from the fixed
    section indexes — the site's `/wp-json/wp/v2/*` REST API went
    dark in mid-2026: returns 404 / silent homepage fallback)
    → ingest::html::fetch_all (post id + category from body/article
                                class tokens; body from .post-content)
    → ingest::chunk::chunk    (~300-token slices, "# {title}" prefix)
    → ingest::embedder::embed (OpenAI text-embedding-3-small, batch 256)
    → data/corpus.json        (committed; server boots offline)
```

## 3. Crate layout and the lib/bin split

```
visit-quang-nam-planner  (single crate, lib + bin)
├── src/lib.rs              re-exports domain, ingest, retrieval
│                           (shared by wasm client, axum server,
│                            and the build_corpus xtask — minimal
│                            surface, no server-only deps reachable
│                            from here)
├── src/main.rs             bin entry: `dioxus::launch(App)`
│                           declares bin-internal modules only:
│                           app, components, copies, server, util
├── src/app.rs              root component + use_resource state machine
├── src/components/         UI (Phase 4/5)
├── src/server/             plan_trip, llm, prompts, shared_{retriever,llm}
├── src/bin/build_corpus.rs xtask (uses lib surface only)
├── src/domain/             Chunk, Corpus, Itinerary, DayPlan, Activity,
│                           Preferences, TripSummary, enums — serde types
│                           that cross the client/server boundary
├── src/ingest/             html, chunk, embedder (Phase 1)
├── src/retrieval/          Retriever + Embed traits, InMemoryRetriever
├── data/corpus.json        committed RAG corpus (git-tracked)
└── assets/tailwind.css     generated from ../input.css (gitignored)
```

The lib/bin split is load-bearing: anything `bin/build_corpus.rs` needs
goes through `src/lib.rs`, so circular imports between `server` and
`ingest` are impossible. `app`, `components`, `server` are
**bin-internal** — the library never re-exports them, which keeps the
wasm client from pulling server-only transitive deps.

## 4. Feature gating and the wasm/client boundary

`Cargo.toml` defines two features:

| feature   | enables                                             |
|-----------|-----------------------------------------------------|
| `web`     | `dioxus/web` — wasm client target                   |
| `server`  | `dioxus/server` + every server-only optional dep    |
| `default` | `["web", "server"]` — type-check only; **not a runnable target** (see §10) |

Server-only deps (`async-openai`, `reqwest`, `tokio`, `scraper`,
`thiserror`, `tiktoken-rs`, `async-trait`) are declared **optional** and
enabled only via the `server` feature. Any code touching them must be
either inside a `#[server]` function body or gated with
`#[cfg(feature = "server")]`. The wasm client is the stricter target and
is checked explicitly:

```sh
cargo check --target wasm32-unknown-unknown --no-default-features --features web
```

Key consequence: the `Retriever` and `Embed` traits, `InMemoryRetriever`,
`LlmCompleter`, `LlmClient`, the `shared_*` singletons, and the
`prompts` module all live behind `#[cfg(feature = "server")]`. The
`plan_trip` symbol itself is exported unconditionally because the Dioxus
`#[post]` macro generates a wasm-side `client_query` stub for it; its
*body* is gated so the helpers it calls never link into the client.

## 5. Module responsibilities

### `src/domain/` (shared, no server deps)

Serde types crossing the wire in either direction, plus the on-disk
corpus shape. Everything here derives `Serialize + Deserialize` and has
**zero** server-only dependencies, so it compiles cleanly to wasm.

- `Chunk`, `Corpus` — Phase 1 ingest output and the retriever's backing
  store (committed as `data/corpus.json`).
- `Preferences` — client → server request body (clamped to
  `duration_days ∈ 1..=14` server-side).
- `Itinerary`, `DayPlan`, `Activity`, `TripSummary`, `WeatherHint`,
  plus the `Interest` / `Pace` / `BudgetTier` / `Month` / `Category`
  enums — server → client response shape. `TripSummary` carries a
  `sustainability_score` (0..=100) and an additive
  `sustainability_breakdown: Vec<(String, u8)>` (Phase 5).

### `src/ingest/` (server-only, except `chunk`)

Phase 1 corpus builder:

- `html` — rendered-HTML scraper against `visitquangnam.com` article
  pages. The site's WordPress REST API (`/wp-json/wp/v2/{posts,pages}`)
  went dark in mid-2026 (every endpoint 404s or 301s into a 404, and
  `?rest_route=…` silently falls back to homepage HTML), so discovery
  crawls a fixed list of section indexes (`/`, `/places/`,
  `/experiences/<sub>/`, `/events/`, `/practical-tips/`,
  `/green-travel/`) and fetches each linked article. The Uncode theme
  stamps stable hooks onto the markup — `<body class="postid-N">` for
  the id, `<article class="category-NAME">` for the category, `<h1>`
  for the title, `<div class="post-content">` for the body — and the
  scraper recovers those without any JSON. Concurrency is bounded via
  `futures::stream::buffer_unordered(8)` for politeness. Output URLs are
  canonicalised to the **bare** `https://visitquangnam.com/` form (the
  site's own `<link rel="canonical">`) so they match
  `plan_trip`'s `ALLOWED_URL_PREFIX` and the prompt template.
- `chunk` — paragraph chunker, ~300-token slices, first chunk prefixed
  with `# {title}\n\n` so the embedding carries title context. **No
  server-only deps** → runs under `cargo test --all` on any target.
- `embedder` — OpenAI `text-embedding-3-small` batch embedder
  (`embed(texts) -> Vec<Vec<f32>>`) plus the runtime `OpenAiEmbedder`
  used by `InMemoryRetriever` for query embedding.

### `src/retrieval/` (server-only trait + impl)

- `Retriever` trait — `search(&self, query: &str, k: usize) -> Vec<Chunk>`
  and `len()`. The orchestrator (`plan_trip_inner`) **only ever** calls
  this trait — never a concrete struct — so a future
  `PgVectorRetriever` (`SELECT ... ORDER BY embedding <=> $1 LIMIT k`) is
  a drop-in swap inside `shared_retriever()` with no change to the
  orchestrator. Do not branch on backend inside `plan_trip`.
- `Embed` trait — query-time embedding seam so tests can inject a
  `MockEmbedder` returning a canned vector.
- `InMemoryRetriever` — reference backend: loads `data/corpus.json` once
  per process, embeds each query with the injected `Embed`, scores by
  cosine, returns top-K.

### `src/server/` (bin-internal, server-only)

- `mod` — `shared_retriever()`, `shared_llm()`, and
  `shared_concurrency_limit()` — process singletons.
  - `shared_retriever()` / `shared_llm()` use
    `OnceLock<anyhow::Result<Arc<dyn …>>>`: the first call initialises;
    if init fails the **error is cached for the process lifetime** and
    subsequent calls return the same error without retrying (MVP choice
    — operator restarts the server). **Every** subsequent failing call
    re-emits a `tracing::error!("… init failed (cached from boot)")` so
    an operator who missed the boot log still gets a per-request
    diagnostic (audit #7).
  - `shared_concurrency_limit()` returns a process-wide
    `tokio::sync::Semaphore` capping concurrent `plan_trip` LLM calls
    (default 4, env `OPENCODE_MAX_CONCURRENCY`). `plan_trip` acquires a
    permit before driving `plan_trip_inner` and drops it on return so
    the permit is released on both the success and error paths. This
    bounds in-flight LLM traffic. Client-disconnect cancellation is
    handled by axum dropping the future (propagates `Cancelled` up the
    `.await` chain); the per-call `LlmClient` 60s timeout
    (`OPENCODE_TIMEOUT_SECS`) bounds how long one permit can be held
    (audit #10).
  - `CORPUS_PATH` configures the corpus file (default
    `data/corpus.json`). `retriever_smoke` is a `#[server]` smoke
    endpoint returning the chunk count.
- `plan_trip` — the `#[post("/api/plan-trip")]` server function. Body is
  gated; the orchestration core `plan_trip_inner(prefs, retriever, llm)`
  is callable directly with mocks (used by `tests/plan_trip.rs`, no
  network). Pipeline:
  1. `validate_prefs` (duration 1..=14, ≥1 interest, ≥1 adult).
  2. `build_retrieval_query` (hand-rolled natural-language sentence —
     works better for cosine against article-text embeddings than a YAML
     blob).
  3. `retriever.search(query, TOP_K=5)` (≈1.5K tokens of grounding).
  4. `prompts::SYSTEM_PROMPT` (template) + `prompts::build_user_prompt`
     (chunks inlined).
  5. `llm.complete_itinerary(system, user)` — Zen returns
     `response_format: json_object`; we do **not** use `json_schema`
     (OpenAI-only, not guaranteed on Zen) and instead rely on the next
     step.
  6. `post_validate` — rejects day-count mismatches, hallucinated
     `source_url`s (every activity URL must appear in the retrieved
     chunk set), and `sustainability_score > 100`. This is the
     authoritative contract; parse-time is the secondary one. URL
     guards (audit #9): activity URLs must be non-empty AND start with
     `https://visitquangnam.com/`; the allowed-set built from chunks is
     itself filtered to non-empty on-domain URLs, so a malformed corpus
     chunk (`source_url: ""`) can never seed the allowed set.
- `llm` — `LlmCompleter` trait (non-generic, dyn-compatible) +
  `LlmClient` built on `async-openai` pointed at `OPENCODE_BASE_URL`.
  Per-call 60s `tokio::time::timeout` (env `OPENCODE_TIMEOUT_SECS`,
  audit #4) bounds the chat completion so a hung Zen endpoint can't
  hold an axum worker indefinitely.
- `prompts` — system + user prompt assembly. Enum values are rendered
  via each enum's `as_str()` (audit #8), not Rust's `Debug` impl, so
  the prompt text is documented and decoupled from `#[derive(Debug)]`.
  Notably `Interest::GreenTravel.as_str() == "Green travel"` matches
  the WP category string the model already sees in the chunks block.
- `llm` — `LlmCompleter` trait (non-generic, dyn-compatible) +
  `LlmClient` built on `async-openai` pointed at `OPENCODE_BASE_URL`.
- `prompts` — system + user prompt assembly.

### `src/app.rs` + `src/components/` (client-facing, mostly wasm)

- `app.rs` — `App` root component. Parent-owned `Signal<Preferences>`
  feeds the form; `use_resource(move || { … plan_trip(prefs).await })`
  drives a four-state machine (not-submitted / pending / error /
  success) keyed off `submitted` + a `submit_nonce` (so re-submitting
  identical prefs still re-runs the resource — Dioxus 0.7 otherwise
  caches identical closures). Phase 5 adds a request-scoped 20s loading
  countdown, an 8s "taking longer" hint, and a 60s client-side hard
  cap (backstop to the server's own reqwest timeout), all via
  `gloo-timers::future::TimeoutFuture` (wasm-safe; `tokio::time` is
  server-only). The render path keys off the reactive
  `Resource::state().cloned()` value, not `Resource::pending()`, because
  `pending()` is a non-subscribing peek helper and won't itself trigger a
  re-render when the request flips to `Ready`.
- `components/planner_form` — the preferences form, mutates the
  `Signal<Preferences>` and bumps `submit_nonce` on submit.
- `components/itinerary_view` + `day_card` + `activity_row` +
  `trip_summary` — render the typed `Itinerary`. `trip_summary` also
  hosts the "More ideas" footer row that **replaced** the SVG mockup's
  separate "AI Recommended For You" sidebar (no `suggestions.rs`).
- `components/error_box` — Phase 5 error surface with "Try again"
  (restarts the resource).
- `components/preference_chip` — header chip + form toggle.

### `src/bin/build_corpus.rs` (xtask, server feature)

Standalone bin: `OPENAI_API_KEY=… cargo run --release --bin build_corpus`.
Scrapes every article page discoverable from the fixed section indexes
of `visitquangnam.com` (via `ingest::html::fetch_all`), chunks, batch-
embeds (256/batch), writes `data/corpus.json` with `model`,
`generated_at`, and `chunks` fields. Sanity-checks every chunk's
embedding is 1536-dim. The reqwest client pins
`redirect::Policy::limited(10)` so the bare→www and http→https 301s
the upstream issues don't silently fail. Re-run to refresh; commit the
result so the server boots offline.

## 6. Request lifecycle (`POST /api/plan-trip`)

1. **Client** — form mutates `Signal<Preferences>`; on submit,
   `submit_nonce` bumps, `submitted` flips true, the `use_resource`
   re-runs and calls `plan_trip(prefs).await` through the Dioxus-generated
   wasm client stub.
2. **Transport** — the stub serialises `Preferences` (serde) and POSTs to
   `/api/plan-trip` on the same origin.
3. **Server function** — Dioxus' axum layer dispatches to `plan_trip`,
   which calls `shared_retriever()` + `shared_llm()` (initialised eagerly
   at boot in `main()` — not lazily; errors cached per the OnceLock
   contract — every subsequent failing request re-logs the cached
   init error, audit #7). Before driving `plan_trip_inner`, it acquires
   a permit from `shared_concurrency_limit()` (process-wide
   `tokio::sync::Semaphore`, default 4, env `OPENCODE_MAX_CONCURRENCY`,
   audit #10) so the N+1th concurrent request waits instead of piling
   onto axum workers. The permit is dropped on return (success or
   error). If the wasm client disconnects, axum drops the future,
   cancelling the in-flight `.await` chain.
4. **`plan_trip_inner`** — `validate_prefs` → `build_retrieval_query` →
   `retriever.search(query, 5)` (which calls `OpenAiEmbedder` once to
   embed the query, then cosine top-K) → `prompts::build_user_prompt` →
   `llm.complete_itinerary` (Zen chat completion, JSON mode) →
   `serde_json::from_str::<Itinerary>` → `post_validate` → `Ok(Itinerary)`.
5. **Response** — `Result<Itinerary, ServerFnError>` serialised back to
   the client. `post_validate` failures surface as `ServerFnError` → the
   client's `ErrorBox` renders the message with a "Try again" button
   (which calls `itinerary.restart()`, re-running the resource).

## 7. The two seams (and why they exist)

Both seams exist to keep hot paths testable without network and to make
the production backend swappable without touching the orchestrator.

### Retrieval seam — `Retriever` + `Embed`

`plan_trip_inner` takes `&dyn Retriever`. The production impl is
`InMemoryRetriever` loaded from `data/corpus.json`; tests inject a
`MockRetriever` that returns a canned chunk set. A future
`PgVectorRetriever` implements the same trait against
`SELECT … ORDER BY embedding <=> $1 LIMIT k` and is wired in by editing
**one line** of `shared_retriever()`. The orchestrator never branches on
backend.

`Embed` exists alongside it so the in-memory backend can embed queries
without forcing `ingest` to depend on `retrieval` (or vice-versa): tests
pass a `MockEmbedder`.

### LLM seam — `LlmCompleter`

`plan_trip_inner` takes `&dyn LlmCompleter`. The production impl is
`LlmClient` (async-openai → Zen → `mimo-v2.5-free`); tests inject a
`MockLlm` returning a canned `Itinerary` so the orchestration
(expressly: `post_validate`) is exercised without HTTP. The trait method
is non-generic (returns `Itinerary`, not `T`) so it stays
`dyn`-compatible and `shared_llm()` can return `Arc<dyn LlmCompleter>`.

## 8. Persistence and state

- **Stateless MVP, by design.** No DB, no auth, no per-user or
  per-session store. `data/corpus.json` is the only persisted data, and
  it is read-only at runtime (written only by the `build_corpus` xtask).
- **Process singletons** — `shared_retriever()` and `shared_llm()` are
  `OnceLock`-initialised, single-instance, error-caching. If init fails
  the operator restarts the server after exporting the missing key;
  there is no in-process retry. This trades resilience for simplicity
  and matches the AGENTS.md MVP contract.
- **Client state** — entirely in Dioxus signals owned by `App`:
  `Preferences`, `submitted`, `submit_nonce` (re-submit trigger),
   `active_day` (day-tab index), `countdown_tick` (20s pending-state
   loading countdown driver; the displayed seconds are derived from it),
   `show_slow_hint`, `timed_out`, plus the reactive `itinerary_state`
   read from `use_resource(...).state()`. Nothing is persisted
   client-side; a refresh starts over.

## 9. Configuration surface

All keys are environment variables (see [`.env.example`](./.env.example)
for the canonical list). None ever ship to wasm.

| var                  | required? | used by                  | purpose                                    |
|----------------------|-----------|--------------------------|--------------------------------------------|
| `OPENAI_API_KEY`     | runtime + xtask | `ingest/embedder`, `bin/build_corpus` | query-time + corpus-build embeddings (real OpenAI only — Zen has no `/embeddings`) |
| `OPENCODE_API_KEY`   | runtime   | `server/llm`             | chat completions (`mimo-v2.5-free`)   |
| `OPENCODE_BASE_URL`  | optional  | `server/llm`             | default `https://opencode.ai/zen/v1`        |
| `OPENCODE_MODEL`     | optional  | `server/llm`             | default `mimo-v2.5-free`               |
| `CORPUS_PATH`        | optional  | `server/mod`             | default `data/corpus.json`                  |
| `PORT`               | optional  | dioxus-cli               | axum listen port, default `8080`            |

## 10. Build, test, and run

Daily gates (run all before considering a task complete; failures are
blocking):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo check --target wasm32-unknown-unknown --no-default-features --features web
```

> **`cargo build` is not a runnable target.** With default features
> `["web","server"]`, `cargo build --release` compiles both the wasm
> client and the axum server against a native host; the resulting
> `target/release/visit-quang-nam-planner` links the wasm-client
> (`js-sys`) renderer into a native process and panics at
> `dioxus::launch` (`cannot access imported statics on non-wasm
> targets`). It is a type-check artifact only. Run the app via `dx
> serve --web` (dev) or `dx bundle --release --platform web` (prod;
> run `./dist/visit-quang-nam-planner` from `dist/`). For an API-only
> smoke test (no browser UI): `cargo build --release
> --no-default-features --features server`, then run
> `./target/release/visit-quang-nam-planner`. The repo targets stable
> Rust with no version pin and no `rustup` override. A stray
> `target-cpu=native` build rustflag breaks the wasm client build —
> see README.md "Troubleshooting: wasm client build failure" (the
> in-repo `.cargo/config.toml` neutralises it defensively).

Local dev (web client + axum server, hot-reload):

```sh
dx serve --web
# in a second terminal during UI work:
npx @tailwindcss/cli -i ./input.css -o ./assets/tailwind.css --watch
```

Rebuild the RAG corpus (Phase 1, Phase 6 cron):

```sh
OPENAI_API_KEY=sk-... cargo run --release --bin build_corpus
# then commit data/corpus.json
```

Production bundle and container:

```sh
dx bundle --release --platform web
cd dist && ./visit-quang-nam-planner   # run from dist/ so it finds public/
# or, containerised:
docker compose up --build      # uses ./Dockerfile
```

## 11. Tests

- **Unit tests** — co-located with the code they exercise
  (`#[cfg(test)] mod tests`). `domain` round-trips serde;
  `ingest::chunk` validates slice boundaries; `retrieval/in_memory`
  covers cosine ranking with a `MockEmbedder`;
  `server/plan_trip::tests` exercises `validate_prefs`,
  `post_validate`, and a full `plan_trip_inner` end-to-end with
  `MockLlm` + `MockRetriever` (no network).
- **Integration tests** — `tests/plan_trip.rs` (deeper orchestration
  cases against fixture corpora), `tests/load_corpus.rs` (boots
  `shared_retriever()` against `tests/fixtures/corpus.json`).
- **Fixtures** — `tests/fixtures/corpus.json` (small RAG corpus) and
  `tests/fixtures/llm_response.json` (canned Zen response) keep tests
  fully offline.

All tests run under `cargo test --all`; the `server` feature is on by
default. There is no separate integration-test harness.

## 12. Scaling and swap points (intentional seams)

| Concern               | MVP implementation                | Swap point                              |
|-----------------------|-----------------------------------|-----------------------------------------|
| Retrieval backend    | `InMemoryRetriever` (cosine RAM)  | `PgVectorRetriever` implements `Retriever`; one-line change in `shared_retriever()` |
| LLM provider         | Zen `mimo-v2.5-free`          | `LlmClient::from_env` reads `OPENCODE_BASE_URL`/`OPENCODE_MODEL`; repoint at real OpenAI without code change |
| Embeddings model     | `text-embedding-3-small` (1536-d) | constant in `build_corpus`; re-run xtask after changing it (the corpus's `model` field records which model produced each build) |
| Persistence          | none (stateless)                  | out of scope for MVP; see AGENTS.md     |
| i18n                 | English only                      | out of scope for MVP; see AGENTS.md     |

## 13. Cycle and dependency hygiene

- `domain` depends on nothing in `src/` (only `serde`, `chrono`).
- `ingest::chunk` has no server deps and is the only `ingest` module
  reachable from a `cargo test --all` run on the wasm target.
- `ingest::embedder` depends on `retrieval::Embed` (the trait), not the
  other way around — `Embed` lives in `retrieval` precisely to avoid an
  `ingest ↔ retrieval` cycle.
- `server` and `app`/`components` are **bin-internal** — the library
  never re-exports them, so neither pulls the other into the wasm
  client via the lib surface.
- `bin/build_corpus.rs` uses **only** the lib surface (`domain`,
  `ingest`), keeping the xtask decoupled from the live server.

## 14. Maintenance checklist

When changing the codebase, update this document if **any** of the
following changes:

- [ ] Module structure (new/removed/renamed file or directory under `src/`).
- [ ] The lib/bin module split or what `src/lib.rs` re-exports.
- [ ] Feature flags in `Cargo.toml` or which deps are gated.
- [ ] The `plan_trip` pipeline or the `post_validate` contract.
- [ ] Either trait seam (`Retriever`, `LlmCompleter`, `Embed`) — its
      methods, callers, or production impl.
- [ ] The set of environment variables the runtime consults.
- [ ] The persistence model (today: none; if a DB lands, this whole
      section needs rewriting).
- [ ] The client state machine in `app.rs` (signals, timeout constants,
      error/timeout behaviour).

If in doubt: a reader who has only read `AGENTS.md` and this file should
be able to navigate the codebase cold. If they can't, this file is
wrong.
