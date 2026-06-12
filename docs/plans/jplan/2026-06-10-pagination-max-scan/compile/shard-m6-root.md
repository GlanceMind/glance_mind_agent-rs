# Traceability Compile — 分片报告:M6 + root(2026-06-10)

> 执行者:Traceability Compiler 分片(只读;唯一写入 = 本文件)。
> 输入:`plans/modules/m6-once-guard.md`、`plans/root.md`、`03-split.md`、`04-adjudications.md`(D-03/06/07/08/09/13/14/15)、ledgers/{requirements, test-suite, cross-service-contracts, production-dependencies, framework-test-research, non-code-exceptions, anti-gaming-test-quality, invariants-failures}.md、`handoffs/07-to-08.md` §Compiler 须知、`00-manifest.md`、patches/07-batch{B,D,E}.md、交叉抽样 m1/m2/m3/m5 计划。

---

## 检查项 1:03-split §3 归 M6 的全部 ID 有认领且任务实存 — **PASS**

M6 §5 映射表逐 ID 核对 03-split §3.1~§3.5 点名集(R-010 / R-011 scheduler / R-013、I-007、F-008、T-020~T-022、C-003 / C-004 scheduler、P-007、FR-002 / FR-004、N-002 scheduler / N-003、AG-013),全部出现且认领任务实存。下钻核实(8 条 ≥ 要求 6 条):

| ID | 认领声明 | 下钻证据(任务实存且载荷对应) |
|---|---|---|
| R-010 | M6-T1 + M6-T2 | T1 真值表 row3(`underscan_warning=true`)+ T1.11「不补派」决策钉;T2.1 WARN+`underscan_warning_campaign_ids` 指标落地;强度 = D-08 引用一致(不补派、保留升级点) |
| R-013 | M6-T1 + T2 + T3 | T1.4 `SearchExhausted`;T2.2 返回 `"SEARCH_EXHAUSTED"`;T3 落库断言 `completed_reason == Some("SEARCH_EXHAUSTED")`;终名 = D-06 一致 |
| I-007 | M6-T1.11 + T2.1~2.3 | T1.11 断言 `MarkCompleted` 不变(非 Dispatch);T2 测试仅断言观测字段(零状态副作用) |
| F-008 | M6-T1.4(枯竭不告警)+ T1.1(欠扫告警) | 双向断言齐,账本 F-008「两向断言」要求满足 |
| C-004(scheduler) | M6-T1(读取骨架 + 1.2/1.3) | T1.2 NULL 容忍、T1.3 未知值容忍;schema/entity 声明级接线(§2.2-d,非 migration);agent 侧 pin 归 M1-T6(03-split shared 标注一致) |
| P-007 | M6-T3 | gated 测试任务实存,形态 = D-07 裁决(gated 测试,N-003 降级) |
| FR-002 / AG-013 | M6-T4 / M6-T5+T4 | T4 = mutation-scheduler.yml(D-09);T5 命令序列含 AG-013 本地预检 |
| N-002(scheduler)/ FR-004 | M6-T2 / M6-T1 | T2 注释措辞与 C-002/N-002 账本行(limit=页提示、offset=仅观测)一字对应;T1 沿 make_task/表驱动(FR-004) |

M6 §5 末自查行与 03-split §3 点名集互为镜像,无缺漏、无多余认领。

## 检查项 2:M6 每任务四件套 + 真值表完备 + 33 断言零触碰自洽 + 金丝雀具体 — **PASS**

