# Cross-Service Contract Ledger(Step 02)

> 范围:跨仓共享的协议字段与取值(scheduler ⇄ agent-rs ⇄ API/admin/前端/e2e)。
> 规则:每条契约写明 写方/读方(带 file:line 证据)、变更类型、破坏性评估、验证方式。所有读方枚举均为 2026-06-10 本地工作区 grep 实证(排除 node_modules、`.claude/worktrees`、`.worktrees`、`.vidu-e2e-ctx` 镜像)。

## 1. 契约条目

| ID | 契约 | 写方 | 读方 | 本计划变更 | 破坏性评估 | 验证 |
|----|------|------|------|------------|------------|------|
| C-001 | Redis crawler task 协议(`max_count`/`search_limit`/`search_offset` 字段) | scheduler `dispatch_task`(lib.rs:313-383;`search_limit=page_size`、`search_offset=total_scanned`,lib.rs:354-355) | agent-rs `to_domain_task_config`(redis.rs:438-465;现状仅用 `max_count`,两字段被忽略) | **形状零变更**;仅语义固化(见 C-002)。agent 侧从「忽略 search_limit」变为「用作单页提示」 | 无:字段已存在且 scheduler 已在写;agent 行为变化向后兼容(老 task 无 limit 时回退平台默认页大小) | T-003(映射单测)、PT-5/T-034(offset 行为等价);P-006 |
| C-002 | `search_limit` = 单页大小提示(agent clamp 到平台页上限);`search_offset` = 仅观测字段,agent 不读、不参与取数;**不删字段、不动 schema** | scheduler 继续按现状写(page_size / total_scanned) | agent-rs(读 limit 作页提示);运维/调试(读 offset 作观测) | 语义文档化:两仓代码注释 + 本契约行为权威定义 | 无(DECIDED,01 §5.3;推导:3/4 可分页上游为 opaque cursor,offset 无上游语义,B3) | I-008 断言「offset 任意值行为不变」;N-002(文档落地);Step 06 Cross-Service Reviewer 复核 |
| C-003 | `gm_campaigns.completed_reason` 取值集(**O1 闭环**,读方枚举见 §2) | scheduler `mark_campaign_completed(id, "ONCE_EXECUTED")`(lib.rs:166-177、db.rs:113-131);glance_mind_rust DB 过程写 `'BUDGET_EXHAUSTED'`/`'FINALIZED'`(crates/db/migrations/2026-01-21-100000_add_budget_procedures/up.sql:187、398) | 见 §2 枚举表 —— **全部读方为自由文本透传/展示,唯一取值敏感读方是 gm-e2e 的子串白名单** | 新增枯竭区分值,候选 `SEARCH_EXHAUSTED`(R-013) | **无破坏(已实证)**:列为 `Nullable<Text>` 非枚举(glance_mind_rust crates/db/src/schema.rs:746);无任何读方做穷举匹配;gm-e2e 子串白名单含 `"EXHAUST"`,`SEARCH_EXHAUSTED` 直接通过(test_10_status_transitions.py:216-218)。**约束**:Step 04 最终值必须含 `BUDGET/EXHAUST/FINALIZED/ONCE_EXECUTED` 之一为子串,否则需改 gm-e2e 断言(走该仓 ASSERTION-CHANGE-JUSTIFIED 流程)——选 `SEARCH_EXHAUSTED` 则完全规避 | T-021(scheduler 单测写区分值);部署后 SQL 抽查(N-003) |
| C-004 | `gm_crawler_tasks.terminal_reason` 取值集(agent 写 → scheduler 新增读取) | agent-rs `TaskTerminalReason::{completed, completed_with_partial_errors, no_more_possible_data, provider_failure, cancelled, internal_error}`(progress_tracker.rs:60-98;持久化 postgres.rs:2123-2181,含存储过程缺失 fallback) | **新读方 = scheduler eval_once**(R-010 WARN 条件、R-013 枯竭区分都需读 task 的 terminal_reason;现状 scheduler 不读此列,需在 db.rs 加查询) | agent 侧:不新增值,只把翻页枯竭/部分失败接到既有值;scheduler 侧:**新增跨服务读依赖**(本计划唯一新增的契约边) | 低:值集不变;**约束**:agent 不得重命名既有值字符串;scheduler 读方必须容忍未知值/NULL(向前兼容,老 task 无 terminal_reason) | T-002/T-016(agent 写入正确值);T-020/T-021(scheduler 按值分支);P-005(real-DB 持久化);Step 06 Cross-Service Reviewer |
| C-005 | **验证任务 V1**:Instagram TikHub `general_search`(V3/V2)请求端是否接受分页 token(`max_id`/`pagination_token`/`rank_token`) | —(上游 TikHub 契约事实) | agent-rs instagram 适配器(分支选择:翻页 vs 单页+如实上报) | 方法:(a) TikHub OpenAPI 文档核查(N-001);(b) real-provider gate 探测 T-053/P-004(带首页 `next_max_id`(V3)/`pagination_token`(V2) 重发)。判定:非错误且内容异于首页 → 支持翻页;4xx 或相同首页 → 单页能力 | **阻塞面**:仅 R-006/T-014/PV-005 的分支选择;不阻塞 R-001~R-005、R-007~R-013(P0 facebook、P1 tiktok/reddit/twitter 不受影响) | 判定结果写回 `assumptions.md` V1 行 + 本条目;Step 04 instagram 模块任务前置 |

