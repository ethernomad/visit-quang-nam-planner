# Trip Generation Performance Optimization Roadmap

## Current pipeline (per request)

| Step | Time |
|---|---|
| 1. Load 9MB `corpus.json` into memory | 0 (once, eager at boot) |
| 2. Embed query via OpenAI `text-embedding-3-small` | 0.5–2s |
| 3. Cosine top-K across 273 chunks | <1ms |
| 4. Call Zen `mimo-v2.5-free` (non-streaming) | 10–30s |
| **Total** | **~12–32s** |

The LLM call dominates. Steps 2+4 are serial — retrieval must finish before the prompt can be built.

---

## Completed

**1. Reduce `MAX_TOKENS` from 16,384 to 8,192** (`src/server/llm.rs:59`)
The LLM generates up to 8K output tokens instead of 16K — enough for a 14-day itinerary with 5 activities per day, but cuts generation time by preventing verbose filler. Still overridable via `OPENCODE_MAX_TOKENS`.

**2. Reduce `TOP_K` from 8 to 5** (`src/server/plan_trip.rs:55`)
Fewer chunks means a shorter prompt (~1.5K vs ~2.4K tokens of grounding) and less work for the LLM to process and cite. Negligible retrieval quality loss.

**3. Warm up retriever + LLM on server startup** (`src/main.rs`)
Singletons are now eagerly initialized at boot time instead of lazy-loaded on the first request. Eliminates the 100–500ms corpus.json load + parse from the first user's critical path. Failures are cached in the existing `OnceLock` contract.

---

## Opportunities (ordered by ROI)

### Tier 1: High impact, low effort

**4. Switch corpus to a binary wire format**
`src/retrieval/in_memory.rs:43-44` — The 9MB JSON file (422K lines, one float per line) is the slowest possible serialization. Switching to `postcard` or `rkyv` (zero-copy deserialization) would cut load time from ~100-200ms to ~5ms. Alternatively, `simd-json` deserialization would speed up the existing JSON path.

---

### Tier 2: Medium impact, medium effort

**5. Cache query embeddings**
Identical preferences produce identical `build_retrieval_query` output. A small `LruCache<String, Vec<f32>>` (32 entries) in `InMemoryRetriever` would eliminate the OpenAI embedding round-trip (0.5–2s) for repeat/similar queries. Even a hit rate of 20% helps under load.

**6. Streamline the system prompt**
`src/server/prompts.rs:27-100` — The prompt is verbose (~2,000 chars). Removing the full TypeScript schema example (lines 63-95) and replacing it with a terse field list would cut ~1K input tokens without weakening the LLM contract (the schema is already enforced by `serde` deserialization + `post_validate`).

**7. Increase LLM concurrency limit**
`src/server/mod.rs:89` — The current default is 4 concurrent LLM calls. Under load, request 5+ queues. Bumping this to 8–16 (if the Zen endpoint allows) reduces queue wait — but doesn't help individual request latency.

---

### Tier 3: High impact, high effort

**8. Stream the LLM response via SSE**
This is the single biggest perceived speed win. Instead of waiting 10–30s for the full JSON, stream tokens as they arrive from Zen:
- Server: Use `stream: true` in the OpenAI chat completion call, stream chunks to an SSE endpoint.
- Client: Parse partial JSON tokens, render `Itinerary` incrementally (e.g., Day 1 appears after 5s, Day 2 after 10s).
- Dioxus 0.7: Would need an `EventSource`-based resource or a `use_coroutine` pattern. The axum side is straightforward (`Sse::new(stream)`).

**9. Switch to a faster model (or add fallback)**
`mimo-v2.5-free` is free but not the fastest. Letting the operator set `OPENCODE_MODEL` to `gpt-4o-mini` or `gpt-4o` (which are often faster for JSON generation) via env already works. Add a configurable fallback: if Zen times out, retry on real OpenAI.

**10. Pre-compute common trip types**
The most common combinations (e.g., "3-day food tour", "5-day family beach trip") could be pre-generated offline and cached. The query embedding + LLM path would only be used for novel preference combinations.

---

### What NOT to optimize yet

- **Cosine over 273 chunks** — already sub-millisecond. Only matters at 10K+ chunks.
- **Parallelizing retrieval + LLM** — the LLM prompt depends on retrieval results, so they can't overlap.
- **pgvector** — adds operational complexity for a 273-chunk corpus that fits in 9MB. The in-memory retriever is fine for MVP scale.
