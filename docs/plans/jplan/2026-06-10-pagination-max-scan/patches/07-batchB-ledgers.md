# Patch Note: Step 07 批 B — 账本族(2026-06-10)

> 文件:`ledgers/{invariants-failures, anti-gaming-test-quality, test-suite, production-dependencies, providers, non-code-exceptions, assumptions}.md`(7 个)。实现:独立子 agent(14 项)+ 两轮微补(2+3 处);spec 审:✅ 16/16(零弱化、零超界、未改账本抽查通过);质量审:✅(3 Minor 已修,grep 核验落位)。

## 关闭的 findings

| Finding | 改动 | 复审方 |
|---|---|---|
| DR-01 账本侧 | F-010 新行(过滤后零交付+原始有进展 → Err)+ I-002(e) | Failure-Recovery |
| DR-09 | §3「进展定义」行(= FetchOutcome.contents 过滤后条目数) | State-machine |
| DR-10 账本侧 | F-002 收窄(=RateLimited)+ I-005 边界注 + retry 耗尽衔接句 | Failure-Recovery |
| DR-14 | AG-008 新增(live 探针三判据 + 防滥用句) | Test-Gate |
| DR-22 | F-001/F-007/§3 重试行/A007 改述(每 tick 重派、跨 tick 无上限) | Failure-Recovery |
| DR-07/DR-08 账本侧 | P-001/P-002 现状纠偏 + 结论节 D-14 口径 ×2 + providers 控制汇总 | Dependency |
| TG-07 | §4 R-002 脚注 + test-suite §8 两条 justified gap(facebook 实现级 PT、F-010 构造器) | Test-Gate |
| TG-08 | N-005/N-006(含 gh api 命令示例)/N-007 | Test-Gate |
| FR#5 | F-009 验证列如实改述(T-054 仅主路径) | Failure-Recovery |

## 质量审确认项

三处 D-14/M1-T0 引用口径一字一致;N-007 与 M6-T4 三段实证逐字对应;requirements/cross-service 两账本与新口径无冲突;AG-008 防滥用句完整。

## 遗留(已转交)

root.md「failed 仅一次重派」旧措辞 → Task 5(root patch)的 DR-22 项处理(spec 审观察项,已在该任务 spec 内)。
