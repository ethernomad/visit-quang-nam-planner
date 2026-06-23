use dioxus::prelude::*;

/// Root component for the Visit Quang Nam AI Trip Planner.
///
/// This is a Phase 0 scaffold — renders the page chrome (header, brand,
/// preference input placeholder) using Tailwind, with the green palette
/// from the Visit Quang Nam brand. Real planner logic lands in Phase 3.
#[component]
pub fn App() -> Element {
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

                div { class: "relative max-w-5xl mx-auto px-6 pt-8 pb-16",
                    // Brand
                    div { class: "flex items-center gap-2 mb-10",
                        span { class: "font-[Georgia] text-xl font-bold tracking-widest text-[#a8d5ba]", "VISIT" }
                        span { class: "font-[Georgia] text-xl font-bold tracking-wide text-white",
                            "QUANG NAM" }
                    }

                    h1 { class: "font-[Georgia] text-4xl font-bold text-center mb-2",
                        "Your AI-Powered Trip Planner" }
                    p { class: "text-center text-[#a8d5ba] text-base mb-8",
                        "Personalized itineraries crafted with local knowledge. Just tell us what you love."
                    }

                    // Preference input card (placeholder form for Phase 0)
                    div { class: "bg-white/10 border border-white/15 rounded-2xl p-4 max-w-3xl mx-auto",
                        div { class: "flex gap-3",
                            div { class: "flex-1 bg-white rounded-full px-5 py-3 text-[#889b8e] text-sm",
                                "\"5 days in March, love food & beaches, traveling with kids...\"" }
                            button { class: "bg-[#a8d5ba] text-[#1a4f3a] font-bold text-sm px-6 py-3 rounded-full hover:bg-[#9bc8ab] transition",
                                "✨ Plan My Trip" }
                        }
                        div { class: "flex gap-2 mt-3 flex-wrap",
                            PreferenceChip { label: "5 days" }
                            PreferenceChip { label: "March" }
                            PreferenceChip { label: "Foodie" }
                            PreferenceChip { label: "Beaches" }
                            PreferenceChip { label: "Family" }
                        }
                    }
                }
            }

            // ===== Body placeholder =====
            main { class: "max-w-5xl mx-auto px-6 py-12",
                h2 { class: "font-[Georgia] text-2xl font-bold text-[#1a4f3a] mb-2",
                    "Your Custom Itinerary" }
                p { class: "text-[#6b8a78] text-sm mb-6",
                    "Curated for family travel in March, with a focus on food, beaches, and slow experiences."
                }

                div { class: "bg-white rounded-2xl shadow-lg border border-[#e8f0eb] p-8 text-center",
                    div { class: "text-5xl mb-3", "🚧" }
                    h3 { class: "font-semibold text-lg text-[#1a4f3a] mb-1",
                        "Planner coming online" }
                    p { class: "text-sm text-[#6b8a78] max-w-md mx-auto",
                        "Phase 0 scaffold is up. The LLM orchestration (Phase 3), itinerary rendering, and RAG retrieval will plug into this layout over the next steps."
                    }
                }
            }

            // ===== Footer =====
            footer { class: "bg-[#1a4f3a]/5 border-t border-[#1a4f3a]/10 mt-8 py-6 text-center text-xs text-[#8a9e92]",
                "Visit Quang Nam — Official Tourism Website | AI Trip Planner (scaffold)" }
        }
    }
}

#[component]
fn PreferenceChip(label: String) -> Element {
    rsx! {
        span { class: "bg-white/20 text-white text-xs px-3 py-1 rounded-full", "{label}" }
    }
}
