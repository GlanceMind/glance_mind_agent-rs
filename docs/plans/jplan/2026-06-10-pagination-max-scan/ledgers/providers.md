# Provider Test Ledger(Step 02)

> 规则:每个 provider-backed 行为必须同时具备 (a) 确定性 mock-provider 测试与 (b) 真实 provider gate。
> 本计划的 provider = 内容上游(TikHub 各端点、Facebook RapidAPI)。LLM provider(DeepSeek)不在本计划改动面(见 `llm-api-boundary.md`)。

| ID | Provider / API 面 | 行为 | mock-provider 测试(确定性) | real-provider gate | 凭据/成本/限流/清理 | 验收证据 |
|----|--------------------|------|------------------------------|---------------------|----------------------|----------|
| PV-001 | Facebook RapidAPI `facebook-scraper3` 搜索/页面帖子(cursor 分页,facebook.rs:309-322, 522-644) | cursor 翻页到 max_count;RateLimited 部分截断;空页/重复 cursor 防环 | T-010, T-015~T-017, T-040(扩展既有 mock-HTTP 样板 facebook.rs:1169-1203/1269-1311,断言 cursor 跨请求转发) | T-050(P-001) | `FACEBOOK_RAPIDAPI_KEY`;按调用计费;≤3 调用;只读 | mock:3 页→50 条;real:第二页非空且异于首页 |
| PV-002 | TikHub TikTok `fetch_video_search_result`(offset+count≤20;has_more/cursor 响应,types.rs:25-26/297) | offset 翻页循环;has_more=0 终止 | T-011(mock-HTTP 断言 offset 递进;fixtures 已有 `tests/fixtures/tiktok/search_travel_us.json` 可扩展第二页 fixture) | T-051(P-002) | `TIKHUB_API_KEY`;按请求计费;≤3 调用;只读 | mock:offset 序列 0,20,40;real:第二页差异 + has_more 实测 |
| PV-003 | TikHub Reddit `fetch_dynamic_search`(after cursor;pageInfo,reddit_types.rs:61-99) | content 路径 after 翻页(评论侧模式移植) | T-012(mock-HTTP 断言 after 转发、hasNextPage=false 终止) | T-052(P-003) | 同 PV-002 | mock:after 链;real:第二页前进 |
| PV-004 | TikHub Twitter `fetch_search_timeline`(cursor;next_cursor,twitter_types.rs:44/360-378) | content 路径 cursor 翻页(评论侧模式移植) | T-013(mock-HTTP 断言 cursor 转发与终止) | T-052(P-003) | 同 PV-002 | 同上 |
| PV-005 | TikHub Instagram `general_search` V3/V2(请求端分页能力 = V1 待验证;响应端 next_max_id/pagination_token,instagram_types.rs:62-90) | V1 → 翻页 或 单页+如实上报 | T-014(分支确定后写;两分支各自的 mock 形态已在 test-suite.md 列出) | T-053 = V1 探测本身(P-004) | 同 PV-002;≤4 调用 | V1 判定记录(写回 assumptions.md)+ 分支 mock 测试 RED→GREEN |

## 控制汇总

- 所有 real gate:env 未设即 skip(既有模式),不进默认 `cargo test`,不进 PR 必跑集(live 漂移见 AG-007)。
- 清理:全部只读端点,无写副作用,无清理需求。
- mock 一致性:mock 响应形状必须取自真实响应样本(fixtures 目录既有模式),禁止凭空捏造字段;V1/T-050~T-053 的真实输出应回灌为 fixture。
- 所有 real gate 预算以单次 gated run 计;mutation/默认 CI 不得运行 live 集(D-14;M1-T0 守卫 + workflow 去 key)。
