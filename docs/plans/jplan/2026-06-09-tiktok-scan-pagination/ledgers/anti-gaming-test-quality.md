# Anti-Gaming Test Quality Ledger

Every feature below carries: RED→GREEN evidence requirement, assertion-immutability rule, mutation-test expectation, and property/invariant coverage. Implementers MUST NOT modify assertions to pass.

| Feat | Behavior | RED test (must fail first, for the right reason) | Expected RED failure | GREEN cmd | Mutation/Property expectation |
|------|----------|--------------------------------------------------|----------------------|-----------|-------------------------------|
| F1 | Search paginates to target across pages | mock HTTP: page1=20 items `has_more=1 cursor=20`, page2=20 `has_more=0`; assert `search(target=40).len()==40` | `assertion failed: left==40 right==20` (current single-fetch) | `cargo test tikhub_search_paginates_to_target` | mutation on loop break/`+=`/`>=` must be killed; assertion is exact `==40`, immutable |
| F2 | Stops at `has_more=0` without overshoot | page1=20 `has_more=1`, page2=15 `has_more=0`; target=100 → assert `==35` | `left==35 right==20` | same test module | killed by mutants flipping termination |
| F3 | Empty mid page terminates | page1=20 `has_more=1`, page2=0 → assert `==20` | n/a (RED is the loop missing) | same | invariant: never returns < first page when more pages empty |
| F4 | Cross-page dedup | page1 & page2 share an `aweme_id` → assert unique count | duplicate present in naive concat | same | invariant: output `aweme_id`s are unique |
| F5 | Per-request cap respected | assert each captured mock request has `count<=20` | n/a | mockito request assertion | mutant raising page size caught |
| F6 | User-video path paginates (max_cursor) | analogous 2-page mock on `fetch_user_post_videos` | `left==40 right==20` | `cargo test tikhub_user_videos_paginates` | termination mutants killed |
| F7 | Target<=20 still single page | target=10 → exactly 1 request, `len()==10` | n/a | mockito request count assertion | mutant removing early-stop caught |
| F8 | Property: for any (page_sizes, has_more chain, target), output len == min(target, total_supplied) and is duplicate-free | `proptest` over generated page sequences against an in-memory fake transport | varied | `cargo test proptest_pagination_invariants` | core-logic property gate |

Rules:
- 任何任务不得以「改/删断言、加 `#[ignore]`」作为达成手段。
- 若发现某 RED 期望本身写错，须停下、带 `ASSERTION-CHANGE-JUSTIFIED:` 修正并重走 RED→GREEN。
- 核心模块 = 翻页累积逻辑：必须有 F8 属性测试；CI `cargo mutants --in-diff` 须无漏杀（missed→红）。
