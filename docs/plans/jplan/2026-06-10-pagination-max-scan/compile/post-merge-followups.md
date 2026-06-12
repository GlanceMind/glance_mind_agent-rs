# Post-merge follow-ups (root §5 RT-2/RT-4, N-005/N-006)

Status of the non-blocking items after the chain gate. RT-1/RT-3 are already closed
(RT-3 → M1-T0 merged; RT-1 F-007 doc folded into the plan family).

## RT-4 — strip live API keys from the agent mutation gate — ✅ DONE

`.github/workflows/mutation-rust.yml` (agent repo) injected `TIKHUB_API_KEY` /
`FACEBOOK_RAPIDAPI_KEY` into both mutation jobs' `.env`. Under mutation testing each
mutant re-runs the suite, so those keys would fire paid live calls per mutant.

- **Fix:** removed both keys from both jobs; kept `DATABASE_URL` (real-DB tests gated by
  `RUN_REAL_DB_TESTS`); no other env vars touched. Unblocked because M1-T0 env-guards
  (merged) make the live suites self-skip when keys are absent.
- **Bonus fix:** dropped `--annotations=github` (+ unused `GITHUB_TOKEN`) — pinned
  `cargo-mutants 24.11.0` rejects that flag, so this gate would have died at
  argument-parse the moment it activated (the same hollow-gate defect fixed in the
  scheduler workflow / worker PR #70).
- **Landed in:** PR #4 (`chore/ci-front-runner`, commit `cb70473`) — the PR that
  introduces `mutation-rust.yml` to `main` (the file is not on `main` yet, so RT-4 cannot
  be a standalone PR off `main`; it rides the introduction PR so the file lands correct).

## N-006 — mutation gate as a protected-branch required check — ⏳ USER / ADMIN

**Current state (verified 2026-06-12):** `main` on **both** repos has **no branch
protection at all** (`GET /branches/main/protection` → HTTP 404 "Branch not protected").
So this is not "add a context to existing protection" — it is establishing branch
protection from scratch, which changes the team workflow (no direct pushes to `main`,
PRs must pass checks). That is a governance policy decision for the repo owner; left to
the user deliberately (plan registered N-005/N-006 as a manual one-time config).

**Prerequisite ordering:** a status check can only be marked *required* after its context
exists on `main` — i.e. after the workflow is merged and has run at least once:
1. Merge worker **PR #70** → `mutation-scheduler.yml` on worker `main` (context
   `Mutation Test PR Diff (scheduler)`).
2. Merge agent **PR #4** → `mutation-rust.yml` on agent `main` (context
   `Mutation Test PR Diff`).
3. Then configure protection (admin; `gh` already has admin on both):

```bash
# WORKER main — require the scheduler mutation gate (+ harness)
gh api -X PUT repos/GlanceMind/glance_mind_worker/branches/main/protection \
  -F required_status_checks.strict=true \
  -f 'required_status_checks.contexts[]=Mutation Test PR Diff (scheduler)' \
  -f 'required_status_checks.contexts[]=Scheduler Harness Worker' \
  -F enforce_admins=false \
  -F required_pull_request_reviews.required_approving_review_count=1 \
  -F restrictions=null

# AGENT main — require the mutation gate (+ existing test/harness gates)
gh api -X PUT repos/GlanceMind/glance_mind_agent-rs/branches/main/protection \
  -F required_status_checks.strict=true \
  -f 'required_status_checks.contexts[]=Mutation Test PR Diff' \
  -f 'required_status_checks.contexts[]=Rust Test Gates' \
  -F enforce_admins=false \
  -F required_pull_request_reviews.required_approving_review_count=1 \
  -F restrictions=null
```

(Adjust the exact context strings to the names GitHub records after the first run, and
tune `required_approving_review_count` / `enforce_admins` to team policy.)

## RT-2 — backlog (no code, non-gating)

- Promote the mock-HTTP test helper to `src/testing` (3rd copy across adapters) —
  independent refactor PR, not part of this plan family's gate.
- "retry-limit" observation item for eval_once per-tick re-dispatch (F-007 doc notes it
  as a backlog observable; current behavior is intended).