- **四件套**(覆盖 ID / 文件 / RED 预期失败信息 / GREEN+最终验收命令,另加反作弊声明):T1、T2 完整(T1 含骨架 `todo!()` panic 文本与断言型失败示例两级;T2 以 `E0425` 编译失败为正确 RED 并给出骨架后断言 RED);T3 为 gated「允许先绿」,带 AG-006 论证(gated 不入变异集 + 确定性对应物 T1.10/T2)与书面理由交 Test-Gate 的义务——符合 handoff §Compiler 须知三形态之 AG-008/gated 类;T4 为非测试交付物,以三段实证替代 RED→GREEN(合规,见检查项 3);T5 收尾 gate 带命令序列+判据。五任务均有反作弊声明。
- **真值表穷举完备**:2 布尔输入 4 格全覆盖——(false,false)=T1.7、(false,true)=T1.8、(true,true)=T1.4、(true,false 含 NULL/未知)=T1.1/1.2/1.3;NULL→false 显式定义(DR-21 句);裸 code 无冒号 = T1.5(splitn 无冒号取整串,论证成立);前缀非子串防误判 = T1.6(取 token 精确比较天然满足,GREEN 指引禁止 `contains`/`starts_with` 改写);边界 `>=` = T1.9(51/50 过交付)。无遗漏格。
- **既有 33 断言零触碰设计自洽**:§2.2-a 并行纯函数 `eval_once_completion`(决策枚举不加载荷)使 `evaluate`/`eval_once` 返回形状不变 → 既有断言无需动;`make_task` 仅补 `terminal_reason: None` 字段初始化并显式声明「结构体字面量补全,非断言改动」;T1/T5 验收均含 `git diff -- src/test_schedule_evaluator.rs` 复核。设计闭环。
- **金丝雀程序具体且含伴随红声明**:T1.11(completed 分支→`Dispatch` 须红)、T1.12(failed 分支→`Skip` 须红)各写明:只动生产代码、还原后 33+新增全绿、输出留存、伴随红(既有断言同步变红属预期,观察对象=新增测试)——与 batch D「表观张力消解」声明一致;§4 完成判据同步要求(DR-12:eval_once 零改动场景变异预检不覆盖,金丝雀为唯一有效证明)。

## 检查项 3:M6-T4 三段实证 + N-007 入账;哨兵两处;--relative 命令族一致 — **PASS**

- **三段实证**:① YAML 静态校验(actionlint 或 python yaml);② 本地等价命令实跑(AG-013 同款,`--relative`);③ M6 PR 上工作流实跑可见且与本地预检一致。`non-code-exceptions.md` N-007 行存在且指回「按 m6-once-guard.md M6-T4 原文」三段——逐字对应(batch B 质量审确认项亦核过)。
- **哨兵判据两处**:M6-T4 验收第 4 条 + M6-T5 验收判据第 2 条,均为「Found N mutants 且 N≥1;N==0 即门禁配置失败」,且 T4 补「N==0 须先修 --relative 再重跑,不得视为通过」。root §4 判据⑧ 为双仓哨兵(第三处,加强)。
- **--relative 命令族一致**:CI 形态 = worker 仓根 `git diff --relative=glance_mind_scheduler … -- glance_mind_scheduler`(M6-T4);本地形态 = 子目录内裸 `--relative`(M6-T5 §3、M6 §4 独立验证、root §4 chain gate)。两种等价形式,无第三种矛盾形式(batch E spec 审已实证,本次复核同结论)。

## 检查项 4:P-007 gated 测试控制齐;N-003 降级注明 — **PASS**

- Gate 控制:`DATABASE_URL` 未设 / `GITHUB_ACTIONS` 下未显式 `RUN_REAL_DB_TESTS` → 显式 skip + eprintln(不 panic、不 `#[ignore]`);gate 模式镜像 `facebook_real_db_test.rs:271-289`(与 P-005 账本行同源模式)。
- 自建行+清理:唯一标记 `__m6_p007__<uuid>` 直插、断言后 DELETE、失败路径 guard/finally 清理;幂等声明齐;成本 = 本地/CI DB 无外部成本(与 production-dependencies P-007 行一致)。
- N-003 降级:T3 验收注「部署后人工 SQL 抽查改为可选运维步骤,不入本计划验收」+ §5 行「D-07 后非验收必需」——与 D-07 裁决文本及 N-003 账本 CONDITIONAL 状态(「Step 04 若决定建最小 gated 测试则本条作废/降级」)一致。

## 检查项 5:root §1 冻结表与模块计划逐项一致;§2/§4/§5/§6 — **PASS**(1 条 P3 观察)

