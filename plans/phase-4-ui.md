# Phase 4 — UI

**Goal:** Replace the Phase 0 placeholder `app.rs` with the real
interactive planner UI. Match the SVG mockup at
[`/home/jbrown/ai-trip-planner-mockup.svg`](file:///home/jbrown/ai-trip-planner-mockup.svg)
section-by-section: header + preference form, day tabs, timeline,
trip summary, suggestions sidebar. All powered by Phase 3's `plan_trip`
server function.

**Status:** pending
**Depends on:** Phase 3 (the `plan_trip` server fn must exist with its
`Preferences`/`Itinerary` types so the UI can call it).

## Files to create / edit

- `src/app.rs` — root component, header chrome, layout. Already exists
  from Phase 0; rewrite.
- `src/components/mod.rs` — re-export `planner_form`,
  `itinerary_view`, `trip_summary`, `suggestions`, `preference_chip`,
  `day_card`, `activity_row`.
- `src/components/preference_chip.rs` — small chip pill component.
- `src/components/planner_form.rs` — the input panel: duration slider,
  month dropdown, interest chips, travelers + pace + budget + green
  checkbox, "Plan My Trip" button. Calls `plan_trip`.
- `src/components/itinerary_view.rs` — day tabs + active-day render +
  day card header.
- `src/components/day_card.rs` — renders a `DayPlan` (header, weather,
  dot-and-line timeline of activities).
- `src/components/activity_row.rs` — renders an `Activity` (time, title,
  description, category tag, price, source link).
- `src/components/trip_summary.rs` — `TripSummary` rendering (duration,
  destinations, budget, sustainability bar).
- `src/components/suggestions.rs` — "AI Recommended For You" sidebar.
- `src/app.css` (Tailwind CSS-only, **no** new asset files) — add a
  few custom utility classes to `input.css` if the Tailwind defaults
  aren't ergonomic for the card-shadow look. Keep custom CSS to a
  minimum.
- `input.css` — leave the `@import "tailwindcss";` line; only append
  `@layer components { ... }` blocks if needed.

## Suggested component tree

```
App                                  (src/app.rs)
├── document::Stylesheet { tailwind }
├── Header                           (in src/app.rs)
│   └── PreferenceChips              (decorative)
└── PlannerPanel                     (state owner)
    ├── PlannerForm                  (src/components/planner_form.rs)
    │   ├── DurationSlider
    │   ├── MonthSelect
    │   └── InterestChips ...
    └── match resource.state()
        ├── Loading → Spinner
        ├── Error   → ErrorBox
        └── Ready(itin) →
            ├── DayTabs                 (in itinerary_view.rs)
            ├── DayCard                 (src/components/day_card.rs)
            │   └── ActivityRow × N    (src/components/activity_row.rs)
            ├── TripSummary             (src/components/trip_summary.rs)
            └── Suggestions             (src/components/suggestions.rs)
```

## Reactive wiring

`PlannerForm` lifts the `Preferences` into a `Signal<Preferences>` in
the parent (or via `use_context_provider`). The "Plan My Trip" button
calls a closure that runs `plan_trip(prefs)`.

```rust
// pseudo-pattern (Dioxus 0.7, no cx/Scope/use_state)
let mut prefs = use_signal(Preferences::default);
let itinerary = use_resource(move || {
    let prefs = prefs();
    async move {
        if prefs.submitted {
            Some(crate::server::plan_trip::plan_trip(prefs).await.ok())
        } else {
            None
        }
    }
});
```

> `Preferences` needs a `Default` for this shape to compile. Give it
> one: 5 days, March, [Food, Beaches], 2 adults / 0 kids, Moderate,
> Mid, green_travel=true. This is the same input the SVG mockup shows.

For the "submitted" flag, either add a `submitted: bool` field to
`Preferences` OR — cleaner — use a separate `Signal<bool>` for
`submitted` that the form's on-submit handler flips to `true`, and the
`use_resource` re-runs when either `prefs` or `submitted` reads change.

## Server-function call site

```rust
use crate::server::plan_trip::plan_trip;
use crate::domain::{Itinerary, Preferences};

async fn fetch_itinerary(prefs: Preferences) -> Result<Itinerary, ServerFnError> {
    plan_trip(prefs).await
}
```

Dioxus serialises the call automatically (see `AGENTS.md`). Don't write
`reqwest` calls by hand.

## Loading / error / empty states

- **Unsubmitted** (`resource.state() == Uninit`): show the form only,
  with no itinerary panel below.
- **Loading** (`resource.state() == Pending`): show a skeleton of 5
  blank day-tab pills + a shimmer placeholder where the timeline and
  summary will go.
- **Error** (`resource.state() == Ready(Err(...))`): show an error card
  with the message and a "Try again" button that re-runs the resource.
  Dioxus's `use_resource` exposes the error via `resource()`.
- **Success** (`resource.state() == Ready(Ok(itin))`): render the full
  result.

Do NOT show a generic "Loading…" string — the server call can take
10–20 seconds (LLM round trip + retrieval). Use a branded spinner
("✨ Curating your trip…") so the user knows progress is happening.

## Day tabs + active day

```rust
let mut active_day = use_signal(|| 0usize); // index into itin.days
```

The tabs render as a row above the active day card. Clicking a tab
writes `active_day`. The active day card renders
`itin.days[active_day()]`.

Make tabs keyboard-accessible — `tabindex` + arrow-key navigation is
a nice-to-have for Phase 5, not a Phase 4 acceptance criterion.

## Activity row

Each `Activity` becomes:

```
┌──────────────────────────────────────────────────────────────┐
│ 🟢 10:00 AM  Morning coffee at Cong Caphe                    │
│              Kick off your trip with a coconut coffee ...    │
│              [🍴 Food & Drink] [⏱ 45 min] [💵 ~50,000 VND]   │
│              ↳ Read more on visitquangnam.com               │
└──────────────────────────────────────────────────────────────┘
```

The "Read more" link is an actual `<a href={activity.source_url}>`
targeting the real Visit Quang Nam article. Open in a new tab:
`target: "_blank", rel: "noopener noreferrer"`.

Category → icon/color mapping (use a `match` in the component):

| Category | Icon | Tailwind class |
|----------|------|----------------|
| Food | 🍴 | `bg-[#fce4ec] text-[#c62828]` |
| Nature | 🌿 | `bg-[#e8f5e9] text-[#2e7d32]` |
| Culture | 🏛 | `bg-[#f3e5f5] text-[#6a1b9a]` |
| Beach | 🏖 | `bg-[#e3f2fd] text-[#1565c0]` |
| Wellness | 🧘 | `bg-[#fff3e0] text-[#e65100]` |

## Trip summary + sustainability score

Render `TripSummary` as a green-bordered card. The
`sustainability_score` is a horizontal bar:

```rust
div { class: "w-full bg-[#e6efe9] rounded-full h-3",
    div {
        class: "bg-[#2d7a5e] h-3 rounded-full",
        style: "width: {summary.sustainability_score}%"
    }
}
```

Show the numeric score next to the bar ("82/100").

## Suggestions sidebar

Phase 3 returns days + summary; "AI Recommended For You" was originally
in the mockup as a separate sidebar. If you keep that as a Phase 4
deliverable, hijack acticity rows from `itin.days` that are not on the
active day and present them flat. OR — simpler — drop the sidebar from
Phase 4 and surface the same content as a "More ideas" footer row below
TripSummary. Pick one and document the choice in the file's top
comment.

If you drop the sidebar entirely, also delete its entry from the
project layout in `AGENTS.md` to keep the docs in sync.

## Tasks

1. Read `/home/jbrown/ai-trip-planner-mockup.svg` end-to-end before
   starting. It is the visual spec.
2. Add `Default` for `Preferences` in `domain/mod.rs`.
3. Rewrite `src/app.rs` to the layout shown above (replace Phase 0
   placeholder).
4. Implement the components one at a time, smallest first:
   `preference_chip` → `activity_row` → `day_card` → `trip_summary` →
   `planner_form` → `itinerary_view` → (optional) `suggestions`.
5. Wire the `use_resource` to `plan_trip`.
6. Implement the loading / error / empty states.
7. Verify the Tailwind classes you used are actually in
   `assets/tailwind.css` (re-run the Tailwind watcher to regenerate —
   the watcher at `/home/jbrown/visit-quang-nam-planner/node_modules/.bin/tailwindcss
   --cwd /home/jbrown/visit-quang-nam-planner -i .../input.css -o
   .../assets/tailwind.css --watch` will pick them up automatically
   thanks to `@source "./src/**/*.{rs,html,css}"` in input.css).
8. Click through every state manually in the dev server. Take a
   screenshot for the PR.

## Acceptance criteria

- [ ] `dx serve --web` launches and the form renders with the Visit
      Quang Nam green palette and typography from the SVG.
- [ ] Submitting the default Preferences (5 days, March, Food+Beaches,
      2 adults, Moderate, Mid, green=true) returns and renders a
      multi-day itinerary with day tabs.
- [ ] Clicking a day tab swaps the active day card.
- [ ] Each activity has a "Read more" link to its `source_url`.
- [ ] TripSummary renders the sustainability bar with the correct %.
- [ ] Loading state ("✨ Curating your trip…") shows during the server
      call.
- [ ] Error state shows the message from `ServerFnError` and a "Try
      again" button.
- [ ] All four CI gates pass. Add at least one component-level unit
      test (e.g., `activity_row` renders `source_url` unchanged; do
      this by extracting the row's render into a `fn_activity_row(act)
      -> LazyNodes` or equivalent and invoking from the test). If
      component-render tests are fiddly in Dioxus 0.7, fall back to a
      pure helper function (`format_price(vnd) -> String`) with unit
      tests.
- [ ] The wasm client build compiles without server deps:
      `cargo check --target wasm32-unknown-unknown
      --no-default-features --features web`.

## Notes for the agent

- `dioxus::launch(App)` lives in `main.rs`. Do not change it.
- The `document::Stylesheet { href: asset!("/assets/tailwind.css") }`
  must stay mounted at the App root.
- Dioxus 0.7 `use_resource` returns a `Resource<T>`; reading it via
  `resource()` returns `Option<T>` (Some when ready). For typed state
  inspection (loading vs error vs ready), use `resource.state()` which
  yields `UseResourceState::Pending | Uninit | Ready(Ok/Err)`.
- Do NOT inside a component body call `.await` directly — always go
  through `use_resource` or `use_server_future` so Dioxus owns the
  future and re-renders on completion.
- Don't introduce a CSS framework other than Tailwind. The SVG was
  designed against Tailwind utility classes; keep it that way for
  maintainability.
- If `gpt-4o-mini` occasionally returns activities with empty
  `source_url`, handle it gracefully in the row (suppress the link
  rather than crash). Don't paper over — file a Phase 3 bug if you
  see this repeatedly.
- Match the SVG's exact colors: `#1a4f3a`, `#2d7a5e`, `#a8d5ba` for
  greens; `#fce4ec`, `#fff3e0`, `#e3f2fd`, `#f3e5f5` for category
  accents. Don't guess tones.