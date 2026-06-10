# Root Plan: pagination-max-scan(2026-06-10)

> 计划族:`docs/plans/jplan/2026-06-10-pagination-max-scan/`。消费 M1~M6 模块计划 + 全部账本横切行 + `04-adjudications.md`(用户裁决 D-01~D-12,绑定)。
> 职责(03-split §4):共享接口冻结、跨模块不变量终审、执行顺序与跨仓协调、chain gate 最终验收、评审编排。root 自有源码改动面 = 仅 RT-3(D-10 测试基建任务);其余为验收/文档/协调。

## 1. 共享接口冻结(自此起变更须经本计划修订 + Cross-Service Reviewer)

| 接口 | 定义处 | 冻结内容(含模块期修订) |
|---|---|---|
| **D1 欠交付契约** | M1 §2 | `FetchShortfall::{Exhausted, PartialFailure{message}}`、`FetchOutcome{contents, shortfall}`、trait 默认方法 `fetch_by_keyword_with_outcome`(默认 None;滚动兼容语义) |
| **D2 翻页状态机** | M1 §2 | `PaginationLoop`(`accept_page`/`StopReason` 四值/`shortfall_for`/`MAX_EMPTY_PAGES=3`)。**消费形态(D-01)**:facebook 特例保留手写循环 + M2-T2.8 语义对齐断言;tiktok/reddit/twitter/instagram-分支A 必须接状态机 |
| **D3 orchestrator 映射** | M1 §2 | 映射表六行(以 M1 §2 D3 现行表为准,含两条现状回归行与一条不可达防御行) + 「contents 非空 + Exhausted 不调 stop_campaign_gracefully」(D-03);campaign 级完结区分唯一实现 = M6 |
| **D4 页大小语义** | M1 §2 + 修订 | `TaskConfig.page_size_hint`(redis 映射 clamp);**修订措辞(D-02)**:hint 按平台可控性消费——tiktok 经 `extra_keys::PAGE_SIZE` 下发(M3 §2.1 载体,**一并冻结**);facebook/reddit/twitter/instagram(V3/V2)上游无单页参数,文档化豁免(各计划注释固化) |
| **platform_page_cap 冻结表(D-04)** | M1 §2 D4 | fb=20 / tiktok=20 / reddit=100 / twitter=100 / ig=50 / unknown=20;修订须上游实测证据 + 本计划更新 + ASSERTION-CHANGE 流程 |
| **C-004 terminal_reason 词汇** | M1-T6 pin | 6 个 code 字符串 + `"CODE: message"` 形状;scheduler 按冒号前 token 精确比较读取、容忍 NULL/未知(M6 §2.1);**M6 读方为冒号前 token 精确比较(DR-21)** |
| **C-003 completed_reason 取值** | M6/D-06 | `ONCE_EXECUTED` | `SEARCH_EXHAUSTED`(含 EXHAUST 子串,gm-e2e 白名单安全) |

## 2. 跨模块不变量终审(root 验收时执行)

