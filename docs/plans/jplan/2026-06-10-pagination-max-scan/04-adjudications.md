# 04 - 用户裁决记录(2026-06-10,准则 = 功能正确性优先)

> 性质:用户(计划族最高权限)对 M1/M2 起草期开放问题与 M6 待决项的**绑定裁决**。后续 Step 04 起草(M6/M3/M4/M5/ROOT)以本文件为准;Step 06 评审仍可提 findings,但默认值已定,否决须给出功能性反证。
> 引用方式:各模块计划 §6 的对应条目状态更新为「已裁决,见 04-adjudications.md D-xx」。

## 裁决表

| ID | 议题(来源) | 裁决 | 功能性理由 |
|---|---|---|---|
| D-01 | facebook 循环形态(M2 §6.1) | **采纳 M2 方案:保留既有手写循环 + shortfall 接线,不改接 `PaginationLoop`**;M2-T2.8 语义对齐断言为强制项 | `reached_post_limit` 以 date-filter 后数量判定达量(facebook.rs:518-520),状态机 id 计数模型不含过滤语义,改接会改变 START_DATE/END_DATE 任务行为;两级 candidates 循环无单游标对应形态;P0 事故路径以行为保真为最高功能要求 |
| D-02 | page_size_hint facebook 豁免 + D4 措辞(M2 §6.3) | **采纳:fb 文档化 no-op**;root 冻结 D4 时写入「hint 按平台可控性消费,上游无单页参数的平台文档化豁免」 | facebook-scraper3 端点仅 query/cursor 入参(facebook.rs:309-322 实证),hint 无下发对象;功能上 hint 的唯一风险是被误当总量上限,M2-T1.3 已钉死 |
| D-03 | Exhausted(有进展)不调 `stop_campaign_gracefully`(M1 §6.1 / M2 §6.2) | **采纳 M1 D3 裁决** | 枯竭是**时点状态非永久状态**:RECURRING campaign 之后可能出现新内容,agent 停 campaign 会错杀;campaign 生命周期须保持 scheduler 单一写方(职责分离);ONCE 的完结理由区分由 M6 读 terminal_reason 落 completed_reason,功能闭环不缺失 |
| D-04 | platform_page_cap 冻结表(M1 §6.2) | **冻结:fb=20 / tiktok=20 / reddit=100 / twitter=100 / ig=50 / unknown=20** | 取值来源 = 各 strategy 现行 cap + 上游硬约束(tikhub types.rs:297 等);修订路径保留:平台模块持上游实测证据 → root 更新冻结表 → 测试期望走 ASSERTION-CHANGE-JUSTIFIED |
| D-05 | PT-5 变异豁免预案(M1 §6.3) | **采纳预案原文** | 若 cargo-mutants 不生成「读 search_offset」类变异,以 T-003c+PT-5 联合钉死字段语义为书面豁免;Test-Gate Reviewer 复核豁免文本 |
| D-06 | R-013 completed_reason 终名(M6 待决) | **定名 `SEARCH_EXHAUSTED`** | 语义精确(区分于 BUDGET_EXHAUSTED / ONCE_EXECUTED,如实记录「搜索面枯竭」);含 `EXHAUST` 子串直接通过 gm-e2e 白名单(C-003),零跨仓断言改动 = 零功能风险 |
| D-07 | P-007 补偿控制形态(M6 待决) | **必须控制 = scheduler 仓新建最小 `DATABASE_URL`-gated 测试**(mark_campaign_completed 写 `SEARCH_EXHAUSTED` 落库可查);N-003 人工 SQL **降级为部署后可选抽查**(运维剧本步骤,非验收必需) | 可重复的自动化证据 > 一次性人工核查;diesel 薄封装测试成本小;功能验收不应依赖人到场 |
| D-08 | R-010 防御强度(M6 待决) | **确认:WARN 日志 + 指标,不自动补派**;保留升级扩展点 | 自动补派有预算重复消耗与「上游持续欠交付→无限补派」循环风险,且会掩盖根因缺陷;M2 修复后欠扫应基本消失,守卫定位 = 观测哨;I-007 纯观测(零状态改变)是功能安全下界 |
| D-09 | scheduler 仓 mutation CI(AG-013/FR-002 待决) | **补 CI:M6 计划须含一个任务,为 `glance_mind_worker` 仓新增 scheduler crate 的 mutation 工作流**(镜像 agent-rs `.github/workflows/mutation-rust.yml`,pinned cargo-mutants 24.11.0,PR `--in-diff`);AG-013 本地预检照跑(CI 落地前的过渡证据) | 三层强制模型中 CI 是唯一真·强制层(全局约束 §5.2);只靠本地预检 = scheduler 仓的反作弊门禁长期缺位 |
| D-10 | facebook_real_api_test 无 env-skip 守卫的既有偏差(root 登记项) | **立项修复,但作为 root 阶段独立测试基建任务**:对齐 `facebook_real_db_test.rs` 的 env-gate 模式(凭据未设 → 显式 skip + eprintln),**不混入 M2 事故 diff,不改任何断言** | 当前「凭据未设即 panic-fail」污染默认 `cargo test` 信号(项目记忆:CI live 红噪);修复属 gate 守卫对齐 P-001 既述行为,非断言弱化;与事故修复解耦保持 M2 diff 聚焦 |
| D-11 | M2-T5 RED 取证方式(执行期) | **主路径 = 在 pre-M2 基线(main 的 git worktree)实跑两条事故重演测试取红**;worktree 不可行时按 AG-006 金丝雀补证 | 基线实跑的 RED 是对 campaign 269 事故的最真实重演证据,优于人工金丝雀 |
| D-12 | V1(Instagram 翻页能力) | **维持 OPEN,只 gate M5 分支**;起草/执行序不变(M6→M3→M4→M5) | 不阻塞任何 P0/P1 功能面;探测(≤4 调用)在 M5 首任务执行最经济 |
| D-13 | 多 keyword 预算/达量语义(Step 06 DR-04) | **max_count 为任务级语义**:orchestrator 跨 keyword 传递 remaining(`max_videos − 已累计`),后续 keyword 的 count = remaining,total ≤ max_count;M1-T4 扩一条 K=2 确定性测试 | 预算 reserve 即按任务级 max_count×单价预留(lib.rs:325-329),consume 上界必须与之对齐;I-001 账本行本就写「单 task ≤ max_count」——(b) 是唯一使账本、预算、行为三者一致的选项;(a) 实证依赖现网数据易漂移,(c) 留下计费缺口 |
| D-14 | mutation×live 相乘修法(Step 06 DR-07) | **双修**:① live-API 测试文件加 `GITHUB_ACTIONS && !RUN_REAL_API_TESTS → skip` 守卫(与 real-DB 同构,零断言改动,并入 M1-T0);② mutation workflow `.env` 移除 TIKHUB/FACEBOOK live key(独立小 PR,root 横切任务) | 纵深:仅 ① 则非 mutation 的常规 CI 仍逐 PR 打 live(既有噪声源);仅 ② 则本地带 .env 预检仍打 live;两者皆零断言面 |
| D-15 | facebook 活性缺口修法(Step 06 DR-03) | **改生产行**:facebook 三循环的空页计数从 `page_count == 0` 改为「本页**新增**(去重后)数 == 0」,与 D2 `empty_streak` 按 `newly_accepted.is_empty()` 的定义对齐;归 M2-T2 改动面,受既有 mock 测试 + AG-012 保护 | 入账豁免会让五平台「单一语义」在活性维度上分叉,且把无界循环风险留在 P0 平台;改动一行判定、行为仅在「重复内容连发」病态形状下变化(从潜在无界 → 3 页停),无正常路径回归面 |