- **D1**:root 行(`FetchShortfall::{Exhausted, PartialFailure{message}}`、`FetchOutcome{contents, shortfall}`、默认方法 None/滚动兼容)与 M1 §2 D1 代码块逐项一致。
- **D2**:`PaginationLoop`/`accept_page`/`StopReason` 四值(ReachedMaxCount/UpstreamExhausted/CursorLoop/EmptyPageLimit,M1 实为恰好 4 变体)/`shortfall_for`/`MAX_EMPTY_PAGES=3` 一致;消费形态(fb 特例 D-01 + M2-T2.8;其余接状态机)与 M2 §6.1、M3 §2 一致。
- **D3 六行**:M1 §2 D3 现行表 = 恰好六行(None/Exhausted/PartialFailure/空+Exhausted-None 回归/Err 回归/空+PartialFailure 不可达防御)——root「六行,含两条现状回归行与一条不可达防御行」精确匹配;「contents 非空+Exhausted 不调 stop_campaign_gracefully」与 M1 D3 设计裁决、M6 §2.1「M1 D3 前提」三处一致(D-03)。
- **D4 + cap 表**:`page_size_hint` + D-02 措辞(tiktok 经 `extra_keys::PAGE_SIZE`,M3 §2.1 实存且标「形状由 root 冻结」;fb/reddit/twitter/ig 文档化豁免)一致;cap 表 fb=20/tiktok=20/reddit=100/twitter=100/ig=50/unknown=20 与 M1 §2 D4 `platform_page_cap` 取值及 D-04 裁决逐值相等。
- **C-004 行(重点)**:root 加粗注「M6 读方为冒号前 token 精确比较(DR-21)」与 M6 §2.1/§2.2-a/T1 GREEN(`splitn(2,':')` 取 token 精确比较)一致。*P3 观察*:同行前半句残留措辞「scheduler 前缀匹配读取」为粗粒度旧描述,与紧随的 DR-21 加粗注并置易生歧义;规范语义由 DR-21 注承载,无执行风险——建议择机把「前缀匹配读取」改为「取冒号前 token 精确比较读取」(纯措辞,非阻塞)。
- **C-003 行**:`ONCE_EXECUTED | SEARCH_EXHAUSTED` 含 EXHAUST 子串,与 M6 T1.10 钉子、D-06、cross-service C-003/§2 白名单证据一致。
- **§2 七项终审**:R-012 diff 命令族 / 契约不变四条(含 M6-T1.2/1.3、T1.10 在册引用,实存)/ F-007(RT-1,per-tick 口径与账本 F-001/F-007 DR-22 改述一致)/ I-006 双证 / AG 横切 / D-13(M1-T4 K=2,M1 计划实存)/ 0/1 边界显式接受——七项齐,各有可执行命令或证据落点。
- **§4 chain gate**:命令块按序可执行——agent 仓段(隐含 cwd=agent 仓)→ gated 段 → scheduler 段两处显式 `cd` 绝对路径隔离(gated DB 测试与 mutants 各自带 cd,batch E 质量审修正项落实);scheduler diff 独立文件 `/tmp/scheduler-pr.diff` 不与 agent `/tmp/pr.diff` 混用;判据② 已按 TG-01 改述(合 main 后空 diff 不可作证据);判据⑧ 双仓哨兵。
- **§5 RT-1~RT-4 各有验收**:RT-1(文档存在+Cross-Service 读签,N-005 对应);RT-2(三子项各有落点:backlog 登记 / N-005/N-006 跟进 / 引用一致性核对);RT-3(迁移指针→M1-T0,root 仅验收引用,M1-T0 实存且四件套齐);RT-4(workflow diff + run 日志 live skip,前提 M1-T0,BLOCKED 语义明确)。
- **§6 评审编排引用实存**:M5-T1(M5 计划首任务实存)、RT-3 零断言 diff(经迁移指针解析至 M1-T0)、D-11(04-adjudications 实存含增补)、M1 §2 D2 + M2 §6.1 T2.8(实存)、M1-T6/M6-T1.10(实存)、N-002 双仓注释(M6-T2 + M1 D4 注释义务)、§1 冻结表——全部可解析。

