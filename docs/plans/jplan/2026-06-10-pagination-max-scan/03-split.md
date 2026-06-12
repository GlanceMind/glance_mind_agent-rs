# 03 - Scope Split and Module Map

> 输入:`00-manifest.md`、`01-first-principles.md`、`handoffs/02-to-03.md`、全部 ledgers、`00-context-inventory.md`(仅路径)。
> 裁决:**root + 6 modules**(初步队列 3 模块经粒度核查后细分,理由见 §2)。

## 1. Split 裁决与触发证据

**裁决:root-plus-modules。** 命中的 split triggers(step-03 规范):

| Trigger | 证据 |
|---|---|
| >7 implementation tasks | test-suite 账本 T-001~T-054 共 ~25 个测试载体,加实现任务远超 7 |
| >5 production files | agent-rs:5 个 strategies + 5 个 adapters + orchestrator.rs + redis.rs + content_gateway.rs + entities.rs;scheduler:schedule_evaluator.rs + lib.rs + db.rs |
| >2 independent subsystems | strategies 层 / adapters 翻页层 / orchestrator 终态映射 / scheduler eval_once(跨 git 仓) |
| independent provider lanes | PV-001(RapidAPI)与 PV-002~PV-005(TikHub 四端点)是独立 provider lane,各有 mock+real 双 gate |
| 单计划 >250 行 | 仅 facebook 一条 lane(T-001/T-010/T-015~T-017/T-040/T-050/T-054 + RED→GREEN 证据要求)就接近上限 |
| reviewers 需不同专长 | manifest Reviewer Queue 已列三类(Test-Gate / Concurrency / Cross-Service) |

**初步队列(3 模块)的修订理由**:`agent-rs/platform-strategies` 覆盖 5 平台 ×(strategy 解耦 + 适配器翻页 + mock 测试 + real gate)≈ 20+ 任务,单 Step 04 上下文起草不完(违反 manifest「每模块一次 Step 04 起草完」约束)。按平台优先级(01 §8:P0 facebook / P1 tiktok·reddit·twitter / P2 instagram)与 provider lane 切为 4 个平台模块;tiktok 与 reddit/twitter 再分两模块,因机制不同(tiktok=offset 循环新构造;reddit/twitter=同仓评论侧 cursor 模式镜像移植,反作弊证据形态一致,合并仍 ≤8 任务)。

**V1 裁决:并入 instagram 模块,不设独立模块。** 理由:V1 工作量小(N-001 文档核查 + T-053 探测 ≤4 次调用),唯一消费方是 R-006 分支选择(C-005),独立成模块将产生单任务计划;instagram 模块计划以「V1 探测为第一任务 + 判定标准(assumptions.md V1 行)+ 两分支(翻页 / 单页+如实上报)各自的任务与测试」形式一次起草完,分支由 V1 结果在执行期选择。

## 2. 定稿模块队列

| module_id | 仓库 | 职责(owner surface) | 依赖 | Step 04 起草序 |
|---|---|---|---|---|
| M1 `agent-rs/pagination-core` | glance_mind_agent_rs | 共享契约:fetch 路径暴露欠交付原因(exhausted/partial-failure;cursor 不过 trait,A005 drop)、orchestrator 终态映射、页失败语义、翻页循环安全不变量 + proptest 基建(FR-001)、redis 字段语义 agent 侧、terminal_reason 脱敏 | — | 1 |
| M2 `agent-rs/facebook-p0` | glance_mind_agent_rs | 事故平台修复:facebook strategy cap 解除 + 既有适配器翻页接通 + 失败注入/枯竭/防环集成样板 + 事故重演回归 + real-API/real-DB gates | M1 | 2 |
| M3 `agent-rs/tiktok-p1` | glance_mind_agent_rs | tiktok strategy cap 解除 + 适配器 offset 翻页循环(单页≤20,has_more 终止)+ mock/real gates | M1(M2 为样板参照,软依赖) | 4 |
| M4 `agent-rs/reddit-twitter-p1` | glance_mind_agent_rs | reddit/twitter strategy cap 解除 + content 路径启用 cursor 翻页(各自评论侧既有模式镜像移植)+ mock/real gates | M1(M2 软依赖) | 5 |
| M5 `agent-rs/instagram-p2` | glance_mind_agent_rs | V1 验证(首任务,N-001+T-053)→ 分支:同构翻页 或 单页+如实 `NO_MORE_POSSIBLE_DATA` | M1;分支选择 gated on V1(C-005) | 6 |
| M6 `scheduler/once-guard` | glance_mind_worker/glance_mind_scheduler(**worker 仓子目录,`glance_mind_worker` 单一 git 仓;独立 PR/提交流程**——勘误 per Step 07 DR-06,原「独立 git 仓」为误判) | eval_once 防御 WARN+指标(不补派)、completed_reason 枯竭区分值(候选 `SEARCH_EXHAUSTED`,命名 Step 04 终确认,须满足 C-003 子串约束)、新增读 `gm_crawler_tasks.terminal_reason`(C-004)、字段语义注释固化(N-002 scheduler 侧)、AG-013 本地变异证据 | M1(语义依赖:消费 agent 写入的 terminal_reason 值;代码无依赖;部署无硬顺序——读方须容忍 NULL/未知值,契约不变约束 §3.3) | 3 |

