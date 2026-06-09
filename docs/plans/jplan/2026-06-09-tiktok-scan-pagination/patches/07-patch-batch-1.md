# 07 - Patch Batch 1 + Re-review

## Findings closed (patched into plans/root.md)
| ID | Sev | Resolution | Location in root.md |
|----|-----|-----------|---------------------|
| F-P1-A | P1 | Helper cursor type → `i64`; SearchPageFetcher casts to u32 offset | Design block (VideoPage/trait) |
| F-P1-B | P1 | F8 RED via documented §8 canary (break dedup/truncate, capture, restore) | Task T4 |
| F-P2-C | P2 | UserVideoPageFetcher carries id kind (unique_id/sec_user_id) | Design block bullet |
| F-P2-D | P2 | 500ms throttle only between pages | Design block bullet |
| F-P3-E | P3 | Ignore stale `SearchOptions.offset`; start cursor 0 | Helper doc comment |
| F-P3-borrow | P3 | Convert per page before reassigning `response` | Design block bullet |
| F-P3-guard | P3 | `warn!` when max_pages guard trips | Design block bullet |

## Re-review verdict
- All P1 closed and reflected in the plan text. No new P0/P1 introduced by the patches.
- P2/P3 folded into design notes (implementation-time, low risk).
- Product trade-off (5× quota/budget burn) explicitly accepted by user; runaway guard present. No open blocker.

Status: ready for traceability compile (Step 08).
