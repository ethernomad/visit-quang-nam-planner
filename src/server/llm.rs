// LLM client for Phase 3 chat orchestration.
//
// Per AGENTS.md (the locked tech stack), the planner uses OpenCode Zen's
// `opencode/big-pickle` via an OpenAI-chat-compatible endpoint at
// `OPENCODE_BASE_URL` (default `https://opencode.ai/zen/v1`), authenticated
// with `OPENCODE_API_KEY`. Zen is free during its stealth period and speaks
// the OpenAI `/chat/completions` shape, so `async-openai` drives it directly
// by pointing its `OpenAIConfig` at the Zen base URL.
//
// Embeddings still go through real OpenAI (`OPENAI_API_KEY`,
// `text-embedding-3-small`) — Zen has no `/embeddings` endpoint. That path
// lives in `src/ingest/embedder.rs` and is only used by `InMemoryRetriever`
// at query time; it is NOT touched by this module.
//
// JSON mode: Zen responds to `response_format: json_object` reliably. The
// newer structured-outputs `json_schema` mode is OpenAI-specific and not
// guaranteed on Zen, so we use `json_object` and rely on `plan_trip`'s
// `post_validate` for the contract — see `plan_trip.rs`.
//
// Testability: orchestration depends on the `LlmCompleter` trait (below),
// not on `LlmClient` directly. `plan_trip_inner` takes `&dyn LlmCompleter`, so
// `tests/plan_trip.rs` can inject a `MockLlm` returning a canned
// `Itinerary` without touching the network. The `#[post]` wrapper builds a
// real `LlmClient` via `shared_llm()`.

#![cfg(feature = "server")]

use std::env;

use anyhow::Context;
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs, ResponseFormat,
};
use async_trait::async_trait;
use serde::de::DeserializeOwned;

use visit_quang_nam_planner::domain::Itinerary;

/// Default chat model on the Zen stealth endpoint. Override with the
/// `OPENCODE_MODEL` env var (useful for A/B against `gpt-4o-mini` once Zen
/// sunsets and we re-point `OPENCODE_BASE_URL` at real OpenAI).
const DEFAULT_MODEL: &str = "opencode/big-pickle";
/// Default Zen base URL. Overridable via `OPENCODE_BASE_URL`.
const DEFAULT_BASE_URL: &str = "https://opencode.ai/zen/v1";

/// Seam used by `plan_trip_inner`. The real implementation is `LlmClient`
/// below; tests inject a `MockLlm` that returns a canned `Itinerary` so the
/// orchestration (retrieve → prompt → parse → validate) is exercised
/// without hitting OpenAI/Zen.
///
/// The trait method is non-generic (returns `Itinerary`, not `T`) so the
/// trait stays dyn-compatible — `shared_llm()` returns
/// `Arc<dyn LlmCompleter>`. `Itinerary` is the only type the planner ever
/// asks the LLM for, so this isn't a meaningful loss of generality.
#[async_trait]
pub trait LlmCompleter: Send + Sync {
    /// Send `system` + `user` as a two-message chat completion in JSON mode,
    /// parse the single returned object into `Itinerary`, and surface a
    /// clear error (including the raw model output) if parsing fails — the
    /// caller never loses the offending payload.
    async fn complete_itinerary(&self, system: &str, user: &str) -> anyhow::Result<Itinerary>;
}

/// OpenAI-chat-compatible client pointed at Zen (or real OpenAI if
/// `OPENCODE_BASE_URL` is overridden). One client per process, held in the
/// `shared_llm()` `OnceLock` in `src/server/mod.rs`.
pub struct LlmClient {
    client: Client<OpenAIConfig>,
    model: String,
}

impl LlmClient {
    /// Construct from env. Required: `OPENCODE_API_KEY`. Optional:
    /// `OPENCODE_BASE_URL` (default `https://opencode.ai/zen/v1`),
    /// `OPENCODE_MODEL` (default `opencode/big-pickle`).
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = env::var("OPENCODE_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENCODE_API_KEY not set — cannot run planner LLM"))?;
        let base_url = env::var("OPENCODE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
        let model = env::var("OPENCODE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        let config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(base_url);
        Ok(Self {
            client: Client::with_config(config),
            model,
        })
    }

    /// Configured model name (exposed for diagnostics/logging in `plan_trip`).
    #[allow(dead_code)]
    pub fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl LlmCompleter for LlmClient {
    async fn complete_itinerary(&self, system: &str, user: &str) -> anyhow::Result<Itinerary> {
        let itinerary: Itinerary = self.complete_json(system, user).await?;
        Ok(itinerary)
    }
}

impl LlmClient {
    /// Generic chat-completion helper. Not on the trait (a generic async
    /// method isn't dyn-compatible); callers that need a non-`Itinerary`
    /// schema can use `LlmClient` directly. `complete_itinerary` on the
    /// trait is a thin wrapper around this.
    async fn complete_json<T: DeserializeOwned + Send>(
        &self,
        system: &str,
        user: &str,
    ) -> anyhow::Result<T> {
        let system_msg: ChatCompletionRequestMessage =
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system)
                .build()?
                .into();
        let user_msg: ChatCompletionRequestMessage =
            ChatCompletionRequestUserMessageArgs::default()
                .content(user)
                .build()?
                .into();

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .response_format(ResponseFormat::JsonObject)
            .messages([system_msg, user_msg])
            .build()?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .context("chat completion request to LLM failed")?;

        let content = response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| anyhow::anyhow!("LLM returned no message content"))?;

        serde_json::from_str::<T>(&content).map_err(|e| {
            // Wrap with the raw payload so the user/operator can see what
            // tripped the model — don't lose it. `post_validate` is the
            // authoritative guardrail; this is the parse-time one.
            anyhow::anyhow!(
                "failed to parse LLM JSON into {}: {e}\n--- raw model output ---\n{content}",
                std::any::type_name::<T>()
            )
        })
    }
}

/// Convenience free fn used outside the trait object context. Kept so callers
/// with a concrete `&LlmClient` can request the itinerary with one call.
#[allow(dead_code)]
pub async fn complete_itinerary(
    llm: &dyn LlmCompleter,
    system: &str,
    user: &str,
) -> anyhow::Result<Itinerary> {
    llm.complete_itinerary(system, user).await
}
