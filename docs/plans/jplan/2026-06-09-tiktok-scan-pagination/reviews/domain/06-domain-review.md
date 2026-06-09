# 06 - Domain Reviewer Passes

## (a) Rust async / correctness reviewer
- Confirms F-P1-A (cursor i64). Also: `extract_videos`/`extract_user_videos` return borrowed `&AwemeInfo`; the loop must `convert_content` (clone into domain `Content`) before the borrowed `response` is dropped/reassigned next iteration — design already converts per page, OK. Note explicitly in T2 to avoid borrow-after-move.
- `#[async_trait]` already used in this file (`ContentGateway`/`CommentGateway`); the new `VideoPageFetcher` trait should use it too for object safety (`&dyn VideoPageFetcher`). ✅ consistent.
- Termination guard `max_pages = target/page_size_cap + 2` — fine; add `log::warn!` if guard trips (signals API misbehavior). **P3.**

## (b) Test-Gate reviewer (anti-gaming)
- F-P1-B **Property test (F8) RED-for-the-right-reason.** An invariant test written after the helper is correct would pass on first run — violating 先红后绿. Resolution: produce RED evidence via a **canary** — temporarily break one line (e.g. comment out the dedup `insert`/`truncate`) to show F8 fails with the expected invariant violation, capture output to `compile/red-evidence-proptest.txt`, then restore. This is the sanctioned §8 canary technique, NOT a skip. **Severity P1 (process clarity).** Patch the plan to state this explicitly.
- F1/F6 RED evidence is naturally a real failure (current code returns 20) — good, that is RED-for-the-right-reason with no canary needed.
- Confirms assertions are exact equality (`==40`, `==35`) and immutable. No task may pass by editing them. ✅
- Mockito request-count/`count<=20` assertions (F5/F7) guard against an implementation that over-pages or raises page size — good mutation resistance.

## (c) api-contract / cross-service reviewer
- Return type unchanged (`Vec<Content>`); no DTO/response-shape change → `api-contract-guard` not triggered. ✅
- Consumers: `orchestrator::process_keyword` processes all returned contents via `buffer_unordered(max_concurrent_videos)` — bounded concurrency, so returning 100 does not blow up concurrency; memory holds ≤100 `Content` structs transiently — acceptable. ✅
- No consumer assumes "≤20 results" as a contract (searched; the 20 was only the implicit single-page artifact). ✅
- `cross-service-guard`: no shared contract / Redis task schema change. ✅

## Findings rollup
| ID | Sev | Summary |
|----|-----|---------|
| F-P1-A | P1 | Helper cursor must be i64 (user max_cursor), search casts to u32 |
| F-P1-B | P1 | F8 property RED via documented canary, then restore |
| F-P2-C | P2 | User fetcher supports unique_id + sec_user_id |
| F-P2-D | P2 | Throttle only between pages |
| F-P3-E | P3 | Ignore stale `SearchOptions.offset`; start cursor 0 |
| F-P3-borrow | P3 | Convert per page before reusing response (borrow) |
| F-P3-guard | P3 | warn! when max_pages guard trips |
