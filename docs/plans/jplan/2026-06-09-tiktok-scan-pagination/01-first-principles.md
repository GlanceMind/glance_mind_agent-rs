# 01 - First Principles Reduction

## The irreducible problem
A scan task says "fetch up to N items" (N=`max_scan_count`, e.g. 100). The only data source (TikHub) returns **at most 20 items per HTTP request** and exposes a continuation token (`cursor`/`max_cursor` + `has_more`). Therefore: **fetching N>20 items is inherently a multi-request accumulation problem.** Any single-request implementation can never exceed 20. This is not a tuning bug; it is a missing loop.

## What must be true for correctness (necessary conditions)
- **C1 Accumulation**: To return up to N items, the system MUST issue ⌈N/20⌉ requests, advancing the continuation token each time.
- **C2 Per-request cap is real**: 20/request is an external API constraint, not ours to raise. Keep it.
- **C3 Target ≠ page size**: The "100" must survive end-to-end as a *total target*; clamping it to a *page size* destroys the requirement. The current `.min(20)` conflates the two.
- **C4 Termination**: The loop MUST stop on ANY of: reached target N; empty page; `has_more != 1`; missing continuation token (else it spins). A defensive max-page bound guards against a misbehaving API claiming `has_more=1` forever.
- **C5 No duplicates**: Pages may overlap; the same `aweme_id` must not be processed twice (wastes downstream comment-fetch + AI spend, and inflates counts).
- **C6 Preserve "empty search → end campaign"**: Today, zero results ends the campaign (`orchestrator.rs:470`). After the change, only a **truly empty first page** is "zero results"; a mid-stream empty page just terminates pagination and returns what was collected.

## What is NOT essential (avoid scope creep / overengineering)
- Reworking `SearchOptions` into a new "target vs page" type pair — unnecessary. `SearchOptions.count` already does not clamp (`entities.rs:836`); we reuse it as the target and page internally inside the adapter. (karpathy: smallest diff that satisfies C1–C6.)
- Touching other platforms' adapters — out of scope per user.
- Changing the comment pagination — already correct.
- Parallel page fetching — pages are inherently sequential (each needs the prior cursor); keep sequential like `fetch_all_comments`.

## Decision inputs already resolved (no remaining ASSUMED)
- Quota: 照配置抓满 (≈⌈N/20⌉ requests acceptable). → CONFIRMED by user.
- Test strategy: mock HTTP server (mockito) so the loop + parsing actually run. → CONFIRMED by user.
- Platform scope: TikTok only. → CONFIRMED by user.

## Reuse anchor
`TikHubClient::fetch_all_comments` (client.rs:447) is an existing, correct, idiomatic pagination loop in the same module (loop + `remaining` + cursor + `has_more` + empty-check + 500ms throttle). The video/user-video loops MUST mirror its shape rather than invent a new one.
