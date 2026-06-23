# Phase Plan Index

The Visit Quang Nam AI Trip Planner is delivered in 7 phases. **Phase 0 is
already complete** (scaffold). To implement a phase, CD into the project
root and ask opencode:

```
opencode
# then in the TUI:
Implement plans/phase-1-ingest.md
```

Each plan file is self-contained: it states the goal, files to create/edit,
concrete code sketches, the verification gates, and any dependencies on
prior phases.

| Phase | File | Status | Goal |
|-------|------|--------|------|
| 0 — Scaffold | — | ✅ done | Repo, Cargo.toml, Tailwind, AGENTS.md, hello world |
| 1 — Ingest + corpus | [`phase-1-ingest.md`](./phase-1-ingest.md) | ✅ done | `build_corpus.rs` pulls WP REST, chunks, embeds, writes `corpus.json` |
| 2 — Retrieval | [`phase-2-retrieval.md`](./phase-2-retrieval.md) | ✅ done | `Retriever` trait + `InMemoryRetriever` with cosine search tests |
| 3 — LLM orchestration | [`phase-3-llm.md`](./phase-3-llm.md) | ✅ done | `plan_trip` server fn returns typed `Itinerary` (JSON-schema validated) |
| 4 — UI | [`phase-4-ui.md`](./phase-4-ui.md) | ✅ done | Form, day tabs, timeline, summary, suggestions matching the SVG mockup |
| 5 — Polish | [`phase-5-polish.md`](./phase-5-polish.md) | ✅ done | Loading states, error surfacing, sustainability score, EN strings |
| 6 — Ship | [`phase-6-ship.md`](./phase-6-ship.md) | ✅ done | Dockerfile, README, demo link |

## Cross-phase rules (apply to all plans)

- Read [`../AGENTS.md`](../AGENTS.md) **first** — it captures the locked
  tech stack, server-only-dep gating pattern, and the commands you must
  run before considering a task done.
- Run all four gates before declaring a phase complete:
  ```sh
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo test --all
  cargo check --target wasm32-unknown-unknown --no-default-features --features web
  ```
- Dioxus 0.7 only. Do NOT use `cx`, `Scope`, or `use_state`. Use
  `use_signal`, `use_memo`, `use_resource`, `use_server_future`.
- Server functions are `#[get("/api/...")]` / `#[post("/api/...")]` /
  `#[server]`. They must be `async` and return `Result<T>` (anyhow via
  the prelude) or `Result<T, ServerFnError>`.
- Server-only deps (`async-openai`, `reqwest`, `tokio`, `scraper`,
  `thiserror`) are already optional in `Cargo.toml` and enabled via the
  `server` feature. Any new server-only dep must follow the same pattern.
- Never commit secrets. The OpenAI key lives in `OPENAI_API_KEY` env var.
- Do not update this index's status table — the user does that by
  checking off phases as they ship.

## Phase ordering

Phases 1 and 2 are independent and may be implemented in parallel, but
Phase 3 depends on both. Phase 4 depends on Phase 3 (the server fn
signature must exist before the UI can call it). Phases 5 and 6 depend
on Phase 4.

```
  Phase 1 ─┐
           ├─▶ Phase 3 ─▶ Phase 4 ─┬─▶ Phase 5
  Phase 2 ─┘                        └─▶ Phase 6
```