### 模块图(依赖方向:被依赖 ← 依赖方)

```text
                    ┌──────────────────────────────┐
                    │ ROOT(跨模块不变量/验收/评审) │
                    └──────────────────────────────┘
   M1 agent-rs/pagination-core(共享契约:欠交付原因 + 循环不变量 + redis 语义)
        ↑               ↑               ↑              ↑               ↑(语义依赖 C-004)
   M2 facebook-p0   M3 tiktok-p1   M4 reddit-twitter-p1   M5 instagram-p2   M6 scheduler/once-guard
   (P0,事故面)  (P1)          (P1)                (P2,gated V1)  (兄弟仓)
        ·····样板参照(软)·····→ M3 / M4 / M5
```

执行/起草顺序:**M1 → M2 → M6 → M3 → M4 → M5**。理由:M1 定共享接口;M2 关闭 P0 事故面;M6 关闭事故的 scheduler 半边(防御+完结理由区分,与 M2 合并构成事故完整闭环);M3/M4 为 P1 复制;M5 殿后且 gated on V1。M3/M4 互相独立,可并行起草/执行。

## 3. 账本覆盖映射(每条 ID → 唯一 owner;shared 显式标注)

### 3.1 需求(R)与验证任务

| ID | Owner | Shared 说明 |
|---|---|---|
| R-001 | **shared(按平台分行)**:fb→M2;tiktok→M3;reddit/twitter→M4;instagram→M5 | 解耦机制(options.count=总量、单页大小另行传递的接口形状)归 M1 定义;各平台 cap 行删除与 T-001 各自平台行归各平台模块 |
| R-002 | M2 | — |
| R-003 | M3 | — |
| R-004 / R-005 | M4 | — |
| R-006 | M5 | 分支 gated on V1 |
| R-007 | M1 | facebook 集成证据(T-016)落 M2(M2 依赖 M1) |
| R-008 | M1 | 失败注入集成证据(T-015)落 M2 |
| R-009 | M1(proptest 套件 + 循环纯逻辑) | 各平台适配器循环须满足同一不变量;per-platform 终止断言含于 T-011~T-014 |
| R-010 | M6 | Step 04 评审确认防御强度(handoff 02-to-03 预告①) |
| R-011 | **shared**:agent 侧(T-003/I-008/PT-5)→ M1;scheduler 侧(注释固化,N-002)→ M6 | 账本原 owner 即双模块 |
| R-012 | **root**(约束型,全模块生效) | 各模块 diff 自查 + root 终审(无 migration/schema/预算改动) |
| R-013 | M6 | 命名须满足 C-003 子串约束 |
| V1 | M5(首任务) | 仅 gate R-006 分支,不阻塞 M1~M4/M6 |

### 3.2 不变量(I)与失败模式(F)

