# Domain Review — Failure/Recovery Reviewer(2026-06-10,只读子 agent 原文)

> 归一化映射:#1→DR-01(P1,与 state-machine#F1 双确认)、#2→DR-09(P2)、#3→DR-10(P2,与 state-machine#F4 同源)、#4→DR-22(P2)、#5/#6→P3 批。

核对范围:invariants-failures 全部 F-001~F-009 与 §3、M1 §2 D1~D5 + M1-T4、M2~M6 任务清单、root §1~§5、04-adjudications;源码 facebook.rs:518-644、orchestrator.rs:330-513、progress_tracker.rs、postgres.rs:2119-2211、schedule_evaluator.rs:109-151。

## #1 High → DR-01:D3 缺行 `contents 空 + PartialFailure`(可达,经 date-filter 全滤除 + 第 2 页 429)
- 与 state-machine#F1 同一发现(独立得出);细节与 patch 见 summary DR-01。补充证据:facebook partial 分支判**原始** `!posts.is_empty()`(L569)但返回 `filter_posts(posts)`(L581);orchestrator 零结果路径无条件 stop_campaign(L470-513)不看 shortfall。失败矩阵无此形状行、无 owner、无测试。

## #2 Medium → DR-09:「零进展」定义跨平台不一致
- facebook = 过滤前(L569 先例);twitter(M4-T3.9)= 过滤后;tiktok/instagram = accepted(状态机口径)。同一失败形状在 fb → COMPLETED_WITH_PARTIAL_ERRORS(结算),在 twitter → failed(重派)——终态/重派行为随平台漂移,M6 underscan 判定输入不一致。
- Patch:invariants-failures §3(或 M1 D1)显式定义「进展 = 进入 FetchOutcome.contents 的过滤后条目数」;M2-T2 GREEN 改判过滤后非空(与 T2.8 一并钉死),或写入 D-01 裁决文本并补反例测试。

## #3 Medium → DR-10:F-002 行过度承诺——「第 k>1 页失败 → PARTIAL」实际仅对 RateLimited 成立;「可恢复错误」全计划未定义
- facebook 仅 429 走 partial,其余错误即使已收 30 条也整体 Err(L577,收集内容丢弃);M3/M4/M5 用语「可恢复错误」无枚举;无测试钉「非 RateLimited + 有进展」。retry 交互本身核验通过(facebook request_json 限流退避、tikhub with_retry 按 is_retryable;循环错误分支见到的是 retry 耗尽后失败)。
- 双向风险:写宽 → auth 失效/契约漂移被吞进 degraded COMPLETED 不再重派;写窄 → F-002 行虚标。
- Patch:M1 D1 冻结 PartialFailure 触发集 = `GatewayError::RateLimited`;M1-T3 与各平台补「第 2 页非 RateLimited 错误 + 有进展 → Err」RED 测试;F-002 措辞收窄,非可恢复中途失败归 F-001 语义并注明「已收集未落库内容丢弃属预期(I-005 仅保护已落库进展)」。

## #4 Medium → DR-22:「failed 一次重派」是误读——实为每 tick 重派、跨 tick 无上限
- schedule_evaluator.rs:119-127:`"failed" => Dispatch` 无计数器;注释原文 "exactly one retry **per scheduler tick**"。失败 task 每 tick 重派直至预算 reserve 失败/人工干预。冲突文本:F-001「保留一次重派」、F-007「限制在 failed 重派一次内」、M1 D3、M6 §1.10、root RT-1(将把错误声明固化成文档)。
- Patch:纯文档修正:统一改述「每 tick 一次重派,跨 tick 无上限;实际上界 = 预算 reserve 失败/campaign 终止」;RT-1 按真实语义撰写;「failed 重试上限」登记 backlog 观察项。

## #5 Low(P3):F-009「real-DB gate」验证描述高估——prod-shape DB 上三参过程恒在,fallback 分支不被 T-054 驱动;实际兜底 = 既有判定单测 + postgres.rs 零 diff 自查(推理成立:新终态走既有 code 值)。Patch:F-009 验证列改述;不建议为降级环境新增 gated 测试(成本>收益)。

## #6 Low(P3,执行风险):工作树已有 `M src/adapters/postgres.rs` 未提交改动,将污染 M1-T7/M2-T7/root 零 diff 自查与 in-diff 变异范围。Patch:root §3 加前置「各模块从干净基线分支起做;现存 postgres.rs 改动先独立 commit/revert 出计划族 diff」。

## 已核验无发现
失败注册完备(F-001~F-009 各有 owner 与载体,除 #1 缺行);retry 与翻页错误分支交互无双重重试;I-009 脱敏链路无绕过(唯一持久化入口经构造器;set_task_error 只落 as_terminal_message;M6 WARN 读到的已是脱敏值);I-005 部分结果保留断言齐;从状态可重建(持久化 + fallback 双写 + M6 NULL 容忍);回滚语义与机制吻合;D-03/D-08 一致(#1 是唯一绕过面)。
