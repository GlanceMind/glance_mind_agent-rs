# Root Plan — TikTok scan pagination fix

Plan family: 2026-06-09-tiktok-scan-pagination
Scope: single cohesive module (TikHub content fetch). No module split needed (see 03 split rationale). One root plan with sequenced TDD tasks.

> Anti-gaming (binding on every task below): tests are a contract. Implementers MUST NOT weaken/delete/comment assertions, add `#[ignore]`/skip, swap expected↔actual, or special-case test input to pass. Failures are fixed by changing PRODUCTION code, or — only if an expectation is genuinely wrong — by a stop + `ASSERTION-CHANGE-JUSTIFIED:<reason>` + re-run RED→GREEN. Every task records RED failure output and GREEN output.

## Design (recommended shape — smallest diff satisfying C1–C6)

Two call sites (`search`, `fetch_user_content`) share identical accumulate/terminate/dedup/truncate logic; only the per-page fetch differs (search=offset/cursor, user=max_cursor). So extract ONE generic async pagination helper and give it two thin real fetchers + one fake fetcher for property tests.

```rust
// new in src/adapters/tikhub.rs (or a sibling module pagination.rs)
// NOTE (F-P1-A): cursor is i64 — search offset is small (cast to u32 in the search
// fetcher), but user-video max_cursor is a large i64. A single i64 cursor type fits both.
struct VideoPage { videos: Vec<AwemeInfo>, next_cursor: Option<i64>, has_more: bool }

#[async_trait]
trait VideoPageFetcher {
    async fn fetch(&self, cursor: i64, count: u32) -> Result<VideoPage, TikHubError>;
}

/// Core accumulation. page_size_cap = 20. target = desired total.
/// Terminates on: target reached / empty page / has_more==false / missing cursor / max_pages guard.
/// Dedupes by aweme_id. Truncates to target. Starts at cursor 0 (F-P3-E: ignore stale offset).
async fn paginate_videos(
    target: usize,
    page_size_cap: u32,
    fetcher: &dyn VideoPageFetcher,
) -> Result<Vec<AwemeInfo>, TikHubError>;
```

- `paginate_videos` mirrors `TikHubClient::fetch_all_comments` (client.rs:447): `loop` + `remaining` + cursor advance + `has_more` check + empty-check + `max_pages = target/page_size_cap + 2` guard.
- **Throttle (F-P2-D):** 500ms sleep ONLY between pages (i.e. right before fetching the next page when continuing) — never after the terminal page.
- **Guard (F-P3-guard):** `warn!` if the `max_pages` guard trips (signals a misbehaving `has_more`).
- **Borrow (F-P3-borrow):** `extract_videos`/`extract_user_videos` return `&AwemeInfo` borrowed from `response`; clone/convert each page's videos into owned `AwemeInfo`/`Content` BEFORE the next loop iteration reassigns `response`.
- Real fetchers wrap `client.search_videos_with_retry` and `client.fetch_user_videos_with_retry`, mapping response `cursor`/`max_cursor` + `has_more`→`VideoPage`. `SearchPageFetcher` casts the i64 cursor→u32 offset (small, safe). `UserVideoPageFetcher` (F-P2-C) carries the id kind (unique_id OR sec_user_id) and uses max_cursor directly.
- Fake fetcher (test-only) holds a `Vec<VideoPage>` → makes F8 property test pure (no HTTP). mockito tests still cover real fetchers end-to-end.
- Fallback if the trait proves awkward: inline two loops, but then F8 property test targets an extracted pure fold helper. Duplication is the worse option; prefer the trait.

## Tasks (sequenced; each is RED→GREEN)

### T0 — Add dev-dependency (enabler, no behavior)
- Add `mockito` to `[dev-dependencies]` in `Cargo.toml`.
- Validation: `cargo test --no-run` compiles. No assertion change.

### T1 — RED: search pagination contract (F1–F5, F7)
- Add `src/adapters/tikhub.rs` test module (or `tests/tikhub_pagination.rs`) using mockito:
  - Mock `/api/v1/tiktok/app/v3/fetch_video_search_result` to return scripted pages keyed by `offset`.
  - F1: page1(20, has_more=1, cursor=20) + page2(20, has_more=0) → `search(count=40)` returns 40.
  - F2: page2 returns 15 has_more=0, target=100 → returns 35.
  - F3: page2 empty → returns 20.
  - F4: pages share an aweme_id → unique count asserted.
  - F5: assert every received request had `count<=20`.
  - F7: target=10 → exactly 1 upstream request, len==10.
