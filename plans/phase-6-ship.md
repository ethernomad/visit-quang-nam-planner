# Phase 6 — Ship

**Goal:** Containerise the fullstack app, write a README that lets a
stranger clone and run it, and produce a deployable artifact for a
demo. No new product features.

**Status:** pending
**Depends on:** Phase 4 (and, ideally, Phase 5 so the demo looks good).
Phase 3 must be done because the demo needs an OpenAI key.

## Files to create / edit

- `Dockerfile` — multi-stage build (Rust builder → slim runtime).
- `.dockerignore` — trim the context.
- `docker-compose.yml` — optional convenience file for local runs.
- `fly.toml` — Fly.io app config (the chosen hosting target per
  `plans/README.md`; user said budget is not a concern, but pick the
  cheap shared-CPU-1x default anyway).
- `README.md` — the repo's front door. Replace the placeholder that
  exists if any (Phase 0 may have written a stub).
- `.env.example` — document required env vars without committing them.
- `scripts/bootstrap.sh` — convenience script that runs Tailwind
  build, bundle, and prints the local demo URL.
- `plans/README.md` — mark Phase 6 as ✅ done in the table (this is
  the ONLY place you edit the index after a phase ships).
- `AGENTS.md` — refresh if any commands changed during Phase 5 (e.g.
  adding `compose up` as an alternative to `dx serve`). Do not
  introduce friction here.

## Dockerfile

```dockerfile
# ---- Builder -------------------------------------------------------------
FROM rust:1.96-slim AS builder

# DX Linux deps (webview not needed; web target only — but dx itself
# benefits from these basics for the bundle step)
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*

# Install the Dioxus CLI binary with cargo-binstall (much faster than
# building from source)
RUN cargo install cargo-binstall --locked --version '^1.11' \
 && cargo binstall dioxus-cli --version 0.7.9 --locked --no.confirm

WORKDIR /app

# Cache deps separately from source
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY Dioxus.toml input.css package.json package-lock.json* ./
COPY assets ./assets
COPY data ./data

# Pre-build tailwind so the asset! macro can resolve during cargo build
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
 && apt-get install -y nodejs \
 && npm install --no-audit --no-fund

RUN ./node_modules/.bin/tailwindcss --cwd /app -i /app/input.css -o /app/assets/tailwind.css --silent

# Build the server binary (dx produces a single axum server that
# serves the wasm client + /api/* server functions)
RUN dx bundle --release --platform web

# ---- Runtime -------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# The axum server is statically linked except for libssl/libgcc
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libgcc-s1 \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the dx bundle output. dx writes to dist/ by default.
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/data ./data
COPY --from=builder /app/assets ./assets

# Env contract (Phase 3 + Phase 2)
ENV CORPUS_PATH=/app/data/corpus.json \
    OPENAI_MODEL=gpt-4o-mini \
    ROCKET_PORT=8080 \
    DX_PORT=8080

EXPOSE 8080

# dx emits a runnable server binary; locatate it from the bundle and run.
CMD ["./dist/visit-quang-nam-planner"]
```

Notes:
- The exact `dx bundle` output path may differ in Dioxus 0.7. Verify by
  running `dx bundle --release --platform web` locally once and
  inspecting `dist/`. Adjust the `COPY --from=builder` lines
  accordingly. **Do not guess the path** — running the bundle is part
  of the verification gate.
- If `dx bundle` doesn't ship a standalone binary (it might expect the
  server to launch from the crate root via `dx serve --release --web`
  in production mode), fall back to running `cargo build --release` and
  invoking the binary directly, pointing it at the static
  `assets/` + `dist/` directories. The point is to **verify**, not
  assume. Update the Dockerfile CMD accordingly.

## .dockerignore

```
/target
/dist
node_modules
.git
*.log
.env*
.vscode
.idea
```

## fly.toml

```toml
app = "visit-quang-nam-planner"
primary_region = "sin"   # Singapore — closest to Vietnam users

[build]
  dockerfile = "Dockerfile"

[http_service]
  internal_port = 8080
  force_https = true
  auto_stop_machines = true
  auto_start_machines = true
  min_machines_running = 0   # scale to zero between requests

[[http_service.checks]]
  interval = "30s"
  timeout = "5s"
  graceful_period = "20s"
  method = "GET"
  path = "/"          # app accepts GET /
  protocol = "http"

[vm]
  size = "shared-cpu-1x"
  memory = "512mb"

[env]
  OPENAI_MODEL = "gpt-4o-mini"
  CORPUS_PATH = "/app/data/corpus.json"
  # OPENAI_API_KEY set via: fly secrets set OPENAI_API_KEY=sk-...
```

Scale-to-zero keeps idle cost ~$0 for a demo. The first request pays a
~300 ms machine-boot penalty, which is negligible next to the LLM call.

## docker-compose.yml

```yaml
services:
  planner:
    build: .
    ports:
      - "8080:8080"
    environment:
      OPENAI_API_KEY: ${OPENAI_API_KEY}
      OPENAI_MODEL: gpt-4o-mini
      CORPUS_PATH: /app/data/corpus.json
```

## README.md

Write the user-facing entrypoint. Required sections:

1. **Title + one-line description.** "Visit Quang Nam AI Trip Planner —
   a personalised itinerary generator grounded in the content of
   visitquangnam.com."