| ID | Owner | Shared 说明 |
|---|---|---|
| I-001~I-003 | M1(PT-1~PT-3) | 各平台 mock 测试为实例化证据(M2~M5) |
| I-004 | M1(PT-4 + T-002) | F-003 实例证据在 M2(T-016) |
| I-005 | M1 | 集成证据 T-015 在 M2 |
| I-006 | M2(T-054 real-DB)+ root(R-012 diff 审查) | shared |
| I-007 | M6 | — |
| I-008 | M1(T-003 + PT-5) | — |
| I-009 | M1(T-004) | — |
| F-001 / F-002 | M1(语义)+ M2(注入证据 T-015) | shared(机制/证据分层) |
| F-003 | M1(映射)+ M2(T-016)+ M6(R-013 区分值) | shared |
| F-004 / F-005 | M1(PT-2)+ M2(T-017 样板) | M3/M4/M5 各自循环复制防环/空页终止 |
| F-006 | M2(facebook RateLimited 先例回归) | 模式由 M3/M4 复用 |
| F-007 | **root**(文档化;无代码改动;既有 real-DB 唯一约束为证据) | — |
| F-008 | M6(T-021 双向断言) | — |
| F-009 | M1(fallback 路径兼容实现)+ M2(T-054 证据) | shared |

### 3.3 测试(T)

| Owner | 测试 ID |
|---|---|
| M1 | T-002, T-003, T-004, T-030~T-034(PT-1~PT-5) |
| M2 | T-001(fb 行), T-010, T-015, T-016, T-017, T-040, T-050, T-054 |
| M3 | T-001(tiktok 行), T-011, T-051 |
| M4 | T-001(reddit/twitter 行), T-012, T-013, T-052 |
| M5 | T-001(instagram 行), T-014, T-053(=V1 探测) |
| M6 | T-020, T-021, T-022 |

(T-001 为 shared 需求 R-001 的按平台分行;每平台行归对应模块,见 §3.1。)

### 3.4 契约(C)、生产依赖(P)、provider(PV)

| ID | Owner | Shared 说明 |
|---|---|---|
| C-001 | M1(agent 映射行为) | scheduler 写方零改动;M6 计划须引用本条为「不变」前提 |
| C-002 | **shared**:M1(agent 侧注释+clamp 实现)+ M6(scheduler 侧注释,N-002) | Step 06 Cross-Service Reviewer 复核 |
| C-003 | M6 | 子串约束为 M6 计划硬约束 |
| C-004 | **shared(双侧接线,handoff 02-to-03 要求双现)**:M1(agent 不重命名既有值字符串、枯竭/部分失败接入既有值)+ M6(scheduler 新增读取,容忍 NULL/未知值) | 本计划唯一新增跨服务读依赖 |
| C-005 | M5 | — |
| P-001 | M2(T-050) | — |
| P-002 | M3(T-051) | — |
| P-003 | M4(T-052) | — |
| P-004 | M5(T-053) | — |
| P-005 | M2(T-054) | M1 实现 fallback 兼容,证据在 M2 |
| P-006 | M1(T-003;部分 inapplicability 维持) | 若 Step 04 改动入队/出队代码则升级 real gate |
| P-007 | M6(补偿控制形态 Step 04 定:最小 gated 测试 vs N-003 人工 SQL;handoff 02-to-03 预告②) | — |
| PV-001 | M2 | — |
| PV-002 | M3 | — |
| PV-003 / PV-004 | M4 | — |
| PV-005 | M5 | — |

### 3.5 反作弊(AG)、框架研究(FR)、非代码(N)

| ID | Owner / 引用方 |
|---|---|
| AG-001~AG-007 | 全模块计划强制逐条引用(root 验收复核) |
| AG-010~AG-012 | root(CI 门禁)+ M1~M5(每模块收尾本地预检 AG-012) |
| AG-013 | M6(本地 `cargo mutants --in-diff` 无 missed + 输出留存;是否补 CI 由 Step 04 评审定) |
| AG-020~AG-024 | M1(PT 套件实现);M2~M5 引用其适用性 |
| FR-001 | M1(前置任务:proptest 引入,per handoff 02-to-03) |
| FR-002 | root(agent-rs CI 已有)+ M6(scheduler 缺口) |
| FR-003 | M3/M4/M5 引用(mock helper 复用裁决);M2 扩展既有样板 |
| FR-004 | M6 |
| FR-005 | M1/M2 引用(无待办) |
| N-001 | M5(V1 文档侧) |
| N-002 | shared:M1(agent 侧)+ M6(scheduler 侧) |
| N-003 | M6(conditional,P-007 fallback) |
| N-004 | DONE(Step 02 已闭环,无 owner) |

