# 00 - Manifest(路由唯一真源)

## Request Summary

修复「campaign 配置 `max_scan_count=50` 实际只扫 20 即被标记 COMPLETED」(生产事故:campaign 269 / task 4285, 2026-06-10)。
已确认的设计方向:**agent-rs 在单 task 内按上游 cursor 翻页直到 max_count(`search_limit` 退化为页大小提示);glance_mind_scheduler 保留时间调度职责并增加防御性校验(ONCE 活动 task 完成但未扫满时告警/兜底),不承担翻页**。涉及两个仓库:`glance_mind_agent_rs`(主)与 `glance_mind_worker/glance_mind_scheduler`(次)。

## Plan Family Status

- status: `in-progress`
- created: 2026-06-10
- directory: `docs/plans/jplan/2026-06-10-pagination-max-scan/`

## Steps

- current_step: `07-patch-iterate`(Step 06 完成:7 评审并行交付,**P1×8 + P2×14 + P3 批**;路由 Step 07)
- next_step: `07-patch-iterate`(权威输入 = `reviews/domain/summary.md`;批次与裁决提请见 `handoffs/06-to-07.md`)
- completed_steps:
  - `00-init` — 2026-06-10
  - `01-first-principles` — 2026-06-10
  - `02-ledgers` — 2026-06-10
  - `03-split` — 2026-06-10
  - `04-draft(M1)` — 2026-06-10
  - `04-draft(M2)` — 2026-06-10
  - `04-draft(M6)` — 2026-06-10
  - `04-draft(M3/M4/M5/ROOT)` — 2026-06-10(用户授权单会话连续起草)
  - `05-autoplan-review` — 2026-06-10(手工等价;无 P0/P1)
  - `06-domain-review` — 2026-06-10(7/7 评审,并行只读子 agent;P1×8 → 路由 Step 07)

## Artifact Map

| Artifact | Path | Status |
|---|---|---|
| Manifest | `00-manifest.md` | ✅ |
| Context inventory | `00-context-inventory.md` | ✅ |
| Testing constraints | `constraints/testing-constraints.md` | ✅ |
| Handoff 00→01 | `handoffs/00-to-01.md` | ✅ |
| First principles | `01-first-principles.md` | ✅ |
| Handoff 01→02 | `handoffs/01-to-02.md` | ✅ |
| Assumption ledger | `ledgers/assumptions.md` | ✅(Step 02 增补) |
| Requirement ledger | `ledgers/requirements.md` | ✅ |
| Invariant/failure matrix | `ledgers/invariants-failures.md` | ✅ |
| Test suite ledger | `ledgers/test-suite.md` | ✅ |
| Anti-gaming test quality | `ledgers/anti-gaming-test-quality.md` | ✅ |
| Production dependencies | `ledgers/production-dependencies.md` | ✅ |
| Provider tests | `ledgers/providers.md` | ✅ |
| LLM API boundary | `ledgers/llm-api-boundary.md` | ✅(INAPPLICABLE 带证据) |
| Cross-service contracts | `ledgers/cross-service-contracts.md` | ✅(O1 闭环) |
| Framework test research | `ledgers/framework-test-research.md` | ✅ |
| Non-code exceptions | `ledgers/non-code-exceptions.md` | ✅ |
| Handoff 02→03 | `handoffs/02-to-03.md` | ✅ |
| Split / module map | `03-split.md` | ✅ |
| Handoff 03→04 | `handoffs/03-to-04.md` | ✅ |
| Handoff 04(M1)→04(M2) | `handoffs/04-m1-to-04-m2.md` | ✅ |
| Handoff 04(M2)→04(M6) | `handoffs/04-m2-to-04-m6.md` | ✅ |
| 用户裁决记录(功能优先) | `04-adjudications.md` | ✅(2026-06-10;D-01~D-12,绑定后续起草) |
| Root plan | `plans/root.md` | ✅(2026-06-10;接口冻结表 §1 / chain gate §4 / RT-1~RT-3 / 评审编排 §6) |
| Module plan M1 | `plans/modules/m1-pagination-core.md` | ✅(2026-06-10;共享接口 D1~D4 待 root 冻结) |
| Module plan M2 | `plans/modules/m2-facebook-p0.md` | ✅(2026-06-10;7 任务;§6 已经 D-01~D-04 裁决) |
| Handoff 04(M6)→04(M3) | `handoffs/04-m6-to-04-m3.md` | ✅ |
| Module plan M6 | `plans/modules/m6-once-guard.md` | ✅(2026-06-10;5 任务;兄弟仓 surface;eval_once_completion 并行纯函数设计见 §2.2) |
| Module plan M3 | `plans/modules/m3-tiktok-p1.md` | ✅(2026-06-10;4 任务;PAGE_SIZE extra 载体待 root 冻结确认,root §1 已收录) |
| Module plan M4 | `plans/modules/m4-reddit-twitter-p1.md` | ✅(2026-06-10;5 任务;hint 双平台豁免实证) |
| Module plan M5 | `plans/modules/m5-instagram-p2.md` | ✅(2026-06-10;4 任务;V1 首任务 + 分支 A/B 预起草互斥) |
| Handoff 04→05 | `handoffs/04-to-05.md` | ✅(Step 05 只读清单 + 评审待核索引) |
| Autoplan review | `reviews/autoplan/{ceo,eng,summary}.md` | ✅(2026-06-10;手工等价,design N/A;无 P0/P1) |
| Handoff 05→06 | `handoffs/05-to-06-or-07.md` | ✅(路由 = Step 06) |
| Domain reviews | `reviews/domain/{first-principles,protocol,state-machine,test-gate,dependency,failure-recovery,plan-integrator,summary}.md` | ✅(2026-06-10;7/7 并行只读;P1×8) |
| Handoff 06→07 | `handoffs/06-to-07.md` | ✅(批次 A/B/C + 三项裁决提请) |
| Domain reviews | `reviews/domain/` | ⬜ |
| Patches | `patches/` | ⬜ |
| Traceability compile | `compile/` | ⬜ |
| Final handoff | `handoff.md` | ⬜ |

