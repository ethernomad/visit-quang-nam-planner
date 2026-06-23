// `ErrorBox` — dedicated component for surfacing `ServerFnError` cleanly
// (Phase 5 §"Error state"). Promotes the Phase 4 inline error block into a
// reusable component that classifies the error into a user-readable
// (title, body) pair, so a panic-y anyhow string doesn't make every server
// hiccup look like a bug.
//
// `classify_error` is a pure free fn (no DOM) so it has unit tests for each
// arm; the component itself is a thin renderer around it.
//
// Written against the real `dioxus_fullstack_core::ServerFnError` enum that
// `dioxus::prelude::ServerFnError` aliases (Dioxus 0.7.9):
//
//     pub enum ServerFnError {
//         ServerError { message: String, code: u16, details: Option<Value> },
//         Request(RequestError),
//         StreamError(String),
//         Registration(String),
//         UnsupportedRequestMethod(String),
//         MiddlewareError(String),
//         Deserialization(String),
//         Serialization(String),
//         Args(String),
//         MissingArg(String),
//         Response(String),
//     }
//
// `ServerError` is a struct variant (matches the Phase 5 plan doc's sketch);
// the `code` field lets us branch on 503 (model overloaded) vs 500 (generic
// server error) without sniffing the message string for HTTP semantics.

use dioxus::prelude::*;

use crate::copies;

#[derive(Props, Clone, PartialEq)]
pub struct ErrorBoxProps {
    error: ServerFnError,
    on_retry: Callback<()>,
}

#[component]
pub fn ErrorBox(props: ErrorBoxProps) -> Element {
    let (title, body) = classify_error(&props.error);

    rsx! {
        div { class: "bg-white border-2 border-red-300 rounded-2xl p-8 text-center shadow-lg",
            div { class: "text-4xl mb-3", "⚠️" }
            h3 { class: "font-semibold text-lg text-red-700 mb-1", "{title}" }
            p { class: "text-sm text-[#6b8a78] max-w-md mx-auto mb-4", "{body}" }
            button {
                class: "bg-[#1a4f3a] text-white font-bold text-sm px-6 py-3 rounded-full hover:bg-[#2d7a5e] transition shadow-md",
                onclick: move |_| props.on_retry.call(()),
                "{copies::TRY_AGAIN}"
            }
        }
    }
}

/// Map a `ServerFnError` to a user-readable `(title, body)` pair.
///
/// Categorisation rules (highest-leverage UX change per the Phase 5 plan):
///   - `Request(_)`                     → network/transport (offline, server down)
///   - `ServerError { code: 503, .. }`   → model overloaded (try again)
///   - `ServerError { message, .. }`     → sniff `message`:
///       * mentions `OPENAI_API_KEY`/`OPENCODE_API_KEY` → server config
///       * mentions `no grounding chunks`               → not enough content
///       * mentions validation strings                  → user input
///       * else                                        → generic, surface msg
///   - `Deserialization(_)`              → "unexpected response" (don't leak JSON)
///   - `Serialization`/`Args`/`MissingArg` → "couldn't send preferences"
///   - `StreamError`/`Response`/`Registration`/`UnsupportedRequestMethod`/
///     `MiddlewareError`                 → generic
pub fn classify_error(e: &ServerFnError) -> (&'static str, String) {
    match e {
        ServerFnError::Request(_) => (
            copies::ERROR_NETWORK_TITLE,
            copies::ERROR_NETWORK_BODY.to_string(),
        ),

        ServerFnError::ServerError { message, code, .. } if *code == 503 => (
            "The planner is taking a break",
            "The model backend returned 503 (overloaded). Wait a moment and try again.".to_string(),
        ),

        ServerFnError::ServerError { message, .. } => {
            let m = message.as_str();
            if m.contains("OPENAI_API_KEY") || m.contains("OPENCODE_API_KEY") {
                (
                    copies::ERROR_NO_KEY_TITLE,
                    copies::ERROR_NO_KEY_BODY.to_string(),
                )
            } else if m.contains("no grounding chunks") {
                (
                    copies::ERROR_NO_GROUNDING_TITLE,
                    copies::ERROR_NO_GROUNDING_BODY.to_string(),
                )
            } else if is_validation_message(m) {
                (
                    copies::ERROR_VALIDATION_TITLE,
                    format!("{m} Please adjust your selections and try again."),
                )
            } else {
                (copies::ERROR_GENERIC_TITLE, m.to_string())
            }
        }

        ServerFnError::Deserialization(_) => (
            copies::ERROR_PARSE_TITLE,
            copies::ERROR_PARSE_BODY.to_string(),
        ),

        ServerFnError::Serialization(_) | ServerFnError::Args(_) | ServerFnError::MissingArg(_) => {
            (
                copies::ERROR_SEND_TITLE,
                copies::ERROR_SEND_BODY.to_string(),
            )
        }

        // StreamError, Response, Registration, UnsupportedRequestMethod,
        // MiddlewareError — all operator-side infra problems; surface the
        // raw string for diagnostics but under a generic title.
        _ => (copies::ERROR_GENERIC_TITLE, e.to_string()),
    }
}

