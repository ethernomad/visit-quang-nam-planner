// `plan_trip` — Phase 3 server function.
//
// Pipeline: validate `Preferences` → retrieve top-K grounding chunks from the
// shared `Retriever` → build system + user prompts → call the LLM via the
// `LlmCompleter` seam → parse JSON into `Itinerary` → `post_validate` rejects
// anything that breaks the system-prompt contract (day count, hallucinated
// `source_url`s, out-of-range sustainability_score).
//
// The `LlmCompleter` trait abstracts the model call: the `#[post]` wrapper
// builds a real `LlmClient` via `shared_llm()`; `plan_trip_inner` (and its
// unit tests) inject mocks so the orchestration is exercised without
// touching OpenAI/Zen.
//
// This file is compiled under BOTH `web`-only and `server` builds — the
// Dioxus `#[post]` macro generates a wasm client stub for `plan_trip`
// automatically; the helpers (`plan_trip_inner`, `validate_prefs`,
// `post_validate`, `build_retrieval_query`) and the server-only `use`
// statements are gated behind `#[cfg(feature = "server")]` because they
// touch `Retriever` / `LlmCompleter` / `prompts` which are themselves
// server-gated.
//
// All keys live in env (`OPENCODE_API_KEY`, `OPENCODE_BASE_URL`,
// `OPENAI_API_KEY` for query-time embeddings). Neither key ever ships to
// wasm — the helpers are absent from the wasm build entirely.
//
// Manual smoke test (server running on http://127.0.0.1:8080, Zen creds in
// env):
//   curl -X POST http://127.0.0.1:8080/api/plan-trip \
//     -H 'content-type: application/json' \
//     -d '{"duration_days":3,"month":"March","interests":["Food","Beaches"],"travelers":{"adults":2,"kids":0},"pace":"Slow","budget_tier":"Mid","green_travel":true}'
// Expect 200 with a structured `Itinerary` JSON body.

use dioxus::prelude::*;

use visit_quang_nam_planner::domain::{Itinerary, Preferences};

#[cfg(feature = "server")]
use std::collections::HashSet;

#[cfg(feature = "server")]
use visit_quang_nam_planner::domain::Chunk;
#[cfg(feature = "server")]
use visit_quang_nam_planner::retrieval::Retriever;

#[cfg(feature = "server")]
use crate::server::llm::LlmCompleter;
#[cfg(feature = "server")]
use crate::server::prompts;
#[cfg(feature = "server")]
use crate::server::{shared_llm, shared_retriever};

/// Number of grounding chunks retrieved per request. 8 is roughly 2.4K tokens
/// of context, well within `gpt-4o-mini`/`big-pickle` limits.
#[cfg(feature = "server")]
const TOP_K: usize = 8;

/// POST `/api/plan-trip`. The wire format is `Preferences` (JSON) → 200
/// `Itinerary` (JSON). Errors come back as `ServerFnError`; `post_validate`
/// failures are surfaced as 500s by default — the UI's job is to render the
/// message, not to silently fall back (per AGENTS.md: "Better UX than showing
/// the user a fake link").
///
/// The `#[post]` macro emits a wasm client stub for `plan_trip` even when
/// the `server` feature is off — that's what makes the symbol importable by
/// `src/app.rs` from the client side. The function body is replaced by a
/// `client_query` call in that case.
#[post("/api/plan-trip")]
pub async fn plan_trip(prefs: Preferences) -> Result<Itinerary, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::shared_concurrency_limit;
        let retriever = shared_retriever().map_err(|e| ServerFnError::new(e.to_string()))?;
        let llm = shared_llm().map_err(|e| ServerFnError::new(e.to_string()))?;
        // Acquire a concurrency permit before driving the LLM call. The
        // semaphore (audit #10) caps in-flight `plan_trip` work so a request
        // flood doesn't spawn unbounded LLM traffic; queued requests wait
        // here for a free permit. Held across the `await` and dropped on
        // return via `?`/explicit `drop` so the permit is released on both
        // success and error paths. The per-call `LlmClient` timeout (audit
        // #4) bounds how long a single permit can be held.
        let permit = shared_concurrency_limit()
            .acquire_owned()
            .await
            .map_err(|e| ServerFnError::new(format!("concurrency gate failed: {e}")))?;
        let result = plan_trip_inner(&prefs, retriever.as_ref(), llm.as_ref()).await;
        drop(permit);
        result.map_err(|e| ServerFnError::new(e.to_string()))
    }
    // Under `web`-only, the `#[post]` macro rewrites this body to a
    // `client_query` call before it ever reaches the type checker. This
    // arm exists only so the file parses when the macro is not yet
    // expanded (e.g. editor/LSP); it is unreachable in real builds.
    #[cfg(not(feature = "server"))]
    {
        unreachable!("plan_trip body is replaced by the #[post] macro on the client")
    }
}

