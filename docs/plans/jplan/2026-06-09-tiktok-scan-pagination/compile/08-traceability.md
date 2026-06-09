# 08 - Traceability Compile Gate

## Requirement → Task → Test coverage
| Req | Task(s) | Test(s) | Covered |
|-----|---------|---------|---------|
| R1 search to N | T2 | F1,F2,F3 (mockito) | ✅ |
| R2 user path to N | T3 | F6 (mockito) | ✅ |
| R3 page ≤20 | T2 (keep `with_count.min(20)`) | F5,F7 (request assertion) | ✅ |
| R4 termination | T2,T3,T4 | F2,F3,F8 + max_pages guard | ✅ |
| R5 dedup | T2 | F4,F8 | ✅ |
| R6 empty-first-page→end campaign | T5 | T5 regression test + existing orchestrator tests | ✅ |
| R7 quota to config | T2 design (no extra cap) | N/A (product) | ✅ accepted |
| R8 mock HTTP | T0,T1,T3 | mockito harness | ✅ |
| R9 TikTok-only | scope (03-split) | n/a | ✅ |
| R10 ContentId unchanged | (no change) | existing tests stay green | ✅ |

## Anti-gaming gate checks
- [x] Every behavior feature (F1–F8) has RED→GREEN evidence requirement; RED is for the right reason (real failure for F1/F6; documented canary for F8 invariant).
- [x] No task achieves its goal by altering/deleting/weakening assertions, adding `#[ignore]`/skip, or special-casing test input. Stated as binding header in root.md.
- [x] Assertions are exact (`==40/==35/==20`) and declared immutable.
- [x] Core module (pagination accumulation) has a property test (F8) — satisfies §2.5/§6 core-logic property requirement.
- [x] CI mutation gate named: `cargo mutants --in-diff pr.diff` over touched files; missed mutants in loop/termination → red (T6).
- [x] Real external dependency (TikHub HTTP) exercised via mockito on the actual client path — real-dependency gate met at HTTP boundary.
- [x] No LLM/provider behavior change → no provider gate gap.

## Open items
- None ASSUMED. All requirements CONFIRMED. No P0/P1 open.

## Gate result: PASS
