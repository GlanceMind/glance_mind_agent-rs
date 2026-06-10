# Assumption Ledger(Step 01 产出;Step 02 起为只增账本)

> 规则:无条目可残留 `ASSUMED`;每条走 Why-Ladder(≥3 层或到 bedrock)。
> 类别:BEDROCK(不可再分,带来源)/ DERIVED(由 bedrock 推导,带引用)。
> Bedrock Floor(反作弊测试底线、feature⇒mock gate、real-dependency⇒real gate)不入本表、不可挑战。

| ID | Item | 子问题 | Why-ladder | Class | Disposition | 证据/推导 |
|----|------|--------|-----------|-------|-------------|-----------|
| B1 | agent 在单 task 内翻页直到 max_count;scheduler 只做时间调度+防御校验,不翻页 | S1/S5 | why?→用户 2026-06-10 明确确认的设计方向(显式 user-stated need) | BEDROCK | keep | `00-context-inventory.md` §1 |
| B2 | max_scan_count=N 必须扫满,或如实记录扫不满的原因 | S3 | why?→事故本质是静默欠交付;why?→用户为 N 条付费/预留预算;why?→错误的 COMPLETED 是错误答案(用户需求) | BEDROCK | keep | 生产事故证据(inventory §2);§1 目标 |
| B3 | 各上游单页上限与分页契约(fb cursor / tiktok offset≤20 / reddit after / twitter cursor / ig 待验证) | S2 | why?→外部 API 硬契约,代码与文档可证 | BEDROCK | keep | `01-first-principles.md` §3 事实表(逐行号) |
| B4 | 预算成本力:reserve 按 max_count 粒度、consume 按条;cursor 不可跨 task 恢复 | S6 | why?→DB 数值 150=50×3 + dispatch_task/存储过程实现;why?→cursor 是上游会话态,task 结束即失效 | BEDROCK | keep | inventory §2 预算行;scheduler lib.rs:313-383;postgres.rs 进度过程 |
| B5 | 测试约束底线(反作弊、先红后绿、mock+real 双 gate、变异门禁) | 全部 | (Bedrock Floor,不可挑战) | BEDROCK | keep | `constraints/testing-constraints.md` |
| A001 | 「Facebook RapidAPI 可能不支持分页,需要替代策略(多关键词变体/时间窗切分)」 | S2 | why?→handoff 00-to-01 的待验证项;核查代码:适配器已实现 cursor 翻页且有 mock-HTTP 测试 | DERIVED(事实裁决) | keep-justified(替代策略分支 **drop**) | facebook.rs:309-322、522-644、1169-1203;§3 表 |
| A002 | 「tiktok/reddit/twitter/instagram 分页能力未知」 | S2 | why?→inventory §6.2;核查代码:三平台 client 层参数+响应字段齐备;instagram 请求端不可判定 | DERIVED(事实裁决) | keep-justified;instagram 转 **计划内验证任务 V1**(方法+判定标准见 01 §5.2) | §3 事实表逐行号;client.rs:274/1249-1253/1496-1500 |
| A003 | 「修复 = 在 orchestrator 加 cursor 循环」(manifest 模块队列初稿措辞) | S1 | why?→「翻页就该在编排层」是类比/习惯;再问:bedrock 只要求达量或说明原因(B2);Facebook 已证明适配器内翻页满足 B2/B3 | ASSUMED→DERIVED(改写) | keep-justified(改写为:翻页位置由 Step 03/04 定,默认适配器内部;orchestrator 只需看到欠交付原因) | facebook.rs:522-582 为被测试证明的构造 |
| A005 | 「ContentGateway trait 必须扩展返回 cursor/has_more」 | S1/S3 | why?→A003 的衍生;why?→orchestrator 看 cursor 干什么?→无 bedrock 力(cursor 是平台 opaque 值);真正的力是 B2:必须区分 exhausted/partial-failure | ASSUMED | **drop**(替代:fetch 路径暴露「欠交付原因」即可,形态 Step 04 定) | trait 现返回 `Vec<Content>`(content_gateway.rs:33-37)丢失原因——这才是要修的契约缺口 |
| A006 | `search_limit` 退化为页大小提示;`search_offset` 仅观测、agent 不读 | S4 | why?→用户方向(B1)+ 三上游为 opaque cursor、无 offset 语义;why?→跨 task offset 需全局稳定排序,无上游承诺(B3) | DERIVED | keep(DECIDED,01 §5.3;不动 schema) | lib.rs:354-355;redis.rs:449-465(现状两字段已被 agent 忽略) |
| A007 | 翻页中途页失败:有进展→`completed_with_partial_errors`,零进展→`failed` | S3 | why?→已扫数据已付成本不可丢(B4);why?→零进展需保留 eval_once 的 failed 重派语义 | DERIVED | keep(DECIDED,01 §5.4) | orchestrator.rs:412;facebook.rs:569-576;schedule_evaluator.rs:119-127。**注(DR-22)**:『一次重派』为**每 tick** 语义(注释原文 per scheduler tick),跨 tick 无上限;计划族相关文本已按此改述(Step 07) |
| A008 | 枯竭时 agent 上报 `no_more_possible_data`;scheduler 侧 completed_reason 取值 OPEN | S3/S4 | why?→B2 要求区分枯竭;机制已存在只需接线;scheduler 枚举值需先枚举读方 | DERIVED + OPEN(仅 scheduler 枚举值) | keep(agent 侧 DECIDED);OPEN 项带所需信息清单(01 §5.5)交 Step 02 契约账本 | progress_tracker.rs:79;postgres.rs:2123-2181 |
| A009 | scheduler 防御 = WARN+指标,不自动补派 | S5 | why?→补派只能从头扫(cursor 不可恢复,B4);why?→task 级去重导致重复消耗预算;why?→防御层职责是检测回归非纠正 | DERIVED | keep(DECIDED-推荐,Step 04 评审确认;保留升级扩展点) | 01 §5.6 |
| A010 | 「五个平台都要在本次修复中启用翻页」 | S2 | why?→「一次修全」是习惯;事故只发生在 facebook;tiktok/reddit/twitter 能力已证、增量成本低;instagram 能力未证 | ASSUMED→DERIVED(收窄) | keep-justified(P0=facebook,P1=tiktok/reddit/twitter,P2=instagram gated on V1;01 §8) | §3 事实表 |
| A011 | 「重复页可作提前终止信号」(inventory §3.1 末行) | S1 | why?→循环必须终止(成本上界,B4);seen-cursor/重复内容/空页上限都是终止保障的实例 | DERIVED | keep(作为属性测试不变量:任意页序列下循环终止) | facebook.rs:28、461-495 既有实现 |
| A012 | 「预算模型无需改动」 | S6 | why?→reserve 已按 max_count 粒度(150=50×3),翻页只是在已预留额度内取数;consume 按条不变 | DERIVED | keep | inventory §2;lib.rs:325-345 |