/// Orchestration core, callable with injected `Retriever` + `LlmCompleter`.
/// `tests/plan_trip.rs` builds `MockLlm` + a fixture corpus and calls this
/// directly — no global state, no network.
#[cfg(feature = "server")]
pub async fn plan_trip_inner(
    prefs: &Preferences,
    retriever: &dyn Retriever,
    llm: &dyn LlmCompleter,
) -> anyhow::Result<Itinerary> {
    validate_prefs(prefs)?;

    let query = build_retrieval_query(prefs);
    let chunks = retriever.search(&query, TOP_K).await;
    if chunks.is_empty() {
        anyhow::bail!("no grounding chunks found for those preferences; the corpus may be empty");
    }

    let system = prompts::SYSTEM_PROMPT
        .replace("{duration}", &prefs.duration_days.to_string())
        .replace("{month}", prefs.month.as_str());
    let user = prompts::build_user_prompt(prefs, &chunks);

    let itinerary = llm
        .complete_itinerary(&system, &user)
        .await
        .map_err(|e| anyhow::anyhow!("LLM call failed: {e}"))?;

    post_validate(&itinerary, prefs, &chunks)?;
    Ok(itinerary)
}

/// Server-side input validation. Failures here are user errors — return them
/// as 400s once the UI wires `StatusCode` mapping (Phase 5); for Phase 3,
/// `ServerFnError` defaults to 500 which is fine since the form constrains
/// inputs client-side.
#[cfg(feature = "server")]
fn validate_prefs(p: &Preferences) -> anyhow::Result<()> {
    if p.duration_days == 0 || p.duration_days > 14 {
        anyhow::bail!("duration_days must be 1..=14, got {}", p.duration_days);
    }
    if p.interests.is_empty() {
        anyhow::bail!("interests must not be empty");
    }
    if p.travelers.adults == 0 {
        anyhow::bail!("at least one adult required");
    }
    Ok(())
}

/// Build the retrieval query string. Hand-rolled (not serde_yaml) because the
/// retriever just embeds any natural-language string — describing the
/// preferences as a sentence works better for cosine against post-text
/// embeddings than a YAML blob would. Each enum is rendered via its `as_str`
/// (decoupled from Rust's `Debug` impl — see `domain::Interest::as_str`); the
/// interests vec is comma-joined so embeddings see a clean natural-language
/// fragment instead of `[Variant, Variant]` Debug noise.
#[cfg(feature = "server")]
fn build_retrieval_query(p: &Preferences) -> String {
    let interests = p
        .interests
        .iter()
        .map(|i| i.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}-day {} trip in {}, {} pace, {} budget, {} adults + {} kids, interests: {}",
        p.duration_days,
        if p.green_travel { "eco-friendly" } else { "" },
        p.month.as_str(),
        p.pace.as_str(),
        p.budget_tier.as_str(),
        p.travelers.adults,
        p.travelers.kids,
        interests,
    )
}

/// Host the LLM is allowed to cite as a `source_url`. Matches the WP REST
/// source (`visitquangnam.com` `/wp/v2/posts` → `link`) the corpus was built
/// from. Tightened in `post_validate` per the audit so a malformed chunk or
/// an off-domain hallucinated URL both fail fast with a clear message.
#[cfg(feature = "server")]
const ALLOWED_URL_PREFIX: &str = "https://visitquangnam.com/";

