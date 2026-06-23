// OpenAI embeddings client for the RAG corpus.
//
// Embeds text slices with `text-embedding-3-small` (1536-dim) via the
// `async-openai` crate. Server-only: depends on `async-openai`,
// `reqwest`, `tokio`, `futures`, all gated behind the `server` feature.
//
// Big Pickle / OpenCode Zen is a chat-completions provider and exposes
// no `/embeddings` endpoint, so it cannot be used here. Phase 3 (LLM
// orchestration) will NAT chat to Zen via `OPENCODE_API_KEY` +
// `OPENCODE_BASE_URL=https://opencode.ai/zen/v1` with model
// `opencode/big-pickle`; this Phase 1 ingest path keeps using
// `OPENAI_API_KEY` against the real OpenAI API for the one-time
// `build_corpus` run. The committed `data/corpus.json` then needs no
// key at server startup.
//
// Batches are sent concurrently with `futures::future::try_join_all`,
// 256 inputs per call (OpenAI's per-request limit for this model). On a
// failed batch we retry once after 5 s and then surface the underlying
// error — a missing embedding silently breaks retrieval, so we never
// skip a chunk.

#![cfg(feature = "server")]

use std::env;
use std::time::Duration;

use anyhow::Context;
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::{CreateEmbeddingRequestArgs, EmbeddingInput};
use futures::future::try_join_all;

const MODEL: &str = "text-embedding-3-small";
/// OpenAI's per-request input cap for `text-embedding-3-small` is 2048
/// in array form, but the documented practical batch for parallel
/// throughput is 256; staying under both keeps us well clear of the
/// limit and the per-JSON size cap.
const BATCH: usize = 256;
const RETRY_DELAY: Duration = Duration::from_secs(5);

/// Produce one 1536-dim embedding per input string, preserving input
/// order. Reads `OPENAI_API_KEY` from the environment; bails with a
/// human-readable error if missing so the bin exits cleanly rather than
/// 401-ing deep inside `async-openai`.
pub async fn embed(texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let api_key = env::var("OPENAI_API_KEY")
        .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set — cannot embed corpus"))?;
    // Allow callers to override the base URL (e.g. for staging) via the
    // standard async-openai env var; default is api.openai.com.
    let config = OpenAIConfig::new().with_api_key(api_key);
    let client = Client::with_config(config);

    let batches: Vec<Vec<Vec<f32>>> = try_join_all(
        texts
            .chunks(BATCH)
            .map(|batch| embed_batch(&client, batch.to_vec())),
    )
    .await?;

    let mut all = Vec::with_capacity(texts.len());
    for emb in batches {
        all.extend(emb);
    }
    Ok(all)
}

fn build_request(
    batch: Vec<String>,
) -> anyhow::Result<async_openai::types::CreateEmbeddingRequest> {
    let input: EmbeddingInput = batch.into();
    Ok(CreateEmbeddingRequestArgs::default()
        .model(MODEL)
        .input(input)
        .build()?)
}

async fn embed_batch(
    client: &Client<OpenAIConfig>,
    batch: Vec<String>,
) -> anyhow::Result<Vec<Vec<f32>>> {
    match client
        .embeddings()
        .create(build_request(batch.clone())?)
        .await
    {
        Ok(resp) => Ok(order_embeddings(resp.data)),
        Err(e) => {
            tracing::warn!(error = %e, "embedding batch failed; retrying once after 5s");
            tokio::time::sleep(RETRY_DELAY).await;
            let resp = client
                .embeddings()
                .create(build_request(batch)?)
                .await
                .context(
                    "embedding batch failed after one retry — surfacing underlying OpenAI error",
                )?;
            Ok(order_embeddings(resp.data))
        }
    }
}

/// OpenAI returns embeddings in arbitrary order; each carries an `index`
/// pointing back to its position in the input array. Sort by index so
/// we restore input order before zipping back into `Chunk`s.
fn order_embeddings(data: Vec<async_openai::types::Embedding>) -> Vec<Vec<f32>> {
    let mut indexed: Vec<(usize, Vec<f32>)> = data
        .into_iter()
        .map(|e| (e.index as usize, e.embedding))
        .collect();
    indexed.sort_by_key(|(i, _)| *i);
    indexed.into_iter().map(|(_, e)| e).collect()
}
