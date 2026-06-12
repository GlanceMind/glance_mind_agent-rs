# Patch Note: Step 07 批 E — root 计划(2026-06-10)

> 文件:`plans/root.md`。实现:独立子 agent(10 项,DONE);spec 审:✅ 9/10(唯一 ❌ = 本文件曾悬空,现已落);质量审:✅(2 Minor 已修:F-007 句内去重、gated 段 scheduler 测试行补 cd 隔离)。

## 关闭的 findings 与闭合理由

| Finding | 改动 | 复审方 |
|---|---|---|
| DR-06(P1,root 侧) | §4 scheduler 段改 worker 仓子目录绝对路径 + 独立 `--relative` diff(`/tmp/scheduler-pr.diff`,不复用 agent 仓 diff);gated 段补 `real_db_completed_reason_test`(M6-T3/D-07);验收判据⑧ 双仓哨兵(Found N mutants,N≥1) | Dependency / Test-Gate |
| TG-01(P2) | 验收判据② 变异门禁判据改述 = 逐模块 PR 的 AG-012/AG-013 预检输出 + CI 运行链接汇总(合 main 后空 diff 不可作证据);「无 missed」要求经 AG-013 定义保留,语义等价且加严 | Test-Gate |
| DR-07/D-14②(P1) | 新增 RT-4:mutation-rust.yml 移除 TIKHUB/FACEBOOK live key(保留 DATABASE_URL),前提 M1-T0,验收 = workflow diff + run 日志 live skip 零外部调用,独立小 PR | Dependency |
| DR-08(P1) | RT-3 迁移指针(已提前为 M1-T0;root 仅验收引用不重复执行) | Test-Gate |
| DR-22(P2) | RT-1 与全文「一次重派」→ per-tick 口径(grep 零残留) | Failure-Recovery |
| F-10 / D-13 / SM#F9(P3/P1 关联) | §1 D3「六行」对齐 M1 现行表;§2 补 D-13 任务级 max_count 终审条 + 0/1 边界显式接受条 | Plan-Integrator / State-machine |
| F-03 / FR#6(P2/P3) | §3 补「每模块独立分支/PR,diff 基线 = main」+「干净基线(postgres.rs 现存改动先 commit/revert 出 diff)」 | Plan-Integrator |
| 簿记 | §1 C-004 行 DR-21 注;RT-2.2 引用 N-005/N-006;本文件引用闭合 | — |

## 命令形态一致性(spec 审实证)

CI 用 `--relative=glance_mind_scheduler`(worker 仓根上下文)、本地用子目录内无参 `--relative`——两种等价形式,root 与 M6 一致,无第三种矛盾形式。
