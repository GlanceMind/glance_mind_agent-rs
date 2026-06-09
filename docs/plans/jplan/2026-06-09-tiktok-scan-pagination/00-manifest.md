# 00 - Manifest (routing source of truth)

## Request
修复 TikTok 任务 `max_scan_count=100` 但爬虫只抓 20 条的 bug。根因：(1) 无翻页循环；(2) `tiktok.rs:96` 的 `.min(20)` 把目标总量削平。决策已锁定：照配置抓满（配额可接受）+ mock HTTP 测试 + 仅 TikTok。

## Plan family status: COMPLETE (ready for implementation)
- current_step: 09-handoff (done)
- next_step: IMPLEMENTATION (execute plans/root.md tasks T0→T6)

## Completed steps
- [x] 00 init + inventory + constraints
- [x] 01 first principles
- [x] 02 ledgers (requirements, anti-gaming)
- [x] 03 scope split (single root plan, no module split)
- [x] 04 root plan draft
- [x] 05 autoplan review (CEO/Eng/Design)
- [x] 06 domain review (rust-async / test-gate / api-contract)
- [x] 07 patch batch 1 + re-review (1×P1 cursor, 1×P1 canary, 2×P2, 3×P3 closed)
- [x] 08 traceability compile — PASS
- [x] 09 handoff

## Artifact map
- Constraints: `constraints/testing-constraints.md`
- Inventory: `00-context-inventory.md`
- First principles: `01-first-principles.md`
- Ledgers: `ledgers/requirements.md`, `ledgers/anti-gaming-test-quality.md`
- Split: `plans/modules/03-split.md`
- **Root plan (the implementation spec): `plans/root.md`**
- Reviews: `reviews/autoplan/05-autoplan-review.md`, `reviews/domain/06-domain-review.md`
- Patches: `patches/07-patch-batch-1.md`
- Traceability: `compile/08-traceability.md`
- Handoff: `handoff.md`

## Module queue: (none — single root plan)
## Reviewer queue: (none — all run)
## Patch queue: (empty — all P0/P1 closed)
## Open blockers: NONE. No requirement ASSUMED.

## Files required to START IMPLEMENTATION
- `plans/root.md` (tasks T0–T6)
- `ledgers/anti-gaming-test-quality.md` (F1–F8 test contract)
- `constraints/testing-constraints.md`
- Code: `src/adapters/tikhub.rs`, `src/strategies/tiktok.rs`, `src/tikhub/{client.rs,types.rs}`, `Cargo.toml`
- Reuse anchor: `src/tikhub/client.rs:447` (`fetch_all_comments`)
