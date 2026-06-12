# Domain Review — Test-Gate Reviewer(反作弊门禁)(2026-06-10,只读子 agent 原文)

> 归一化映射:TG-01→DR-06 一部(P1)、TG-02→DR-12(P2)、TG-03→DR-13(P2)、TG-04→DR-02 一部(P1)、TG-05→DR-15(P2)、TG-06→DR-16(P2)、TG-07~TG-11→P3 批。

已通读 root + m1~m6 + constraints + anti-gaming + test-suite §8 + non-code + 04-adjudications + autoplan/eng.md(F-01~F-05 不重复);实证核对 5 个 cap 行、real_db env-gate、real_api `expect` panic、源头规范。

## TG-01(P2 → 并入 DR-06)chain gate scheduler 变异命令复用 agent 仓 diff → 空转绿
root §4 L38/L45;另:各模块逐个合 main 后 chain gate 的 `git diff main...HEAD` 趋于空 diff,「无 missed」结论无法由该命令产出。Patch:scheduler 段独立生成 diff;chain gate 变异判据改述为「逐模块 PR 的 AG-012/AG-013 预检输出 + CI 运行链接汇总」。

## TG-02(P2 → DR-12)「允许先绿 → AG-006 由 in-diff 预检证明」对钉零改动代码的测试结构性不可行
- M2-T3.1~3.4 钉 facebook 既有 seen-cursor/空页行为——这些行不在 M2 diff 内,预检生成不出对应变异;任务预设「无对应变异 → 书面豁免」= 证明义务预设全豁免。M2-T2.7 论证(mutants 使 fetch_by_keyword 误走 outcome 分支)不成立——零改动函数不被变异。M6-T1.11/1.12 同病(eval_once 零改动)。
- Patch:逐条改 M1-T6 式手工金丝雀(写明变异对象):注释 seen-cursor break → T3.1 红;改 MAX_EMPTY_CURSOR_HOPS 判定 → T3.2/3.3 红;让 fetch_by_keyword 返回截断 Vec → T2.7 红;completed 分支改 Dispatch → M6-T1.11 红;移除 failed 重派 → T1.12 红。豁免仅当金丝雀也不可行;M6 db-glue 豁免须附实际 missed 清单,不得空白盖章。

## TG-03(P2 → DR-13)M2-T4 RED 原因错置:override 不在测试路径,且按依赖序 RED 不可取得
MockContentGateway 装配下真实 FacebookAdapter override 从不执行;RED 说明「override 未接通 → 失败」不成立;T4 在 T1+T2 后执行时测试 1/2/4 立即全绿。Patch(推荐 a):(a) 改名实义「strategy 解截断 + D3 映射的 facebook 形状实例」,RED 取证点 = pre-M2-T1(失败原因 = cap 截断 → COMPLETED 而非 NO_MORE),override 集成覆盖由 M2-T2 + T2.8 承担并在 §5 注明;(b) 增加真实 FacebookAdapter + mock-HTTP 进 orchestrator 的装配测试真正打穿 override。

## TG-04(P2 → 并入 DR-02)M1-T2 PT-2 断言过强且 harness 语义未定义 → 不可修复的红
反例:单页 20 唯一 id、fresh cursor、max=200 → Continue 后注入页耗尽,永无 Stop。Patch:写死 harness 规范(页耗尽 = 补 `(空, None)`),断言改「len+1 步内 Stop」(或 len+MAX_EMPTY_PAGES+1,取定写明)。计划期修正不触发 ASSERTION-CHANGE;拖到执行期就会触发。

## TG-05(P2 → DR-15)簿记外先绿测试:M2-T1.4 与 M4-T1.3
M2-T1 RED 证据明文「测试 4 绿」但未标允许先绿、未挂 AG-006、不在 §5 清单;M4-T1.3「RED 义务由 1/2 承担」是 AG 框架外第三类。两条变异证明其实可行(cap 行在 diff 内)。Patch:均改标「允许先绿+AG-006(AG-012 覆盖)」入清单;删「RED 义务由 1/2 承担」措辞。

## TG-06(P2 → DR-16)M4-T4 twitter live 断言与容忍条款自相矛盾
断言「id 集合不全同」vs「cursor 相同不视为失败」——重复 cursor 现实结局常是内容相同 → 断言必红,把改断言的决定推到 live 现场。Patch:计划内写死分支断言:cursor 相同 → 记录证据 + 仅断言非错误 PASS;不同 → 断言 id 集合不全同。

## TG-07(P3)anti-gaming §4 R-002 行 PT 覆盖虚标(D-01 后 facebook 不经 PaginationLoop)→ 加脚注 + test-suite §8 增 justified gap。
## TG-08(P3)非代码例外未回填:补 N-005(RT-1 文档)、N-006(required-check 手工配置,gh api 输出留存)、N-007(M6-T4 工作流,三段实证)。
## TG-09(P3)M2-T6 T-050 RED 大概率不可满足(现状已有翻页)→ 改探针化(允许先绿+性质说明),T-054 同步复核;M3-T3「同 M2-T4 模式」引用错 → 改「同 M5-T1/M2-T6 探针性质说明」。
## TG-10(P3)「env 未设自动 skip」不实句子复制进 M2-T7(F-01 patch 只修 M1-T7)→ 扩展至 M1-T7/M2-T7/M3-T4/M4-T5 统一脚注:RT-3′ 落地前无凭据环境以 `cargo test --lib` 替代并注明。
## TG-11(P3)D-11 主路径隐含「M1 已合 main」前提(否则 main worktree 编译失败非正确红);fallback 金丝雀无程序。Patch:基线 =「含 M1、不含 M2 的 commit」;写死双金丝雀(恢复 min(20) → 形状 1 红;override 短路 None → 形状 2 红)。

## 检查通过项(抽样)
RED 规格质量(逐任务断言文本与现状实证一致;两处编译错误作 RED 理由正当);mock+real 双 gate 齐备、预算封顶、fixture 回灌一致;M5-T1 探测豁免成立(保守判分支 B + 写回义务);M6 穷举豁免成立(12 条全角;33 断言零触碰靠并行纯函数);D-05/PT-5 豁免可复核;M6-T3 先绿理由成立(须附实际 missed 清单);M6-T4 反作弊条款封死三类伪造;RT-3 范围纪律合规;**无任何任务以改弱/删断言/skip 为达成手段**;「事故根因行不接受静默豁免」是正确收紧。

结论:反作弊骨架总体扎实,但 1 个空转 gate(TG-01)、一类结构性失效的 AG-006 路径(TG-02)、两处执行期必然取不到的 RED(TG-03/TG-09)、一处自相矛盾 live 断言(TG-06),建议全部并入 patch queue 后再放行执行。
