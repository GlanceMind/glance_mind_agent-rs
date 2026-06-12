# Invariant / Failure Matrix(Step 02)

> I-xxx 为不变量(多数将落为 proptest/单测断言,见 `test-suite.md` 与 `anti-gaming-test-quality.md`);F-xxx 为失败模式行。

## 1. 不变量

| ID | 不变量 | 范围 | 验证方式 | 关联需求 |
|----|--------|------|----------|----------|
| I-001 | 任意页序列(任意长度/条数/重复)下,单 task 已处理数 ≤ max_count | 所有平台翻页循环 | proptest(PT-1)+ 单测 | R-002~R-006, R-009 |
| I-002 | 翻页循环必终止:终止当且仅当 (a) 达 max_count,(b) 上游显式枯竭信号(`has_more=false`/cursor 缺失/`hasNextPage=false`),(c) 空页/重复 cursor 达上限(facebook 样板 `MAX_EMPTY_CURSOR_HOPS=3`,facebook.rs:28),(d) 不可恢复错误,(e) 连续『零新增』页(重复内容、cursor 各异)达上限——empty_streak 按 newly_accepted 计(M1 D2 冻结) | 所有平台翻页循环 | proptest(PT-2:任意有限页序列+任意 cursor 串都终止)+ 单测 | R-009 |
| I-003 | 重复页/重复条目不重复计数、不重复入库(`(task_id, video_id)` 唯一;循环内按 id 去重,facebook.rs 样板) | 翻页循环 + postgres 落库 | proptest(PT-3)+ real-DB gate 既有唯一约束测试 | R-009 |
| I-004 | 枯竭信号必然来自上游(`has_more=false`/空页/cursor 缺失),不得由本地猜测合成;枯竭且未达量 → 上报 `NO_MORE_POSSIBLE_DATA` | fetch 路径 → orchestrator 映射 | 单测 + proptest(PT-4:仅当注入枯竭信号时才产生 exhausted) | R-006, R-007 |
| I-005 | 已落库进展不可因后续页失败而作废(部分进展 + 页失败 → `COMPLETED_WITH_PARTIAL_ERRORS`,process_count 保留) | orchestrator 终态映射 | 失败注入单测 | R-008 |
| I-006 | 预算守恒:consume 严格按实际处理条数递增;翻页不引入额外 reserve;消耗 ≤ reserved(reserve=max_count×单价,150=50×3) | postgres 进度过程 + scheduler 预算 | real-DB gate 断言 consumed 与 process_count 一致;diff 审查零预算代码改动 | R-012 |
| I-007 | scheduler 防御校验为纯观测:WARN+指标分支不改变任何 campaign/task 状态、不派发新 task | scheduler eval_once | scheduler 单测:决策枚举不变 + 无副作用 | R-010 |
| I-008 | `search_offset` 不参与 agent 取数路径(任何值不改变取数行为);`search_limit` 仅影响单页大小且被 clamp 到平台页上限 | redis task → TaskConfig 映射 | 单测:offset 任意值行为不变(可 proptest);limit clamp 断言 | R-011 |
| I-009 | terminal_reason 消息经 redaction(`redact_terminal_reason_message`,progress_tracker.rs)— 新增 exhausted/partial 路径不得绕过 | 终态写入 | 单测:注入含敏感串的上游错误,断言已脱敏 | R-007, R-008 |

## 2. 失败模式矩阵