1. **R-012 零形状改动(全模块 diff 终审)**:agent 仓 `git diff main...HEAD --stat -- migrations/ src/schema.rs src/db/schema.rs src/protocol_gen/` 零命中;预算函数零改动(`src/adapters/postgres.rs` 进度过程、scheduler `lib.rs` reserve 路径仅注释);scheduler 仓 `migrations/` 零命中(M6-T5 已查,root 复核)。
2. **契约不变约束 §3 全四条**:无跨服务形状变更;TaskTerminalReason 字符串未重命名(`git diff -- src/ports/progress_tracker.rs` 仅 M1 新增构造器/测试);scheduler 读方 NULL 容忍测试在册(M6-T1.2/1.3);SEARCH_EXHAUSTED 子串断言在册(M6-T1.10)。
3. **F-007 文档化(root owner)**:task 重派重扫属预期(task 级 `(task_id,video_id)` 唯一;eval_once 对 failed **每 tick 重派一次、跨 tick 无上限**(上界 = 预算 reserve 失败/campaign 终止;task 级唯一约束保证重派不产生重复行);「重试上限」登记 backlog 观察项)——RT-1 落为文档段,既有 real-DB 唯一约束测试为证据,无代码改动。
4. **I-006 终审**:M2 T-054 守恒实跑输出 + 本节 1 的预算零改动 diff,双证齐备。
5. **AG 横切**:各模块「允许先绿」清单的 AG-006 证明逐项在册(M2 §5、M3~M6 各 §3);justified gaps(test-suite §8)理由仍成立复核。
6. **D-13 任务级 max_count**:任务级 max_count(跨 keyword remaining)已裁决并由 M1-T4 承载,root 核对其 K=2 测试证据在册。
7. **0/1 内容边界显式接受**:0/1 内容边界的 campaign 命运不连续(交付 0 条停 campaign、≥1 条不停)为**现状回归保护,显式接受**(state-machine#F9;F-1 修复后该边界仅剩 Exhausted/None 两形状)。

## 3. 执行顺序与跨仓协调

- **执行序 = 起草序**:M1 → M2 → M6 → M3 → M4 → M5(M3/M4 可并行;M5 的 T1/T2 可先行,分支任务 gated on V1)。
- **跨仓部署兼容(C-004)**:无硬部署顺序——scheduler 读方容忍 NULL/未知值(老 agent 不写 terminal_reason 或写旧值均安全);agent 先上 / scheduler 先上皆可。M6 为兄弟仓独立 PR 流程,合并窗口不与 agent 仓绑定。
- **回滚语义**:任一平台模块回滚不影响其他平台(per-platform override;默认方法滚动兼容);M6 回滚仅失去区分值/WARN,不破坏完结流程。
- **每模块独立分支/PR,diff 基线 = main**(AG-012 in-diff 范围与回滚粒度);各模块从**干净基线**起做——现存工作区改动(如 `src/adapters/postgres.rs` 未提交改动)先独立 commit/revert 出计划族 diff,否则零 diff 自查与变异范围被污染。

## 4. Chain gate(最终验收;全部通过才可声明计划族完成)

```bash
# agent 仓(逐项输出留存)
cargo test --lib                                     # 确定性全绿
cargo test                                           # 全量;live 红按 AG-007 区分记录
git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1   # 无 missed 或在册豁免
cargo test --lib -- --nocapture incident_269         # T-040 两形状(chain gate 核心项)
# gated(有凭据环境)
FACEBOOK_RAPIDAPI_KEY=… cargo test --test facebook_real_api_test -- --nocapture
DATABASE_URL=… cargo test --test facebook_real_db_test -- --nocapture
TIKHUB_API_KEY=… cargo test --test real_api_test -- --nocapture
# scheduler 仓(M6-T3/D-07 证据;须在 scheduler 目录执行)
cd /Users/jacksoom/programer/aihub/glance_mind_worker/glance_mind_scheduler
DATABASE_URL=… cargo test --test real_db_completed_reason_test -- --nocapture
# scheduler(worker 仓子目录;独立生成 diff,--relative 剥前缀——复用 agent 仓 diff 会 0 变异体恒绿)
cd /Users/jacksoom/programer/aihub/glance_mind_worker/glance_mind_scheduler
git diff --relative main...HEAD > /tmp/scheduler-pr.diff
cargo test && cargo mutants --in-diff /tmp/scheduler-pr.diff -- --test-threads=1
```

**验收判据**:① T-040 两形状 green(事故重演钉死);② 变异门禁判据 = **逐模块 PR 的 AG-012/AG-013 预检输出 + CI mutation 工作流运行链接汇总**(各模块合 main 后在基线上重跑 `git diff main...HEAD` 会得空 diff,不可作为「无 missed」证据);chain gate 命令块中的变异命令仅用于「存在未合并改动时」的终验;③ M6 AG-013 证据 + mutation CI 工作流实跑可见(D-09);④ `rust-verify-change` 通过(agent 仓);⑤ live 红区分上游漂移记录在案(AG-007);⑥ 本计划 §2 七项终审全过;⑦ V1 判定与所选分支证据在册(M5);⑧ **两仓 mutants 运行输出均须含 `Found N mutants` 且对含 src 改动的 diff N ≥ 1;N==0 即门禁配置失败**(空转哨兵),输出留存。

## 5. Root 自有任务

### RT-1 F-007 文档化(无代码)
**写出**:本计划族 `compile/` 或仓 docs 适当位置一段「task 重派重扫语义」说明(失败矩阵 F-007 行扩写):eval_once 对 failed **每 tick 重派一次、跨 tick 无上限**(上界 = 预算 reserve 失败/campaign 终止);重派重扫属预期(task 级唯一约束);「重试上限」登记 backlog 观察项。引用既有 real-DB 唯一约束测试为证据。验收 = 文档存在 + Step 06 Cross-Service Reviewer 读签。

### RT-2 横切裁决落账(无代码)
1. mock-HTTP helper 第三次复制(M3 §6.3)→ **裁决:落为后续重构任务**(提升 `src/testing`,独立 PR,非本计划族 gate);登记 backlog 条目。
2. M6 mutation CI required-check 设置(M6 §6.3)→ GitHub 分支保护手工配置一次,验收清单跟进项(N-005/N-006,账本已建)。
3. M2 §6 各裁决终审状态同步(已由 04-adjudications.md D-01~D-04 定,本任务仅核对各计划引用一致)。

### RT-3 D-10:facebook_real_api_test env-skip 守卫对齐(测试基建,独立小任务)

> **已提前迁移为 M1-T0**(D-14①/DR-08,Step 07);本节保留为验收引用——root 终审核对 M1-T0 证据(无凭据环境全 skip 输出 + 零断言改动 diff),不重复执行。

**文件**:`tests/facebook_real_api_test.rs`。**改动**:增加与 `facebook_real_db_test.rs:271-289` 同构的 env 守卫(凭据未设 → 显式 skip + eprintln,**不用 `#[ignore]`**);**零断言改动**(只加 gate 前置,所有既有断言原文保留——任何断言触碰即违反 AG-002,本任务无此需要)。**验收**:无凭据环境 `cargo test --test facebook_real_api_test` 全 skip 不 panic;有凭据环境行为不变。**RED/GREEN**:基建型改动,以「无凭据环境从 panic-fail 变 skip」的前后输出对照为证据。**反作弊声明**:不得借本任务弱化/删除任何 live 断言。

### RT-4 mutation workflow 去 live key(D-14②)

**文件**:`.github/workflows/mutation-rust.yml`(agent 仓根)。
**改动**:从两个 job 的 `.env` 写入步骤移除 `TIKHUB_API_KEY`/`FACEBOOK_RAPIDAPI_KEY`(保留 `DATABASE_URL`——受既有 `RUN_REAL_DB_TESTS` 守卫保护);不移除任何其他环境变量。
**前提**:M1-T0 已落地(live 测试有 env-skip 守卫,baseline 不 panic);若 M1-T0 未完成则此任务 BLOCKED。
**验收**:workflow diff 可见两 key 移除 + 一次 mutation run 日志显示 live 测试 skip、零外部调用。
**独立小 PR**:与其他模块 PR 解耦,单独 review 合并。

## 6. 评审编排(Step 06;Reviewer Queue 输入物分配)

| Reviewer | 输入物 | 复核点 |
|---|---|---|
| **Test-Gate**(反作弊) | 全模块计划 §「允许先绿」清单 + AG-006 证明、AG-011/AG-013 豁免文本、M5-T1 探测性质说明、RT-3 零断言改动 diff、`04-adjudications.md` D-11 | 无任务以改/删断言达成;变异豁免逐条成立;金丝雀程序输出在档 |
| **Concurrency/Resource** | M1 §2 D2 + M2 §6.1(双实现语义对齐 T2.8)、M3/M4 循环任务、预算面(I-006/A012) | 循环终止/防环/预算上界;「两套循环单一语义」可接受性(D-01 已裁决,否决须功能性反证) |
| **Cross-Service Contract** | C-001~C-005 + §3 四条、M1-T6/M6-T1.10 契约钉、N-002 双仓注释、本计划 §1 冻结表 | 字段语义跨仓一致;滚动部署兼容;SEARCH_EXHAUSTED 子串安全 |

评审否决已裁决项(D-01~D-12)须附功能性反证并经用户确认(04-adjudications.md 评审接口节)。

## 7. 模块完成判据汇总(03-split §5 沿用,root 只引用不复抄)

M1~M6 各模块计划 §4/§5(或同名节)为准;root 验收时逐模块核对 RED→GREEN 证据包 + gated 输出 + 变异预检记录齐备。

---

Step 07 patch 记录:见 `patches/07-batchE-root.md`
