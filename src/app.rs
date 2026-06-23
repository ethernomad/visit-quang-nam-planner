// `App` — Visit Quang Nam AI Trip Planner root component (Phase 5 polish).
//
// Layout per `plans/phase-4-ui.md` and the SVG mockup:
//   - green-gradient header containing the brand and the `PlannerForm`
//   - main panel whose body depends on the `use_resource(plan_trip)` state:
//       not submitted → empty (form only)
//       pending       → shimmer skeleton (day-tab pills + day-card +
//                       timeline placeholder) + 8s "taking longer" hint
//       error         → `<ErrorBox>` (Phase 5: promoted from inline block)
//       success       → `<ItineraryView itinerary=itin />`
//
// Phase 5 resilience additions:
//   - **Shimmer skeleton**: the pending state renders structured placeholder
//     cards using the `.shimmer` Tailwind component class, matching the
//     day-card + timeline layout, instead of a generic spinner.
//   - **8-second hint**: a `use_resource` timer fires after 8s of pending
//     and flips `show_slow_hint` so a "Taking a little longer than usual —
//     the model is thinking." message fades in. Non-blocking: the resource
//     keeps running.
//   - **60-second client cap**: a second timer aborts the wait at 60s by
//     surfacing a typed `ServerFnError::ServerError` (timeout) routed through
//     `ErrorBox`. The server (Phase 3 `LlmClient`) should also enforce a
//     reqwest timeout — the client cap is the backstop, not the only guard.
//     Implementation: the cap future sets `timed_out` to `true`; the
//     render loop treats `timed_out && still pending` as an error state.
//
// State shape:
//   `Signal<Preferences>` (parent-owned, passed to `PlannerForm`)
//   `Signal<bool> submitted` (false until first submit; gates the resource)
//   `Signal<u32> submit_nonce` (bumped on every submit so re-submitting the
//       same prefs still re-runs the resource — Dioxus 0.7 otherwise caches
//       identical resource closures)
//   `Signal<usize> active_day` (index into `itin.days`, owned here so day
//       tabs persist across re-renders)
//   `Signal<bool> show_slow_hint` (8s hint visibility)
//   `Signal<bool> timed_out` (60s hard cap fired)
//
// Manual smoke test for the success state (server running, creds exported):
//   curl -X POST http://127.0.0.1:8080/api/plan-trip \
//     -H 'content-type: application/json' \
//     -d '{"duration_days":3,"month":"March","interests":["Food","Beaches"],"travelers":{"adults":2,"kids":0},"pace":"Slow","budget_tier":"Mid","green_travel":true}'
// Expect 200 with `Itinerary` JSON; the UI renders it after the same payload
// is submitted via the form. Empty / loading / error states are exercised
// without creds: empty is the initial render, loading is the immediate
// post-submit frame, error resolves when the call fails (e.g. no
// `OPENCODE_API_KEY` in env).

use dioxus::prelude::*;

use visit_quang_nam_planner::domain::{Itinerary, Preferences};

use crate::components::error_box::ErrorBox;
use crate::components::itinerary_view::ItineraryView;
use crate::components::planner_form::PlannerForm;
use crate::copies;
use crate::server::plan_trip::plan_trip;

/// 8 seconds — show the "taking longer" hint.
const SLOW_HINT_MS: u32 = 8_000;
/// 60 seconds — give up waiting and surface a timeout error.
const HARD_CAP_MS: u32 = 60_000;

