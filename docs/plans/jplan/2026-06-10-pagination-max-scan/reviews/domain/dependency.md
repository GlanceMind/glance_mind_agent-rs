# Domain Review — Dependency Reviewer(2026-06-10,只读子 agent 原文)

> 归一化映射:F-1→DR-07(P1)、F-2→DR-08(P1,升级 Step05 F-01)、F-3→DR-05(P1)、F-4→DR-19(P2)、F-5→DR-06 主体(P1)、F-6→DR-20(P2)。

## F-1 · High → DR-07:变异门禁与 live gated 测试相乘——「real gate 不进默认 cargo test」前提在 mutation CI / 本地预检环境不成立
- Evidence:mutation-rust.yml:48-56/105-114 两 job 均写入含 TIKHUB/FACEBOOK key 的 `.env` 再跑全量 mutants;real-DB 有 `GITHUB_ACTIONS && !RUN_REAL_DB_TESTS → skip` 守卫(×4 处),**live-API 无等价守卫**(real_api_test.rs:33-41 仅 key 缺失 skip;facebook_real_api_test.rs:53-66 key 缺失 panic);key 在 CI `.env` 恒存在 → 每变异体实跑付费 live;账本前提(「不进默认 cargo test/PR 必跑集」「≤3/≤4 次调用」按单次 gated run 计)与此矛盾;各模块 AG-012 预检同样全量;计划族新增 5+ live 测试且无任务改 workflow 或加守卫。
- Consequence:单 PR 数十变异体 × live 若干条 × 1-3 调用 → 数百次计费调用;nightly 更甚;变异判定混入上游抖动 → 「唯一真·强制层」结果不可信。
- Patch:横切任务(挂 root 或 M1):① live-API 文件加 `GITHUB_ACTIONS && !RUN_REAL_API_TESTS → skip` 守卫(与 real-DB 同构,零断言改动);或 ② mutation workflow `.env` 移除两把 live key(DATABASE_URL 受既有守卫保护可留)。AG-012 文本注明「预检须在 live key 未设环境执行」;P-001~P-004/providers 控制汇总补注。

## F-2 · High → DR-08:RT-3 排最后,但 M1~M5 每个收尾 gate 都依赖它先落地
- facebook_real_api_test 现状 key 缺失 panic(P-001 行「未设 → skip」与代码不符);执行序 M1→…→root,M1-T7 起 gate 第 1/2 步都跑该目标。无凭据 → baseline abort;有凭据 → 落入 F-1。
- Patch:RT-3 提升为 M1 前置(M1-T0);P-001 措辞改「现状 panic,RT-3 修复后 skip」;RT-3 范围并入 F-1 的 `RUN_REAL_API_TESTS` CI opt-in 条款(只写「凭据未设 → skip」在 CI 恒有凭据下永不触发)。

## F-3 · High → DR-05:M2-T2.5/T3.4 的 429 期望与 facebook 既有重试循环矛盾,按 AG-002 不可达绿
- Evidence:`RATE_LIMIT_RETRY_DELAYS_SECS=[2,5,10]`(facebook.rs:27),request_json 对 429 自动重试共 4 次请求;既有测试 `test_governor_rate_limiter_applies_to_retries`(facebook.rs:1406-1437)断言 requests==4 且 mock 排队 4×429 带 `retry-after: 0`(否则实睡 17s;mock server 只 accept responses.len() 次连接,排队不足第 3 请求被拒)。M2 计划却写 T2.5「第 2 页 mock 返回 429」(单响应)、T3.4 断言 requests==2。
- Consequence:不动断言无法转绿——唯一路径是删生产重试(回归)或改断言(AG-002 死局),发生在 P0 模块。
- Patch:T2.5/T3.4 改为第 2 页排队 4×429(retry-after: 0)、期望 requests == 1+4 == 5,shortfall 断言不变;RED 文本同步;T-050 预算注「live 429 触发 4 次计费重试,预算按 HTTP 请求计」。

## F-4 · Medium → DR-19:TikHub 客户端 429 默认睡 60s/次(error.rs:140-143,max_retries=3),M3/M4/M5 的 429 mock 测试未指定 Retry-After/重试配置 → 单条最坏 ~180s,两条超 mutants 300s timeout;real 侧 ≤3 调用按适配器调用计、计费 HTTP 最多 4 倍。
- Patch:429 测试统一规定 mock 带 `Retry-After: 0` 且按重试次数排队(或 `with_retry_config` 零重试 + 断言计费请求数);real 探针改无重试调用或零重试配置,使预算 = HTTP 请求上界。

## F-5 · High → DR-06 主体:scheduler `--in-diff` 在子目录布局下选中 0 个变异体;root chain gate 还喂 agent 仓 diff
- Evidence:**实证 `glance_mind_scheduler/.git` 不存在,`git rev-parse --show-toplevel` = `glance_mind_worker`——scheduler 是 worker 仓子目录,非 M6 头部声称的「独立 git 仓」**。`git diff` 产出 `glance_mind_scheduler/src/...` 前缀路径;`cd` 子目录后 cargo-mutants 以包根匹配 `src/...` → 前缀不匹配 → 变异体集为空 → 恒绿。root §4 复用 agent 仓 `/tmp/pr.diff` 同样 0 变异体。
- Consequence:D-09 的「真·强制层」从第一天起形同虚设——恰是 M6-T4 反作弊声明列举的「使门禁形同虚设」形态,以无心方式达成。
- Patch:① M6-T4/T5/AG-013 统一 `git -C glance_mind_scheduler diff --relative=glance_mind_scheduler main...HEAD > /tmp/scheduler.diff`(或子目录内 `git diff --relative`);② 验收哨兵判据「mutants 报告 Found N mutants,N≥1;N==0 即配置失败」;③ root chain gate 为 scheduler 单独生成 diff;④ M6 头部改「worker 仓子目录、独立 PR 流程」;⑤ M6-T4 补 `runs-on`(agent 仓为 [self-hosted, front])。

## F-6 · Medium → DR-20:M4 mock fixture 来源声明不成立
- Evidence:tests/fixtures/ 仅 tiktok/;reddit.rs 测试模块无 mock-HTTP、无 JSON 样本;M4-T2 却写「取自既有 reddit fixture/测试样板字段集」;M4-T4 缺 M3 式「实测不符 → 停下上报」对账义务。
- Consequence:mock 形状只能从 serde 定义反推(pageInfo 在真实 content-search 响应的存在性/嵌套无样本背书);T-052 实测形状不同时已绿 mock 不自动暴露 → 真实响应永远走「cursor 缺失 → Exhausted」单页化,欠扫以合法外观复发。
- Patch:M4-T2/T3 改述「形状取自 reddit_types.rs/twitter_types.rs serde 定义 + 评论侧请求样板,标注待回灌确认」;M4-T4 增对账义务(不符 → 停下上报,mock 修订走 ASSERTION-CHANGE-JUSTIFIED + root 知会)。

## 已核查无发现
P-001~P-005 gated 测试/凭据/只读/证据齐;P-006 部分 inapplicability 有证据;P-007 经 D-07 升级且含幂等清理;PV-001~005 双 gate 齐备;M5 探测先行使 fixture 时序自洽;M3 依托既有真实 fixture + 对账义务;LLM INAPPLICABLE 有证据;proptest 研究齐(版本/种子入库/预算条款;明确拒 wiremock/mockito;双仓同 pin 24.11.0);凭据全部 from_env,task payload 仅数值型新字段,无注入面;C-003/C-004 契约钉 + 滚动部署论证成立。
