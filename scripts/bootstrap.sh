#!/usr/bin/env bash
# Convenience script for a fresh clone — runs the one-time setup steps in
# sequence: toolchain check, JS deps, Tailwind build, RAG corpus build, then
# prints the dev-server URL. Not a replacement for reading README.md.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "== Checking tools =="
command -v rustc >/dev/null || { echo "rustc not found — install Rust via https://rustup.rs"; exit 1; }
rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown || {
  echo "wasm32-unknown-unknown target missing — installing..."
  rustup target add wasm32-unknown-unknown
}
command -v dx >/dev/null || {
  echo "dioxus CLI (dx) not found — installing 0.7.9 via cargo-binstall..."
  cargo install cargo-binstall --locked --version '^1.11'
  cargo binstall dioxus-cli --version 0.7.9 --locked --no-confirm
}
command -v npm >/dev/null || { echo "npm not found — install Node.js 22+"; exit 1; }

echo "== Installing JS deps =="
npm install --no-audit --no-fund

echo "== Building Tailwind =="
npm run tailwind:build

echo "== Building RAG corpus =="
if [ -z "${OPENAI_API_KEY:-}" ]; then
  echo "WARNING: OPENAI_API_KEY is not set — skipping corpus build."
  echo "         The dev server will 500 on /api/plan-trip until you run:"
  echo "         OPENAI_API_KEY=sk-... cargo run --release --bin build_corpus"
else
  cargo run --release --bin build_corpus
fi

echo ""
echo "== Setup complete. Start the dev server with:"
echo "   dx serve --web"
echo "   (in a separate terminal: npm run tailwind:watch)"
echo ""
echo "   Demo URL: http://127.0.0.1:8080"
