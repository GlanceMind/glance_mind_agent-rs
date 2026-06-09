# 00 - Context Inventory

Request: 修复 TikTok 社媒任务配置 `max_scan_count=100`（单次扫描）但爬虫总在 20 条停止的 bug。
Locked decisions (from debug session): 照配置抓满（配额可接受）；测试用 mock HTTP server（mockito/wiremock）。

## Root cause (verified in debug session, evidence-backed)
1. 没有翻页循环：每个关键词只发一次 TikHub 请求，搜索接口单次最多回 20 条。
2. `.min(20)` 把目标总量在进 adapter 前砍成 20。
`types.rs` 的 `count.min(20)` 是正确的单页上限，保留。

## Relevant files (paths + one-line relevance)
- `src/strategies/tiktok.rs:96` — `.min(20)` 总量截断点（要去掉）。
- `src/strategies/tiktok.rs:181` — `max_videos_per_search()=20`，注释语义改为"每页大小"。
- `src/adapters/tikhub.rs:149` — `ContentGateway::search`，搜索路径单次请求（要加 offset/cursor 翻页）。
- `src/adapters/tikhub.rs:221` — `fetch_user_content`，用户主页单次请求（要加 max_cursor 翻页）。
- `src/adapters/tikhub.rs:201` — `fetch_by_keyword` SecUserId 分支，重复单次抓取（归并进翻页 helper）。
- `src/tikhub/client.rs:447` — `fetch_all_comments`：**已有的正确翻页范本**，复刻其结构。
- `src/tikhub/client.rs:246` — `search_videos`：搜索 HTTP 调用，offset/count 入参。
- `src/tikhub/client.rs:560` — `fetch_user_videos`：用户视频 HTTP 调用，max_cursor 入参。
- `src/tikhub/types.rs:15-27` — `SearchResponse`/`SearchData`（`has_more`、`cursor`）。
- `src/tikhub/types.rs:250-262` — `UserVideosData`（`has_more`、`max_cursor`）。
- `src/tikhub/types.rs:270-307` — `SearchParams`（`offset`/`count`，`with_count` clamps 20 — 保留）。
- `src/tikhub/types.rs:350-385` — `UserVideoParams`（`max_cursor`/`count`，`with_count` clamps 20 — 保留）。
- `src/domain/entities.rs:784-839` — `SearchOptions`（`count` 不截断，可承载目标总量）。
- `src/orchestrator.rs:462` — `process_keyword`→`fetch_content`→`fetch_by_keyword` 单次调用链。
- `src/orchestrator.rs:470` — 零结果触发 `stop_campaign_gracefully`（翻页后须只对"首页空"生效）。
- `src/db/models.rs:54` / `src/schema.rs:512` — `Campaign.max_scan_count`（来源字段）。
- `src/adapters/redis.rs:464` — `.with_max_videos(max_count)`（config→TaskConfig 流入点）。
- `src/testing/mock_gateway.rs` — 现有 gateway mock（测 orchestrator，但测不到 adapter 内循环）。
- `Cargo.toml` — 需加 dev-dependency: mockito（mock HTTP server）。

## TikHub pagination semantics (verified)
- Search endpoint `/api/v1/tiktok/app/v3/fetch_video_search_result`: offset-based; response `SearchData.cursor`(下一 offset) + `has_more`(==1 表示还有).
- User videos endpoint `/...fetch_user_post_videos`(by client.rs:560): `max_cursor`-based; response `UserVideosData.max_cursor` + `has_more`.
- Per-request hard cap = 20 (enforced in `SearchParams::with_count`/`UserVideoParams::with_count`).

## Out of scope (this plan)
- Facebook / Twitter / Instagram / Reddit 平台的同类翻页问题（用户确认只修 TikTok）。
- 评论翻页（`fetch_all_comments` 已正确实现）。
