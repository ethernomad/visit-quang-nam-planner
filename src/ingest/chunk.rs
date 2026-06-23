// Paragraph-aware chunker for the RAG corpus.
//
// Splits cleaned article text into ~256–300-token slices so each chunk
// is small enough to pack into a Phase 3 retrieval prompt while still
// carrying topical coherence. The first chunk of each post is prefixed
// with `# {title}\n\n` so the embedding captures title context.
//
// Token counting uses `tiktoken_rs::cl100k_base()` (the tokenizer that
// matches OpenAI's `text-embedding-3-small`). On the first call we
// lazily init the BPE and cache it for the process. `tiktoken-rs`
// downloads `cl100k_base.tiktoken` from `openaipublic.blob.core.windows.net`
// on first use; if that download fails (offline build, no network
// egress), we fall back to a `chars / 4` heuristic so the bin still runs.
// The fallback is logged once via `tracing::warn!`. Chunk boundaries
// shift slightly under the heuristic but embedding quality is unaffected
// — only chunk sizes move.
//
// This module has no server-only deps and compiles cleanly to wasm, so
// its unit tests run on the host target used by `cargo test --all`.

#[cfg(feature = "server")]
mod token_count {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tiktoken_rs::CoreBPE;

    static BPE: OnceLock<Option<CoreBPE>> = OnceLock::new();
    static FALLBACK_WARNED: AtomicBool = AtomicBool::new(false);

    fn get() -> Option<&'static CoreBPE> {
        BPE.get_or_init(|| match tiktoken_rs::cl100k_base() {
            Ok(bpe) => Some(bpe),
            Err(e) => {
                if !FALLBACK_WARNED.swap(true, Ordering::SeqCst) {
                    tracing::warn!(
                        error = %e,
                        "tiktoken cl100k_base init failed (offline?); falling back to chars/4 \
                         heuristic for chunk sizing"
                    );
                }
                None
            }
        })
        .as_ref()
    }

    pub fn token_count(s: &str) -> usize {
        match get() {
            Some(bpe) => bpe.encode_with_special_tokens(s).len(),
            None => s.chars().count().div_ceil(4),
        }
    }
}

#[cfg(not(feature = "server"))]
mod token_count {
    /// Wasm client build never chunks anything — chunking is a bin-only
    /// operation — but `chunk.rs` is part of the library surface so it
    /// must compile without the `server` feature. Skip tiktoken entirely
    /// and use the same chars/4 heuristic the server fallback uses.
    pub fn token_count(s: &str) -> usize {
        s.chars().count().div_ceil(4)
    }
}

use token_count::token_count as token_count_pub;

/// Soft close: once a chunk reaches this many tokens we flush it and
/// start the next one. Picked to keep retrieval slices well under the
/// 8192-token input limit of `text-embedding-3-small` while leaving
/// room for the title prefix and prompt packing in Phase 3.
const SOFT_CLOSE: usize = 256;

/// Hard cap: no chunk is allowed to exceed this. Paragraphs that on
/// their own breach the cap are pre-split on sentence boundaries before
/// accumulation begins.
const MAX_CHUNK: usize = 300;

/// Split `text` into ``(chunk_index, chunk_text)`` pairs ready for
/// embedding. `title` (when non-empty) is prepended as `# {title}\n\n`
/// to the first chunk so its embedding carries title context.
///
/// Strategy:
/// 1. Split on blank lines (``\n\n``) into paragraphs.
/// 2. If a single paragraph exceeds `MAX_CHUNK` tokens, split it on
///    sentence boundaries (`.` `。` `!` `?`) until each leaf fits.
/// 3. Greedily accumulate paragraph units into a current chunk. Close
///    the chunk as soon as it reaches `SOFT_CLOSE` tokens, or before
///    adding a unit that would push it past `MAX_CHUNK`.
/// 4. Prefix chunk 0 with the title.
///
/// Empty or whitespace-only input returns `vec![]`. No chunk is ever
/// empty.
pub fn chunk(text: &str, title: &str) -> Vec<(usize, String)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // 1. Build leaf units. A paragraph that fits is one unit; a
    //    paragraph that exceeds MAX_CHUNK is split into sentences.
    let mut units: Vec<String> = Vec::new();
    for paragraph in trimmed.split("\n\n") {
        let p = paragraph.trim();
        if p.is_empty() {
            continue;
        }
        if token_count_pub(p) > MAX_CHUNK {
            for sentence in split_sentences(p) {
                let s = sentence.trim();
                if !s.is_empty() {
                    units.push(s.to_string());
                }
            }
        } else {
            units.push(p.to_string());
        }
    }

    // 2. Greedy accumulate.
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    for unit in units {
        let candidate = if cur.is_empty() {
            unit.clone()
        } else {
            format!("{cur}\n\n{unit}")
        };
        if !cur.is_empty() && token_count_pub(&candidate) > MAX_CHUNK {
            chunks.push(std::mem::take(&mut cur));
            cur = unit;
        } else {
            cur = candidate;
        }
        if token_count_pub(&cur) >= SOFT_CLOSE {
            chunks.push(std::mem::take(&mut cur));
        }
    }
    if !cur.trim().is_empty() {
        chunks.push(cur);
    }

    // 3. Title prefix on chunk 0.
    let title = title.trim();
    let mut out = Vec::with_capacity(chunks.len());
    for (idx, mut body) in chunks.into_iter().enumerate() {
        if idx == 0 && !title.is_empty() {
            body = format!("# {title}\n\n{body}");
        }
        out.push((idx, body));
    }
    out
}

