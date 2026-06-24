# Audit — Improvement Opportunities

Audited `2026-06-25` by reading every source file, plan doc, Dockerfile,
and test, then running all four gates (fmt, clippy, test, wasm check).

Gates pass — 49 unit + 5 integration tests green. Findings below are
ordered by estimated impact.

---

## Critical — breaks documented behaviour

### 1. `data/corpus.json` is missing

`AGENTS.md` (contract), `ARCHITECTURE.md` §1, `README.md`, the Dockerfile,
and `docker-compose.yml` all state the corpus is *"committed so the server
boots offline."* It is not present — only `data/README.md` exists.

- `shared_retriever()` 500s on first request.
- `docker run` of the published image fails unless the operator manually
  ran `build_corpus` first.
- `load_corpus_and_search` silently skips (see #2).

**Fix:** run `cargo run --release --bin build_corpus` and commit the
resulting `data/corpus.json`, OR stop claiming it's committed and add a
`build_corpus` step to the Dockerfile.

### 2. `tests/load_corpus.rs` is a false green

The test `return`s early when `data/corpus.json` is absent:

```rust
if !path.exists() {
    eprintln!("skipping");
    return;
}
```

Since the corpus is absent (#1), `cargo test --all` reports "1 passed"
even though the disk-load path is never exercised.

**Fix:** either `#[ignore = "needs data/corpus.json"]` with a clear
message, or add an `assert!(path.exists(), "corpus must be committed")`
so the failure is loud when the file is missing.

### 3. Runtime `tracing` is never initialised

`main.rs` is just `dioxus::launch(App)`. Only the `build_corpus` xtask
calls `tracing_subscriber::fmt().init()`. Result: every
`tracing::info!` / `warn!` / `error!` in the server code is silently
dropped at runtime — no boot log, no query-embed-failure log, no
duplicate-activity diagnostic.

Affected call sites:
- `server/mod.rs` — `retriever init failed`
- `retrieval/in_memory.rs` — `loaded InMemoryRetriever`, query embed error
- `server/plan_trip.rs` — any error mapped to `ServerFnError`
- `components/itinerary_view.rs` — duplicate activity warning

**Fix:** add a `tracing_subscriber` init (gated behind
`#[cfg(feature = "server")]`) in `main.rs` or a `fn init_logging()` in
`server/mod.rs`.

### 4. No server-side timeout on `LlmClient`

`app.rs` comments (lines 22–23) explicitly state the 60s wasm client cap
is *"the backstop, not the only guard"* and that *"the server (Phase 3
`LlmClient`) should also enforce a reqwest timeout."* It doesn't.

A hung Zen endpoint (`async-openai`'s reqwest client has no default
timeout) holds an axum worker indefinitely. The wasm client's 60s cap
surfaces the error to the user but does not free the server thread.

**Fix:** wrap the `client.chat().create(request)` call in
`tokio::time::timeout(Duration::from_secs(60), ...)`, or configure a
reqwest client builder timeout on the underlying HTTP client inside
`async-openai`.

### 4b. Same issue on `OpenAiEmbedder::embed_query`

`InMemoryRetriever.search()` calls `self.embedder.embed_query(query).await`;
if OpenAI is slow, the await has no bound. The in-memory impl logs and
returns empty on `Err`, but the call itself is unbounded.

**Fix:** add a timeout (e.g. 30s) around the embed call, same as #4.

---

## High — stale / inconsistent docs

### 5. `AGENTS.md` links to nonexistent `./plan.md`

Line 4: `[`plan.md`](./plan.md) as a standalone Dioxus 0.7 fullstack web app.`

`plan.md` does not exist in the repo root. The real plan index is
`plans/README.md` (`ARCHITECTURE.md` gets this right).

**Fix:** update the link to `./plans/README.md`.

### 6. `plans/phase-6-ship.md` contradicts shipped code

The plan doc (written pre-ship) still mentions:
- `OPENAI_MODEL=gpt-4o-mini` (shipped: `OPENCODE_MODEL=opencode/big-pickle`)
- `ROCKET_PORT=8080` (shipped: `PORT=8080`)
- `fly.toml` (does not exist in repo; README "Deploy" says "any container host")
- LLM env var section doesn't mention `OPENCODE_API_KEY`

The shipped `Dockerfile`, `docker-compose.yml`, and `.env.example` are
all correct — only the plan doc is stale.

**Fix:** update `plans/phase-6-ship.md` to reflect what actually shipped,
or add a note that it's historical and the live artifacts are authoritative.

---

## Medium — code robustness

### 7. Cached init errors are silent on subsequent requests

`shared_retriever()` and `shared_llm()` use `OnceLock<anyhow::Result<…>>`
(MVP choice). The first request logs the init failure; every subsequent
request returns the cached error but emits no log — an operator who
missed the boot log sees only a generic 500 on every `/api/plan-trip`.

**Fix:** add `tracing::error!("retriever init failed (cached from boot)")`
in the error path of `shared_retriever` and `shared_llm`.

### 8. Prompt text coupled to Rust `Debug` formatting

`build_retrieval_query` and `prompts::format_preferences` both use
`{:?}` (Debug impl) for enums — e.g. `format!("{p.month:?}")` produces
`"March"`. This works today but couples the prompt string to Rust's
`Debug` contract, which is a stability hazard.

**Fix:** add `pub fn as_str(&self) -> &'static str` to each enum
(`Month`, `Pace`, `BudgetTier`, `Interest`) so the prompt text is
explicit, documented, and decoupled from Rust derive.

### 9. `post_validate` has no guard against empty `source_url`

A malformed WordPress post (empty `link` field) would produce a `Chunk`
with `source_url: ""`. The LLM could then legitimately return `""` and
pass `post_validate` (since `""` is in the allowed set).

Per `plans/phase-4-ui.md`, the UI already suppresses "Read more" for
empty URLs, but the server should also reject them.

**Fix:** add a `post_validate` check that every activity URL is non-empty
and actually looks like a URL (starts with `"https://visitquangnam.com/"`).

### 10. Server spawns unbounded work per request

`plan_trip_inner` has no cancellation mechanism beyond the wasm client's
60s cap. If the user closes the browser tab, the axum worker continues
running the LLM call.

**Fix (deferred):** wire `tokio::spawn` with a graceful-shutdown
mechanism, or the timeout from #4. Not urgent for MVP.

---

## Low — polish

### 11. Dead code in `server/llm.rs`

Two items are `#[allow(dead_code)]`:
- `LlmClient::model()` (line 95) — never called.
- Free fn `pub async fn complete_itinerary(...)` (line 164) — a one-line
  delegation with no callers.

**Fix:** either wire `model()` into a per-request `tracing::span!` (useful
for A/B model comparison) or drop both.

### 12. No test for `LlmClient` parse-error path

`complete_json` has a branch that wraps the raw model output in the error:

```rust
serde_json::from_str::<T>(&content).map_err(|e| {
    anyhow::anyhow!(
        "failed to parse LLM JSON into {}: {e}\n--- raw model output ---\n{content}",
        ...
    )
})
```

This is the whole point of that error shape — it's untested. A unit test
with a canned malformed JSON response would lock the contract.

### 13. No CI workflow

The four gates (fmt, clippy, test, wasm-check) are documented as
*"blocking; treat failures as blocking"* but are not enforced on push or
PR. No `.github/workflows/` directory exists.

**Fix:** add a GitHub Actions workflow that runs the four gates on every
push and PR to `master`.

### 14. `assets/styles/` is an empty unused directory

`input.css` only writes to `assets/tailwind.css`. The `assets/styles/`
directory exists but contains nothing. Dead directory.

### 15. Dockerfile toolchain pin lacks upstream issue link

`Dockerfile` pins `rust:1.95-slim` due to a wasm-bindgen 0.2.125 / rustc
1.96 incompat. The comment describes the symptom but does not link to the
upstream wasm-bindgen issue or track it for removal.

**Fix:** add a link (e.g. `https://github.com/rustwasm/wasm-bindgen/issues/XXXX`)
so someone can periodically check whether the pin can be dropped.

---

## Summary by file

| File | Issues |
|------|--------|
| `data/` | #1 corpus missing |
| `tests/load_corpus.rs` | #2 false green |
| `src/main.rs` | #3 no tracing init |
| `src/server/llm.rs` | #4 no timeout, #11 dead code, #12 untested parse error |
| `src/server/mod.rs` | #7 silent cached error |
| `src/server/plan_trip.rs` | #8 Debug coupling, #9 empty URL guard |
| `src/retrieval/in_memory.rs` | #4b no embed timeout |
| `AGENTS.md` | #5 stale plan.md link |
| `plans/phase-6-ship.md` | #6 stale doc |
| `.github/workflows/` | #13 missing |
| `assets/styles/` | #14 dead dir |
| `Dockerfile` | #15 untracked pin |