### 3.6 覆盖完整性自查

- R-001~R-013 + V1:全部有 owner(R-001/R-011 shared 已显式标注;R-012 归 root)。✅
- I-001~I-009、F-001~F-009:全部有 owner(F-007 归 root)。✅
- T-001~T-054 全部分配;justified gaps(跨仓全链 e2e、页级重试、T-014 gated)沿 test-suite §8,root 验收时复核 gap 理由仍成立。✅
- C/P/PV/FR/N/AG 全部有 owner 或显式「全模块引用」。✅

## 4. Root plan 职责(plans/root.md,Step 04 末尾起草)

1. **共享接口定稿与冻结**:M1 产出的「欠交付原因」类型形状(trait 返回形态)、SearchOptions 总量/页大小语义 —— M2~M5 的消费契约;M1 计划评审通过后冻结,变更须回到 root。
2. **跨模块不变量**:R-012(预算/schema/Redis 形状零改动,终审 diff)、契约不变约束(cross-service-contracts §3 全四条)、F-007 文档化、I-006 终审。
3. **执行顺序与跨仓协调**:M1→M2→M6→M3→M4→M5;C-004 滚动部署兼容(scheduler 读方容忍 NULL,无硬部署顺序)写入两侧模块完成判据。
4. **最终验收(chain gate)**:T-040 事故重演 green;agent-rs CI 变异门禁(AG-010/011)无 missed 或带豁免;M6 本地变异证据留存(AG-013);`rust-verify-change` 通过;live-API 红区分上游漂移(AG-007,项目记忆)。
5. **评审编排**:Reviewer Queue 三评审(Test-Gate / Concurrency-Resource / Cross-Service)的输入物与复核点分配。

## 5. 每模块完成判据与独立验证命令

| Module | 独立完成判据(可单独验收) | 验证命令(gated 项另列) |
|---|---|---|
| M1 | proptest 套件(T-030~T-034)+ T-002/T-003/T-004 全 green,且每个新测试有 RED 证据;trait/映射改动经本地变异预检无 missed | `cargo test`(指定测试名);`git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1` |
| M2 | T-001(fb)/T-010/T-015~T-017/T-040 green(各带 RED 证据);gated:T-050/T-054 实跑输出留存 | `cargo test`;`FACEBOOK_RAPIDAPI_KEY=… cargo test --test facebook_real_api_test -- --nocapture`;`DATABASE_URL=… cargo test --test facebook_real_db_test -- --nocapture`;AG-012 预检 |
| M3 | T-001(tiktok)/T-011 green + RED 证据;gated:T-051 输出留存(has_more/cursor 实测值回灌 fixture) | `cargo test`;`TIKHUB_API_KEY=… cargo test --test real_api_test -- --nocapture`;AG-012 预检 |
| M4 | T-001(reddit/twitter)/T-012/T-013 green + RED 证据;gated:T-052 输出留存 | 同 M3 形式;AG-012 预检 |
| M5 | V1 判定记录写回 assumptions.md V1 行 + C-005;所选分支的 T-014 green + RED 证据 | `TIKHUB_API_KEY=… cargo test --test …`(T-053 探测 ≤4 调用);`cargo test`;AG-012 预检 |
| M6 | T-020~T-022 green(T-020/T-021 带 RED 证据;T-022 先绿须经变异证明,AG-006);本地 mutants 无 missed 输出留存(AG-013);P-007 补偿控制完成(形态 Step 04 定);N-002 scheduler 侧注释落地 | scheduler 仓内:`cargo test`;`git diff --relative main...HEAD > /tmp/scheduler-pr.diff && cargo mutants --in-diff /tmp/scheduler-pr.diff -- --test-threads=1`(子目录内执行,`--relative` 必须;以 m6-once-guard.md §4 现行命令为准——勘误 per DR-06) |

## 6. 模块文件面(input files / output files;per step-03 规范 action 4)

> 行号为 2026-06-10 工作区状态(与账本一致);Step 04 起草对应模块时只读本表 + 该模块账本行,不重读 `00-context-inventory.md` 结论。

