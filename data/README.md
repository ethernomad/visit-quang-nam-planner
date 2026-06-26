# RAG corpus directory.

This directory holds `corpus.json` — the prebuilt chunk + embeddings cache
consumed by `InMemoryRetriever` at server startup. It's produced by:

    cargo run --release --bin build_corpus

`corpus.json` is produced by `build_corpus` and committed so the server boots offline.