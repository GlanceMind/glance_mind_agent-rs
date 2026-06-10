# Production Dependency Test Ledger(Step 02)

> 规则:每个真实生产外部依赖必须有真实依赖路径测试(独立凭据 gate,不混默认 `cargo test`)或有证据的 inapplicability。
> 既有 gated 套件:`tests/real_api_test.rs`(TikHub)、`tests/facebook_real_api_test.rs`、`tests/{facebook,instagram,twitter}_real_db_test.rs`、`tests/twitter_postgres_test.rs`。

| ID | 依赖 | 真实路径 | 测试(文件/命令) | 凭据/env gate | 幂等/清理 | 成本/限流 | 通过证据要求 |
|----|------|----------|-------------------|----------------|------------|------------|----------------|
| P-001 | Facebook RapidAPI(facebook-scraper3) | 搜索 + cursor 第二页 | T-050:扩展 `tests/facebook_real_api_test.rs`;`FACEBOOK_RAPIDAPI_KEY=... cargo test --test facebook_real_api_test -- --nocapture` | `FACEBOOK_RAPIDAPI_KEY`(+ 可选 HOST/BASE_URL,facebook_real_api_test.rs:54-58);**现状:凭据未设即 panic**(facebook_real_api_test.rs:54-58,与本表原描述不符,Step 06 实证);**M1-T0 落地后** → env-skip + CI opt-in(`GITHUB_ACTIONS && !RUN_REAL_API_TESTS → skip`) | 只读,无清理 | RapidAPI 按调用计费;第二页冒烟 ≤3 次调用;已知 live 漂移干扰(AG-007) | 实际输出:第二页内容非空且 cursor 异于首页 |
| P-002 | TikHub TikTok(fetch_video_search_result) | offset=20 第二页行为 | T-051:扩展 `tests/real_api_test.rs`;`TIKHUB_API_KEY=xxx cargo test --test real_api_test -- --nocapture` | `TIKHUB_API_KEY`(real_api_test.rs:3-4 既有约定;**注**:real_api_test.rs 与 P-001 同模式——凭据未设即 panic,M1-T0 覆盖) | 只读 | TikHub 按请求计费;≤3 次调用 | 第二页与首页内容差异 + has_more/cursor 字段实测值 |
| P-003 | TikHub Reddit / Twitter(fetch_dynamic_search / fetch_search_timeline) | after/cursor 第二页 | T-052:同 P-002 文件或新 gated 测试 | `TIKHUB_API_KEY` | 只读 | 同上 | 第二页转发 cursor 后返回非错误且内容前进 |
| P-004 | TikHub Instagram(general_search V3/V2) | **V1 验证探测**:带首页 `next_max_id`(V3)/`pagination_token`(V2)重发 | T-053:新 gated 测试或一次性探测脚本(结果须写回 assumptions.md V1 行) | `TIKHUB_API_KEY` | 只读 | ≤4 次调用 | 判定标准(assumptions.md V1):非错误且内容异于首页 → 支持;4xx/相同首页 → 单页 |
| P-005 | prod-shape Postgres(存储过程 update_task_progress / complete_task / terminal_reason 持久化与 fallback,postgres.rs:2123-2181) | 翻页多批落库 → process_count/consumed 守恒、新 terminal_reason 值写入 | T-054:扩展 `tests/facebook_real_db_test.rs` 模式;`DATABASE_URL=... cargo test --test facebook_real_db_test -- --nocapture` | `DATABASE_URL`;未设/GitHub Actions 受限 → skip(facebook_real_db_test.rs:278-285 既有模式) | 既有测试自带 task/数据隔离与清理模式;新增用例沿用;`(task_id,video_id)` 唯一约束实测 | 本地/CI DB,无外部成本 | consumed 增量 == 实际处理条数;NO_MORE_POSSIBLE_DATA / COMPLETED_WITH_PARTIAL_ERRORS 行落库可查 |
| P-006 | Redis task 协议(scheduler 写 → agent 读) | task JSON → `TaskConfig` 映射(redis.rs:438-465) | 映射为纯函数,确定性单测 T-003 覆盖字段语义;真实 Redis 传输面无本计划改动 | — | — | — | **inapplicability(部分)**:本计划不改 Redis 读写路径,只改映射语义;映射有确定性测试 + C-001 契约行。若 Step 04 改动入队/出队代码则升级为 real gate |
| P-007 | scheduler 仓 Postgres(gm_campaigns 写 completed_reason,db.rs:113-131) | mark_campaign_completed 写区分值 | scheduler 仓单测(diesel 层薄封装)+ 现网验证步骤(部署后查 campaign 行) | scheduler 仓测试基建(test_* 文件为内存构造;DB 集成按该仓现状) | 只读校验为主 | — | R-013 值落库可查;**注**:scheduler 仓无既有 real-DB gated 测试基建,新建最小 gated 测试或以部署后人工 SQL 验证为补偿控制(Step 04 定,须写入模块计划) |

## 结论

- 无依赖被标记为「跳过真实测试且无理由」。P-006 为证据型部分 inapplicability(代码路径不变);P-007 带补偿控制要求。
- 所有 real gate 不进默认必绿集;env-skip 既有模式仅 real-DB 系列成立,live-API 系列现状为凭据未设即 panic,待 M1-T0 守卫落地后全面成立(Step 06 实证),符合 testing-constraints「独立凭据 gate」要求。
- 所有 live 调用预算(≤3/≤4 次)以**单次 gated run** 计;mutation CI 与默认 PR CI 必须被排除在 live 集之外(D-14:M1-T0 守卫 + mutation workflow 移除 live key)。T-050 等探针类(AG-008)通常仅 GREEN 一轮取证;若需 RED/GREEN 两轮按每轮预算计。