## Module Queue(Step 03 定稿;模块图/账本覆盖映射/完成判据见 `03-split.md`)

按 Step 04 起草序(= 执行序;M3/M4 可并行):

1. **M1 `agent-rs/pagination-core`** ✅ 已起草(`plans/modules/m1-pagination-core.md`,7 任务)— 共享契约:fetch 路径暴露欠交付原因(exhausted/partial,trait 不传 cursor,A005 drop)+ orchestrator 终态映射 + 页失败语义 + 循环不变量/proptest 基建(FR-001 前置)+ redis 字段语义 agent 侧 + 脱敏。覆盖:R-007/R-008/R-009/R-011(agent)、I-001~I-005/I-008/I-009、T-002~T-004/T-030~T-034、C-001/C-002/C-004(agent)、P-006。依赖:无。共享接口 D1~D4 定形于该计划 §2,待 root 冻结。
2. **M2 `agent-rs/facebook-p0`** ✅ 已起草(`plans/modules/m2-facebook-p0.md`,7 任务 T1~T7;T1~T5/T7 确定性、T6 real gates 独立凭据 gate)— 事故平台:strategy cap 解除 + 既有翻页接通(保留手写循环 + shortfall 接线,裁决 §6.1)+ 失败/枯竭/防环集成样板 + 事故重演(T-040 两形状)+ real gates。覆盖:R-001(fb)/R-002、T-001(fb)/T-010/T-015~T-017/T-040/T-050/T-054、PV-001、P-001/P-005、F-001~F-006/F-009(证据侧)、I-006(全部认领,见该文件 §7)。依赖:M1。
3. **M6 `scheduler/once-guard`** ✅ 已起草(`plans/modules/m6-once-guard.md`,5 任务 T1~T5;兄弟仓 surface)— eval_once_completion 真值表(并行纯函数,既有 33 断言零触碰)+ WARN+指标(不补派,D-08)+ `SEARCH_EXHAUSTED`(D-06)+ terminal_reason 读取(C-004,NULL/未知容忍)+ N-002 注释 + P-007 gated 测试(D-07)+ mutation CI 工作流(D-09)+ AG-013 预检。覆盖:R-010/R-011(scheduler)/R-013、I-007、F-008、T-020~T-022、C-003/C-004(scheduler)、P-007、FR-002/FR-004(全部认领,见该文件 §5)。依赖:M1(语义,部署无硬序)。
4. **M3 `agent-rs/tiktok-p1`** ✅ 已起草(4 任务)— offset 翻页循环接 PaginationLoop(D-01 强制;单页≤20,has_more 终止);PAGE_SIZE extra 载体(root §1 冻结)。覆盖:R-001(tiktok)/R-003、T-001(tiktok)/T-011/T-051、PV-002、P-002(全认领,见该文件 §5)。依赖:M1。
5. **M4 `agent-rs/reddit-twitter-p1`** ✅ 已起草(5 任务)— content 路径 cursor 翻页(评论侧模式镜像 ×2,PaginationLoop);hint 双平台豁免(上游无单页参数实证)。覆盖:R-001(reddit/twitter)/R-004/R-005、T-001(两行)/T-012/T-013/T-052、PV-003/PV-004、P-003(全认领,见该文件 §5)。依赖:M1。
6. **M5 `agent-rs/instagram-p2`** ✅ 已起草(4 任务,分支互斥)— V1 首任务(N-001+T-053 ≤4 调用),分支 A 翻页 / 分支 B 单页+如实上报均预起草。覆盖:R-006/V1、T-001(ig)/T-014/T-053、PV-005、P-004、C-005、N-001(全认领,见该文件 §5)。依赖:M1;分支 gated on V1。
7. **ROOT `plans/root.md`** ✅ 已起草 — 共享接口冻结表(§1,含 D-01~D-04 修订)、R-012/F-007/契约不变终审(§2)、跨仓协调(§3)、chain gate(§4)、RT-1~RT-3(含 D-10 任务)、评审编排(§6)。

