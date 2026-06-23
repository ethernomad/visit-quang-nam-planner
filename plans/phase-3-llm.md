# Phase 3 — LLM orchestration

**Goal:** Implement the `plan_trip` server function that turns a
`Preferences` payload into a typed, validated `Itinerary`. The function
retrieves grounding chunks from the shared `Retriever` (Phase 2),
hands them plus the user's preferences to OpenAI `gpt-4o-mini` with a
JSON-schema-shaped prompt, parses the response into an `Itinerary`, and
returns it. JSON-mode + serde validation means the UI in Phase 4 never
sees a malformed payload.

**Status:** pending
**Depends on:** Phase 2 (`Retriever` trait + `InMemoryRetriever` +
`shared_retriever()`), Phase 1 (`Chunk` type + `corpus.json` on disk).

## Files to create / edit

- `src/domain/mod.rs` — add the input/output types (Phase 2 only added
  `Chunk`/`Corpus`; this phase adds the planner types).
- `src/server/mod.rs` — re-export `plan_trip`, `llm`.
- `src/server/llm.rs` — OpenAI chat completion wrapper with strict JSON
  output.
- `src/server/prompts.rs` — system + user prompt builders. Keep prompt
  templates in one file so iteration doesn't touch orchestration code.
- `src/server/plan_trip.rs` — the `#[post("/api/plan-trip")]` function.
- `src/lib.rs` — ensure `pub mod server;` is exported so client code
  can call `server::plan_trip::plan_trip(...)`.

## Domain types — `src/domain/mod.rs`