/// Heuristic: does this server error message look like a `validate_prefs`
/// rejection? The validator emits strings like `"duration_days must be
/// 1..=14"`, `"interests must not be empty"`, `"at least one adult required"`.
fn is_validation_message(m: &str) -> bool {
    m.contains("duration_days")
        || m.contains("interests must not be empty")
        || m.contains("at least one adult")
        || m.contains("must be 1..=14")
}

#[cfg(test)]
mod tests {
    use super::*;
    // `RequestError` is re-exported through `dioxus::fullstack` (which
    // re-exports `dioxus_fullstack_core::*`), not through `dioxus::prelude`.
    use dioxus::fullstack::RequestError;

    fn server_err(msg: &str, code: u16) -> ServerFnError {
        ServerFnError::ServerError {
            message: msg.into(),
            code,
            details: None,
        }
    }

    #[test]
    fn classifies_network_request_error() {
        let e = ServerFnError::Request(RequestError::Request("fetch failed".into()));
        let (title, body) = classify_error(&e);
        assert_eq!(title, copies::ERROR_NETWORK_TITLE);
        assert_eq!(body, copies::ERROR_NETWORK_BODY);
    }

    #[test]
    fn classifies_503_as_overloaded() {
        let e = server_err("upstream 503", 503);
        let (title, body) = classify_error(&e);
        assert_eq!(title, "The planner is taking a break");
        assert!(body.contains("503"));
    }

    #[test]
    fn classifies_missing_openai_key() {
        let e = server_err("retriever init failed: OPENAI_API_KEY not set", 500);
        let (title, body) = classify_error(&e);
        assert_eq!(title, copies::ERROR_NO_KEY_TITLE);
        assert_eq!(body, copies::ERROR_NO_KEY_BODY);
    }

    #[test]
    fn classifies_missing_opencode_key() {
        let e = server_err(
            "LLM init failed: OPENCODE_API_KEY not set — cannot run planner LLM",
            500,
        );
        let (title, _) = classify_error(&e);
        assert_eq!(title, copies::ERROR_NO_KEY_TITLE);
    }

    #[test]
    fn classifies_no_grounding_chunks() {
        let e = server_err(
            "no grounding chunks found for those preferences; the corpus may be empty",
            500,
        );
        let (title, body) = classify_error(&e);
        assert_eq!(title, copies::ERROR_NO_GROUNDING_TITLE);
        assert_eq!(body, copies::ERROR_NO_GROUNDING_BODY);
    }

    #[test]
    fn classifies_validation_duration() {
        let e = server_err("duration_days must be 1..=14, got 0", 400);
        let (title, body) = classify_error(&e);
        assert_eq!(title, copies::ERROR_VALIDATION_TITLE);
        assert!(body.contains("duration_days"));
        assert!(body.contains("adjust your selections"));
    }

    #[test]
    fn classifies_validation_interests() {
        let e = server_err("interests must not be empty", 400);
        let (title, _) = classify_error(&e);
        assert_eq!(title, copies::ERROR_VALIDATION_TITLE);
    }

    #[test]
    fn classifies_validation_no_adults() {
        let e = server_err("at least one adult required", 400);
        let (title, _) = classify_error(&e);
        assert_eq!(title, copies::ERROR_VALIDATION_TITLE);
    }

    #[test]
    fn classifies_deserialization_as_parse_error() {
        let e = ServerFnError::Deserialization("unexpected token at line 1".into());
        let (title, body) = classify_error(&e);
        assert_eq!(title, copies::ERROR_PARSE_TITLE);
        assert_eq!(body, copies::ERROR_PARSE_BODY);
        // Don't leak raw parse details to the user.
        assert!(!body.contains("unexpected token"));
    }

    #[test]
    fn classifies_args_as_send_error() {
        let e = ServerFnError::Args("missing field".into());
        let (title, body) = classify_error(&e);
        assert_eq!(title, copies::ERROR_SEND_TITLE);
        assert_eq!(body, copies::ERROR_SEND_BODY);
    }

    #[test]
    fn classifies_serialization_as_send_error() {
        let e = ServerFnError::Serialization("serde error".into());
        let (title, _) = classify_error(&e);
        assert_eq!(title, copies::ERROR_SEND_TITLE);
    }

    #[test]
    fn classifies_generic_server_error_falls_through() {
        let e = server_err("LLM call failed: rate limited", 500);
        let (title, body) = classify_error(&e);
        assert_eq!(title, copies::ERROR_GENERIC_TITLE);
        assert!(body.contains("rate limited"));
    }

    #[test]
    fn classifies_registration_as_generic() {
        let e = ServerFnError::Registration("poisoned lock".into());
        let (title, _) = classify_error(&e);
        assert_eq!(title, copies::ERROR_GENERIC_TITLE);
    }

    #[test]
    fn classifies_response_as_generic() {
        let e = ServerFnError::Response("axum body error".into());
        let (title, _) = classify_error(&e);
        assert_eq!(title, copies::ERROR_GENERIC_TITLE);
    }

    #[test]
    fn classifies_stream_error_as_generic() {
        let e = ServerFnError::StreamError("body read failed".into());
        let (title, _) = classify_error(&e);
        assert_eq!(title, copies::ERROR_GENERIC_TITLE);
    }

    #[test]
    fn classifies_middleware_error_as_generic() {
        let e = ServerFnError::MiddlewareError("auth failed".into());
        let (title, _) = classify_error(&e);
        assert_eq!(title, copies::ERROR_GENERIC_TITLE);
    }
}