## Reviewer Queue(Step 06 用,初步)

- Test-Gate Reviewer(反作弊门禁)
- Concurrency/Resource Reviewer(翻页循环 × 并发处理 × 预算递增)
- Cross-Service Contract Reviewer(task 协议字段语义跨仓一致)

## Patch Queue(Step 05 + Step 06 合并;**权威明细 = `reviews/domain/summary.md`**,Step 07 按 `handoffs/06-to-07.md` 批次执行)

**P1(批 A,must-patch)**:DR-01(D3 缺行 空+PartialFailure,双确认)、DR-02(PT-2 属性不可满足,双确认)、DR-03(循环活性缺口/empty 语义)、DR-04(多 keyword 预算超界 → **用户三选一裁决**)、DR-05(M2 429 与重试循环矛盾)、DR-06(scheduler 变异门禁空转族:子目录非独立仓 + --relative + root §4,四重确认)、DR-07(mutation CI 每变异体打付费 live)、DR-08(RT-3 提前为 M1-T0,升级原 F-01)。

**P2(批 B)**:DR-09(进展定义统一)、DR-10(recoverable=仅 RateLimited 冻结)、DR-11(last-Some-wins + 混合 shortfall)、DR-12(AG-006 金丝雀化)、DR-13(M2-T4 RED 错置)、DR-14(AG-008 live 探针类别)、DR-15(簿记外先绿)、DR-16(twitter live 分支断言)、DR-17(T2.8 域 + candidates 聚合)、DR-18(M3 伪代码矛盾)、DR-19(TikHub 429 睡眠×mutants)、DR-20(M4 fixture 来源)、DR-21(C-004 前缀碰撞)、DR-22(failed 重派语义文档)+ Step05 F-02(日志规格)/F-03(PR 纪律)。

**P3(批 C)**:summary §P3 清单(PI F-07~F-11、TG-07~TG-11、FR#5/#6、SM#F6~F9、protocol#2~4、T-010 形状、T-050 预算口径)+ Step05 F-04/F-05。

## Files Required for Next Invocation(Step 07,Patch-Iterate)

必读(per `handoffs/06-to-07.md`,Step 07 只读此清单):
- `docs/plans/jplan/2026-06-10-pagination-max-scan/00-manifest.md`
- `~/.claude/skills/jplan/references/step-07-patch-iterate.md`(权威流程)
- `docs/plans/jplan/2026-06-10-pagination-max-scan/reviews/domain/summary.md`(patch 唯一权威输入)
- `docs/plans/jplan/2026-06-10-pagination-max-scan/handoffs/06-to-07.md`(批次 A/B/C + 受影响文件定位 + 三项裁决提请)
- 受影响计划与账本(按 finding 定位,清单在 handoff §2/§3)
- `docs/plans/jplan/2026-06-10-pagination-max-scan/04-adjudications.md`(patch 不得违背 D-01~D-12;D-04 多 keyword 裁决为新增提请)

起草顺序:M1 ✅ → M2 ✅ → M6 ✅ → M3 ✅ → M4 ✅ → M5 ✅ → ROOT ✅(**Step 04 全部完成**)。

## Open Blockers

- 无 P0/P1 流程阻塞。
- **O1 已闭环(Step 02)**:`completed_reason` 读方五仓枚举完毕,新增区分值安全(证据 `ledgers/cross-service-contracts.md` C-003/§2);**最终命名已裁决 = `SEARCH_EXHAUSTED`(04-adjudications.md D-06,满足 gm-e2e 子串约束)**。
- **V1 仍 OPEN(计划内验证任务)**:仅 gate instagram 分支选择(R-006);登记于 C-005 + N-001 + T-053/P-004;不阻塞 Step 03/04 其他模块。
