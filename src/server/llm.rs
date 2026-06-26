// LLM client for Phase 3 chat orchestration.
//
// Per AGENTS.md (the locked tech stack), the planner uses OpenCode Zen
// via an OpenAI-chat-compatible endpoint at
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
use std::time::Duration;

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
/// `OPENCODE_MODEL` env var. Uses `mimo-v2.5-free` (a non-reasoning model)
/// instead of `big-pickle` which routes to a reasoning model that burns
/// all token budget on thinking tokens before producing content.
const DEFAULT_MODEL: &str = "mimo-v2.5-free";
/// Default Zen base URL. Overridable via `OPENCODE_BASE_URL`.
const DEFAULT_BASE_URL: &str = "https://opencode.ai/zen/v1";
/// Server-side cap on the chat-completion call. The wasm client's 60s
/// cap (see `app.rs`) is the backstop; this is the authoritative guard
/// that frees the axum worker even if the Zen endpoint hangs. Tunable
/// via `OPENCODE_TIMEOUT_SECS`.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
/// Max completion tokens per LLM call. A 14-day itinerary with 5 activities
/// each fits comfortably in ~8K tokens; a lower cap speeds generation by
/// preventing the model from inflating output verbosity. Reasoning models
/// (e.g. `big-pickle` which routes to `deepseek-v4-flash` with thinking)
/// will generate unbounded reasoning tokens without a cap, hitting the
/// timeout. Tunable via `OPENCODE_MAX_TOKENS`.
const DEFAULT_MAX_TOKENS: u32 = 8192;

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
    timeout: Duration,
    max_tokens: u32,
}

impl LlmClient {
    /// Construct from env. Required: `OPENCODE_API_KEY`. Optional:
    /// `OPENCODE_BASE_URL` (default `https://opencode.ai/zen/v1`),
    /// `OPENCODE_MODEL` (default `mimo-v2.5-free`),
    /// `OPENCODE_MAX_TOKENS` (default 16384).
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = env::var("OPENCODE_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENCODE_API_KEY not set — cannot run planner LLM"))?;
        let base_url = env::var("OPENCODE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
        let model = env::var("OPENCODE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        let timeout = env::var("OPENCODE_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TIMEOUT);
        let max_tokens = env::var("OPENCODE_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_TOKENS);
        let config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(base_url);
        Ok(Self {
            client: Client::with_config(config),
            model,
            timeout,
            max_tokens,
        })
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
            .max_tokens(self.max_tokens)
            .response_format(ResponseFormat::JsonObject)
            .messages([system_msg, user_msg])
            .build()?;

        let response = tokio::time::timeout(self.timeout, self.client.chat().create(request))
            .await
            .context(format!(
                "chat completion timed out after {:?}",
                self.timeout
            ))?
            .context("chat completion request to LLM failed")?;

        let content = response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("LLM returned no message content"))?;

        parse_llm_json::<T>(&content)
    }
}

/// Parse the LLM's raw JSON response into `T`, wrapping the deserialise error
/// with the offending payload so the operator never loses what the model
/// actually returned. `post_validate` is the authoritative guardrail on the
/// `Itinerary` shape; this is the parse-time one. Extracted from
/// `complete_json` so the error-wrapping contract is unit-testable without a
/// network round-trip (audit #12).
fn parse_llm_json<T: DeserializeOwned>(content: &str) -> anyhow::Result<T> {
    serde_json::from_str::<T>(content).map_err(|e| {
        anyhow::anyhow!(
            "failed to parse LLM JSON into {}: {e}\n--- raw model output ---\n{content}",
            std::any::type_name::<T>()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::parse_llm_json;
    use visit_quang_nam_planner::domain::Itinerary;

    // A minimal-but-schema-valid `Itinerary` JSON. Kept small so the
    // happy-path parse test stays robust to domain field additions; only
    // the fields `Itinerary` actually requires are present (`TripSummary`
    // has `#[serde(default)]` on the optional `sustainability_breakdown`).
    fn valid_itinerary_json() -> String {
        serde_json::json!({
            "days": [],
            "summary": {
                "duration": "0 days",
                "destinations": [],
                "budget_estimate": "$0",
                "sustainability_score": 0
            }
        })
        .to_string()
    }

    #[test]
    fn parse_llm_json_returns_ok_for_valid_json() {
        let json = valid_itinerary_json();
        let parsed = parse_llm_json::<Itinerary>(&json);
        assert!(parsed.is_ok(), "valid JSON should parse cleanly");
    }

    #[test]
    fn parse_llm_json_preserves_raw_output_on_error() {
        // A string that is clearly not JSON. The point of this error shape
        // (per audit #12) is that the raw model output is preserved in the
        // error message so an operator can see what tripped the model.
        let raw = "{ not valid json";
        let err = parse_llm_json::<Itinerary>(raw).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("failed to parse LLM JSON"),
            "error must identify the failure: {msg}"
        );
        assert!(
            msg.contains(raw),
            "error must preserve the raw model output, got: {msg}"
        );
        assert!(
            msg.contains(std::any::type_name::<Itinerary>()),
            "error must name the target type, got: {msg}"
        );
    }
}
