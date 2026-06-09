# Final Implementation Handoff

## Summary
Fix TikTok scan cap-at-20 bug by adding offset/cursor pagination to the TikHub content-fetch path and removing the total-count clamp. Plan family is complete and traceability-passed.

## Read order for the implementer
1. `plans/root.md` — the spec (tasks T0–T6, design with all patches folded in).
2. `ledgers/anti-gaming-test-quality.md` — F1–F8 immutable test contract.
3. `constraints/testing-constraints.md` — RED→GREEN + no-assertion-gaming rules.
Reuse `src/tikhub/client.rs:447` (`fetch_all_comments`) as the loop template.

## Execution order (TDD, 先红后绿)
- T0 add `mockito` dev-dep.
- T1 RED search tests (mockito) → expect `==40` failing as `right==20`; save `compile/red-evidence-search.txt`.
- T2 GREEN: remove `.min(20)` at `tiktok.rs:96`, add `VideoPage`/`VideoPageFetcher`/`paginate_videos`(i64 cursor)/`SearchPageFetcher`, rewrite `TikHubAdapter::search`. Save GREEN evidence.
- T3 RED→GREEN user-video path (max_cursor) incl. SecUserId branch.
- T4 RED(canary)→GREEN proptest F8 (invariants); save `compile/red-evidence-proptest.txt`.
- T5 regression: empty-first-page → empty Vec (campaign-end intact); mid-page-empty → partial Vec.
- T6 verify: `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test` + `rust-verify-change`; CI `cargo mutants --in-diff`.

## Keep / Don't touch
- KEEP `SearchParams::with_count`/`UserVideoParams::with_count` `.min(20)` (correct per-page cap).
- DON'T modify other platforms or comment pagination.
- DON'T weaken any RED assertion to pass — fix production code instead.

## Validation commands
```
cargo test --no-run
cargo test tikhub        # pagination tests
cargo test proptest_pagination_invariants
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
# CI/module-completion:
cargo mutants --in-diff pr.diff -- --all-features
```

## Status
P0/P1: 0 open. P2/P3: folded into design notes (implementation-time). No ASSUMED requirements. Ready to implement.
