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

The app is served at `http://127.0.0.1:8080`. The planner LLM needs
`OPENCODE_API_KEY` in the environment (see `.env.example`); without it the
form still renders but `/api/plan-trip` returns a 500.

Alternatively, run `scripts/bootstrap.sh` to run steps 1–2 interactively.

### Toolchain note (rustc 1.96)

`dx serve`/`dx bundle` build the wasm client through `wasm-bindgen`, whose
0.2.125 release (the latest compatible with dioxus 0.7.9's bundled CLI) is
incompatible with rustc 1.96 — it aborts with
`failed to find the __wbindgen_externref_table_dealloc function`. If your
default toolchain is 1.96, pin a prior stable for this repo:

```sh
rustup toolchain install 1.95
rustup override set 1.95
```

The Dockerfile already pins `rust:1.95-slim` for this reason.

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
- [`plans/README.md`](./plans/README.md) — the phased delivery index (Phase 0
  through Phase 6).
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