- Build `TikHubClient`/`TikHubAdapter` with `base_url` = mockito server URL.
- RUN, capture RED: expect `left == 40, right == 20` etc. (proves current single-fetch cap). Save to `compile/red-evidence-search.txt`.
- Assertions here are the immutable contract.

### T2 — GREEN: implement search pagination
- Remove `.min(20)` total clamp at `src/strategies/tiktok.rs:96` → `config.max_videos.map(|v| v as u32).unwrap_or(10)`.
- Update comment at `src/strategies/tiktok.rs:181` (`max_videos_per_search`) to say "page size, not total".
- Implement `VideoPage`/`VideoPageFetcher`/`paginate_videos` + a `SearchPageFetcher`.
- Rewrite `TikHubAdapter::search` (tikhub.rs:149) to call `paginate_videos(options.count as usize, 20, &SearchPageFetcher{...})`, mapping `SearchData.cursor`/`has_more`. Keep region/sort/publish_time wiring.
- RUN T1 → GREEN. Save GREEN output to `compile/green-evidence-search.txt`.
- Do NOT touch `SearchParams::with_count`'s `.min(20)` (correct per-page cap).

### T3 — RED→GREEN: user-video pagination (F6)
- RED: mockito mock `/api/v1/tiktok/app/v3/fetch_user_post_videos` keyed by `max_cursor`; 2-page script; assert `fetch_user_content(target=40)` returns 40. Capture RED (`right==20`).
- GREEN: add `UserVideoPageFetcher` (wraps `fetch_user_videos_with_retry`, maps `UserVideosData.max_cursor`/`has_more`), route `fetch_user_content` (tikhub.rs:221) AND `fetch_by_keyword` SecUserId branch (tikhub.rs:201) through `paginate_videos`.
- Save RED/GREEN evidence.

### T4 — RED→GREEN: property test (F8, core-logic gate)
- Add `proptest` test driving `paginate_videos` with a `FakeFetcher{pages: Vec<VideoPage>}`.
- Strategy: generate a vec of page sizes (0..=20), a has_more chain, and a target.
- Invariants asserted:
  1. `result.len() == min(target, total_unique_supplied_until_has_more_false_or_empty)`.
  2. result has no duplicate aweme_id.
  3. result never exceeds target.
  4. number of pages consumed ≤ max_pages guard.
- RED-for-the-right-reason (F-P1-B): an invariant test on a correct helper would pass immediately. Produce RED evidence via the sanctioned §8 **canary**: temporarily break ONE line (comment out the dedup `insert` OR the final `truncate`), run F8, capture the invariant-violation failure to `compile/red-evidence-proptest.txt`, then RESTORE the line and re-run to GREEN. This is a canary, NOT a `#[ignore]`/skip, and the assertions are never weakened.
- Save RED (canary) + GREEN evidence.

### T5 — Regression guard: empty-first-page still ends campaign (R6)
- Verify/keep existing behavior: `paginate_videos` returning empty for a truly empty first page → `search` returns `vec![]` → orchestrator.rs:470 path unchanged.
- Add/confirm a test that first-page-empty yields empty Vec (so campaign-end logic still triggers) and mid-page-empty yields the partial Vec (no false campaign end). Existing orchestrator tests must stay green.

### T6 — Verify
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all green.
- Run `rust-verify-change` guard.
- (CI/local) `cargo mutants --in-diff pr.diff -- --all-features` over the touched files: no missed mutants in pagination logic.

## Anti-Gaming Test Quality Ledger
See `ledgers/anti-gaming-test-quality.md` (F1–F8). Bound to T1–T4. No task may achieve its goal by altering assertions or skipping.

## Real-dependency coverage
TikHub HTTP is the real external dependency. mockito serves the actual HTTP path; `TikHubClient`'s real request-building, status handling, JSON parsing, retry, and the new loop all execute (only the network endpoint is faked). This satisfies the "real production-dependency test exercises the actual dependency path" gate at the HTTP boundary.

## Provider/LLM boundary
No LLM behavior changes. AI analysis downstream is unaffected (it consumes whatever videos pagination yields). No new provider gate needed.