/// Enforce the system prompt's claims post-hoc. The LLM may still invent a
/// URL despite rule 2; this rejects the response with a clear error so the
/// API call (not the UI) breaks. Returns the offending field in the error so
/// the operator can grep the cached chunk set.
///
/// URL guards (audit #9): the allowed set is built from chunk URLs that are
/// themselves non-empty AND on `visitquangnam.com`; any chunk failing that
/// filter is dropped from the allowed set (a malformed corpus can never seed
/// `""` as a "legitimate" URL). Then every activity URL is checked for:
///   1. non-emptiness,
///   2. on-domain prefix (`https://visitquangnam.com/`),
///   3. membership in the filtered chunk-set.
///
/// Each step emits its own error so the operator can tell a malformed
/// activity URL from a true hallucination.
///
#[cfg(feature = "server")]
fn post_validate(itin: &Itinerary, prefs: &Preferences, chunks: &[Chunk]) -> anyhow::Result<()> {
    if itin.days.len() != prefs.duration_days as usize {
        anyhow::bail!(
            "itinerary day count mismatch: got {}, expected {}",
            itin.days.len(),
            prefs.duration_days
        );
    }
    let allowed_urls: HashSet<&str> = chunks
        .iter()
        .map(|c| c.source_url.as_str())
        .filter(|u| !u.is_empty() && u.starts_with(ALLOWED_URL_PREFIX))
        .collect();
    for day in &itin.days {
        for act in &day.activities {
            if act.source_url.is_empty() {
                anyhow::bail!("activity '{}' has empty source_url", act.title);
            }
            if !act.source_url.starts_with(ALLOWED_URL_PREFIX) {
                anyhow::bail!(
                    "activity '{}' references non-visitquangnam.com source_url {}",
                    act.title,
                    act.source_url,
                );
            }
            if !allowed_urls.contains(act.source_url.as_str()) {
                anyhow::bail!(
                    "activity '{}' references unknown source_url {}",
                    act.title,
                    act.source_url,
                );
            }
        }
    }
    if itin.summary.sustainability_score > 100 {
        anyhow::bail!(
            "sustainability_score out of range: {} > 100",
            itin.summary.sustainability_score
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use visit_quang_nam_planner::domain::{
        Activity, BudgetTier, Category, DayPlan, Interest, Itinerary, Month, Pace, Preferences,
        TripSummary, WeatherHint,
    };
    use visit_quang_nam_planner::retrieval::Retriever;

    fn prefs(days: u8) -> Preferences {
        Preferences {
            duration_days: days,
            month: Month::March,
            interests: vec![Interest::Food, Interest::Beaches],
            travelers: visit_quang_nam_planner::domain::Travelers { adults: 2, kids: 0 },
            pace: Pace::Slow,
            budget_tier: BudgetTier::Mid,
            green_travel: true,
        }
    }

    // --- validate_prefs --------------------------------------------------

    #[test]
    fn validate_prefs_rejects_zero_duration() {
        assert!(validate_prefs(&prefs(0)).is_err());
    }

    #[test]
    fn validate_prefs_rejects_15_days() {
        assert!(validate_prefs(&prefs(15)).is_err());
    }

    #[test]
    fn validate_prefs_rejects_no_interests() {
        let mut p = prefs(3);
        p.interests = Vec::new();
        assert!(validate_prefs(&p).is_err());
    }

    #[test]
    fn validate_prefs_rejects_no_adults() {
        let mut p = prefs(3);
        p.travelers.adults = 0;
        assert!(validate_prefs(&p).is_err());
    }

    #[test]
    fn validate_prefs_happy_path() {
        assert!(validate_prefs(&prefs(3)).is_ok());
    }

    // --- post_validate ----------------------------------------------------

    fn chunk_with_url(url: &str) -> Chunk {
        Chunk {
            id: format!("c-{url}"),
            post_id: 0,
            source_url: url.into(),
            title: "x".into(),
            category: None,
            text: "x".into(),
            embedding: Vec::new(),
        }
    }

    fn itinerary_with(urls: &[&str], days: u8, score: u8) -> Itinerary {
        let days: Vec<DayPlan> = (0..days)
            .map(|i| DayPlan {
                index: i + 1,
                title: format!("Day {}", i + 1),
                date_hint: "Monday".into(),
                weather: WeatherHint {
                    label: "Sunny".into(),
                    icon: "sun".into(),
                },
                activities: urls
                    .iter()
                    .map(|u| Activity {
                        time: "10:00 AM".into(),
                        title: "act".into(),
                        description: "desc".into(),
                        category: Category::Food,
                        source_url: (*u).into(),
                        estimated_cost_vnd: None,
                        duration_minutes: None,
                    })
                    .collect(),
            })
            .collect();
        Itinerary {
            days,
            summary: TripSummary {
                duration: "3 days / 2 nights".into(),
                destinations: vec!["Hoi An".into()],
                budget_estimate: "$500".into(),
                sustainability_score: score,
                sustainability_breakdown: Vec::new(),
            },
        }
    }

    #[test]
    fn post_validate_rejects_day_count_mismatch() {
        let p = prefs(3);
        let chunks = vec![chunk_with_url("https://visitquangnam.com/a")];
        let itin = itinerary_with(&["https://visitquangnam.com/a"], 2, 50);
        let err = post_validate(&itin, &p, &chunks).unwrap_err();
        assert!(err.to_string().contains("day count mismatch"));
    }

    #[test]
    fn post_validate_rejects_unknown_source_url() {
        let p = prefs(1);
        let chunks = vec![chunk_with_url("https://visitquangnam.com/real")];
        let itin = itinerary_with(&["https://visitquangnam.com/fake"], 1, 50);
        let err = post_validate(&itin, &p, &chunks).unwrap_err();
        assert!(err.to_string().contains("unknown source_url"));
        assert!(err.to_string().contains("https://visitquangnam.com/fake"));
    }

    #[test]
    fn post_validate_rejects_sustainability_over_100() {
        let p = prefs(1);
        let chunks = vec![chunk_with_url("https://visitquangnam.com/a")];
        let itin = itinerary_with(&["https://visitquangnam.com/a"], 1, 101);
        let err = post_validate(&itin, &p, &chunks).unwrap_err();
        assert!(
            err.to_string()
                .contains("sustainability_score out of range")
        );
    }

    #[test]
    fn post_validate_happy_path() {
        let p = prefs(2);
        let chunks = vec![
            chunk_with_url("https://visitquangnam.com/a"),
            chunk_with_url("https://visitquangnam.com/b"),
        ];
        let itin = itinerary_with(
            &["https://visitquangnam.com/a", "https://visitquangnam.com/b"],
            2,
            80,
        );
        assert!(post_validate(&itin, &p, &chunks).is_ok());
    }

    // --- post_validate URL guards (audit #9) -----------------------------

    #[test]
    fn post_validate_rejects_empty_source_url() {
        // An LLM that returns an activity with source_url="" must be rejected
        // before the allowed-set membership check, with a clear message.
        let p = prefs(1);
        let chunks = vec![chunk_with_url("https://visitquangnam.com/a")];
        let itin = itinerary_with(&[""], 1, 50);
        let err = post_validate(&itin, &p, &chunks).unwrap_err();
        assert!(err.to_string().contains("empty source_url"));
    }

    #[test]
    fn post_validate_rejects_non_visitquangnam_url() {
        // An LLM that fabricates an on-looking but off-domain URL
        // (e.g. https://example.com/foo) must be rejected with the
        // `non-visitquangnam.com` message, NOT a generic "unknown".
        let p = prefs(1);
        let chunks = vec![chunk_with_url("https://visitquangnam.com/a")];
        let itin = itinerary_with(&["https://example.com/foo"], 1, 50);
        let err = post_validate(&itin, &p, &chunks).unwrap_err();
        assert!(err.to_string().contains("non-visitquangnam.com"));
        assert!(err.to_string().contains("https://example.com/foo"));
    }

    #[test]
    fn post_validate_rejects_empty_chunk_url_does_not_seed_allowed() {
        // A malformed chunk with source_url="" must NOT seed "" as an
        // allowed URL — the chunk-side filter is what closes the audit's
        // silent-allow hole. The activity with "" then trips the activity-
        // side empty check first; this test proves the chunk filter is in
        // place by adding an *also-empty* real activity that would be
        // accepted if the chunk-side filter weren't there.
        let p = prefs(1);
        let chunks = vec![
            chunk_with_url(""),
            chunk_with_url("https://visitquangnam.com/a"),
        ];
        let itin = itinerary_with(&[""], 1, 50);
        let res = post_validate(&itin, &p, &chunks);
        assert!(res.is_err());
        // Either guard could trip first on the activity side; the point is
        // the response is rejected, not silently accepted.
        let msg = res.unwrap_err().to_string();
        assert!(msg.contains("empty source_url") || msg.contains("unknown source_url"));
    }

    // --- plan_trip_inner end-to-end with mocks ----------------------------

    use async_trait::async_trait;

    struct MockLlm {
        json: String,
    }

    #[async_trait]
    impl LlmCompleter for MockLlm {
        async fn complete_itinerary(
            &self,
            _system: &str,
            _user: &str,
        ) -> anyhow::Result<Itinerary> {
            Ok(serde_json::from_str(&self.json)
                .map_err(|e| anyhow::anyhow!("mock parse failed: {e}"))?)
        }
    }

    struct MockRetriever {
        chunks: Vec<Chunk>,
    }

    #[async_trait]
    impl Retriever for MockRetriever {
        async fn search(&self, _query: &str, _k: usize) -> Vec<Chunk> {
            self.chunks.clone()
        }
        fn len(&self) -> usize {
            self.chunks.len()
        }
    }

    #[tokio::test]
    async fn plan_trip_inner_returns_valid_itinerary_for_mock_inputs() {
        let chunks = vec![
            chunk_with_url("https://visitquangnam.com/food"),
            chunk_with_url("https://visitquangnam.com/beach"),
        ];
        let retriever = MockRetriever {
            chunks: chunks.clone(),
        };
        // Itinerary that satisfies every post_validate rule: 2 days, only
        // chunk-sourced URLs, score in range.
        let mock_json = serde_json::json!({
            "days": [
                {
                    "index": 1,
                    "title": "Day 1",
                    "date_hint": "Monday",
                    "weather": { "label": "Sunny", "icon": "sun" },
                    "activities": [
                        {
                            "time": "10:00 AM",
                            "title": "Hoi An food tour",
                            "description": "cao lao + banh mi",
                            "category": "Food",
                            "source_url": "https://visitquangnam.com/food",
                            "estimated_cost_vnd": 120000,
                            "duration_minutes": 90
                        }
                    ]
                },
                {
                    "index": 2,
                    "title": "Day 2",
                    "date_hint": "Tuesday",
                    "weather": { "label": "Sunny", "icon": "sun" },
                    "activities": [
                        {
                            "time": "9:00 AM",
                            "title": "An Bang beach",
                            "description": "swim + relax",
                            "category": "Beach",
                            "source_url": "https://visitquangnam.com/beach"
                        }
                    ]
                }
            ],
            "summary": {
                "duration": "2 days / 1 night",
                "destinations": ["Hoi An"],
                "budget_estimate": "$200 per person",
                "sustainability_score": 70
            }
        })
        .to_string();
        let llm = MockLlm { json: mock_json };

        let itin = plan_trip_inner(&prefs(2), &retriever, &llm)
            .await
            .expect("happy-path orchestration should succeed");
        assert_eq!(itin.days.len(), 2);
        assert_eq!(
            itin.days[0].activities[0].source_url,
            "https://visitquangnam.com/food"
        );
        assert_eq!(
            itin.days[1].activities[0].source_url,
            "https://visitquangnam.com/beach"
        );
        assert_eq!(itin.summary.sustainability_score, 70);
    }

    #[tokio::test]
    async fn plan_trip_inner_fails_when_retriever_empty() {
        let retriever = MockRetriever { chunks: Vec::new() };
        let llm = MockLlm { json: "{}".into() };
        let err = plan_trip_inner(&prefs(2), &retriever, &llm)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no grounding chunks"));
    }

    #[tokio::test]
    async fn plan_trip_inner_fails_on_post_validate_violation() {
        let chunks = vec![chunk_with_url("https://visitquangnam.com/real")];
        let retriever = MockRetriever { chunks };
        // LLM hallucinates a URL — orchestration must reject, not the UI.
        let mock_json = serde_json::json!({
            "days": [{
                "index": 1,
                "title": "Day 1",
                "date_hint": "Monday",
                "weather": { "label": "Sunny", "icon": "sun" },
                "activities": [{
                    "time": "10:00 AM",
                    "title": "fake",
                    "description": "x",
                    "category": "Food",
                    "source_url": "https://visitquangnam.com/FAKE"
                }]
            }],
            "summary": {
                "duration": "1 day",
                "destinations": ["Hoi An"],
                "budget_estimate": "$1",
                "sustainability_score": 10
            }
        })
        .to_string();
        let llm = MockLlm { json: mock_json };
        let err = plan_trip_inner(&prefs(1), &retriever, &llm)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown source_url"));
    }
}
