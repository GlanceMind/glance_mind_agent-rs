# 03 - Scope Split & Module Map

## Decision: NO multi-module split. Single root plan.

Rationale:
- The change is one cohesive concern: add pagination to the TikHub content-fetch path.
- Touched surface is small and tightly coupled: `tikhub.rs` (adapter, primary), `tiktok.rs` (one-line clamp + comment), `Cargo.toml` (dev-dep). All within one ownership boundary.
- Reuses an existing in-repo pattern (`fetch_all_comments`), so no new architecture to fan out.
- Splitting would add manifest/handoff overhead without isolation benefit.

## Module map (informational, all in root plan)
| Surface | File(s) | Change |
|---------|---------|--------|
| Strategy clamp | `src/strategies/tiktok.rs:96,181` | remove total clamp; fix comment |
| Adapter pagination | `src/adapters/tikhub.rs` (149, 201, 221) | new helper + trait + 2 fetchers; rewrite 3 entry points |
| Types (read-only) | `src/tikhub/types.rs` | reuse `SearchData`/`UserVideosData`; per-page `with_count` clamp kept |
| Test infra | `Cargo.toml`, new test module | add `mockito`; mock-HTTP + proptest |

## Reviewer queue (downstream)
- autoplan (CEO/Eng/Design equivalent) — Step 05.
- domain reviewers — Step 06: (a) Rust-correctness/async, (b) Test-Gate (anti-gaming), (c) api-contract/cross-service (does returning >20 break any consumer assuming ≤20?).
