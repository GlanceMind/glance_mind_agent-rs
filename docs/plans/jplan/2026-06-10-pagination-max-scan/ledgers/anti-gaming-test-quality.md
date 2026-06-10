# Anti-Gaming Test Quality Ledger(Step 02)

> 源头规范:`~/.claude/TESTING_CONSTRAINTS.md` + `constraints/testing-constraints.md`(本计划族常驻摘要)。
> 本账本对每个 feature 规定:RED→GREEN 证据要求、断言不可变规则、变异门槛、属性测试。Step 04 起草任务时必须逐条引用;traceability 编译(Step 08)将校验无任务以「改/删断言、加 skip」为达成手段。

## 1. 全局规则(对所有 feature 生效)

| ID | 规则 |
|----|------|
| AG-001 | **先红后绿**:每个新测试(T-001~T-021、T-030~T-040)必须先以「正确的原因」失败。任务文档必须写明:预期 RED 失败信息(具体到断言文本,如 `expected options.count == 50, got 20`)、GREEN 命令、最终验收命令。RED 与 GREEN 输出均须留存为任务完成证据。 |
| AG-002 | **断言不可变**:计划中定义的 RED 测试是契约;实现子 agent 不得修改断言。失败 → 改生产代码或停下上报。例外仅 `ASSERTION-CHANGE-JUSTIFIED: <原因>` + 重走 RED→GREEN(本地 hook 留痕,PR diff 审查复核)。 |
| AG-003 | **禁绕过**:不得新增 `#[ignore]`、不得吞 `Result`/`unwrap_or_default()` 掩盖失败、不得把 expected 改成 actual、生产代码不得特判测试输入(如识别 mock URL 而走特殊分支)。 |
| AG-004 | **职责分离**:subdriven 执行时,测试任务与实现任务分属不同子 agent(测试上下文 ≠ 实现上下文)。 |
| AG-005 | **过度 mock 禁令**:只 mock 外部依赖(HTTP 上游、DB、Redis);不得 mock 被测对象本身(翻页循环、strategy cap 逻辑、eval_once 决策)。 |
| AG-006 | **回归测试(先绿类,如 T-022)**:必须经变异门禁证明有效(对应代码被注入变异时测试变红),否则视为无效证据。 |
| AG-007 | **如实报告**:live-API 套件红(Facebook/Twitter 漂移,见项目记忆)不得作为弱化断言或标记 passed 的理由;须区分上游漂移与本改动失败。 |
| AG-008 | **live 契约探针/gated real 测试的有效性判据**(此类测试不入变异运行集,不适用 AG-006 变异证明):① 实跑原始输出留存;② 响应回灌 fixture 且 mock 字段以回灌样本为源(providers.md 控制);③ 判定逻辑可由留存输出复算。**防滥用**:确定性测试不得借此类别逃避 AG-006;类别归属由计划文本显式标注并经 Test-Gate Reviewer 复核。 |

## 2. 变异测试门槛(真·强制层 = CI)

| ID | 内容 |
|----|------|
| AG-010 | **门禁命令(CI 已存在,`.github/workflows/mutation-rust.yml`)**:PR 上 `cargo mutants --in-diff /tmp/pr.diff --timeout 300 --annotations=github -- --all-features --test-threads=1`(cargo-mutants 24.11.0);nightly 全量。 |
| AG-011 | **门槛**:本计划 agent-rs 全部 diff(strategies cap 解耦、适配器翻页循环、orchestrator 终态映射、redis 映射)**无 missed mutants**,或对每个 missed 写书面豁免(豁免须说明该变异为何不可测/等价变异,入 PR 描述并由 Step 06 Test-Gate Reviewer 复核)。 |
| AG-012 | **本地预检命令**(实现任务收尾时,不等 CI):`git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1`。 |
| AG-013 | **scheduler 仓**:`glance_mind_scheduler` 当前无等价 mutation CI 工作流 —— Step 04 的 scheduler 模块计划必须包含:本地 `cargo mutants --in-diff` 跑通无 missed(命令同 AG-012,在 scheduler 仓内执行),结果输出留存为完成证据;是否为该仓补 CI 工作流由 Step 04 评审定(推荐补,改动小)。 |

