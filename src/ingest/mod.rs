// Ingest pipeline. Phase 1: `wordpress` (fetch /wp-json/wp/v2/posts), `chunk`
// (paragraph chunker), `embedder` (OpenAI text-embedding-3-small), `corpus`
// (serialise to data/corpus.json). Run as an xtask via scripts/build_corpus.rs.
