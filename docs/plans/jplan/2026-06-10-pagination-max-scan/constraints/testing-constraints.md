# Testing Constraints(摘自 `~/.claude/TESTING_CONSTRAINTS.md`,对本计划族全程强制)

> 源头规范:`~/.claude/TESTING_CONSTRAINTS.md` + `~/.claude/CLAUDE.md` 硬规则。本文件是计划族内的常驻摘要;起草/补丁步骤开始前必须重读源文件。

## 一句话原则

目标是「代码符合规格」,「测试变绿」只是证据。冲突时**改代码,不准改证据**。

## 硬规则(不可违背)

1. 不准为过测试而改弱/改写/注释/删除已有断言;只能 (a) 修生产代码,(b) 期望确属错误时带 `ASSERTION-CHANGE-JUSTIFIED: <原因>` 重走 RED→GREEN,(c) 测试确已废弃则整条删除并说明。
2. 不准新增 skip/xfail/`#[ignore]` 等绕过失败。
3. 不准伪造通过(吞异常、`assert True`、expected 改 actual、生产代码特判测试输入)。
4. 先红后绿:每个测试先为「正确的原因」失败,留 RED→GREEN 证据(失败输出 + 通过输出)。
5. 如实报告:断言未全部显式验证不得标记 passed。
6. 职责分离:写测试的上下文 ≠ 改生产代码的上下文(subdriven 执行时分属不同子 agent)。
7. 任何断言改动都要书面理由。

## 对本计划族的具体落地

- **每个 feature**:确定性 mock 测试(用 `src/testing/mock_gateway.rs` 体系,不打真实上游)。
- **每个真实生产外部依赖**(TikHub / Facebook RapidAPI / prod-shape Postgres 存储过程):真实依赖路径测试 + 明确的 real-provider gate(可标记为需凭据的独立 gate,不混入默认 `cargo test`)。
- **核心翻页循环逻辑**:属性测试(`proptest`)断言不变量,例如:任意页序列下「已处理数 ≤ max_count」「cursor 终止条件单调」「重复页不重复入库」。
- **变异门槛**:本仓 CI 已有 `cargo mutants --in-diff` PR 门禁(真·强制);新增核心模块(翻页循环、策略 cap 逻辑)必须在 diff 内无 missed mutants 或写明豁免。
- **RED 测试是契约**:实现任务不得修改计划中定义的断言;触发本地 assertion guard hook 时默认改生产代码。
- **每个任务必须写明**:先写的 RED 测试、预期 RED 失败信息、GREEN 命令、最终验收命令。
- scheduler 仓(glance_mind_scheduler)同样适用以上全部规则;其测试工具链同为 Rust(cargo test / cargo mutants)。

## 工具链(Rust)

- 变异:`cargo mutants`(PR 用 `--in-diff`,nightly 全量)。
- 属性:`proptest`。
- 已知干扰:CI 把 live-API 测试捆绑进 Rust Test Gates,PR 红可能是上游漂移而非本改动 —— 验收时须区分,不得以此为由弱化断言。
