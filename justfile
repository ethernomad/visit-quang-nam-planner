# Visit Quang Nam AI Trip Planner — justfile
# Run `just` (or `just --list`) to see all available commands.

default := "ci"

# Run all blocking checks: format, lint, tests
ci: fmt-check lint test

# Format check (CI mode)
fmt-check:
    cargo fmt --check

# Format in-place
fmt:
    cargo fmt

# Clippy with warnings as errors
lint:
    cargo clippy --all-targets -- -D warnings

# Run unit tests
test:
    cargo test --all

# Check wasm client compiles (no server deps)
check-wasm:
    cargo check --target wasm32-unknown-unknown --no-default-features --features web

# Start the fullstack dev server (hot-reload)
dev:
    dx serve --web

# Watch and rebuild Tailwind CSS during UI work
tw:
    npx @tailwindcss/cli -i ./input.css -o ./assets/tailwind.css --watch

# One-shot Tailwind CSS build
tw-build:
    npx @tailwindcss/cli -i ./input.css -o ./assets/tailwind.css

# Production bundle
build:
    dx bundle --release --platform web

# Rebuild the RAG corpus from visitquangnam.com
corpus:
    cargo run --release --bin build_corpus

# API-only server (no wasm client, no browser UI)
api-serve: api-build
    ./target/release/visit-quang-nam-planner

# Build the API-only server binary
api-build:
    cargo build --release --no-default-features --features server
