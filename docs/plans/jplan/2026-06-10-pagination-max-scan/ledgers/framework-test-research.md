# Framework Test Research Ledger(Step 02)

> 范围:本计划触及的测试框架/SDK/工具链,逐项给出「现状证据 → 决策 → 待办」。ID 用 `FR-xxx`(避免与 invariants-failures.md 的失败模式 F-xxx 撞号)。
> 所有「现状」均为 2026-06-10 工作区实证。

| ID | 框架/工具 | 现状(证据) | 决策 | 待办(归 Step 04 任务) |
|----|-----------|--------------|------|--------------------------|
| FR-001 | **proptest**(属性测试) | **不在依赖中**:Cargo.toml grep `proptest\|quickcheck` 零命中;`[dev-dependencies]` 仅 `tokio-test = "0.4"` | 引入 `proptest`(1.x)为 dev-dependency。选型非开放问题:`constraints/testing-constraints.md` 与 TESTING_CONSTRAINTS §4 已指定本仓属性测试用 proptest | ① 加 dev-dep(实现时核对最新 1.x 版本与官方文档 https://proptest-rs.github.io/proptest/);② 翻页循环纯逻辑与 async I/O 解耦以便属性驱动:对「页序列」建模为生成的 `Vec<Page>`(条数 0..=页上限、id 小空间制造碰撞、cursor 小字母表制造环),经 mock gateway 注入;③ **与变异门禁的运行时预算**:PR mutants 单变异 timeout 300s(AG-010),proptest 默认 256 cases 须实测可容纳,必要时对 PT 套件设 `PROPTEST_CASES` 下调(下调须写入任务说明,不得以此弱化断言);④ 回归种子文件 `proptest-regressions/` 入库(官方推荐,失败用例可重放) |
| FR-002 | **cargo-mutants**(变异门禁) | CI 已存在且 pinned:`.github/workflows/mutation-rust.yml` L45/L103 `cargo install cargo-mutants --locked --version 24.11.0`;PR `--in-diff` job + nightly 全量 job | 沿用,不调参;门槛见 AG-010~AG-012 | 无 agent-rs 侧待办。**scheduler 仓缺口已实证**:`glance_mind_worker/.github/workflows/` 仅 build-windows-desktop/deploy/harness/image-live,无 test/mutation 工作流;`glance_mind_scheduler/` 无独立 workflows → AG-013 的「本地 `cargo mutants --in-diff` 跑通无 missed + 输出留存」是该仓唯一变异证据,Step 04 scheduler 模块计划必须包含;是否补 CI 由 Step 04 评审定(推荐补)——已裁决补(D-09,M6-T4) |
| FR-003 | **mock-HTTP 测试基建**(确定性集成测试载体) | 自研轻量 helper,**无外部 mock 框架**:`spawn_mock_http_server(_with_capture)` 以模块内测试代码形式分布于 facebook.rs:993/1050、twitter.rs:456、instagram.rs:538、tikhub/client.rs:1788;facebook 已有 cursor 跨请求转发断言样板(facebook.rs:1169-1203、1269-1311);另有 `src/testing/mock_gateway.rs`(gateway 层确定性 mock)与 `tests/fixtures/`(真实响应样本,如 `tests/fixtures/tiktok/search_travel_us.json`) | **复用既有模式,不引入 wiremock/mockito 等新依赖**(facebook 样板已被测试证明,降低供应链与学习面);tiktok 适配器测试可仿照 tikhub/client.rs:1788 helper 或在 adapters/tikhub.rs 测试模块内复制同构 helper | T-011~T-013 各自模块内落地 helper(允许模块内复制,与既有惯例一致);新 fixture 必须取自真实响应样本(providers.md 控制:T-050~T-053 真实输出回灌为 fixture,禁止凭空捏造字段) |
| FR-004 | **scheduler 测试基建** | `glance_mind_scheduler/src/test_schedule_evaluator.rs` 存在(确定性单测,内存构造决策枚举);`tests/` 下为集成/e2e 类(e2e_scheduler_test.rs 等);无 real-DB gated 单测基建(P-007 已记) | T-020~T-022 沿 test_schedule_evaluator.rs 模式写确定性表驱动单测(输入域有限,属性测试豁免见 anti-gaming §3 末) | scheduler 模块计划引用本行;P-007 补偿控制(最小 gated 测试 或 部署后 SQL 验证)Step 04 定 |
| FR-005 | **Postgres 层(diesel/存储过程)与 Redis 协议** | agent-rs 进度存储过程及 fallback 已有 real-DB gated 测试模式(`tests/*_real_db_test.rs`,P-005);R-012 决定预算/schema 零改动;Redis 读写路径不变(P-006) | 无新框架研究需求;不触发 db-migration-guard | 仅 P-005 扩展用例(T-054);无选型待办 |

## 研究方法说明

- 本账本结论全部来自本地代码/配置实证(grep/ls/文件读取),无网络文档依赖项;唯一外部文档核查(TikHub OpenAPI,Instagram V1)已登记为 N-001/C-005,不属于框架选型。
- 唯一潜在 choice-sensitive 决策(mock 框架引入与否,FR-003)已按「既有被证明模式优先、不加依赖」裁决;若 Step 04 起草中发现 helper 复制成本过高,可重开为 dependency-expert 评估项。
- FR-001 的版本号留待实现时核对(计划不锁死 patch 版本,避免计划与 crates.io 漂移)。
