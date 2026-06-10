# 01 - First-Principles Reduction

> 输入:`00-manifest.md`、`00-context-inventory.md`、`constraints/testing-constraints.md`、本地源码核查(本文档所有行号均为 2026-06-10 工作区状态)。
> 产物:本文件 + `ledgers/assumptions.md`。

## 1. 不可再分目标(solution-free)

**一个 ONCE campaign 配置 `max_scan_count=N` 时,系统要么实际扫满 N 条内容,要么如实记录扫不满的原因(上游枯竭/上游失败);不得在未达 N 且无记录原因的情况下被静默标记 COMPLETED。**

(注意:目标里没有「翻页」「cursor」「trait」——这些都是机制。生产事故的本质是「静默欠交付」。)

## 2. 子问题分解(按作用力分开)

| ID | 子问题 | 作用力(为何独立) |
|----|--------|--------------------|
| S1 | 单 task 内取数必须能达到 max_count | 外部硬契约:各上游单页条数上限(20/页等);成本:每条消耗预算 |
| S2 | 各平台上游分页契约差异 | 外部契约逐平台不同(cursor/offset/after/token/未知),变化速率独立 |
| S3 | 终止语义的真实性(扫满 / 枯竭 / 部分失败 / 整体失败) | 失败模式:四种终态语义不同;数据形状:terminal_reason 必须能区分 |
| S4 | 跨服务字段契约(`search_offset`/`search_limit`/max_count) | 行为者不同(scheduler 写,agent 读);两仓独立部署,变化速率不同 |
| S5 | scheduler 防御性检测 | 失败模式:防回归/未知 bug,与 S1 的正确性修复是不同层 |
| S6 | 预算一致性 | 成本力:reserved 按 max_count 粒度(150=50×3),consumed 按条递增 |

## 3. 平台分页能力裁决表(事实,带代码证据)

> 这是对 `00-context-inventory.md` §6 第 1、2 条的本地代码核查结果。结论:**五个平台中四个由代码证据判定支持分页;Instagram 请求端能力本地不可判定,转为计划内验证任务 V1(见 §5),不留 ASSUMED。**

| 平台 | 上游 | 请求端分页参数(证据) | 响应端分页字段(证据) | 适配器现状 | 裁决 |
|------|------|------------------------|------------------------|------------|------|
| facebook | RapidAPI `facebook-scraper3.p.rapidapi.com`(`src/adapters/facebook.rs:24-25`) | `cursor` query 参数,`append_cursor_query`(facebook.rs:313-322) | 响应顶层 `cursor`,`next_cursor()`(facebook.rs:309-311) | **适配器已实现完整 cursor 翻页循环**:`search_posts_paginated`(522-582)、`fetch_page_posts_paginated`(584-644),含按 post_id 去重、seen-cursor 防环、`MAX_EMPTY_CURSOR_HOPS=3`(L28),终止条件 `reached_post_limit` = `options.count`(518-520);mock-HTTP 测试证明 cursor 在请求间转发(facebook.rs:1169-1203、1269-1311) | **支持 cursor;翻页已实现**。事故瓶颈不在适配器,在 strategy 把 `options.count` 截到 20(`src/strategies/facebook.rs:131` `v.min(20)`),循环到 20 即「达标」停止 |
| tiktok | TikHub `/api/v1/tiktok/app/v3/fetch_video_search_result` | `offset` + `count`(client 实际发送:`src/tikhub/client.rs:274`;`SearchParams.with_offset` types.rs:301-304;**单页 count 上限 20**,types.rs:297 注释 "TikHub max is 20") | `SearchData.has_more: Option<i32>`、`cursor: Option<i64>`(`src/tikhub/types.rs:25-26`) | `search()` 把 has_more/cursor 丢弃,只取 videos(`src/adapters/tikhub.rs:186-187`),单次调用 | **支持 offset 翻页**(响应另含 has_more/cursor 可作终止信号);单页≤20,达 50 需循环 |
| reddit | TikHub `/api/v1/reddit/app/fetch_dynamic_search` | `after` cursor(client 实际发送:client.rs:1249-1253;`RedditSearchParams.after` reddit_types.rs:357-361、with_after 388) | `pageInfo.hasNextPage`/`endCursor`(reddit_types.rs:61-62、95-99) | content search 不传 `after`、忽略 pageInfo(`src/adapters/reddit.rs:160-178`);**评论侧已在用同一机制翻页**(reddit.rs:292-371) | **支持 cursor(after)翻页**;content 路径未启用 |
| twitter | TikHub `/api/v1/twitter/web/fetch_search_timeline` | `cursor`(client 实际发送:client.rs:1496-1500;`TwitterSearchParams.cursor` twitter_types.rs:360-378) | `TwitterTimelineData.next_cursor`(twitter_types.rs:44) | `search_params()` 从不设置 cursor(`src/adapters/twitter.rs:161-163`);评论侧已用 cursor 翻页(twitter.rs:331-462) | **支持 cursor 翻页**;content 路径未启用 |
| instagram | TikHub `/api/v1/instagram/v3/general_search`(V2 fallback) | **本地不可判定**:V3 请求只发 `query`+`enable_metadata`(client.rs:716),V2 只发 `keyword`(client.rs:764);general_search 无任何 token 参数结构(其余 instagram 端点有 `with_pagination_token`,instagram_types.rs:496-594,但 search 没有);仓内无 vendored TikHub OpenAPI spec(find 验证为空) | V3 响应含 `rank_token`(86)、`next_max_id`(88)、`has_more`(90);V2 响应含 `pagination_token`(instagram_types.rs:62-63) | 单页 + `take(count)`(`src/adapters/instagram.rs:285-294`) | **响应端有分页 token,请求端能否回传不可由代码判定 → 计划内验证任务 V1**(方法与判定标准见 §5,不留 ASSUMED) |