| ID | 失败模式 | 期望行为 | 现状/先例 | 验证 |
|----|----------|----------|-----------|------|
| F-001 | 第 1 页即失败(零进展) | task `failed`;eval_once **每 tick 一次重派、跨 tick 无上限**(schedule_evaluator.rs:119-127 实证,注释原文 per scheduler tick);实际上界 = 预算 reserve 失败 / campaign 终止 | 既有 provider_failure 路径 | 失败注入 mock 测试 |
| F-002 | 第 k>1 页**可恢复(=RateLimited,D1 冻结触发集)**失败(已有进展) | 截断返回 + `COMPLETED_WITH_PARTIAL_ERRORS`;已扫数据保留。注:非可恢复中途失败归 F-001 语义(整体 Err);已收集未落库内容丢弃属预期(I-005 仅保护已落库进展)。触发前提:RateLimited 先经 client 层既有重试(透明);**retry 耗尽后**冒出的 RateLimited 才触发截断语义(见 §3 重试行;facebook.rs:569-576 先例一致)。 | facebook RateLimited 分支先例(facebook.rs:569-576);orchestrator.rs:412 | 失败注入 mock 测试 |
| F-003 | 上游枯竭(总量 < max_count) | 处理全部可得 + `NO_MORE_POSSIBLE_DATA`;scheduler 完结理由区分(R-013) | reason 枚举已存在(progress_tracker.rs:79) | mock 测试(少量页后 has_more=false) |
| F-004 | 上游返回重复 cursor / 循环 cursor | seen-cursor 检测 → 终止,按已得内容走 F-003 语义 | facebook seen-cursor 先例 | mock 测试 + PT-2 |
| F-005 | 上游返回空页但声称 has_more | 空页计数达上限(样板=3)→ 终止,F-003 语义 | facebook `MAX_EMPTY_CURSOR_HOPS` | mock 测试 + PT-2 |
| F-006 | RateLimited 中途发生 | 同 F-002(有进展)或 F-001(零进展) | facebook 先例 | mock 测试 |
| F-007 | task 重派后重扫相同内容(failed 重派路径) | `(task_id, video_id)` 为 task 级唯一 → 新 task 重新入库属预期(预算重复消耗:R-010 不补派决策之外,failed 重派为**每 tick 一次重派、跨 tick 无上限**(schedule_evaluator.rs:119-127 实证,注释原文 per scheduler tick);实际上界 = 预算 reserve 失败 / campaign 终止) | inventory §3.1 去重约束 | real-DB gate 现有约束;文档化 |
| F-008 | scheduler 误报(枯竭被当欠扫告警) | WARN 条件含 terminal_reason ≠ NO_MORE_POSSIBLE_DATA,枯竭不告警 | — | scheduler 单测两向断言(欠扫告警/枯竭不告警) |
| F-009 | 存储过程缺失(降级环境) | terminal_reason 写入 fallback 已存在(postgres.rs:2123-2181),新 reason 路径必须兼容 | 既有 fallback | 既有判定单测(is_missing_terminal_reason_complete_task_function)+ postgres.rs 零 diff 自查(M1-T7/M2-T7);T-054 仅覆盖主路径(prod-shape DB 上 fallback 分支不被驱动) |
| F-010 | 翻页中途失败且过滤后零交付(原始有进展) | 整体 `Err` → task failed(D1 构造不变量:PartialFailure ⇒ contents 非空,空+Partial 不可构造) | Step 06 DR-01 发现的未定义形状(date-filter 全滤除 + 第 2 页 429) | M1-T3 测试 7(构造器)+ M2 date-filter 形状测试 |

## 3. 横切关注

| 维度 | 裁决 |
|------|------|
| 重试 | 页级重试不在本计划新增(沿用 client 层既有行为);task 级重试 = eval_once 对 failed 每 tick 重派一次、跨 tick 无上限(DR-22 实证语义;上界 = 预算 reserve 失败/campaign 终止),语义不变(R-008 保护) |
| 幂等 | task 内:I-003 去重;task 间:F-007(task 级唯一,预期行为,文档化) |
| 依赖 | TikHub(tiktok/reddit/twitter/instagram)、Facebook RapidAPI、prod-shape Postgres、Redis 协议 —— 见 `production-dependencies.md` |
| 安全 | API key 不入日志(既有约定);terminal_reason 消息脱敏(I-009);不新增任何 secret 面 |
| 可观测 | agent:每页 fetch 日志含累计数/终止原因;scheduler:WARN + 指标(R-010);两者为验收证据的一部分 |
| 集成门 | cross-service 契约(`cross-service-contracts.md`);变异门禁(`anti-gaming-test-quality.md`);real-provider/real-DB gates(`production-dependencies.md`) |
| 进展定义 | 全平台统一:进展 = 进入 `FetchOutcome.contents` 的**过滤后**条目数;degraded(PARTIAL)/failed 边界、PartialFailure 触发判定均以此口径(Step 06 DR-09) |
