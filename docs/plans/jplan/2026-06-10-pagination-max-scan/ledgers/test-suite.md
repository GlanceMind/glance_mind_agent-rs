# Test Suite Ledger(Step 02)

> 既有基建:`src/testing/mock_gateway.rs`(确定性 mock)、facebook mock-HTTP 多页测试样板(`src/adapters/facebook.rs:1169-1203, 1269-1311`)、`tests/` real-API/real-DB gated 套件、`tests/fixtures/`。
> 所有新测试遵守 `constraints/testing-constraints.md`(先红后绿、断言不可变);anti-gaming 细则见 `anti-gaming-test-quality.md`。

## 1. 单元/组件(确定性,默认 `cargo test` 内)

| ID | 测试 | 覆盖需求 | RED 现状(预期失败) |
|----|------|----------|----------------------|
| T-001 | 各平台 strategy:max_count=50 → options.count=50(fb/tiktok/reddit/twitter/instagram 各一条) | R-001 | fb 现状产出 20(`v.min(20)`),tiktok 20,其余按各自 cap |
| T-002 | orchestrator 欠交付映射:exhausted→`NO_MORE_POSSIBLE_DATA`;partial-failure→`COMPLETED_WITH_PARTIAL_ERRORS`;达量→`COMPLETED` | R-007 | 现状 trait 返回 `Vec<Content>` 无原因可映射 |
| T-003 | redis task→TaskConfig:`search_limit` 作单页提示并 clamp 到平台页上限;`search_offset` 任意值不改变行为 | R-011, I-008 | 现状两字段被忽略(redis.rs:449-465)——limit 部分 RED |
| T-004 | terminal_reason 脱敏:exhausted/partial 新路径注入敏感串 → 已脱敏 | I-009 | 新路径不存在 |

## 2. 集成(mock-HTTP / mock-gateway,确定性)

| ID | 测试 | 覆盖需求 | RED 现状 |
|----|------|----------|----------|
| T-010 | facebook 端到端:mock 3 页(20+20+10)→ 50 条处理、终态 COMPLETED(扩展既有 facebook.rs:1169-1203 样板) | R-002 | 现状循环到 20 即停(strategy 截断) |
| T-011 | tiktok offset 翻页:mock 多页,断言 offset 递进、has_more=0 停止、达量停止 | R-003 | 适配器单次调用(tikhub.rs:186-187) |
| T-012 | reddit after 翻页:断言 after 转发、hasNextPage=false 停止 | R-004 | content 路径不传 after(reddit.rs:160-178) |
| T-013 | twitter cursor 翻页:断言 next_cursor 转发、cursor 终止条件 | R-005 | search_params 从不设 cursor(twitter.rs:161-163) |
| T-014 | instagram(V1 后定):翻页分支 或 单页+NO_MORE_POSSIBLE_DATA 分支 | R-006 | gated on V1 |
| T-015 | 失败注入:第 1 页失败→failed;第 2 页失败(有进展)→COMPLETED_WITH_PARTIAL_ERRORS | R-008, F-001, F-002 | 分支不存在 |
| T-016 | 枯竭:2 页后 has_more=false 且未达 50 → NO_MORE_POSSIBLE_DATA | F-003 | 原因丢失 |
| T-017 | 防环/空页:重复 cursor、空页×3 → 终止且按枯竭语义上报 | F-004, F-005 | 仅 facebook 有 |

## 3. scheduler 单测(兄弟仓,确定性,沿用 test_schedule_evaluator.rs 模式)

| ID | 测试 | 覆盖需求 | RED 现状 |
|----|------|----------|----------|
| T-020 | eval_once:completed + process_count<max_count + terminal_reason≠NO_MORE_POSSIBLE_DATA → WARN 决策,状态不变、不派发 | R-010, I-007 | 现状无条件 MarkCompleted |
| T-021 | eval_once:枯竭欠扫(terminal_reason=NO_MORE_POSSIBLE_DATA)→ 不告警,completed_reason 区分值(R-013) | R-013, F-008 | 分支不存在 |
| T-022 | eval_once:扫满 → 无 WARN,ONCE_EXECUTED 不变(回归保护) | R-010 | GREEN 基线(回归测试,允许先绿,须经变异门禁证明有效) |

## 4. 属性测试(proptest;详细不变量定义见 anti-gaming 账本 §3)

| ID | 测试 | 覆盖 |
|----|------|------|
| T-030 | PT-1:任意页序列下处理数 ≤ max_count | I-001 |
| T-031 | PT-2:任意有限页/cursor 序列(含重复、空页、环)循环终止 | I-002 |
| T-032 | PT-3:重复条目不重复计数 | I-003 |
| T-033 | PT-4:exhausted 当且仅当注入枯竭信号 | I-004 |
| T-034 | PT-5:`search_offset` 任意值不改变取数行为 | I-008 |

## 5. 回归(事故重演)

| ID | 测试 | 覆盖 |
|----|------|------|
| T-040 | 事故重演(campaign 269 形状):facebook、max_count=50、上游充足 → 50 条、COMPLETED、无 WARN;同形状上游仅 20 条 → NO_MORE_POSSIBLE_DATA(mock 层) | R-001/R-002/R-007 |

## 6. real-provider / real-DB gates(凭据 gated,不入默认 `cargo test`;详见 production-dependencies.md)

| ID | 测试 | Gate |
|----|------|------|
| T-050 | facebook 真实第二页:live cursor 翻页冒烟(扩展 tests/facebook_real_api_test.rs) | `FACEBOOK_RAPIDAPI_KEY` |
| T-051 | tiktok 真实 offset 第二页行为(扩展 tests/real_api_test.rs) | `TIKHUB_API_KEY` |
| T-052 | reddit/twitter 真实第二页冒烟 | `TIKHUB_API_KEY` |
| T-053 | V1 探测:instagram general_search 带 token 重发(判定标准见 assumptions.md V1) | `TIKHUB_API_KEY` |
| T-054 | real-DB:翻页多批落库后 process_count/consumed 守恒、terminal_reason 持久化(扩展 tests/*_real_db_test.rs 模式) | `DATABASE_URL` |

## 7. E2E/workflow

- 既有 `e2e/` live 栈不新增强制用例(live 漂移干扰已知,见 constraints §工具链);workflow 级保障由 T-010~T-017(mock 集成)+ T-054(real-DB)组合承担。**justified gap**:跨仓(scheduler→redis→agent→db→scheduler)全链 e2e 依赖 docker 栈与真实凭据,成本高且事故路径已被分层测试完整覆盖;若 Step 04 评审认为必要再补。

## 8. Justified gaps 汇总

| Gap | 理由 |
|-----|------|
| 跨仓全链 e2e | 见 §7 |
| instagram 测试(T-014/部分 T-053) | gated on V1,分支未定;V1 本身是计划内验证任务 |
| 页级重试测试 | 本计划不新增页级重试逻辑(invariants-failures.md §3) |
| facebook 实现级循环不变量的属性测试 | D-01 保留手写循环;由既有 8+ mock 测试 + M2-T2.8 语义对齐 + M2-T3 金丝雀承担(Step 06 TG-07) |
| F-010 构造器不变量的独立 mock 测试 | D1 类型级构造器禁止「空+Partial」共现;M1-T3 测试 7 为构造器约束测试,变异门禁覆盖构造器实现——无需独立失败注入 mock |