| Module | Input files(源码) | Output files(计划产物) |
|---|---|---|
| M1 `agent-rs/pagination-core` | `src/ports/content_gateway.rs:33-37`(trait 缺口)、`src/orchestrator.rs`(~L412 终态、L667-682 fetch_content)、`src/adapters/redis.rs:438-465`(映射)、`src/ports/progress_tracker.rs:60-98`(终态枚举+脱敏)、`src/adapters/postgres.rs:2123-2181`(持久化 fallback)、`src/domain/entities.rs`(TaskConfig/SearchOptions)、`src/adapters/facebook.rs:522-644`(循环样板,只读参照)、`src/testing/mock_gateway.rs`、`Cargo.toml`(FR-001 dev-dep) | `plans/modules/m1-pagination-core.md` |
| M2 `agent-rs/facebook-p0` | `src/strategies/facebook.rs:131,238`、`src/adapters/facebook.rs`(522-644 循环;569-576 RateLimited;1169-1203/1269-1311 mock-HTTP 样板)、`tests/facebook_real_api_test.rs:54-58`、`tests/facebook_real_db_test.rs:278-285` | `plans/modules/m2-facebook-p0.md` |
| M3 `agent-rs/tiktok-p1` | `src/strategies/tiktok.rs:96`、`src/adapters/tikhub.rs:186-187`、`src/tikhub/client.rs:274,1788`、`src/tikhub/types.rs:25-26,297,301-304`、`tests/real_api_test.rs:3-4`、`tests/fixtures/tiktok/search_travel_us.json` | `plans/modules/m3-tiktok-p1.md` |
| M4 `agent-rs/reddit-twitter-p1` | `src/strategies/reddit.rs:100`、`src/strategies/twitter.rs:123`、`src/adapters/reddit.rs:160-178,292-371`、`src/adapters/twitter.rs:161-163,331-462`、`src/tikhub/client.rs:1249-1253,1496-1500`、`src/tikhub/reddit_types.rs:61-99,357-388`、`src/tikhub/twitter_types.rs:44,360-378`、`tests/real_api_test.rs` | `plans/modules/m4-reddit-twitter-p1.md` |
| M5 `agent-rs/instagram-p2` | `src/strategies/instagram.rs:99`、`src/adapters/instagram.rs:285-294`、`src/tikhub/client.rs:716,764`、`src/tikhub/instagram_types.rs:62-90,496-594`;账本写回点:`ledgers/assumptions.md` V1 行、`ledgers/cross-service-contracts.md` C-005 | `plans/modules/m5-instagram-p2.md` |
| M6 `scheduler/once-guard` | (仓:`/Users/jacksoom/programer/aihub/glance_mind_worker/glance_mind_scheduler`)`src/schedule_evaluator.rs:86-151`、`src/lib.rs:166-177,313-383`、`src/db.rs:66-77,113-131`(+新增 terminal_reason 读取)、`src/test_schedule_evaluator.rs`(测试模式) | `plans/modules/m6-once-guard.md` |
| Root | 无源码(消费 M1~M6 计划 + 账本横切行) | `plans/root.md` |

## 7. 拆分理由小结(决策记录)

1. **按 provider lane + 平台优先级切平台模块**,而非按「strategies 层/adapters 层」横切:每个平台模块可独立端到端验收(strategy→adapter→mock→real),且 P0/P1/P2 可独立交付——事故面(facebook)不被 P1/P2 进度拖住。横切方案会让每个模块都改 5 个文件且无法独立验收。
2. **M1 抽出共享契约**:欠交付原因形状、循环不变量(proptest)、redis 语义是全部平台模块与 M6 的公共依赖,放任一平台模块都会制造隐式依赖。
3. **reddit+twitter 合并、tiktok 独立**:前两者是同构镜像移植(评论侧模式已在各自文件内),合并 ≤8 任务;tiktok 是 offset 新构造且有「TikHub max 20」硬约束,证据形态不同。
4. **V1 并入 M5**:见 §1 末。
5. **M6 独立(跨 git 仓)**:独立提交流程 + AG-013 本地变异证据 + 两个 Step 04 裁决预告(R-010 强度、P-007 形态)都属 scheduler 专属上下文。
