// Build corpus xtask. Phase 1: fetch all posts/pages from
// https://visitquangnam.com/wp-json/wp/v2/posts, chunk by paragraph,
// embed in batches via OpenAI, and write data/corpus.json.
//
// Run with: cargo run --release --bin build_corpus