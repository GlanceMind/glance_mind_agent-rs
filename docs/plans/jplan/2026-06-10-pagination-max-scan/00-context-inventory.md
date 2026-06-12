# 00 - Context Inventory

> 本文件只盘点路径与事实,不做设计。所有结论性证据来自 2026-06-10 生产调试会话(campaign 269)。

## 1. 用户请求来源

- 用户报告:https://glancemind.org/apps/social-monitor/campaigns/269 配置 `max_scan_count=50`,实际扫描 20 条即被标记 COMPLETED。
- 用户已确认的设计方向(2026-06-10 对话):**agent-rs 在单个 task 内按 cursor 翻页直到 max_count;调度器保留时间调度 + 防御性校验,不承担翻页**。

## 2. 生产事故证据(只读调查,均为 2026-06-10 UTC)

| 事实 | 值 | 来源 |
|---|---|---|
| campaign 269 | `max_scan_count=50, total_scanned=20, status=COMPLETED, completed_reason=ONCE_EXECUTED, schedule_type=ONCE`, platform=facebook(id=3) | prod DB `gm_campaigns`(host 47.236.115.179) |
| task 4285 | `max_count=50, search_limit=20, search_offset=0, process_count=20, status=completed, reserved=150, consumed=60` | prod DB `gm_crawler_tasks` |
| 内容落库 | 20 posts(`gm_agent_facebook_posts`),23 条 AI 建议评论 | prod DB |
| agent 行为 | 单次搜索 `Fetched content count=20`,无翻页;00:13:00 任务正常完成 | agent-rs 容器日志(Portainer endpoint 9, container `agent-rs`) |
| 调度器行为 | 00:13:06 `Campaign marked COMPLETED campaign_id=269 reason="ONCE_EXECUTED"`,未比较 total_scanned vs max_scan_count | scheduler 容器日志 |
| 预算模型 | 预留按 `max_count` 粒度(150=50×3),非按页 | scheduler `dispatch_task` + DB 数值 |

## 3. 根因代码位置(两处叠加)

### 3.1 agent-rs(本仓,主修复面)

| 路径 | 相关性 |
|---|---|
| `src/strategies/facebook.rs:131` | `config.max_videos.map(\|v\| v.min(20))` —— 单页硬上限 20 |
| `src/strategies/facebook.rs:238` | `max_videos_per_search() = 20` |
| `src/orchestrator.rs` (`fetch_content`, ~L667) | 每关键词只调一次 `fetch_by_keyword`,无 cursor 循环 |
| `src/adapters/redis.rs:438-464` | Redis task → `TaskConfig`,`with_max_videos(max_count)`;`search_limit` 字段被忽略 |
| `src/strategies/{tiktok,reddit,twitter,instagram}.rs` | 其余平台策略,均有各自 `max_videos_per_search` 上限,需逐一核对单页/翻页形态 |
| `src/ports/` (content gateway trait) | `fetch_by_keyword(keyword, options)` 签名 —— 翻页需要扩展返回 cursor/has_more |
| `src/adapters/{facebook,tikhub,reddit,twitter,instagram}.rs` | 各上游 provider 适配器;cursor 能力逐个确认(TikHub TikTok 已知有 `cursor`/`has_more`;Facebook RapidAPI 待确认) |
| `src/adapters/postgres.rs` | 进度存储过程(`update_task_progress`/`complete_task`),按条递增 `process_count`/`actual_consumption`,翻页无需改动预算 |
| `src/domain/entities.rs`, `src/protocol_gen/mod.rs` | `TaskConfig`/`SearchOptions` 数据结构 |
| 去重约束 | `(task_id, video_id)` 唯一 —— 单 task 内翻页天然去重;重复页可作提前终止信号 |

### 3.2 glance_mind_scheduler(兄弟仓 `/Users/jacksoom/programer/aihub/glance_mind_worker/glance_mind_scheduler`,次修复面)

| 路径 | 相关性 |
|---|---|
| `src/schedule_evaluator.rs` (`eval_once`, ~L86-151) | task completed → 无条件 `MarkCompleted`,不看扫描进度;需加防御性校验(WARN/补派) |
| `src/lib.rs:166-177` | `MarkCompleted` → `mark_campaign_completed(id, "ONCE_EXECUTED")` |
| `src/lib.rs:313-383` (`dispatch_task`) | `search_limit=page_size`、`search_offset=total_scanned`(offset 实际无上游对应,字段语义需澄清) |
| `src/db.rs:66-77` | `get_platform_page_size`,默认 20 |
| `src/db.rs:113-131` | `mark_campaign_completed` |

## 4. 测试与 CI 现状(本仓)

| 路径/事实 | 相关性 |
|---|---|
| `~246` 单测;`tests/`、`src/testing/mock_gateway.rs`、`src/fixtures/generator.rs` | 既有确定性 mock 基建,可承载翻页 mock 测试 |
| `MUTATION.md`、当前分支 `ci/mutation-gate`、`.github/` | cargo-mutants CI 门禁(PR `--in-diff`,nightly 全量)= 真·强制层 |
| `e2e/` | E2E 目录(live 栈) |
| 记忆:CI Rust Test Gates 捆绑 live-API 测试 | PR 红可能是 Facebook/Twitter live 漂移,与本改动无关,勿误判 |
| `cargo mutants`/`proptest` | 本仓变异/属性测试工具链(TESTING_CONSTRAINTS §4) |

## 5. 仓库规范

- 本仓 `docs/plans/` 此前无 jplan 计划族;采用标准布局 `docs/plans/jplan/2026-06-10-pagination-max-scan/`。
- 提交前需 `rust-verify-change`;DTO/响应变更走 `api-contract-guard`;migrations 走 `db-migration-guard`(预计本修复不动 schema,若动需重新评估)。
- scheduler 仓的提交流程独立(不同 git 仓库),计划需按仓拆模块。

## 6. 已知未决问题(待 Step 01/02 处置)

1. Facebook RapidAPI 搜索接口是否提供 cursor/分页 token?(P1,决定 Facebook 能否翻满 50)
2. 其余平台(reddit/twitter/instagram)上游分页能力各是什么形态?
3. `gm_crawler_tasks.search_offset`/`search_limit` 字段语义最终定义(保留为页大小提示?废弃 offset?)
4. 翻页中途单页失败的语义:partial-error 完结(沿用 `completed_with_partial_errors`)还是整任务 failed?
5. 搜索枯竭(上游不足 50 条)时 campaign 完结理由如何区分(`SEARCH_EXHAUSTED` vs `ONCE_EXECUTED`)?
6. scheduler 防御性校验的动作强度:仅 WARN,还是允许补派一次?