各平台 strategy 单页 cap 现状(S1 的直接根因面):
`facebook.rs:131 min(20)`、`tiktok.rs:96 min(20)`、`reddit.rs:100 min(100)`、`twitter.rs:123 min(100)`、`instagram.rs:99 min(50)`(均在 `src/strategies/`)。

## 4. 既有事实(支持后续推导)

- orchestrator 每关键词只调一次 `fetch_by_keyword`(`src/orchestrator.rs:668-682`),无循环;contents 全量返回后逐条处理。
- terminal reason 体系已存在并持久化:`TaskTerminalReason::{completed, completed_with_partial_errors, no_more_possible_data, provider_failure, cancelled, internal_error}`(`src/ports/progress_tracker.rs:60-98`);写入 DB(`src/adapters/postgres.rs:2123-2181`,含存储过程缺失时的 fallback)。
- Facebook 适配器对 RateLimited 已实现「部分进展则带 WARN 截断返回」(facebook.rs:569-576)——partial 语义有先例。
- scheduler `eval_once`:completed→无条件 MarkCompleted(不看进度),failed→允许重派一次(`schedule_evaluator.rs:109-127`)。
- scheduler `dispatch_task`:`search_offset = campaign.total_scanned`、`search_limit = page_size`(`lib.rs:354-355`);agent 侧 `to_domain_task_config` 只用 `max_count`,两字段均被忽略(`src/adapters/redis.rs:449-465`)。
- 预算:reserve 按 `max_count` 成本粒度(`lib.rs:325` calculate_task_cost),consume 按条递增(postgres.rs 进度存储过程)——**单 task 内多翻几页不改变预算模型**。

## 5. 六个未决问题的裁决

