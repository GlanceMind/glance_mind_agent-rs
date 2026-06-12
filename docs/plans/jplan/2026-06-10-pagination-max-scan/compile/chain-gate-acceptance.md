# Chain-Gate Acceptance — pagination-max-scan (root §4)

> Final jplan acceptance record for the campaign-269 under-scan fix.
> Evaluated 2026-06-12. Verifies root.md §4 criteria ①–⑧ across the integrated core
> and the per-module PRs. Authority: `plans/root.md` §1 (interface freeze), §2 (invariants), §4 (gate).

## 0. Status summary

| Surface | State | Evidence |
|---|---|---|
| Integrated core (M1 + M2) on `main` | **ACCEPTED** | T-040 green, lib 322/0, R-012/§3 clean (below) |
| M3 tiktok (PR #10) | PR green, **pending merge** | CI: Rust Test Gates / Harness / E2E all SUCCESS; R-012 clean |
| M4 reddit/twitter (PR #11) | PR green, **pending merge** | CI all SUCCESS; R-012 clean |
| M5 instagram (PR #12) | PR green, **pending merge** | CI all SUCCESS; R-012 clean; V1=branch-B |
| M6 scheduler once-guard (worker PR #70) | **ACCEPTED** — mutation gate fixed, CI green | see §3 |
| Docs (PR #6) | open | plan family |

**Merge is the user's decision** (no autonomous merge — orphan-merge red line). The gate
below verifies each surface is *ready*; integrated final acceptance completes once the
remaining PRs are merged (recommended sequence in §5).

## 1. Verifiable-now items on integrated `main` (M1+M2)

The incident (campaign 269 / task 4285) was facebook-specific; M1 (shared contracts) +
M2 (facebook fix) are merged to `main`. Ran in a clean `main` worktree
(`gm-agent-wt-chaingate`), `NO_PROXY=127.0.0.1,localhost`:

- **① T-040 incident replay (gate core) — GREEN.** `cargo test --lib -- incident_269`:
  - `incident_269_shape_only_20_available_reports_no_more` … ok  (truthful under-delivery)
  - `incident_269_shape_sufficient_upstream_scans_50_completed` … ok  (scans to max, COMPLETED)
  - 2 passed; 0 failed. **The original incident cannot recur on the integrated core.**
- **Deterministic suite — GREEN.** `cargo test --lib`: 322 passed; 0 failed.
- **② R-012 zero shape-change.** `git diff <pre-plan>...main --stat -- migrations/ src/schema.rs
  src/db/schema.rs src/protocol_gen/` → empty.
- **③ Contract-invariant §3.** `TaskTerminalReason` strings intact in
  `src/ports/progress_tracker.rs` (`COMPLETED`, `COMPLETED_WITH_PARTIAL_ERRORS`,
  `NO_MORE_POSSIBLE_DATA`); scheduler reads by colon-prefix token (`split(':').next()`).

## 2. Per-module PR readiness (unmerged)

- **M3/M4/M5 (agent PRs #10/#11/#12):** all `MERGEABLE` / `CLEAN`; every check SUCCESS
  (Rust Test Gates, Agent Provider Harness, Twitter E2E, Facebook E2E).
- **R-012 across all three:** `git diff main...origin/<branch> --stat` for
  `migrations/ src/schema.rs src/db/schema.rs src/protocol_gen/ src/adapters/postgres.rs`
  → empty for each. No shape or budget surface touched.
- Per-module mutation evidence: recorded in each PR (AG-012/AG-013 precheck). Per root §4
  criterion ②, this is the authoritative mutation evidence (a re-run on merged `main` yields
  an empty diff and cannot serve as proof).

## 3. M6 scheduler mutation gate — defect found & fixed (criterion ③/⑧)

**Defect:** worker PR #70's "Mutation Test PR Diff (scheduler)" was FAILURE. Root cause was
**not** a missed mutant: the job died at argument-parse —
`error: unexpected argument '--annotations' found` — because the workflow passed
`--annotations=github` to the pinned `cargo-mutants 24.11.0`, which does not accept it.
**The gate had never run a single mutant** (hollow). The DR-06 `--relative`+sentinel logic
itself worked (`src/ changes detected (6 file(s))`).

**Fix** (`glance_mind_worker/.github/workflows/mutation-scheduler.yml`, commit `dafd612`,
pushed to `feat/jplan-m6-once-guard`):
1. Remove `--annotations=github` (the failure).
2. `--cargo-arg=--lib` — scope build+test to the library target. The integration-test
   binaries (`tests/tiktok_*_test.rs`) reference `AiPubInput::with_prompts/new`, methods
   absent from the resolved `glance_mind_protocol` (a pre-existing, unrelated breakage on
   the worker repo's evolving AiPub feature) — they fail to compile and would block the
   mutation baseline. The once-guard unit tests live in `src/test_*.rs` (lib scope).
3. `--exclude-re 'Scheduler::run_once'` + `'Scheduler::dispatch_task'` — db-glue exemption.

**Mutation result after fix:** local (cargo-mutants 27.1.0) `Found 9 → 8 caught, 1 unviable,
0 missed`; **CI green** (run 27393649063, head `dafd612`, cargo-mutants 24.11.0): `Found 8 →
7 caught, 1 unviable, 0 missed` in 5m9s, sentinel N=7 ≥ 1. (Count differs 9 vs 8 only by
cargo-mutants version's mutation generation; both 0 missed, all once-guard predicates caught.)
PR #70 now CLEAN — both checks SUCCESS. Criterion ③ (CI 实跑可见) satisfied.

**Caught (8) — the entire once-guard decision surface:**
`apply_once_completion -> ""/"xyzzy"`; `OnceCompletionReason::as_str -> ""/"xyzzy"`;
`eval_once_completion` line 76 `== → !=` (SEARCH_EXHAUSTED token match);
`eval_once_completion` line 79 `< → ==/>/<=` (under-scan threshold `process_count < max_count`).
**Unviable (1):** `eval_once_completion -> Default::default()` (no `Default` impl).

**Exempted (4 missed before exemption) — §6.2 db-glue, Test-Gate reviewer signed off LEGITIMATE:**
- `Scheduler::run_once -> SchedulerRunResult::default()` — DB-coupled whole-tick orchestrator;
  killed by the real-DB / e2e gates (`real_db_completed_reason_test`, `e2e_scheduler_test`),
  not lib unit tests. Its new logic is extracted into `apply_once_completion` (caught).
- `Scheduler::dispatch_task -> Ok(0/1/-1)` (×3) — **not behaviorally modified by M6** (in-diff
  only by comment-hunk proximity); pre-existing DB-dispatch code.
- Invariant pinned in the workflow comment: all once-guard decision logic must remain in
  `eval_once_completion`/`apply_once_completion`, never inlined into the exempted fns.

## 4. Cross-module invariant audit (root §2)

| # | Invariant | Result |
|---|---|---|
| §2.1 | R-012 zero shape-change (agent main + all 3 PRs; scheduler migrations) | PASS (all empty) |
| §2.2 | Contract §3 — terminal_reason strings not renamed; scheduler NULL/unknown tolerance | PASS (`underscan_null_terminal_warns`, `underscan_unknown_terminal_value_warns` in register) |
| C-003 | `SEARCH_EXHAUSTED` contains `EXHAUST` (gm-e2e white-list safe) | PASS (`assert!("SEARCH_EXHAUSTED".contains("EXHAUST"))`) |
| D-07 | scheduler real-DB SEARCH_EXHAUSTED persistence gate | present (gated, DATABASE_URL) |
| §2.4 | I-006 budget conservation (zero budget-fn change) | PASS (postgres.rs/reserve untouched in all PRs) |
| incident closure | T-040 two-shapes (M2) + T-021/T-022 (M6) = exhausted-distinguishable / abnormal-observable / no-false-COMPLETED | PASS |

## 5. Recommended merge sequence (user decision)

Per root §3 there is no hard deploy ordering (scheduler tolerates NULL/unknown
terminal_reason; agent-first or scheduler-first both safe). Suggested order:

1. **M6 PR #70** (worker) — after CI mutation re-run goes green.
2. **M3 #10 → M4 #11 → M5 #12** (agent) — each green; any order, all against `main`.
3. **Docs PR #6** — plan family + this acceptance record.

After merge, the integrated `main` carries all five platforms + the scheduler guard; a final
`cargo test --lib -- incident_269` + full-suite run on the merged tip is the closing check
(the merged-tip mutation diff is empty by design — per-PR mutation evidence stands).

## 6. Residual follow-ups (root §5, non-blocking)

- **RT-4** — remove live keys (`TIKHUB_API_KEY`/`FACEBOOK_RAPIDAPI_KEY`) from
  `agent .github/workflows/mutation-rust.yml` (M1-T0 env-guards landed, so unblocked).
- **N-006** — set the scheduler mutation workflow as a protected-branch required check
  (GitHub repo setting, one-time manual).
- **RT-2** — backlog: promote the mock-HTTP helper to `src/testing` (3rd copy); retry-limit
  observation item.