## 2. C-003 读方枚举证据(O1,2026-06-10 实证)

| 仓 | 位置 | 读取方式 | 对新增值敏感? |
|----|------|----------|----------------|
| glance_mind_rust(API) | `crates/db/src/schema.rs:746`(`Nullable<Text>`)、`crates/db/src/entity/campaign.rs:45`(`Option<String>`) | DB 层透传,无值匹配 | 否 |
| glance_mind_rust(DB 过程) | `crates/db/migrations/2026-01-21-100000_add_budget_procedures/up.sql:363`(finalize 检查) | 仅 `IS NOT NULL` 判断,不匹配具体值 | 否 |
| glance_mind_rust(e2e py) | `crates/api/tests/e2e/test_02_campaign_tiktok.py:79` | 断言新建 campaign 为 `None` | 否(只断言初始态) |
| glance_mind_admin(backend) | `backend/src/models/campaign.rs:64,254,286,319`、`backend/src/schema.rs:539` | `Option<String>` 透传;integration_test.rs:5303/5406 断言新建为 null | 否 |
| glance_mind_admin(frontend) | `frontend/src/pages/campaigns/show.tsx:73,192`(原样展示 `\|\| "-"`)、`edit.tsx:280`(自由文本表单项) | 字符串原样展示/编辑 | 否 |
| glance_mind_front(用户前端) | `apps/` 全树 grep `completed_reason\|ONCE_EXECUTED\|SEARCH_EXHAUSTED` **零命中** | 不读取 | 否 |
| gm-e2e | `tests/test_10_status_transitions.py:215-219` | **子串白名单**:`any(r in reason_upper for r in ("BUDGET","EXHAUST","FINALIZED","ONCE_EXECUTED"))` | **唯一取值敏感读方**;`SEARCH_EXHAUSTED` 含 `EXHAUST` → 通过 |
| glance_mind_worker(写方) | `glance_mind_scheduler/src/lib.rs`(写 ONCE_EXECUTED)、`docs/playbooks/2026-04-26-campaign-121-recovery.sql`(运维剧本引用) | 写方/剧本,非读契约 | — |

**O1 裁决:CLOSED-SAFE。** 新增 `SEARCH_EXHAUSTED` 不破坏任何读方;最终命名 Step 04 确认(须满足 C-003「子串约束」)。

## 3. 契约不变约束(给 Step 04/06)

1. 本计划**不改任何跨服务消息/表的形状**(无新字段、无删字段、无 schema migration)——只改字段语义文档与取值。
2. agent-rs 不得重命名 `TaskTerminalReason` 既有序列化字符串(C-004 读方依赖)。
3. scheduler 新增的 terminal_reason 读取必须容忍 NULL/未知值(滚动部署期间新旧 agent 并存)。
4. completed_reason 新值须满足 gm-e2e 子串白名单(C-003),否则升级为三仓改动并走断言变更流程。