## 检查项 6:root 与各模块交叉引用无矛盾(抽样 8 处) — **PASS**(1 条 P3 观察)

| # | 交叉点 | 结论 |
|---|---|---|
| 1 | root §4 scheduler 绝对路径 ↔ M6 头部/§3 仓布局 | 一致(`/Users/jacksoom/programer/aihub/glance_mind_worker/glance_mind_scheduler`,worker 单一 git 仓子目录) |
| 2 | root §2.2「NULL 容忍测试在册(M6-T1.2/1.3)」↔ M6 T1 测试 2/3 | 实存,名称/语义一致 |
| 3 | root §2.2「SEARCH_EXHAUSTED 子串断言(M6-T1.10)」↔ M6 T1.10 | 实存,含 `contains("EXHAUST")` 显式断言 |
| 4 | root §4 gated `real_db_completed_reason_test` ↔ M6-T3 文件名/命令 | 一字一致 |
| 5 | M6 §2.3 状态键 `SchedulerRunResult.underscan_warning_campaign_ids` ↔ M6-T2 测试/实现 ↔ root §2 I-007 终审 | 一致(唯一写方 apply_once_completion) |
| 6 | M6-T4 文件 `glance_mind_worker/.github/workflows/mutation-scheduler.yml` ↔ N-007/N-006(required check `mutation-scheduler` grep) | 一致 |
| 7 | root RT-4 `mutation-rust.yml` 双 job/.env ↔ AG-010/FR-002 账本描述(L45/L103 pinned 24.11.0)↔ M6-T4「与 agent 仓 pinned 同版」 | 一致 |
| 8 | root §3 滚动部署(无硬序、读方容忍 NULL/未知)↔ M6 头部依赖声明 ↔ cross-service §3.3 | 一致 |

*P3 观察*:M6 §1.10 与 T1.12 沿用「一次重派」短语,未带 DR-22 的「每 tick / 跨 tick 无上限」限定(DR-22 批次范围 = 账本 + root,M6 未点名)。T1.12 断言对象为单 tick 内 `failed → Dispatch`,语义无误;权威口径由 root RT-1 / 账本 F-001/F-007 承载。建议(非阻塞):M6 §1.10 补「(每 tick 一次)」四字对齐口径。

## 检查项 7:无 P0/P1 遗留;patch note 抽查 3 个;无 placeholder — **PASS**

- manifest「Patch Queue 全部闭合」声明与 handoff 07-to-08「无 P0/P1 遗留」一致。
- **抽查 3 note 闭合理由成立**:
  - `07-batchD-m6.md`:DR-06(--relative/runs-on/双哨兵)、DR-12(双金丝雀+伴随红)、DR-21(三处同步)、SM#F6/protocol#4(已知边界两条)、SM#F7(写方措辞+幂等注)、F-04/N-007(烟测 SQL+空虚真注+三段实证)——逐条在 M6 现行文本验得落位。
  - `07-batchE-root.md`:DR-06 root 侧(§4 子目录 cd+独立 diff+gated 补行+判据⑧)、TG-01(判据②改述)、RT-4 新增、DR-22(root 文本 per-tick,grep 无残留)、D3 六行对齐、D-13/0-1 边界终审条、§3 干净基线——逐条在 root 现行文本验得落位。
  - `07-batchB-ledgers.md`:F-010 行、AG-008 三判据+防滥用句、N-005/N-006/N-007、F-001/F-007/§3 per-tick 改述、F-002 RateLimited 收窄、test-suite §8 两条新 gap——逐条在账本现行文本验得落位;其「遗留→Task 5」项已由 batch E 闭合。
- **placeholder**:M6/root 全文 grep 无 TBD/TODO/占位;显式开放项均为已声明类(V1、M3 per-request count 待执行期复核、N-006 required check 手工项、RT-2.1/user-videos backlog、M6 §6 预案)——与 handoff「显式开放项非 placeholder」口径一致。

