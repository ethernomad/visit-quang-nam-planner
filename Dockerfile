# ---- Builder -------------------------------------------------------------
# Multi-stage build for the Visit Quang Nam AI Trip Planner (Dioxus 0.7
# fullstack: a single axum server binary that serves the wasm client + the
# /api/* server functions).
#
# Toolchain note: pinned to rust:1.95-slim because rustc 1.96 is incompatible
# with wasm-bindgen 0.2.125 (the latest version dioxus 0.7.9's bundled CLI can
# use). On 1.96, `dx bundle` aborts at the wasm-bindgen step with
#   "failed to find the `__wbindgen_externref_table_dealloc` function"
# (verified: lib 0.2.125 + CLI 0.2.125 both reproduce it; no newer version
# exists; -C target-feature=-reference-types does not fix it). 1.95 is the
# last stable that works with wasm-bindgen 0.2.125. Bump back to latest once a
# wasm-bindgen release fixes 1.96 compat.
FROM rust:1.95-slim AS builder

# DX Linux build deps (web target only — no webview/native GUI libs needed).
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*

# Install the Dioxus CLI binary with cargo-binstall (much faster than building
# the CLI from source). Pin to 0.7.9 to match Cargo.lock / AGENTS.md.
RUN cargo install cargo-binstall --locked --version '^1.11' \
 && cargo binstall dioxus-cli --version 0.7.9 --locked --no-confirm

WORKDIR /app

# Copy the manifest + source first so the dependency layer can cache.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY Dioxus.toml input.css package.json package-lock.json* ./
COPY assets ./assets
# The RAG corpus is a user-supplied prerequisite (see README.md). The build
# itself does not need it, but the runtime does; copy it through the builder so
# the runtime stage can pick it up without a second host COPY.
COPY data ./data

# Pre-build the Tailwind stylesheet so the `asset!("/assets/tailwind.css")`
# macro can resolve during the dx/cargo build.
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
 && apt-get install -y nodejs \
 && npm install --no-audit --no-fund \
 && ./node_modules/.bin/tailwindcss --cwd /app -i /app/input.css -o /app/assets/tailwind.css --silent

# Build the fullstack bundle: a single axum server binary + the wasm client +
# static assets, emitted to dist/ (dx's default --out-dir).
RUN dx bundle --release --platform web

# ---- Runtime -------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# The axum server is statically linked except for libssl/libgcc.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libgcc-s1 \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the dx bundle output (server binary + public/ with the wasm client and
# bundled assets).
COPY --from=builder /app/dist ./dist
# Copy the RAG corpus through. The server loads it at startup via CORPUS_PATH.
COPY --from=builder /app/data ./data

# Env contract (verified against src/):
#  - OPENCODE_API_KEY (required) — planner LLM via Zen. Set at `docker run`.
#  - OPENAI_API_KEY   (required) — query-time embeddings. Set at `docker run`.
#  - OPENCODE_BASE_URL / OPENCODE_MODEL (optional, defaults in src/server/llm.rs)
#  - CORPUS_PATH (optional, default data/corpus.json)
#  - PORT — the dioxus fullstack axum server reads this (dioxus-cli-config),
#    default 8080. NOT ROCKET_PORT (no Rocket in this stack).
ENV CORPUS_PATH=/app/data/corpus.json \
    PORT=8080

EXPOSE 8080

# dx bundle emits the server binary at the dist/ root; running it from within
# dist/ lets the server locate the bundled public/ directory (wasm + assets).
WORKDIR /app/dist
CMD ["./visit-quang-nam-planner"]
