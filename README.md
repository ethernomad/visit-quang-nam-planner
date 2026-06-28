---
title: Visit Quang Nam AI Trip Planner
emoji: 🏝️
colorFrom: green
colorTo: blue
sdk: docker
pinned: false
---

# Visit Quang Nam AI Trip Planner

A personalised itinerary generator grounded in the content of
[visitquangnam.com](https://visitquangnam.com). Tell it your preferences
(duration, month, interests, pace, budget, green-travel preference) and it
retrieves relevant chunks from a RAG corpus built from the destination's
WordPress posts, then asks an LLM to produce a typed 1–14 day itinerary with
source links and a sustainability score.

<!-- TODO: replace with a real screenshot of the running app. The original SVG
     mockup lives outside the repo and can't be linked from a cloned checkout. -->
![App screenshot](docs/screenshot.png)

## What it does

- Takes travel preferences (duration, month, interests, travellers, pace,
  budget, green-travel) via a form.
- Retrieves grounding chunks from a RAG corpus built from
  `visitquangnam.com/wp-json/wp/v2/posts` and embeds the user's query for
  cosine nearest-neighbour search.
- Generates a 1–14 day itinerary with per-day activities, weather hints, a
  trip summary, a sustainability score, and source links back to the
  originating posts — rejecting any LLM-hallucinated URLs.

## Stack

Dioxus 0.7 fullstack — one Rust crate that compiles to a wasm client + an
axum server, sharing `domain`, `ingest`, and `retrieval` modules. Styling is
Tailwind v4. The full locked stack, server-only-dep gating pattern, and
command list live in [`AGENTS.md`](./AGENTS.md).

## Quick start

From a fresh clone:

```sh
# 1. Tools (one-time)
rustup target add wasm32-unknown-unknown
cargo binstall dioxus-cli --version 0.7.9
npm install

# 2. RAG corpus (one-time; needs OPENAI_API_KEY for embeddings)
OPENAI_API_KEY=sk-... cargo run --release --bin build_corpus

# 3. Dev server
npm run tailwind:watch &  # in one terminal
dx serve --web            # in another
```

The app is served at `http://127.0.0.1:8080`. The server needs two API
keys at runtime (see `.env.example`):
- `OPENCODE_API_KEY` — chat completions to Zen (`mimo-v2.5-free`) for
  itinerary generation.
- `OPENAI_API_KEY` — query-time embeddings (`text-embedding-3-small`).
  Each user query is embedded at request time for cosine search against
  the precomputed chunk vectors. Without either key, `/api/plan-trip`
  returns a 500.

Alternatively, run `scripts/bootstrap.sh` to run steps 1–2 interactively.

### Toolchain note (stable Rust)

This repo uses **stable Rust** everywhere — no `rust-toolchain.toml` pin and
no `rustup override`. Contributors should run whatever their rustup default
toolchain is (install it from https://rustup.rs if needed):

```sh
rustup target add wasm32-unknown-unknown
```

The Dockerfile tracks `rust:slim` (latest stable) for the same reason.

### Troubleshooting: wasm client build failure

**Symptom:** `dx serve`/`dx bundle` aborts while building the wasm client with:

```
failed to find the `__wbindgen_externref_table_dealloc` function
```

**Real cause:** a user-level `~/.cargo/config.toml` (or a repo-local
`.cargo/config.toml`) containing:

```toml
[build]
rustflags = ["-C", "target-cpu=native"]
```

`[build]` rustflags apply to *every* target, including
`wasm32-unknown-unknown`. `target-cpu=native` tells LLVM to emit the host
CPU's feature set, which is invalid for the wasm target; wasm-bindgen's
post-processing step then can't find the symbols it expects and aborts.

This was previously misdiagnosed as a rustc 1.96 / wasm-bindgen 0.2.125
incompatibility and worked around by pinning rustc 1.95. The pin and that
narrative have been removed — the rustc version was never the problem.

**Fix:** remove that rustflag, or scope it to native targets only:

```toml
[target.'cfg(not(target_family = "wasm"))']
rustflags = ["-C", "target-cpu=native"]
```

This repo ships a `.cargo/config.toml` that does this defensively, so a
stray user-level `target-cpu=native` won't break the wasm build here. If you
hit this in another project, apply the same scoped form.

## Docker

Build and run the containerised app (the image bakes in whatever
`data/corpus.json` you have locally — generate it first per step 2 above):

```sh
docker build -t visit-quang-nam-planner .
docker run --rm -p 8080:8080 \
  -e OPENAI_API_KEY=sk-... \
  -e OPENCODE_API_KEY=... \
  visit-quang-nam-planner
```

Or with compose (reads the keys from your shell environment):

```sh
export OPENAI_API_KEY=sk-...
export OPENCODE_API_KEY=...
docker compose up
```

Smoke test the running container:

```sh
curl -X POST http://127.0.0.1:8080/api/plan-trip \
  -H 'content-type: application/json' \
  -d '{"duration_days":3,"month":"March","interests":["Food","Beaches"],"travelers":{"adults":2,"kids":0},"pace":"Slow","budget_tier":"Mid","green_travel":true}'
```

Expect `200` with a structured `Itinerary` JSON body when both keys and a
valid `corpus.json` are present.

## Deploy

This project ships a Dockerfile only. Deploy by running the image above on any
container host (Fly.io, Render, Railway, a VPS, etc.). Ensure the container
exposes port `8080` (the dioxus axum server reads the `PORT` env var) and that
`OPENAI_API_KEY` and `OPENCODE_API_KEY` are provided as runtime secrets —
never bake them into the image.

## Project structure

- [`AGENTS.md`](./AGENTS.md) — the locked tech stack, server-only-dep gating,
  and the commands to run before considering a task complete.
- All six phases (Phase 0 — Scaffold through Phase 6 — Ship) are delivered.
- `src/` — `app.rs` (root component), `components/`, `domain/`, `ingest/`,
  `retrieval/`, `server/`, and the `build_corpus` xtask at `src/bin/`.
- `data/corpus.json` — the committed RAG corpus (user-generated via
  `build_corpus`).

## License

MIT (see [`Cargo.toml`](./Cargo.toml)).

## Credits

- [Visit Quang Nam](https://visitquangnam.com) — the source content this
  planner is grounded in, fetched via the WordPress REST API.
- Keen Agency — the destination-marketing case study that inspired this build.