## 检查项 8:跨仓事实一致(「独立 git 仓」零残留;滚动部署兼容) — **FAIL(局部;P2)**

- **M6 计划与 root**:✅ 全文已统一为「worker 仓子目录(单一 git 仓、独立 PR/提交流程)」,零残留;reviews/06-to-07 等历史评审/路由文件中的出现为引述原始 finding,属历史证据,合规。
- **滚动部署兼容**:✅ root §3 声明(老 agent 不写/写旧值均安全、双向部署序皆可)与 M6 T1.2(NULL)/T1.3(未知值)测试一一对应;cross-service §3.3 三处一致。
- **❌ 残留 1(P2)**:`03-split.md:32`(§2 定稿模块队列 M6 行)仍写「glance_mind_worker/glance_mind_scheduler(**独立 git 仓**、独立提交流程)」——03-split 是 Step 08 必读的覆盖映射基准,与 M6/root 现行事实(DR-06 实证:无独立 .git)矛盾。
- **❌ 残留 2(P2,同根因)**:`03-split.md` §5 M6 行验证命令为 `git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --test-threads=1`——**缺 `--relative`**,正是 DR-06 认定的「0 变异体恒绿空转」形态;且 diff 文件名 `/tmp/pr.diff` 与 agent 仓冲突。M6 §4 与 root §4 的现行命令(含 --relative、独立文件名)已正确,root §7 亦声明「以模块计划 §4/§5 为准」,故无执行性风险,但基准文档与冻结事实矛盾,违反「零残留」判据。
- **附带(P3)**:`handoffs/04-m2-to-04-m6.md:7` 同句残留;handoff 为历史交接记录,按惯例不改写,可加一行勘误注或在 compile 终稿登记为已知历史残留。

**Patch 建议(闭合检查项 8)**:对 `03-split.md` 打一条勘误 patch(或在文件头加「Step 07 勘误」节):
1. §2 M6 行仓库列改为「glance_mind_worker/glance_mind_scheduler(worker 单一 git 仓子目录;独立 PR/提交流程;DR-06 实证)」;
2. §5 M6 行命令改为「(在 glance_mind_scheduler/ 子目录内)`git diff --relative main...HEAD > /tmp/scheduler-pr.diff && cargo mutants --in-diff /tmp/scheduler-pr.diff -- --test-threads=1`」,或直接替换为指针「验证命令以 m6-once-guard.md §4 为准」;
3. (可选)`handoffs/04-m2-to-04-m6.md` 加单行勘误注。
均为措辞级 patch,无设计面变更,不需重开评审;建议作为 Step 08 终稿前的 batch G 微补或由 root 验收清单登记。

---

## 分片裁定

**CONDITIONAL PASS(7/8 PASS;1 项局部 FAIL,P2 措辞级,patch 路径明确)**

- FAIL 列表:仅检查项 8 的 `03-split.md` 两处残留(「独立 git 仓」+ 无 --relative 的 M6 验证命令)。无执行性/设计性缺口:现行 M6 §4 与 root §4 命令正确且 root §7 已声明模块计划优先,残留风险被双哨兵判据(N≥1)进一步兜底。
- P3 观察(非阻塞,记录在案):root §1 C-004 行「前缀匹配读取」旧措辞;M6 §1.10/T1.12「一次重派」缺 per-tick 限定;`handoffs/04-m2-to-04-m6.md` 历史残留。
- 其余全部 PASS:M6 全 ID 认领且任务实存(下钻 8 条)、四件套/真值表/33 断言零触碰/金丝雀程序齐备自洽、T4 三段实证+N-007+双哨兵+--relative 命令族一致、P-007 gate 控制齐+N-003 降级、root 冻结表与六模块逐项一致(D3 六行、C-004 DR-21 注、cap 表、PAGE_SIZE 载体)、§2 七项可执行、§4 chain gate 按序可执行、§5 四任务各有验收、§6 引用实存、交叉引用抽样 8 处零矛盾、patch queue 闭合声明经 3 note 抽查成立、无 placeholder。
