# Requirement Ledger(Step 02)

> 来源标记:`user-stated`(用户直述)/ `incident`(生产事故证据)/ `derived`(由 bedrock 推导,引用 `01-first-principles.md`)。
> 规则:无任何条目 `origin: assumed`。owner 取 manifest 模块队列名;Step 03 可重排但 ID 不变。

| ID | Requirement | Source | Decision | Owner(module) | Acceptance Evidence | Origin | First-Principles 依据 |
|----|-------------|--------|----------|----------------|---------------------|--------|------------------------|
| R-001 | Strategy 层「总量」与「单页大小」解耦:`options.count` 传真实目标量(max_videos),不得被单页上限截断(消除 `src/strategies/facebook.rs:131` `v.min(20)` 类截断;同类行:tiktok.rs:96、reddit.rs:100、twitter.rs:123、instagram.rs:99) | incident + user-stated | DECIDED | agent-rs/platform-strategies | 单测:strategy 对 max_count=50 产出 options.count=50(每平台一条);RED 证据 = 现状产出 20(fb) | derived | B1/B2;01 §7 construction 1 |
| R-002 | Facebook 端到端达量:max_count=50 且上游充足时实际处理 50 条(适配器翻页循环已存在,facebook.rs:522-582;只需 R-001 解除截断后接通) | incident | DECIDED | agent-rs/platform-strategies | mock-HTTP 多页测试:3 页(20+20+10)→ 50 条入处理管道;终态 COMPLETED | incident(campaign 269) | B2/B3;01 §3 facebook 行 |
| R-003 | TikTok 适配器内 offset 翻页循环:循环调用直到达 `options.count` 或上游枯竭(`has_more`/空页),单页≤20(types.rs:297) | user-stated | DECIDED | agent-rs/platform-strategies | mock 多页测试:offset 在请求间递进、has_more=0 时停止;proptest 不变量(I-001/I-002) | derived | B1/B3;01 §3 tiktok 行 |
| R-004 | Reddit 适配器 content 路径启用 `after` cursor 翻页(复用评论侧既有模式 reddit.rs:292-371) | user-stated | DECIDED | agent-rs/platform-strategies | mock 多页测试:after 转发、`pageInfo.hasNextPage=false` 时停止 | derived | B1/B3;01 §3 reddit 行 |
| R-005 | Twitter 适配器 content 路径启用 cursor 翻页(复用评论侧既有模式 twitter.rs:331-462) | user-stated | DECIDED | agent-rs/platform-strategies | mock 多页测试:next_cursor 转发、cursor 缺失/重复时停止 | derived | B1/B3;01 §3 twitter 行 |
| R-006 | Instagram:gated on V1 —— 支持请求端 token → 同构翻页;不支持 → 保持单页并如实上报 `NO_MORE_POSSIBLE_DATA`(不得静默 COMPLETED) | user-stated | DECIDED(分支待 V1) | agent-rs/platform-strategies | V1 判定记录 + 对应分支的 mock 测试(翻页 or 单页+terminal reason) | derived | B2/B3;01 §5.2;assumptions V1 |
| R-007 | fetch 路径向 orchestrator 暴露「欠交付原因」(exhausted / partial-failure),orchestrator 映射到既有终态 `NO_MORE_POSSIBLE_DATA` / `COMPLETED_WITH_PARTIAL_ERRORS`(progress_tracker.rs:60-98;cursor 保持适配器内部,不过 trait) | incident | DECIDED | agent-rs/pagination-core | 单测:exhausted→NO_MORE_POSSIBLE_DATA、partial→COMPLETED_WITH_PARTIAL_ERRORS 的映射断言;现状 RED = trait 返回 `Vec<Content>` 丢失原因(content_gateway.rs:33-37) | derived | B2;A005 drop 后的最小契约缺口;01 §6 |
| R-008 | 翻页中途页失败语义:本 task 已落库 >0 条 → `COMPLETED_WITH_PARTIAL_ERRORS`;零进展 → task `failed`(保留 eval_once 对 failed 的一次重派,schedule_evaluator.rs:119-127) | derived | DECIDED | agent-rs/pagination-core | 失败注入 mock 测试两条(有进展/零进展);RED 证据 = 现状无此分支 | derived | B4(已扫已付成本);01 §5.4 |
| R-009 | 翻页循环安全终止与守恒:任意页序列下处理数 ≤ max_count;seen-cursor/offset 防环;空页上限;重复内容不重复计数(`(task_id, video_id)` 唯一) | derived | DECIDED | agent-rs/pagination-core | proptest 不变量套件(I-001~I-003)+ 变异门禁无 missed | derived | B4 成本上界;A011;facebook.rs:28/461-495 既有实现为样板 |
| R-010 | scheduler `eval_once` 防御性校验:completed task 且 `process_count < max_count` 且 terminal_reason ≠ NO_MORE_POSSIBLE_DATA → WARN 日志 + 指标;**不自动补派**(保留升级扩展点) | user-stated | DECIDED(推荐,Step 04 评审确认) | scheduler/once-guard | scheduler 单测(test_schedule_evaluator.rs 模式):欠扫场景产生 WARN 决策、状态机不变 | derived | B1/B4;01 §5.6 |
| R-011 | 跨服务字段语义固化:`search_limit` = 单页大小提示(agent 可用作单页 count,clamp 到平台页上限);`search_offset` = 仅观测,agent 不读;不删字段、不动 schema | derived | DECIDED | scheduler/once-guard + agent-rs/pagination-core | 两仓代码注释 + 契约文档(C-002);agent 侧单测:search_limit 作页提示被 clamp | derived | B3(opaque cursor 无 offset 语义);01 §5.3 |
| R-012 | 预算与 schema 零改动:reserve 按 max_count 粒度、consume 按条递增模型不变;不触发 db-migration-guard | derived | DECIDED(约束型需求) | 全模块 | diff 审查:无 migration/schema/预算函数改动(postgres.rs 进度过程、scheduler lib.rs:325 不动) | derived | B4;A012;01 §7 gate findings |
| R-013 | ONCE campaign 因上游枯竭欠扫完结时,`completed_reason` 须可区分于正常 `ONCE_EXECUTED`(候选值 `SEARCH_EXHAUSTED`;读方契约已验证安全,见 C-003) | derived | DECIDED-推荐(具体值与读取 terminal_reason 的方式 Step 04 定) | scheduler/once-guard | scheduler 单测:枯竭欠扫场景写入区分值;C-003 读方证据 | derived | B2(如实记录原因);01 §5.5;O1 已闭环(C-003) |

## 验证任务(计划内,非 assumed)

| ID | 内容 | 阻塞 | 详情 |
|----|------|------|------|
| V1 | Instagram TikHub general_search 请求端分页能力 | 仅阻塞 R-006 分支选择;不阻塞 R-001~R-005/R-007~R-013 | 方法与判定标准见 `ledgers/assumptions.md` 验证任务表;登记于 `ledgers/cross-service-contracts.md` C-005 与 `ledgers/non-code-exceptions.md` N-001 |
