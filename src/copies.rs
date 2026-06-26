//! English copy table for the Visit Quang Nam planner UI (Phase 5).
//!
//! All user-facing string literals live here as `pub static` constants so
//! the components stay clean and Phase 6 (or a future i18n round) can swap
//! this file for a `fluent` bundle without hunting for literals across the
//! component tree. This is a thin look-up table — no logic, no runtime —
//! so the Phase 5 → 6 swap is mechanical.
//!
//! Do NOT introduce `dioxus-i18n` yet (per `plans/phase-5-polish.md`):
//! keeping copy in plain Rust consts keeps every component file untouched
//! by the i18n decision for now.

// ===== Header / chrome =====
pub static HEADER_TITLE: &str = "Your AI-Powered Trip Planner";
pub static HEADER_SUBTITLE: &str =
    "Personalized itineraries crafted with local knowledge. Just tell us what you love.";
pub static MAIN_HEADING: &str = "Your Custom Itinerary";
pub static MAIN_SUBHEAD: &str =
    "Curated by AI from local Visit Quang Nam content, matched to your preferences.";
pub static FOOTER_TEXT: &str = "Visit Quang Nam — Official Tourism Website | AI Trip Planner";
pub static BRAND_PART_ONE: &str = "VISIT";
pub static BRAND_PART_TWO: &str = "QUANG NAM";

// ===== Form =====
pub static MONTH_LABEL: &str = "Month";
pub static INTERESTS_LABEL: &str = "Interests";
pub static TRAVELERS_LABEL: &str = "Travelers";
pub static PACE_LABEL: &str = "Pace";
pub static BUDGET_LABEL: &str = "Budget";
pub static SUSTAINABILITY_TOGGLE_LABEL: &str = "🌱 Green travel";
pub static SUSTAINABILITY_FORM_LABEL: &str = "Sustainability";
pub static PLAN_BUTTON: &str = "✨ Plan My Trip";
pub static PLAN_BUTTON_PENDING: &str = "✨ Planning…";

// ===== Loading state =====
pub static LOADING_TITLE: &str = "Curating your trip…";
pub static LOADING_SUBHEAD: &str = "Calling the planner — this usually takes 10–20 seconds.";
pub static LOADING_HINT_8S: &str = "Taking a little longer than usual — the model is thinking.";
pub static LOADING_TIMEOUT_TITLE: &str = "The planner is taking too long";
pub static LOADING_TIMEOUT_BODY: &str =
    "We stopped waiting after 60 seconds. The model may be busy — try again in a moment.";

// ===== ErrorBox titles + bodies =====
pub static ERROR_NETWORK_TITLE: &str = "Can't reach the planner";
pub static ERROR_NETWORK_BODY: &str = "Your device seems to be offline, or the planner server isn't responding. Check your connection and try again.";
pub static ERROR_NO_KEY_TITLE: &str = "Server config error";
pub static ERROR_NO_KEY_BODY: &str = "The server has no planner API key configured. This is a misconfiguration the operator needs to fix.";
pub static ERROR_NO_GROUNDING_TITLE: &str = "We don't have enough content for that trip";
pub static ERROR_NO_GROUNDING_BODY: &str =
    "Try widening your interests, picking a different month, or shortening the trip.";
pub static ERROR_VALIDATION_TITLE: &str = "Those preferences won't work";
pub static ERROR_PARSE_TITLE: &str = "The planner returned an unexpected response";
pub static ERROR_PARSE_BODY: &str = "The model replied in a shape we couldn't read. Try again — if it keeps happening, the operator can check the logs.";
pub static ERROR_SEND_TITLE: &str = "Couldn't send your preferences";
pub static ERROR_SEND_BODY: &str = "Something went wrong packaging your request. Please try again.";
pub static ERROR_GENERIC_TITLE: &str = "Something went wrong";
pub static TRY_AGAIN: &str = "Try again";

// ===== Error classification (not surfaced to user directly) =====
pub static ERR_503_TITLE: &str = "The planner is taking a break";
pub static ERR_503_BODY: &str =
    "The model backend returned 503 (overloaded). Wait a moment and try again.";

// ===== Itinerary view =====
pub static NO_ACTIVITIES_HINT: &str =
    "No activities planned for this day. Try adjusting your interests.";
pub static MORE_IDEAS_TITLE: &str = "⭐ More ideas from your other days";
pub static TRIP_SUMMARY_TITLE: &str = "📋 Trip Summary";
pub static SUSTAINABILITY_LABEL: &str = "Sustainability score";
pub static SUSTAINABILITY_TOOLTIP_STATIC: &str =
    "Score reflects eco-friendly choices across your itinerary.";
pub static SUSTAINABILITY_TOOLTIP_BREAKDOWN_LABEL: &str = "How this score was built";
pub static READ_MORE: &str = "↳ Read more on visitquangnam.com";

// ===== Summary card row labels =====
pub static SUMMARY_DURATION_LABEL: &str = "Duration";
pub static SUMMARY_DESTINATIONS_LABEL: &str = "Destinations";
pub static SUMMARY_BUDGET_LABEL: &str = "Budget estimate";