## 3. 属性测试(proptest)——核心翻页/截断逻辑不变量

> proptest 目前**不在本仓依赖中**(Cargo.toml/Cargo.lock 均无),引入为计划内任务(见 framework-test-research.md FR-001)。策略:对「页序列」建模为 `Vec<Page>`(每页含条目数 0..=页上限、可选重复条目、cursor 值含重复/缺失),驱动翻页循环纯逻辑部分(以 mock gateway 注入页序列)。

| ID | 不变量(属性) | 关联 | 生成器要点 |
|----|----------------|------|------------|
| AG-020 / PT-1 | 任意页序列下,已处理数 ≤ max_count | I-001, T-030 | max_count ∈ 1..=200;页数 0..=20;每页条数 0..=页上限 |
| AG-021 / PT-2 | 任意有限页/cursor 序列(含重复 cursor、空页、cursor 环)下循环终止,且终止原因 ∈ {达量, 上游枯竭, 防环/空页上限, 错误} | I-002, T-031 | cursor 从小字母表生成以高概率制造环;空页连发 |
| AG-022 / PT-3 | 含重复条目的页序列下,计数与入库集合按唯一 id 去重(处理数 = 唯一 id 数与 max_count 的较小值) | I-003, T-032 | 条目 id 从小空间生成制造碰撞 |
| AG-023 / PT-4 | 产出 exhausted(→NO_MORE_POSSIBLE_DATA)当且仅当页序列含上游枯竭信号(has_more=false/空页上限/cursor 缺失)且未达量 | I-004, T-033 | 双向断言:无信号时不得产出 exhausted |
| AG-024 / PT-5 | `search_offset` 任意 i64 值不改变 TaskConfig 取数语义(行为等价) | I-008, T-034 | offset 全域生成 |

豁免:scheduler eval_once 决策为有限枚举输入(status × process_count × terminal_reason),组合可被 T-020~T-022 表驱动穷举,**属性测试豁免,理由:输入域有限且已穷举**。

## 4. 每 feature 的证据矩阵

| Feature(需求) | RED→GREEN 证据 | mock 测试 | proptest | 变异门禁 | real gate |
|----------------|----------------|-----------|----------|----------|-----------|
| R-001 strategy 解耦 | T-001 各平台 RED 输出 | T-001 | — (纯常量逻辑,变异门禁覆盖) | AG-011 | — |
| R-002 facebook 达量 | T-010 RED(20≠50) | T-010, T-040 | PT-1~PT-3 ※1 | AG-011 | T-050 |
| R-003 tiktok 翻页 | T-011 RED | T-011 | PT-1~PT-3 | AG-011 | T-051 |
| R-004 reddit 翻页 | T-012 RED | T-012 | PT-1~PT-3 | AG-011 | T-052 |
| R-005 twitter 翻页 | T-013 RED | T-013 | PT-1~PT-3 | AG-011 | T-052 |
| R-006 instagram(V1 后) | T-014 RED | T-014 | 视分支 | AG-011 | T-053(=V1 探测) |
| R-007 欠交付原因 | T-002 RED | T-002, T-016 | PT-4 | AG-011 | T-054 |
| R-008 页失败语义 | T-015 RED | T-015 | — (枚举分支,变异门禁覆盖) | AG-011 | — |
| R-009 安全终止 | T-017 RED | T-017 | PT-1~PT-3 | AG-011 | — |
| R-010 scheduler WARN | T-020 RED | T-020, T-022 | 豁免(§3 末) | AG-013 | — |
| R-011 字段语义 | T-003 RED(limit 部分) | T-003 | PT-5 | AG-011 | — |
| R-013 completed_reason 区分 | T-021 RED | T-021 | 豁免(§3 末) | AG-013 | — |

※1(Step 06 TG-07):facebook 实现级循环不被 PT-1~PT-3 覆盖(D-01 保留手写循环,PT 只测共享状态机);实现级防线 = 既有 mock 套件 + M2-T2.8 对齐 + M2-T3 金丝雀;justified gap 登记于 test-suite §8。