1. **Facebook RapidAPI 是否支持 cursor 分页?** — **DECIDED(事实,代码+测试证据)**:支持,且适配器已完整实现翻页(§3 第一行)。修复面收敛为:strategy 不得把总量截到 20(单页大小与总量解耦),其余机制 Step 04 定。
2. **其余平台上游分页形态?** — **DECIDED(事实)**:tiktok=offset(单页≤20,响应含 has_more/cursor);reddit=after cursor;twitter=cursor —— 三者 client 层参数与响应字段齐备,仅 content 路径未启用。**instagram=计划内验证任务 V1**:
   - **方法**:(a) 查 TikHub OpenAPI 文档(https://api.tikhub.io/docs)中 `/api/v1/instagram/v3/general_search` 与 `/v2/general_search` 接受的 query 参数(是否有 `max_id`/`pagination_token`/`rank_token`);(b) 若文档不明,在 real-provider gate 内做一次探测:用首页响应的 `next_max_id`(V3)或 `pagination_token`(V2)作为参数重发请求。
   - **判定标准**:带 token 的请求返回非错误且内容与首页不同 → 支持翻页,进入与其他平台相同的构造;返回 4xx 或忽略参数返回相同首页 → 单页能力,Instagram 走「单页 + 如实 `no_more_possible_data`/partial 上报」。
   - **时机与不阻塞性**:作为 Step 04 instagram 模块任务的前置验证;其结论不阻塞 facebook(P0)与 tiktok/reddit/twitter(P1)。
3. **`search_offset`/`search_limit` 语义** — **DECIDED**:`search_limit` = 单页大小提示(scheduler 继续写 page_size;agent 可用作单页 count 提示,clamp 到平台页上限);`search_offset` = 仅观测字段,agent 不读、不参与取数(scheduler 继续写 total_scanned 以保向后兼容)。**理由(推导自外部契约 bedrock)**:四个可分页上游中三个是 opaque cursor(无 offset 语义),TikTok 的 offset 也只在同一查询会话内有意义;跨 task 的 offset 续扫要求上游全局稳定排序——无任何上游承诺。不删字段、不动 schema(避免 db-migration 与跨仓破坏)。
4. **翻页中途单页失败语义** — **DECIDED**:已有部分进展(本 task 已落库 >0 条)的页失败 → task 以 `completed_with_partial_errors` 完结(机制已存在,orchestrator.rs:412;facebook 适配器 RateLimited 分支已是此形态);零进展的失败 → task `failed`(保留 `eval_once` 对 failed 的一次重派语义,schedule_evaluator.rs:119-127)。**理由**:已扫数据已付预算成本,不可因后续页失败而作废;零进展时把重试机会留给调度器。
5. **搜索枯竭完结理由** — **agent 侧 DECIDED**:cursor/offset 枯竭仍未达 max_count 时,复用既有 `TaskTerminalReason::no_more_possible_data`(progress_tracker.rs:79;terminal_reason 已持久化到 `gm_crawler_tasks`)。**scheduler 侧 OPEN(留 Step 02/04)**:campaign `completed_reason` 是否新增 `SEARCH_EXHAUSTED` 区分于 `ONCE_EXECUTED`。**决策所需信息**:枚举 `gm_campaigns.completed_reason` 的全部读方(glance_mind API/前端展示逻辑),确认新增枚举值不破坏展示契约;Step 02 列入 cross-service 契约账本。
6. **scheduler 防御动作强度** — **DECIDED(推荐,Step 04 评审确认)**:首版仅 WARN + 指标(completed task 的 `process_count < max_count` 且 terminal_reason 非 no_more_possible_data 时告警),**不自动补派**。**理由(推导自预算 bedrock)**:cursor 不可跨 task 恢复,补派只能从头扫;`(task_id, video_id)` 去重是 task 级,新 task 会为相同内容重复消耗预算;agent 侧修复消除了系统性原因,防御层职责是「检测回归」不是「纠正」;保留升级为补派的扩展点,待观测数据支持后再议。

## 6. Why-Ladder 分类

完整账本见 `ledgers/assumptions.md`。无任何条目残留 ASSUMED;关键结果:

- **BEDROCK**:B1 用户确认的设计方向(agent 单 task 内翻页到 max_count;scheduler 只做时间调度+防御校验);B2 欠交付必须如实记录(§1 目标);B3 各上游单页上限与分页契约(§3 事实表);B4 预算成本力(每条消耗,reserve 按 max_count);B5 测试约束底线(constraints/testing-constraints.md,不可挑战)。
- **关键降级**:「ContentGateway trait 必须扩展 cursor/has_more 给 orchestrator」(manifest 模块队列的初始草案)被判 **ASSUMED → drop**,重derive 出的最小需求是:**fetch 路径必须向上暴露『欠交付原因』(exhausted / partial-failure),cursor 本身保持适配器内部**(Facebook 已证明此构造,522-582)。详见 A005。

## 7. Reconstruction Note

```text
Bedrock forces:
  B1 用户确认方向(00-context-inventory §1):agent 单 task 翻页到 max_count;scheduler 不翻页
  B2 欠交付必须有记录原因(本事故的本质;§1)
  B3 上游分页硬契约(§3 事实表:fb=cursor 已实现 / tiktok=offset≤20页 / reddit=after / twitter=cursor / ig=V1 待验证)
  B4 预算成本(reserve 按 max_count、consume 按条;cursor 不可跨 task 恢复)
  B5 测试约束底线(反作弊、mock+real 双 gate、变异门禁)

Minimal construction(从 bedrock 推导,不多不少):
  1. strategies:把「总量」与「单页大小」解耦——options.count 传真实目标量(max_videos),
     单页大小由各平台页上限/search_limit 提示决定(消除 facebook.rs:131 类总量截断)。
  2. adapters:在适配器内部按各自上游契约翻页直到 options.count 或枯竭
     (facebook 已完成;tiktok offset 循环、reddit after 循环、twitter cursor 循环复制同一模式:
      去重 + seen-cursor/offset 防环 + 空页上限;instagram 待 V1)。
  3. fetch 路径暴露欠交付原因(exhausted / partial-failure),orchestrator 映射到既有
     no_more_possible_data / completed_with_partial_errors 终态(机制已存在,只需接线)。
  4. scheduler:eval_once 对 completed task 比较 process_count vs max_count,欠扫且非枯竭 → WARN+指标;
     字段语义按 §5.3 固化文档。预算与 schema 零改动。

Conventional approaches NOT taken:
  - 「扩展 ContentGateway trait 返回 cursor/has_more,orchestrator 驱动翻页循环」——不取:
    cursor 是平台 opaque 值,跨层传递只增加契约面;Facebook 适配器内部翻页已是被测试证明的构造
    (facebook.rs:522-582 + 1169-1203);bedrock 只要求「达量或说明原因」,不要求 orchestrator 看见 cursor。
  - 「scheduler 以 search_offset 多 task 续扫」——不取:无上游承诺稳定全局排序(B3),
    预算按 max_count 粒度与按页派发错配(B4)。
  - 「scheduler 自动补派兜底」——首版不取:cursor 不可恢复 + task 级去重导致预算重复消耗(B4);见 §5.6。

Gate applicability findings:
  反作弊底线(TESTING_CONSTRAINTS)            REQUIRED —— 永真,不可挑战(B5)
  feature mock-gate(确定性 mock 测试)         REQUIRED —— 每个平台翻页循环都是新 feature;
                                                mock 基建已在(facebook mock-HTTP 测试、src/testing/mock_gateway.rs)
  real-provider gate                            REQUIRED —— TikHub / Facebook RapidAPI 是真实生产依赖;
                                                且 V1(instagram)与 tiktok 第二页行为需 live 证据;独立凭据 gate,不混默认 cargo test
  属性测试(proptest)                          REQUIRED —— 翻页循环不变量:任意页序列下 已处理数≤max_count、
                                                循环必终止(seen-cursor/空页上限)、重复页不重复计数(derived from B2/B4)
  变异门禁(cargo mutants --in-diff)           REQUIRED —— CI 已存在(真·强制层);新增循环/终态映射逻辑必须在 diff 内无 missed mutants
  db-migration-guard                            INAPPLICABLE —— §5.3 决定不动 schema、不删字段(证据:terminal_reason 列已存在,postgres.rs:2123-2181)
  api-contract-guard(HTTP DTO)                INAPPLICABLE —— 本修复不改任何 HTTP 响应形状(agent-rs 无对外 API 面变更);
                                                若 Step 04 引入 DTO 变更则重新评估
  cross-service-guard                           REQUIRED —— Redis task 协议字段语义(search_limit=页大小提示)与
                                                completed_reason 取值(§5.5 OPEN)是两仓共享契约
```

## 8. 范围裁决(平台优先级)

- **P0**:facebook(事故平台;改动最小——主要在 strategy cap 与欠交付原因接线)。
- **P1**:tiktok / reddit / twitter(能力已确认,复制同一翻页模式)。
- **P2(verification-gated)**:instagram(V1 验证后定:翻页 or 单页如实上报)。
- scheduler:once-guard(WARN+指标)+ 字段语义文档化;`completed_reason` 枚举值待 §5.5 OPEN 裁决。