#[component]
pub fn App() -> Element {
    // Lifting preference state into the App lets the header summary chips
    // and (eventually) the body share the same source of truth. The form
    // mutates via the signal handle; the resource reads via `prefs()`.
    let prefs = use_signal(Preferences::default);
    let submitted = use_signal(|| false);
    let submit_nonce = use_signal(|| 0u32);
    let active_day = use_signal(|| 0usize);

    // Phase 5 resilience flags.
    let mut show_slow_hint = use_signal(|| false);
    let mut timed_out = use_signal(|| false);

    // `use_resource` re-runs when its closure's signal dependencies change.
    // Reading `submitted()`, `submit_nonce()`, and `prefs()` inside the
    // closure subscribes the resource to all three. `prefs()` is read
    // *before* the async block so Dioxus owns the future without leaking
    // the signal across an await boundary.
    let itinerary = use_resource(move || {
        let submitted = submitted();
        let _nonce = submit_nonce();
        let prefs = prefs();
        async move {
            if submitted {
                Some(plan_trip(prefs).await)
            } else {
                None
            }
        }
    });

    // Reset the resilience flags whenever a fresh request starts. We watch
    // `submit_nonce` for this: every submit bumps it, which re-triggers the
    // effect, which clears the flags so the new pending state starts clean.
    // The `use_resource`'s own restart (from `ErrorBox`'s "Try again")
    // also bumps pending; we piggyback on `submitted` + nonce as the
    // "new request" signal.
    use_effect(move || {
        let _ = submitted();
        let _ = submit_nonce();
        show_slow_hint.set(false);
        timed_out.set(false);
    });

    // 8-second "taking longer" hint timer. Re-arms on every submit. Fires
    // only while the resource is still pending — if it resolved before 8s,
    // the hint stays hidden because `show_slow_hint` was reset by the
    // effect above and the resource is no longer pending.
    {
        let mut show_slow_hint = show_slow_hint;
        use_resource(move || {
            let nonce = submit_nonce();
            let is_submitted = submitted();
            async move {
                if is_submitted {
                    gloo_timers::future::TimeoutFuture::new(SLOW_HINT_MS).await;
                    // Only flip the hint on if we're still waiting on the
                    // same request (nonce unchanged) and still pending.
                    if submit_nonce() == nonce && itinerary.pending() {
                        show_slow_hint.set(true);
                    }
                }
            }
        });
    }

    // 60-second hard cap. Same re-arm pattern. When it fires while still
    // pending, set `timed_out` so `render_state` can surface a typed
    // timeout error via `ErrorBox`.
    {
        let mut timed_out = timed_out;
        use_resource(move || {
            let nonce = submit_nonce();
            let is_submitted = submitted();
            async move {
                if is_submitted {
                    gloo_timers::future::TimeoutFuture::new(HARD_CAP_MS).await;
                    if submit_nonce() == nonce && itinerary.pending() {
                        timed_out.set(true);
                    }
                }
            }
        });
    }

    rsx! {
        document::Stylesheet {
            href: asset!("/assets/tailwind.css"),
        }
        document::Link {
            rel: "preconnect",
            href: "https://fonts.googleapis.com",
        }
        document::Link {
            rel: "preconnect",
            href: "https://fonts.gstatic.com",
            crossorigin: "anonymous",
        }
        document::Stylesheet {
            href: "https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Georgia&display=swap",
        }

        div { class: "min-h-screen bg-[#f0f4f0] text-[#1a2a1e] font-sans",
            // ===== Header =====
            header { class: "relative bg-gradient-to-br from-[#1a4f3a] to-[#2d7a5e] text-white overflow-hidden",
                div { class: "absolute -top-20 right-0 w-96 h-96 rounded-full bg-white/5" }
                div { class: "absolute top-20 right-20 w-72 h-72 rounded-full bg-white/5" }
                div { class: "absolute -top-10 left-10 w-72 h-72 rounded-full bg-white/5" }

                div { class: "relative max-w-5xl mx-auto px-6 pt-8 pb-16",
                    // Brand
                    div { class: "flex items-center gap-2 mb-10",
                        span { class: "font-[Georgia] text-xl font-bold tracking-widest text-[#a8d5ba]",
                            "{copies::BRAND_PART_ONE}"
                        }
                        span { class: "font-[Georgia] text-xl font-bold tracking-wide text-white",
                            "{copies::BRAND_PART_TWO}"
                        }
                    }

                    h1 { class: "font-[Georgia] text-4xl font-bold text-center mb-2",
                        "{copies::HEADER_TITLE}"
                    }
                    p { class: "text-center text-[#a8d5ba] text-base mb-8",
                        "{copies::HEADER_SUBTITLE}"
                    }

                    // Preference input card hosts the form
                    div { class: "bg-white/10 border border-white/15 rounded-2xl p-5 max-w-3xl mx-auto",
                        PlannerForm {
                            prefs: prefs,
                            submitted: submitted,
                            submit_nonce: submit_nonce,
                            pending: itinerary.pending() && submitted(),
                        }
                    }
                }
            }

            // ===== Main =====
            main { class: "max-w-5xl mx-auto px-6 py-12",
                h2 { class: "font-[Georgia] text-2xl font-bold text-[#1a4f3a] mb-2",
                    "{copies::MAIN_HEADING}"
                }
                p { class: "text-[#6b8a78] text-sm mb-6",
                    "{copies::MAIN_SUBHEAD}"
                }

                { render_state(submitted, itinerary, active_day, show_slow_hint, timed_out) }
            }

            // ===== Footer =====
            footer { class: "bg-[#1a4f3a]/5 border-t border-[#1a4f3a]/10 mt-8 py-6 text-center text-xs text-[#8a9e92]",
                "{copies::FOOTER_TEXT}"
            }
        }
    }
}