## 计划内验证任务

| ID | 内容 | 方法 | 判定标准 | 时机 |
|----|------|------|----------|------|
| V1 | Instagram TikHub general_search 请求端是否接受分页 token(`max_id`/`pagination_token`/`rank_token`) | (a) TikHub OpenAPI 文档核查;(b) real-provider gate 内带上一页 token 的探测请求 | 带 token 返回非错误且内容异于首页→支持翻页;4xx 或返回相同首页→单页能力,走「单页+如实上报」 | Step 04 instagram 模块任务前置;不阻塞 P0/P1 |

## OPEN 项(交 Step 02/04)

| ID | 内容 | 决策所需信息 |
|----|------|--------------|
| O1 | scheduler 侧 campaign `completed_reason` 是否新增 `SEARCH_EXHAUSTED`(区分 `ONCE_EXECUTED`) | 枚举 `gm_campaigns.completed_reason` 的全部读方(glance_mind API/前端),确认新增枚举值不破坏展示契约;列入 Step 02 跨服务契约账本 |

## Step 02 增补(2026-06-10,只增不改)

- **O1 → CLOSED-SAFE**:读方枚举已完成(grep 实证五仓),证据与裁决见 `ledgers/cross-service-contracts.md` C-003/§2。结论:列为自由 TEXT、无读方做穷举匹配;唯一取值敏感读方 gm-e2e `test_10_status_transitions.py:216-218` 的子串白名单含 `"EXHAUST"`,候选值 `SEARCH_EXHAUSTED` 直接通过。最终命名 Step 04 确认(须满足 C-003 子串约束)。R-013 据此为 DECIDED-推荐。
- **V1 登记落位**:契约账本 C-005 + 非代码核查 N-001(文档侧)+ real-provider 探测 T-053/P-004(代码侧);阻塞面仅 R-006 分支选择。
- **新事实(账本期核查)**:① `proptest` 不在本仓依赖中(Cargo.toml 实证),引入登记为 FR-001;② scheduler 仓无任何 test/mutation CI 工作流(FR-002),AG-013 本地变异证据要求由此而来;③ 本计划引入一条**新的跨服务读依赖**:scheduler eval_once 需读 `gm_crawler_tasks.terminal_reason`(C-004),agent 既有值字符串自此为共享契约,不得重命名。
