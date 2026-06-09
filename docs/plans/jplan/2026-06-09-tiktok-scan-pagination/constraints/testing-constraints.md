# Testing Constraints (injected from ~/.claude/TESTING_CONSTRAINTS.md)

These are bedrock for every task in this plan family. Implementers MUST obey them.

## Hard rules (non-negotiable)
1. 禁止为过测试而改弱/改写/注释/删除已有断言。失败时只能：修生产代码（首选）；或带 `ASSERTION-CHANGE-JUSTIFIED:<原因>` 修正确属错误的期望并重走 RED→GREEN；或整条删除已废弃测试并说明。
2. 禁止 `#[ignore]` / `skip` / `xfail` 绕过失败。
3. 禁止伪造通过：`assert!(true)`、吞 `Result`、把 expected 改成 actual、生产代码特判测试输入。
4. 先红后绿：每个测试必须先为「正确的原因」失败，并留 RED→GREEN 证据（失败输出 + 通过输出）。
5. 如实报告：除非所有断言显式验证，否则不得标记 passed。
6. 职责分离：写/拥有测试的上下文 ≠ 改生产代码的上下文。
7. 测试改动高危：任何断言改动都要书面理由。

## Rust toolchain for this repo (glance_mind_agent_rs)
- Unit/integration tests: `cargo test`.
- Mutation testing (CI / module-completion, not inner loop): `cargo mutants --in-diff pr.diff -- --all-features`.
- Property tests for core logic: `proptest`.
- 真·强制层 = CI 变异门禁 + PR diff 审查；本地 hook 仅减速带。

## Application to THIS plan
- Core logic under test = the pagination loop (offset/cursor accumulation, termination, dedup, truncation). It is a "core module" → requires a property-style invariant test where feasible, plus deterministic mock-HTTP tests with RED→GREEN evidence.
- External dependency = TikHub HTTP API. Real-dependency path is exercised through `TikHubClient` with an injected `base_url` pointed at a mock HTTP server (mockito), so the actual request-building + response-parsing + loop code runs (not mocked away).
- Implementer MUST NOT weaken the RED assertions (e.g. asserting `== 40`) to pass.
