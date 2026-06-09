# 05 - Autoplan Review (CEO / Eng / Design equivalent)

## CEO / Product lens
- Goal (fetch to config) is the right product behavior; confirmed by user. ✅
- **Surfaced trade-off (not a blocker):** fetching 100 instead of 20 depletes campaign budget / quota ~5× faster per task and increases downstream comment-fetch + AI-analysis spend proportionally. User accepted ("照配置抓满"). Recorded; no action beyond the `max_pages` runaway guard. → keep.
- No scope expansion warranted; TikTok-only is the right wedge.

## Eng lens (architecture / correctness)
- F-P1-A **Cursor type mismatch (real bug in proposed design).** Helper used `next_cursor: Option<u64>`. But search `offset` is `u32` (small item offset) while user-video `max_cursor` is `i64` (large, timestamp-like; `UserVideoParams.max_cursor: i64`). A single `u64` cursor type is wrong for the user path and risks truncation casting. **Severity P1.** Fix: helper cursor type = `i64`; `SearchPageFetcher` casts to `u32` for `offset` (safe, small); `UserVideoPageFetcher` uses `i64` directly.
- F-P2-C **User fetcher must support both id kinds.** `fetch_user_content` uses `by_unique_id`, the SecUserId branch uses `by_sec_user_id`. The `UserVideoPageFetcher` must carry which constructor to use. **Severity P2.**
- F-P2-D **Throttle placement.** Sleep 500ms only *between* pages (when continuing), never after the terminal page, to avoid needless latency. **Severity P2.**
- F-P3-E **`SearchOptions.offset` is unused by the strategy.** Helper should start from cursor 0 (ignore stale offset) for determinism. **Severity P3.**
- Sequential pagination (not parallel) is correct — each page needs the prior cursor. ✅
- Reusing `SearchOptions.count` as the target (no clamp at `entities.rs:836`) is the minimal correct diff. ✅

## Design lens
- N/A (no UI). Logging: keep structured `info!` per page like `fetch_all_comments` (page, total, has_more) for observability. Minor add, fold into T2/T3.

## Verdict
Plan is sound and minimal. 1×P1, 2×P2, 1×P3 to patch before implementation. Proceed to domain review then patch batch.
