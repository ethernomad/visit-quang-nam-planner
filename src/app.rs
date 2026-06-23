// `App` — Visit Quang Nam AI Trip Planner root component (Phase 4 UI).
//
// Layout per `plans/phase-4-ui.md` and the SVG mockup at
// `/home/jbrown/ai-trip-planner-mockup.svg`:
//   - green-gradient header containing the brand and the `PlannerForm`
//   - main panel whose body depends on the `use_resource(plan_trip)` state:
//       not submitted → empty (form only)
//       pending       → spinner card "✨ Curating your trip…" + day-tab skeleton
//       error         → error card with message + "Try again"
//       success       → `<ItineraryView itinerary=itin />` (day tabs, day card,
//                       trip summary with "More ideas" footer row)
//
// Dioxus 0.7's `UseResourceState` has no `Uninit` variant (only `Pending`,
// `Stopped`, `Paused`, `Ready` — value lives in `Resource::value()`). The
// "not submitted" empty state is therefore discriminated by the
// `Signal<bool> submitted` flag, and the pending/resolved split by
// `Resource::value()` returning `None` (still running) vs `Some(t)`
// (resolved with inner `Option<Result<…>>`).
//
// State shape:
//   `Signal<Preferences>` (parent-owned, passed to `PlannerForm`)
//   `Signal<bool> submitted` (false until first submit; gates the resource)
//   `Signal<u32> submit_nonce` (bumped on every submit so re-submitting the
//       same prefs still re-runs the resource — Dioxus 0.7 otherwise caches
//       identical resource closures)
//   `Signal<usize> active_day` (index into `itin.days`, owned here so day
//       tabs persist across re-renders)
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

use crate::components::itinerary_view::ItineraryView;
use crate::components::planner_form::PlannerForm;
use crate::server::plan_trip::plan_trip;

#[component]
pub fn App() -> Element {
    // Lifting preference state into the App lets the header summary chips
    // and (eventually) the body share the same source of truth. The form
    // mutates via the signal handle; the resource reads via `prefs()`.
    // (No `mut` needed: signals are `Copy`, and we hand `.set`/`.write`
    // through the props.)
    let prefs = use_signal(Preferences::default);
    let submitted = use_signal(|| false);
    let submit_nonce = use_signal(|| 0u32);
    let active_day = use_signal(|| 0usize);

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
                            "VISIT"
                        }
                        span { class: "font-[Georgia] text-xl font-bold tracking-wide text-white",
                            "QUANG NAM"
                        }
                    }

                    h1 { class: "font-[Georgia] text-4xl font-bold text-center mb-2",
                        "Your AI-Powered Trip Planner"
                    }
                    p { class: "text-center text-[#a8d5ba] text-base mb-8",
                        "Personalized itineraries crafted with local knowledge. Just tell us what you love."
                    }

                    // Preference input card hosts the form
                    div { class: "bg-white/10 border border-white/15 rounded-2xl p-5 max-w-3xl mx-auto",
                        PlannerForm {
                            prefs: prefs,
                            submitted: submitted,
                            submit_nonce: submit_nonce,
                        }
                    }
                }
            }

            // ===== Main =====
            main { class: "max-w-5xl mx-auto px-6 py-12",
                h2 { class: "font-[Georgia] text-2xl font-bold text-[#1a4f3a] mb-2",
                    "Your Custom Itinerary"
                }
                p { class: "text-[#6b8a78] text-sm mb-6",
                    "Curated by AI from local Visit Quang Nam content, matched to your preferences."
                }

                { render_state(submitted, itinerary, active_day) }
            }

            // ===== Footer =====
            footer { class: "bg-[#1a4f3a]/5 border-t border-[#1a4f3a]/10 mt-8 py-6 text-center text-xs text-[#8a9e92]",
                "Visit Quang Nam — Official Tourism Website | AI Trip Planner"
            }
        }
    }
}

// Render the body depending on the form + `use_resource` state machine.
// Pulled into a free fn so the four states are co-located and the parent
// `rsx!` stays readable.
fn render_state(
    submitted: Signal<bool>,
    mut itinerary: Resource<Option<Result<Itinerary, ServerFnError>>>,
    active_day: Signal<usize>,
) -> Element {
    // Not submitted yet — nothing below the form.
    if !submitted() {
        return rsx! {};
    }

    // `Resource::value()` is `ReadSignal<Option<T>>` where `T == Option<Result<…>>`.
    // Reading it via the signal's `Fn` impl clones the value out without an
    // unchecked borrow (Dioxus's documented pattern uses `read_unchecked`;
    // we use the safer `Fn` form here since neither reads mutably).
    //
    // Full type being matched: `Option<Option<Result<Itinerary, ServerFnError>>>`.
    //   - `None`                       → resource still pending → spinner
    //   - `Some(None)`                 → resolved with "not submitted" (defensive)
    //   - `Some(Some(Ok(itin)))`       → success
    //   - `Some(Some(Err(e)))`         → error
    let value = itinerary.value()();

    match value {
        None => rsx! {
            div { class: "bg-white rounded-2xl shadow-lg border border-[#e8f0eb] p-8 text-center",
                div { class: "text-4xl mb-3 animate-pulse", "✨" }
                h3 { class: "font-semibold text-lg text-[#1a4f3a] mb-1",
                    "Curating your trip…"
                }
                p { class: "text-sm text-[#6b8a78] max-w-md mx-auto mb-6",
                    "Calling the planner — this usually takes 10–20 seconds."
                }
                div { class: "flex flex-wrap gap-2 justify-center",
                    for i in 0..5 {
                        div {
                            key: "{i}",
                            class: "h-11 w-40 rounded-lg bg-[#e8f0eb] animate-pulse"
                        }
                    }
                }
            }
        },

        Some(None) => rsx! {},

        Some(Some(Err(e))) => {
            let msg = e.to_string();
            rsx! {
                div { class: "bg-white border-2 border-red-300 rounded-2xl p-8 text-center",
                    div { class: "text-4xl mb-3", "⚠️" }
                    h3 { class: "font-semibold text-lg text-red-700 mb-1",
                        "Couldn't plan your trip"
                    }
                    p { class: "text-sm text-[#6b8a78] max-w-md mx-auto mb-4",
                        "{msg}"
                    }
                    button {
                        class: "bg-[#1a4f3a] text-white font-bold text-sm px-6 py-3 rounded-full hover:bg-[#2d7a5e] transition",
                        onclick: move |_| itinerary.restart(),
                        "Try again"
                    }
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