> **D-11 增补(2026-06-10,Step 06 TG-11)**:RED 基线 = 「含 M1、不含 M2 的 commit」;M1 未合 main 时用分支上 M1 完成点 commit 建 worktree。双金丝雀程序写死:形状 1 = 临时恢复 `facebook.rs:131` `v.min(20)` → `incident_269_shape_sufficient…` 须红;形状 2 = 临时把 fb override 的 shortfall 短路为 `None` → `incident_269_shape_only_20…` 须红;均只动生产代码、输出留存后还原、零断言触碰。

## 对后续起草的直接指令

- **M6 起草(下一次 Step 04)**:D-06/D-07/D-08/D-09 为输入事实,不再作为开放问题;任务清单须含「mutation CI 工作流」任务(D-09)与「gated completed_reason 落库测试」任务(D-07)。
- **M3~M5 起草**:D-01 不构成样板约束——tiktok/reddit/twitter/instagram 的新循环**仍按 M1 `PaginationLoop` 接入**(D-01 是 facebook 既有循环的特例豁免,理由不可迁移:其余平台无「已被测试钉住的既有循环」资产)。
- **ROOT 起草**:D-02 写入 D4 冻结措辞;D-04 表入冻结节;D-10 立为独立任务;A/B 组全部条目按本文件标记「已裁决」,root 只做一致性终审。

## 评审接口

Step 06 三类 Reviewer 对以上任何一条的否决,须附**功能性反证**(行为差实例 / 失败场景),并经用户确认后回滚对应裁决;不得仅以风格/偏好理由翻案。