2. **Demo screenshot** — embed the rendered page at the README root
   (screenshot from Phase 4 PR; if those aren't available, link to the
   SVG mockup at
   `/home/jbrown/ai-trip-planner-mockup.svg`).
3. **What it does** — 3 bullet points: takes preferences, retrieves
   RAG chunks, generates a 1–14 day itinerary with source links and a
   sustainability score.
4. **Stack** — link to `AGENTS.md` and summarise in 2 sentences.
5. **Quick start** — exact commands, from a fresh clone:
   ```sh
   # 1. Tools (one-time)
   rustup target add wasm32-unknown-unknown
   cargo binstall dioxus-cli --version 0.7.9
   npm install

   # 2. Corpus (one-time; needs OPENAI_API_KEY)
   OPENAI_API_KEY=sk-... cargo run --release --bin build_corpus

   # 3. Dev server
   npm run tailwind:watch &  # in one terminal
   dx serve --web            # in another
   ```
6. **Docker** — `docker compose up` with the env var.
7. **Deploy** — Fly.io steps:
   ```sh
   flyctl launch --no-deploy
   flyctl secrets set OPENAI_API_KEY=sk-...
   fly deploy
   ```
8. **Project structure** — link to `plans/README.md` for the phase
   index, link to `AGENTS.md` for the dev contract.
9. **License** — MIT (matches `Cargo.toml`).
10. **Credits** — Visit Quang Nam for the source content, Keen Agency
    for the destination-marketing case study that inspired this build.

Use clear prose, not marketing fluff.

## .env.example

```sh
# Required for build_corpus (Phase 1) and plan_trip (Phase 3)
OPENAI_API_KEY=

# Default to gpt-4o-mini for the planner
OPENAI_MODEL=gpt-4o-mini

# Path to the RAG corpus JSON (committed in data/corpus.json)
CORPUS_PATH=data/corpus.json
```

## scripts/bootstrap.sh

A convenience script for a fresh clone — runs the one-time commands
above in sequence with sensible error handling. Make it executable
(`chmod +x`) and have it `set -euo pipefail`.

## Tasks

1. Run `dx bundle --release --platform web` locally to inspect the
   output — confirm the binary name and `dist/` structure.
2. Write `Dockerfile` using the **verified** bundle path.
3. Build the image locally and run it:
   ```sh
   docker build -t visit-quang-nam-planner .
   docker run --rm -p 8080:8080 -e OPENAI_API_KEY=$OPENAI_API_KEY visit-quang-nam-planner
   curl -X POST http://127.0.0.1:8080/api/plan-trip -H 'content-type: application/json' \
     -d '{"duration_days":3,"month":"March","interests":["Food","Beaches"],"travelers":{"adults":2,"kids":0},"pace":"Slow","budget_tier":"Mid","green_travel":true}'
   ```
   The curl must return 200 + a structured JSON itinerary. This is the
   primary acceptance test for Phase 6.
4. Write `fly.toml`, `docker-compose.yml`, `.env.example`, `README.md`,
   `scripts/bootstrap.sh`.
5. (Optional, only if the user gives the go-ahead) deploy to Fly.io and
   record the demo URL in `README.md`.
6. Update `plans/README.md`: mark Phase 6 ✅ done (and any earlier
   phases the user has shipped along the way).

## Acceptance criteria

- [ ] `docker build` succeeds and produces an image <200 MB.
- [ ] `docker run` on the built image serves the app on :8080 and
      responds to `POST /api/plan-trip` with a 200 + structured JSON
      body (the same smoke test from Phase 3).
- [ ] README contains the exact commands a fresh clone needs to run the
      app, verified by copy-pasting them into a shell with no prior
      state.
- [ ] `fly deploy` succeeds (or is clearly documented as
      skipped-pending-user, with the `fly.toml` ready to go).
- [ ] All four CI gates still pass (this phase shouldn't touch Rust
      code, but the gates are still required).
- [ ] `plans/README.md` index reflects the final phase status.
- [ ] `.env.example` lists every env var the runtime consults
      (`OPENAI_API_KEY`, `OPENAI_MODEL`, `CORPUS_PATH`, plus any added
      during Phase 5).

## Notes for the agent

- The user explicitly said budget is not a concern for this demo, so a
  `shared-cpu-1x` Fly.io machine is fine. Don't provision a dedicated
  host.
- If `dx bundle` and `dx serve --release --web` differ in how they
  serve files, prefer `dx serve` inside the Docker runtime and bundle
  only once at image build. The simpler the runtime command, the less
  that can go wrong on the first deploy.
- Do not commit the OpenAI key in any file. Any partial-fill during
  testing must be cleared before git add. Use `git diff --cached` to
  eyeball before commit.
- The `Dockerfile` uses `cargo-binstall` for `dx` — much faster than
  building the CLI from source (the Phase 0 AGENTS.md already
  recommends this path).
- If you decide to skip Fly.io for now, document that clearly in
  `README.md` under "Deploy" with a sentence pointing the user at
  `fly deploy` once they set up an account. Don't leave the gap silent.
- The screenshot embedded in README: if you can't capture one from a
  headless browser in the sandbox, link to the SVG mockup instead and
  leave a TODO to replace it post-demo. Do not block the README merge
  on a screenshot you can't produce here.