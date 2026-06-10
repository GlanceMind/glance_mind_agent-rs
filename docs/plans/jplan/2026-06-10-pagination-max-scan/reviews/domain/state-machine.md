# Domain Review — State-Machine / Concurrency-Resource Reviewer(2026-06-10,只读子 agent 原文)

> 归一化映射:F-1→DR-01(P1,与 failure-recovery#1 双确认)、F-2→DR-04(P1)、F-3→DR-18(P2)、F-4→DR-10(P2,与 failure-recovery#3 同源)、F-5→DR-17(P2)、F-6/F-7/F-8/F-9→P3 批。

核对范围:M1 §2 D2/D3/D5、M2 §2.2/§2.3/§6.1、M3 §2.2、M4 §2.1、M5 §2.2/2.3、M6 §2.2/2.3、root §1/§2、invariants-failures、04-adjudications(D-01/D-03/D-08);源码 facebook.rs:518-737、orchestrator.rs:344-431/452-514、schedule_evaluator.rs:86-151、scheduler lib.rs:147-199/271-359、scheduler entity.rs:164-196、postgres.rs:2119-2200。

## F-1 [High → DR-01] D3 终态映射缺行:`contents 空 + PartialFailure` 可达但无映射,会把瞬时 429 误判为枯竭并停 campaign
- Evidence:D3 表「contents 空」仅配 `(Exhausted 或 None)`;M2-T2 GREEN 按**原始** posts 判进展(facebook.rs:569),交付物却是 `filter_posts(posts)`(facebook.rs:581)——date-filter 任务可 raw 非空、filtered 为空 → `FetchOutcome{contents: [], shortfall: Some(PartialFailure)}`;orchestrator 先判 `contents.is_empty()` 早退(L470-513)。
- Consequence:落入零结果路径 → `stop_campaign_gracefully` + NO_MORE_POSSIBLE_DATA:中途限流被报成枯竭、RECURRING campaign 被 agent 错杀(D-03 明文要避免);M6 写 SEARCH_EXHAUSTED 持久化误分类。测试矩阵无此格。
- Patch:M1 D1 冻结构造不变量「PartialFailure ⇒ contents 非空;零可交付进展一律 Err(F-001)」+ 构造器/出口归一化 + 单测;M2-T2 补「date-filter 全滤除 + 第 2 页 429」形状(预期 Err);或 D3 显式补行「空+PartialFailure → 失败路径、不停 campaign」。

## F-2 [High → DR-04] I-006/I-001 任务级上界在多 keyword 下不成立:解 cap 后 consume 可超 reserve
- Evidence:reserve = `calculate_task_cost(platform, max_count)` 一次性(lib.rs:325-329),与 keyword 数无关;keywords 逗号分隔可多个(lib.rs:272/297/480-495);orchestrator 每 keyword 调 fetch、count=max_videos、跨 keyword 累加(orchestrator.rs:380-383)。修 cap 前被 `min(20)` 意外压低;修复后 K≥2 任务可处理 K×max_count,consume > reserve(I-006「150=50×3」)。I-001 写「单 task ≤ max_count」,全计划只在单 keyword 层验证;T-054 验守恒不验上界。
- Patch:root §2 三选一:(a) 实证 ONCE task 单 keyword 并写入 assumptions;(b) orchestrator 跨 keyword 传 remaining + K≥2 任务级测试;(c) 书面接受并记录预算后果(用户裁决)。建议至少加 K=2 确定性形状。

## F-3 [Medium → DR-18] M3 §2.2 伪代码自相矛盾:`cursor 或 offset+len` 本地合成游标 vs「cursor 缺失归一化 Exhausted」
- 两段直接冲突;本地合成续页信号与 I-004 精神同源冲突;`cursor.parse()` 失败无出口(第六条隐形路径,可能 panic/死循环)。
- Patch:删「或 offset+len」;补「cursor 非数字/parse 失败 → next=None(Exhausted)」+ mock 非数字 cursor 测试。

## F-4 [Medium → DR-10] 错误出口「可恢复集合」未冻结:(不可恢复 × 有进展)格各平台可各自发挥
- facebook 仅 RateLimited 有 partial 分支,其余错误即使收 20 条也 `return Err`(facebook.rs:577);M3/M4 写「可恢复错误」无定义;F-002 账本措辞泛化。写宽 → 不可恢复失败被吞进 degraded COMPLETED 不再重派;写窄 → F-002 行虚标。
- Patch:M1 D1/D2 冻结 recoverable = 仅 `RateLimited`;F-002 行加范围注;M2/M3/M4 各加「硬错误(500)+ 有进展 → Err」钉子。

## F-5 [Medium → DR-17] T2.8 对齐断言防漂移域有限:filter 分歧域与另两条循环在断言之外
- T2.8 只覆盖无 date-filter、单循环、四形状;真正分歧域(filtered 计数下的达量/枯竭)无测试;`fetch_page_posts_paginated` 与两级 candidates 循环的 shortfall 接线无测试;嵌套聚合规则(内层 Exhausted 不得外泄)未写。
- Patch:(a) T2.8 声明对齐域;(b) 补 date-filter 形状测试(raw 50/filtered 30 → contents 30 + Exhausted;filtered 达 count → None);(c) GREEN 写明「shortfall 只由最外层循环最终出口决定」+ candidates 测试(内层枯竭+外层达量 → None);(d) M2-T7 把「三循环出口各有 killing 测试」入不可豁免清单。

## F-6 [Low] 降级环境 fallback 写序竞态:legacy 路径先置 completed 再补写 terminal_reason(postgres.rs:2169-2186),M6 窗口内读到 NULL → 永久误写 ONCE_EXECUTED。Patch:fallback 分支调换写序(terminal_reason 先写)或 M6 §2.1 文档登记 + T-054 备注。

## F-7 [Low] M6 §2.3「completed_reason 唯一写方 = scheduler」系统层面过宽:DB 过程/主后端另有写方(BUDGET/FINALIZED 白名单为证;fn_complete_task 注释提示过程内可动 campaign 终态)。Patch:改「worker 仓应用层唯一写方」+ 注;确认 mark_campaign_completed 对非 active campaign 幂等。

## F-8 [Info] StopReason 同页多信号优先序未定义(终态面已被 PT-4「枯竭族且 accepted<max」合取兜住,无终态级后果)。Patch:D2 注「ReachedMaxCount 优先」+ 组合信号单测,使 T2.8 输入无歧义。

## F-9 [Info] 0/1 内容边界 campaign 命运不连续 —— 确认为已裁决保留项(现状回归保护;M1 §6.1/M2 §6.2 已登记),**不否决**;建议 root §2 加显式接受声明。注:F-1 修复后此边界才真正只剩 Exhausted/None 两形状。

## 无发现部分检查清单
M6 两函数无分叉(MarkCompleted 唯一产点、同 cycle 同实体、无 TOCTOU;max/process_count NOT NULL 无缺格);翻页循环全栈局部无跨任务泄漏;多 keyword 聚合实为 last-Some-wins(已注,计划外接受——并入 DR-11);I-006 reserve 面零新增成立(consume 侧风险 = F-2);工作树 `M src/adapters/postgres.rs` 须干净基线起算(并入 P3 批);终止五出口除 F-3/F-4 缺格外完备。