Append to Phase 1's existing types (do not modify `Chunk`/`Corpus`):

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Interest {
    Food,
    Beaches,
    Culture,
    Nature,
    Wellness,
    GreenTravel,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Pace { Slow, Moderate, Active }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum BudgetTier { Backpacker, Mid, Luxury }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Month {
    January, February, March, April, May, June,
    July, August, September, October, November, December,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Travelers {
    pub adults: u8,
    pub kids: u8,
}

/// Client → server request payload.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Preferences {
    pub duration_days: u8,            // 1..=14
    pub month: Month,
    pub interests: Vec<Interest>,
    pub travelers: Travelers,
    pub pace: Pace,
    pub budget_tier: BudgetTier,
    pub green_travel: bool,
}

/// One stop on a day plan.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Category { Food, Nature, Culture, Beach, Wellness }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Activity {
    pub time: String,                 // "10:00 AM"
    pub title: String,
    pub description: String,
    pub category: Category,
    /// URL back to the Visit Quang Nam article this activity came from
    /// (the RAG chunk's source_url). Lets the UI surface a "Read more" link
    /// and lets users verify the AI's claim.
    pub source_url: String,
    pub estimated_cost_vnd: Option<u32>,
    pub duration_minutes: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WeatherHint {
    pub label: String,                 // "Sunny, 28°C"
    pub icon: String,                  // "☀️"
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DayPlan {
    pub index: u8,                     // 1-based
    pub title: String,                 // "Day 1 — Arrival in Da Nang & Hoi An"
    pub date_hint: String,             // "Monday" — derived from month in prompt
    pub weather: WeatherHint,
    pub activities: Vec<Activity>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TripSummary {
    pub duration: String,              // "5 days / 4 nights"
    pub destinations: Vec<String>,
    pub budget_estimate: String,       // "$520–680 per person (excl. flights)"
    /// 0..=100 — the LLM scores this from the green-travel choices it made.
    pub sustainability_score: u8,
}

/// Server → client response payload.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Itinerary {
    pub days: Vec<DayPlan>,
    pub summary: TripSummary,
}
```

All enums derive `Serialize` + `Deserialize` so they cross the
client/server boundary in `Preferences` and `Itinerary` with zero glue.
The `Month` enum maps to month-of-year; the prompt uses it for weather
context and the date_hint.

## LLM client — `src/server/llm.rs`

- `pub struct LlmClient { openai: async_openai::Client<OpenAIConfig> }`
- `pub fn from_env() -> anyhow::Result<Self>` — reads
  `OPENAI_API_KEY` (required) and `OPENAI_MODEL` (defaults to
  `gpt-4o-mini`).
- `pub async fn complete_json<T: DeserializeOwned>(&self, system: &str, user: &str) -> anyhow::Result<T>`
  - Build a `CreateChatCompletionRequestArgs` with
    `response_format: ChatCompletionResponseFormat::JsonObject` (the
    model returns a single JSON object — schema enforced in the prompt).
    If your pinned `async-openai` exposes structured outputs
    (`response_format: json_schema`), prefer that; otherwise fall back
    to `json_object` and validate post-hoc with `serde_json`.
  - Two messages: `system` (the planner role + output contract) and
    `user` (the grounded context + preferences).
  - Extract `choices[0].message.content`, `serde_json::from_str` into
    `T`. On parse error, wrap with the raw content in the error so the
    user can see what tripped the model — don't lose it.

## Prompt design — `src/server/prompts.rs`

### System prompt (fixed string, ~400 tokens)

```
You are the Visit Quang Nam travel planner. Your job is to build a
personalised day-by-day itinerary from the user's preferences and the
provided local knowledge chunks.

Rules:
1. Use ONLY the information in the provided chunks. Do NOT invent
   restaurants, hotels, beaches, or prices. If a preference can't be met
   from the chunks, say so briefly in that day's title or skip the slot
   rather than fabricate.
2. Each activity's `source_url` MUST come from a chunk's `source_url`.
3. Plan {duration} consecutive days (Day 1 through Day {duration}).
   Each day should have 3–5 activities from morning to evening.
4. Respect the pace (Slow ≤3 activities/day, Moderate 4, Active 5).
5. Respect the budget tier and traveller ages (kids change pace).
6. When `green_travel` is true, prefer chunks tagged Green travel,
   sustainability, or community-based tourism.
7. Weather hints should reflect Quang Nam's climate in {month}.
8. Compute `sustainability_score` 0–100 from how Green Travel-aligned
   the chosen activities are.

Return ONE JSON object matching this TypeScript shape (no markdown
fences, no commentary):

{{
  "days": [
    {{
      "index": 1,
      "title": "Day 1 — Arrival ...",
      "date_hint": "Monday",
      "weather": {{ "label": "Sunny, 28°C", "icon": "☀️" }},
      "activities": [
        {{
          "time": "10:00 AM",
          "title": "...",
          "description": "...",
          "category": "Food" | "Nature" | "Culture" | "Beach" | "Wellness",
          "source_url": "https://visitquangnam.com/...",
          "estimated_cost_vnd": 120000,
          "duration_minutes": 60
        }}
      ]
    }}
  ],
  "summary": {{
    "duration": "5 days / 4 nights",
    "destinations": ["Da Nang", "Hoi An", ...],
    "budget_estimate": "$520–680 per person (excl. flights)",
    "sustainability_score": 82
  }}
}}
```

### User prompt (built per request)

```rust
pub fn build_user_prompt(prefs: &Preferences, chunks: &[Chunk]) -> String {
    let mut s = String::new();
    s.push_str("# Preferences\n\n");
    s.push_str(&serde_yaml::to_string(prefs).unwrap_or_default()); // pick YAML or json
    s.push_str("\n\n# Local knowledge\n\n");
    for (i, c) in chunks.iter().enumerate() {
        s.push_str(&format!(
            "## [{}] {} ({})\n{}\nURL: {}\n\n",
            i + 1, c.title, c.category.as_deref().unwrap_or("General"),
            c.text, c.source_url,
        ));
    }
    s.push_str("Now return the itinerary JSON.");
    s
}
```

Keep prompt-building logic here so iteration doesn't require touching
`plan_trip.rs`. Don't hard-code the JSON schema string above — derive
it from a `const SCHEMA: &str = include_str!("itinerary.schema.json");`
(or use `schemars` to generate from `Itinerary`) so it stays in sync
with the Rust types when they change.

## Server function — `src/server/plan_trip.rs`

```rust
use crate::domain::{Itinerary, Preferences};
use crate::retrieval::Retriever;
use crate::server::{llm::LlmClient, prompts, shared_retriever};
use dioxus::prelude::*;
use std::sync::OnceLock;

static LLM: OnceLock<LlmClient> = OnceLock::new();

fn llm() -> &'static LlmClient {
    LLM.get_or_init(|| LlmClient::from_env().expect("OPENAI_API_KEY missing"))
}

#[post("/api/plan-trip")]
pub async fn plan_trip(prefs: Preferences) -> Result<Itinerary, ServerFnError> {
    validate_prefs(&prefs)?;

    let retriever = shared_retriever().map_err(|e| ServerFnError::from(e.to_string()))?;
    let query = build_retrieval_query(&prefs);
    let chunks = retriever.search(&query, 8).await;

    if chunks.is_empty() {
        return Err(ServerFnError::from("no grounding chunks found for those preferences"));
    }

    let system = prompts::SYSTEM_PROMPT.replace("{duration}", &prefs.duration_days.to_string())
        .replace("{month}", &format!("{:?}", prefs.month));
    let user = prompts::build_user_prompt(&prefs, &chunks);

    let itinerary = llm().complete_json::<Itinerary>(&system, &user).await
        .map_err(|e| ServerFnError::from(format!("LLM call failed: {e}")))?;

    post_validate(&itinerary, &prefs, &chunks)?;
    Ok(itinerary)
}

fn validate_prefs(p: &Preferences) -> Result<(), ServerFnError> {
    if p.duration_days == 0 || p.duration_days > 14 {
        return Err(ServerFnError::from("duration_days must be 1..=14"));
    }
    if p.interests.is_empty() {
        return Err(ServerFnError::from("interests must not be empty"));
    }
    if p.travelers.adults == 0 {
        return Err(ServerFnError::from("at least one adult required"));
    }
    Ok(())
}

fn build_retrieval_query(p: &Preferences) -> String {
    format!(
        "{}-day {} trip in {:?}, {:?} pace, {:?} budget, {} adults + {} kids, interests: {:?}",
        p.duration_days,
        if p.green_travel { "eco-friendly" } else { "" },
        p.month,
        p.pace,
        p.budget_tier,
        p.travelers.adults,
        p.travelers.kids,
        p.interests,
    )
}

/// Enforce the system prompt's claims post-hoc. The LLM may still
/// invent a URL despite the rule; reject the response with a clear
/// error if so.
fn post_validate(
    itin: &Itinerary,
    prefs: &Preferences,
    chunks: &[crate::domain::Chunk],
) -> Result<(), ServerFnError> {
    if itin.days.len() != prefs.duration_days as usize {
        return Err(ServerFnError::from("itinerary day count mismatch"));
    }
    let allowed_urls: std::collections::HashSet<&str> =
        chunks.iter().map(|c| c.source_url.as_str()).collect();
    for day in &itin.days {
        for act in &day.activities {
            if !allowed_urls.contains(act.source_url.as_str()) {
                return Err(ServerFnError::from(format!(
                    "activity '{}' references unknown source_url {}",
                    act.title, act.source_url
                )));
            }
        }
    }
    if itin.summary.sustainability_score > 100 {
        return Err(ServerFnError::from("sustainability_score > 100"));
    }
    Ok(())
}
```

`post_validate` is the real guardrail — if the LLM hallucinates a
restaurant URL, the API call rather than the UI breaks. Better UX than
showing the user a fake link.

## Tasks

1. Add `Itinerary`, `DayPlan`, `Activity`, `Preferences`, et al. to
   `domain/mod.rs`.
2. Create `src/server/llm.rs` with `LlmClient` + `complete_json`.
3. Create `src/server/prompts.rs` with `SYSTEM_PROMPT` and
   `build_user_prompt`. Keep the schema string version-controlled.
4. Create `src/server/plan_trip.rs` with the `#[post]` function and the
   `validate_prefs` / `post_validate` helpers.
5. Register the retriever + LLM singletons in `src/server/mod.rs`
   (`shared_retriever()` is from Phase 2 — add `shared_llm()` or reuse
   the `OnceLock` in `plan_trip.rs` above).
6. Add `schemars = "0.8"` to `Cargo.toml` (optional, behind `server`)
   if you decide to generate the JSON schema from types.
7. Extend `src/server/mod.rs` with `pub mod plan_trip; pub mod llm; pub mod prompts;`.

## Acceptance criteria

- [ ] `cargo check` (server target) compiles. `cargo check --target
      wasm32-unknown-unknown --no-default-features --features web`
      compiles (Phase 3 code lives in server-only modules; the wasm
      build must not reference it).
- [ ] All four CI gates pass.
- [ ] Unit tests for `validate_prefs` covering: duration 0, duration 15,
      no interests, no adults, and a happy path.
- [ ] Unit tests for `post_validate` covering: day count mismatch,
      unknown source_url, sustainability_score over cap, happy path.
- [ ] An integration test (`tests/plan_trip.rs`) that mocks the LLM and
      retriever (use a `MockLlm` trait + fixture corpus) and asserts the
      server function returns a well-formed `Itinerary`. This proves
      the orchestration is correct without hitting OpenAI.
- [ ] A manual `curl` smoke test (documented in a comment in
      `plan_trip.rs`):
      ```sh
      curl -X POST http://127.0.0.1:8080/api/plan-trip \
        -H 'content-type: application/json' \
        -d '{"duration_days":3,"month":"March","interests":["Food","Beaches"],"travelers":{"adults":2,"kids":0},"pace":"Slow","budget_tier":"Mid","green_travel":true}'
      ```
      returns a 200 with a structured JSON body.

## Notes for the agent

- `embeddings` are 1536-dim from `text-embedding-3-small` — phase 1 puts
  them in `Chunk.embedding`. Don't recompute in this phase.
- `gpt-4o-mini` accepts `response_format: json_object` reliably. The
  newer `json_schema` mode (Sept 2024) lets you pass an actual schema
  and rejects non-conforming output server-side. Use it **if**
  `async-openai` 0.27 exposes `ResponseFormat::JsonSchema`. Otherwise
  stick to `json_object` + the `post_validate` belt-and-braces path.
- Keep the system prompt under 600 tokens; the grounding chunks add
  ~8 × 300 = 2.4K tokens; gpt-4o-mini's 128K context is plenty.
- `ServerFnError` implements `From<String>` and `From<anyhow::Error>`
  in dioxus 0.7 — pick the ergonomic conversion and stay consistent.
  If you need structured status codes (400 vs 500), use the
  `AsStatusCode` path documented in `AGENTS.md`.
- The LLM may include fields you didn't ask for (`score`, `tags`).
  Configure `#[serde(deny_unknown_fields)]` on `Activity` if you want
  to reject that; otherwise ignore extras. Pick one and add a unit
  test for it.
- Do not commit intermediate LLM responses. If you snapshot fixtures
  for tests, store them under `tests/fixtures/` and gitignore
  `tests/fixtures/.cache/` for any cache the recorder writes.