/// Split a paragraph into sentences on ASCII/.CJK terminators
/// (`.`, `。`, `!`, `?`), keeping the terminator in each piece and
/// consuming trailing whitespace before the next piece. UTF-8 safe via
/// `char_indices`. If the input contains no sentence terminators the
/// whole paragraph is returned as a single element so the caller still
/// makes progress.
fn split_sentences(p: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut chars = p.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if matches!(c, '.' | '。' | '!' | '?') {
            let mut end = i + c.len_utf8();
            // consume following whitespace so it doesn't end up at the
            // start of the next sentence
            while let Some(&(j, wc)) = chars.peek() {
                if wc.is_whitespace() {
                    end = j + wc.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let slice = &p[start..end];
            if !slice.trim().is_empty() {
                out.push(slice.to_string());
            }
            start = end;
        }
    }
    if start < p.len() {
        let rest = &p[start..];
        if !rest.trim().is_empty() {
            out.push(rest.to_string());
        }
    }
    if out.is_empty() {
        vec![p.to_string()]
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        assert!(chunk("", "title").is_empty());
        assert!(chunk("   \n\n  \n\n ", "title").is_empty());
    }

    #[test]
    fn title_prefixed_on_first_chunk_only() {
        let text = "Hoi An is a UNESCO-listed town. The old port thrives on tailors and lanterns.";
        let chunks = chunk(text, "Hoi An Guide");
        assert!(!chunks.is_empty());
        assert!(
            chunks[0].1.starts_with("# Hoi An Guide\n\n"),
            "chunk 0 should start with the title header, got: {:?}",
            chunks[0].1
        );
        for (_, body) in chunks.iter().skip(1) {
            assert!(
                !body.starts_with('#'),
                "only chunk 0 should carry the title prefix"
            );
        }
    }

    #[test]
    fn no_chunk_exceeds_max_and_none_empty() {
        // A long multi-paragraph article with at least one oversize
        // paragraph to exercise the sentence-splitter path.
        let long_para = "This is one sentence. ".repeat(60); // ~300 tokens-ish
        let text = format!("Short opener about Quang Nam.\n\n{long_para}\n\nShort closing line.");
        let chunks = chunk(&text, "Quang Nam");
        assert!(
            chunks.len() >= 2,
            "expected multiple chunks, got {}",
            chunks.len()
        );
        for (_, body) in &chunks {
            assert!(!body.trim().is_empty(), "no empty chunks");
            let n = token_count_pub(body);
            assert!(n <= MAX_CHUNK, "chunk is {n} tokens, max is {MAX_CHUNK}");
        }
    }

    #[test]
    fn sentence_splitter_keeps_terminators() {
        let pieces = split_sentences("First. Second! Third?Fourth.");
        assert_eq!(pieces, vec!["First. ", "Second! ", "Third?", "Fourth.",]);
    }

    #[test]
    fn sentence_splitter_handles_cjk_period() {
        // `。` is followed directly by `W` with no whitespace, so no
        // trailing space is consumed into the first sentence (matching
        // the ASCII `Third?Fourth.` case). The final sentence terminator
        // is followed by a space which is consumed.
        let pieces = split_sentences("你好。World. ");
        assert_eq!(pieces, vec!["你好。", "World. "]);
    }

    #[test]
    fn sentence_splitter_no_terminators_returns_whole() {
        let pieces = split_sentences("just one block of text");
        assert_eq!(pieces, vec!["just one block of text"]);
    }
}
