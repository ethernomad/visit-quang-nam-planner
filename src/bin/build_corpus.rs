// xtask entry — builds the committed RAG corpus.
//
// Pipeline: fetch every WP post+page → chunk each into ~300-token slices
// with `# {title}` prefix → embed all chunks in batches of 256 against
// OpenAI `text-embedding-3-small` → assemble a `Corpus` and write it to
// `data/corpus.json` so the server boots offline in Phase 2.
//
// Run with:
//
//     OPENAI_API_KEY=sk-... cargo run --release --bin build_corpus
//
// The committed `data/corpus.json` then needs no `OPENAI_API_KEY` at
// server startup. To refresh the corpus, re-run the bin and commit the
// updated file. Big Pickle / OpenCode Zen can't be used here — Zen has
// no `/embeddings` endpoint, so embeddings stay on real OpenAI for the
// one-time corpus build. (Phase 3 will NAT chat to `opencode/big-pickle`
// via `OPENCODE_API_KEY` + `OPENCODE_BASE_URL=https://opencode.ai/zen/v1`.)

#![cfg(feature = "server")]

use std::env;
use std::fs;
use std::path::Path;

use anyhow::Context;
use visit_quang_nam_planner::domain::{Chunk, Corpus};
use visit_quang_nam_planner::ingest;
use visit_quang_nam_planner::ingest::wordpress::RawPost;

const EMBEDDING_MODEL: &str = "text-embedding-3-small";
const OUT_PATH: &str = "data/corpus.json";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if env::var("OPENAI_API_KEY").is_err() {
        anyhow::bail!("OPENAI_API_KEY not set — cannot embed corpus");
    }

    let client = reqwest::Client::builder()
        .user_agent("visit-quang-nam-planner/build_corpus")
        .build()?;
    let posts = ingest::wordpress::fetch_all(&client).await?;
    tracing::info!(count = posts.len(), "fetched posts+pages");

    let mut chunks: Vec<Chunk> = Vec::new();
    for post in posts {
        let RawPost {
            id,
            link,
            title,
            text,
            category,
            ..
        } = post;
        let slices = ingest::chunk::chunk(&text, &title);
        for (idx, body) in slices {
            chunks.push(Chunk {
                id: format!("{id}-{idx}"),
                post_id: id,
                source_url: link.clone(),
                title: title.clone(),
                category: category.clone(),
                text: body,
                embedding: Vec::new(), // filled in by the batch embed below
            });
        }
    }
    tracing::info!(chunks = chunks.len(), "chunked");

    let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
    let embeddings = ingest::embedder::embed(&texts).await?;
    if embeddings.len() != chunks.len() {
        anyhow::bail!(
            "embedding count mismatch: got {} embeddings for {} chunks",
            embeddings.len(),
            chunks.len()
        );
    }
    for (chunk, embedding) in chunks.iter_mut().zip(embeddings) {
        chunk.embedding = embedding;
    }

    // Sanity-check: every chunk has a non-empty 1536-dim embedding.
    for (i, c) in chunks.iter().enumerate() {
        if c.embedding.len() != 1536 {
            anyhow::bail!(
                "chunk {i} ({}) has embedding of dim {}, expected 1536",
                c.id,
                c.embedding.len()
            );
        }
    }

    let corpus = Corpus {
        model: EMBEDDING_MODEL.to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        chunks,
    };

    if let Some(parent) = Path::new(OUT_PATH).parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {} parent", OUT_PATH))?;
    }
    fs::write(OUT_PATH, serde_json::to_string_pretty(&corpus)?)?;
    tracing::info!(path = OUT_PATH, "wrote corpus");
    Ok(())
}