// Render the body depending on the form + `use_resource` state machine.
// Pulled into a free fn so the states are co-located and the parent
// `rsx!` stays readable.
fn render_state(
    submitted: Signal<bool>,
    mut itinerary: Resource<Option<Result<Itinerary, ServerFnError>>>,
    active_day: Signal<usize>,
    show_slow_hint: Signal<bool>,
    timed_out: Signal<bool>,
) -> Element {
    // Not submitted yet — nothing below the form.
    if !submitted() {
        return rsx! {};
    }

    // Phase 5: 60s hard cap fired while still pending → surface a typed
    // timeout error through `ErrorBox` so the user gets a clear message
    // and a "Try again" button (which re-runs the resource).
    if timed_out() && itinerary.pending() {
        // Phase 5: 60s hard cap fired while still pending → surface a
        // typed timeout error through `ErrorBox`. The message carries both
        // the title and body so `classify_error` routes it as a generic
        // `ServerError` (code 504 — Gateway Timeout).
        let timeout_err = ServerFnError::ServerError {
            message: format!(
                "{} — {}",
                copies::LOADING_TIMEOUT_TITLE,
                copies::LOADING_TIMEOUT_BODY
            ),
            code: 504,
            details: None,
        };
        return rsx! {
            ErrorBox {
                error: timeout_err,
                on_retry: Callback::new(move |_| itinerary.restart()),
            }
        };
    }

    // `Resource::value()` is `ReadSignal<Option<T>>` where `T == Option<Result<…>>`.
    // Full type: `Option<Option<Result<Itinerary, ServerFnError>>>`.
    //   - `None`                       → resource still pending → skeleton
    //   - `Some(None)`                 → resolved with "not submitted" (defensive)
    //   - `Some(Some(Ok(itin)))`       → success
    //   - `Some(Some(Err(e)))`         → error
    let value = itinerary.value()();

    match value {
        None => rsx! {
            // Phase 5: structured shimmer skeleton replacing the generic
            // spinner. Mirrors the day-card + timeline layout so the
            // transition to the real content is visually continuous.
            div { class: "space-y-6",
                // Title block so the user sees *something* immediately
                div { class: "bg-white rounded-2xl shadow-lg border border-[#e8f0eb] p-8 text-center",
                    div { class: "text-4xl mb-3 animate-pulse", "✨" }
                    h3 { class: "font-semibold text-lg text-[#1a4f3a] mb-1",
                        "{copies::LOADING_TITLE}"
                    }
                    p { class: "text-sm text-[#6b8a78] max-w-md mx-auto",
                        "{copies::LOADING_SUBHEAD}"
                    }
                    if show_slow_hint() {
                        p { class: "text-sm text-[#b8860b] max-w-md mx-auto mt-4 animate-pulse",
                            "⏳ {copies::LOADING_HINT_8S}"
                        }
                    }
                }

                // Day-tab skeleton row
                div { class: "flex flex-wrap gap-2",
                    for i in 0..5 {
                        div {
                            key: "{i}",
                            class: "h-10 w-32 rounded-lg shimmer"
                        }
                    }
                }

                // Day-card skeleton: header strip + 3 timeline rows
                div { class: "bg-white rounded-2xl shadow-lg border border-[#e8f0eb] overflow-hidden",
                    div { class: "h-16 shimmer" }
                    div { class: "px-6 py-5 space-y-6",
                        for i in 0..3 {
                            div {
                                key: "{i}",
                                class: "flex gap-3",
                                div { class: "w-4 h-4 rounded-full bg-[#e8f0eb] shrink-0" }
                                div { class: "flex-1 space-y-2",
                                    div { class: "h-3 w-20 rounded bg-[#e8f0eb]" }
                                    div { class: "h-4 w-3/4 rounded bg-[#e8f0eb]" }
                                    div { class: "h-3 w-full rounded bg-[#e8f0eb]" }
                                }
                            }
                        }
                    }
                }

                // Summary skeleton
                div { class: "bg-white rounded-2xl shadow-lg border border-[#e8f0eb] p-6 space-y-3",
                    for i in 0..3 {
                        div {
                            key: "{i}",
                            class: "flex items-center gap-3",
                            div { class: "w-4 h-4 rounded bg-[#e8f0eb] shrink-0" }
                            div { class: "h-3 w-36 rounded bg-[#e8f0eb]" }
                            div { class: "h-3 flex-1 rounded bg-[#e8f0eb]" }
                        }
                    }
                }
            }
        },

        Some(None) => rsx! {},

        Some(Some(Err(e))) => {
            rsx! {
                ErrorBox {
                    error: e,
                    on_retry: Callback::new(move |_| itinerary.restart()),
                }
            }
        }

        Some(Some(Ok(itin))) => rsx! {
            ItineraryView {
                itinerary: itin,
                active_day: active_day,
            }
        },
    }
}
