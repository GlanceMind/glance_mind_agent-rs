# Final Implementation Handoff — pagination-max-scan(2026-06-10)

> **计划族就绪声明**:`99-traceability.md` 裁定 **PASS**;无 P0/P1 遗留。本文件是实现阶段的唯一入口。
> **注意**:本计划族只完成了计划与验证设计;**没有任何实现测试被实际运行**——所有 RED/GREEN/变异命令属执行期义务。
> 仓库落点:PR #6(分支 `docs/jplan-pagination-max-scan`,4 commits)。

## 1. 问题与蒸馏目标(01-first-principles.md)

生产事故 campaign 269 / task 4285:`max_scan_count=50` 实扫 20 即被标 COMPLETED。蒸馏目标(无方案词汇):**任务要么交付到配置量,要么如实记录并暴露欠交付原因**。

## 2. 计划文件路径

- Root:`plans/root.md`(接口冻结表 §1 / 跨模块终审 §2 / 执行纪律 §3 / chain gate §4 / RT-1~RT-4 §5 / 评审编排 §6)
- 模块(执行序):`plans/modules/m1-pagination-core.md`(8 任务,含 M1-T0)→ `m2-facebook-p0.md`(7 任务)→ `m6-once-guard.md`(5 任务,worker 仓子目录)→ `m3-tiktok-p1.md`(4 任务,**迁移任务**)∥ `m4-reddit-twitter-p1.md`(5 任务)→ `m5-instagram-p2.md`(4 任务,V1 gated 双分支)
- 裁决:`04-adjudications.md`(D-01~D-15,执行期绑定;否决须功能性反证 + 用户确认)

## 3. 第一性摘要(01-first-principles.md、ledgers/assumptions.md)

- 假设处置:交接零 ASSUMED;**A005 drop**(trait 不传 cursor,改为欠交付原因——显式范围变更);A003 改写(翻页位置默认适配器内);A010 收窄(P0/P1/P2 分级而非一次修全)。
- 被拒常规方案(03-split.md §7):orchestrator 层加 cursor 循环(B2 只需欠交付原因)、按 strategies/adapters 横切分模块(无法独立验收)、独立 V1 模块(单任务计划)。
- 重 gate 例外:db-migration/api-contract guard INAPPLICABLE(R-012 零形状改动,有 diff 自查);scheduler 属性测试豁免(输入域穷举,anti-gaming §3 末);LLM 边界 INAPPLICABLE(ledgers/llm-api-boundary.md 带证据)。

## 4. 拆分理由(03-split.md §1/§7)

root + 6 模块:按 provider lane × 平台优先级切(每平台可独立端到端验收与回滚);M1 抽共享契约防隐式依赖;reddit+twitter 同构合并、tiktok 独立(机制不同);M6 跨仓独立;V1 并入 M5。

## 5. 评审状态

- Step 05(reviews/autoplan/summary.md):手工等价(CEO/Eng,design N/A),无 P0/P1,F-01~F-05 入队。
- Step 06(reviews/domain/summary.md):7 领域评审并行,**P1×8 + P2×14 + P3 批**;DR-06 四重独立确认、DR-01/02 双重确认。
- Step 07(patches/07-batch{A..F}-*.md):全部闭合(subdriven 双审 ×6 批);新增裁决 D-13/D-14/D-15。
- Step 08(99-traceability.md + compile/*.md):**PASS**(4 分片,编译期修复闭环)。**P0/P1 关闭计数:8/8。**

## 6. 测试面摘要(各计划 §3/§5;ledgers/test-suite.md、production-dependencies.md、providers.md、anti-gaming-test-quality.md)

- **Feature mock**:每 feature 确定性 mock 测试带文件/名/命令/预期 RED(T-001~T-017、T-020~T-022、T-040;M1 PT-1~PT-5 属性套件)。
- **生产依赖 real gates**:P-001~P-007 全部有 gated 测试或带证据 inapplicability;预算 = HTTP 请求上界(DR-19 零重试口径);live 测试不入默认必绿集(M1-T0 守卫 + RT-4 去 key,D-14)。
- **Provider 双 gate**:PV-001~PV-005 mock+real 双侧;fixture 回灌 + 逐字段对账义务(禁凭空捏造)。
- **LLM 边界**:INAPPLICABLE(带证据与重评触发器)。
- **反作弊**:AG-001~AG-008 + AG-010~AG-013;「允许先绿」三形态清单与正文精确一致(99 文件核验);变异门禁双哨兵(Found N≥1)+ 事故根因行/三循环出口不可豁免;scheduler 仓 `--relative` 命令族防空转。

## 7. 执行约定(实现 agent 必读)

1. **起步 = M1-T0**(live 测试 env-skip + CI opt-in 守卫,零断言改动)——不先做它,所有模块的本地 AG-012 预检 baseline 会因 live panic 失败。
2. **每模块独立分支/PR,diff 基线 = main;干净基线起做**(现存 `src/adapters/postgres.rs` 未提交改动先 commit/revert 出计划族 diff)——root §3。
3. **subdriven 纪律**:每任务测试载荷与实现载荷分属不同子 agent(AG-004);RED/GREEN 输出留存;金丝雀程序按计划文本逐字执行(只动生产代码、输出留存后还原)。
4. **跨仓**:M6 在 `glance_mind_worker/glance_mind_scheduler`(worker 单一 git 仓子目录);mutants 必须 `--relative`;部署无硬序(NULL 容忍)。
5. **M3 是迁移任务**:main PR #5 已有独立修复;按 M3 头部差异清单迁移到共享契约;f3/r6a 既有测试撞红走 ASSERTION-CHANGE-JUSTIFIED(测试上下文执行)。
6. **chain gate**(root §4):T-040 两形状为核心项;变异判据 = 逐模块预检输出 + CI 链接汇总。

## 8. 开放项移交(7 条,99-traceability.md)

V1 判定(M5-T1)| M3 per-request count 上游行为(T-051)| M2-T4.3 AG-006-diff 失效预案(执行期金丝雀)| M1-T5.3 E0609 输出留存 | N-006 required-check 手工设置 | RT-2.1 helper 提升 backlog | user-videos 双循环归并 backlog。

## 9. 下一个 agent 读什么

`handoff.md`(本文件)→ `plans/root.md` → **只读你将实现的那一个模块计划** → 该计划头部点名的账本行。不要重读全族。
