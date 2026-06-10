# Domain Review — First-Principles Reviewer(2026-06-10,只读子 agent 原文)

> 归一化映射:F1→DR-02(P1)、F2→DR-03(P1)、F3→DR-06 一部(P1)、F4→DR-14(P2)、F5→DR-11(P2)、F6/F7→P3 批。

已核对计划族全部必读文件,并对计划引用的源码锚点做了实地验证(strategies 五个 cap 行与缺省值、`SearchOptions.extra`、facebook.rs:522-582 循环体与 `MAX_EMPTY_CURSOR_HOPS` 语义、orchestrator.rs:39-61/344-423 `KeywordProcessOutcome.terminal_hint` 聚合、scheduler lib.rs:159-167 `last_task`/`mark_campaign_completed`、scheduler schema.rs:133-149(无 terminal_reason)、glance_mind_rust schema.rs:838(terminal_reason 列存在,M6 §2.2-d 前置核查可满足)、progress_tracker.rs redaction `take(500)`/`code`/`as_terminal_message`)。绝大多数事实锚点准确。

**F1 — P1:PT-2(T-031)属性按字面不可满足,会制造"永远红且禁止修断言"的死局**
- Evidence:m1 §3 M1-T2 PT-2:"任意有限序列必在『序列长度』步内产生 `Stop(_)`";生成器"页数 0..=20"。同任务反作弊声明:"收窄生成器 = 改弱断言,同等禁止"。
- Consequence:空序列(0 页)不可能产生任何 `Stop`;短序列(如 1 页、20 个唯一 id、fresh cursor、max=200)合法地以 `Continue` 结束,也无 `Stop`。该属性对正确实现也必然失败 → 实现子 agent 面临"改生产代码无解 / 改断言或生成器被 AG-002 禁止"的死局,唯一出路是执行期大面积 ASSERTION-CHANGE 流程,污染 RED→GREEN 证据链。
- Patch:在 M1-T2 写明驱动器约定并改写属性为可满足形式,例如:"驱动器逐页喂入,Stop 即停;断言 ① accept_page 调用次数 ≤ 序列长度;② 若产生 Stop,reason ∈ 四枚举;③ 对『以 next_cursor=None 结尾』或『cursor 在序列内重复』的子类(生成器显式标记),断言必产生 Stop"。预期 RED 信息相应更新。

**F2 — P1:循环活性缺口——"非空但零新进展页 + fresh cursor"未被任何终止条件覆盖;`MAX_EMPTY_PAGES` 的"空页"语义未定义**
- Evidence:① invariants-failures I-002 终止条件枚举 (a)~(d) 不含"重复内容页连发";② M1 §2 D2 `accept_page` 未定义"空"按 `item_ids.len()==0` 还是 `newly_accepted.is_empty()` 计;M1-T2 单测 5(duplicate ids)只断言计数,不断言 decision;③ 实地验证 facebook.rs:546-558:`empty_hops` 按 `page_count == 0`(原始条数)计,去重后零新增不计入;④ PT-2 生成器有限(≤20 页),无法暴露无限循环。
- Consequence:上游返回同一批内容但每次给新 cursor(分页器抖动/排序漂移是真实形态)时:页非空 → empty 计数不增;cursor 不重复 → 不触发 CursorLoop;accepted 卡在 < max_count → 适配器循环无界发请求。直接违反 bedrock B4(成本上界)与 R-009;M3/M4/M5 新循环原样继承。若 M1 按 newly_accepted 计空而 facebook 按原始条数,M2-T2.8 对齐断言在第五形状不成立——非挑战 D-01,而是 T2.8 形状表缺行。
- Patch:① D2 显式定义 `empty_streak` 按 `newly_accepted.is_empty()` 递增(页数上界 ≈ 4×max_count);② M1-T2 增单测"重复内容页连发 ×3(cursor 各异)→ Stop(EmptyPageLimit)",I-002 补 (e);③ M2-T2.8 形状表加入该形状,facebook 侧改 `page_count==0` 为"新增数==0"(受既有 mock 测试与 AG-012 保护)或入账豁免交 Concurrency Reviewer。

**F3 — P2:root chain gate 的 scheduler 变异命令复用 agent 仓 diff,实跑必然空转(伪绿门)**(并入 DR-06)
- Evidence:root §4 L38 agent 仓生成 `/tmp/pr.diff`,L45 `cd glance_mind_scheduler && … --in-diff /tmp/pr.diff`——diff 内路径在 scheduler 仓不存在 → 0 变异点而绿。
- Patch:scheduler 仓内重新生成 diff;验收加"variants found > 0 或 diff 为空的显式确认"入留存输出。

**F4 — P2:live 契约探针类"允许先绿"不满足 AG-006 字面要求,各模块局部逐个豁免,缺账本级例外类别**(DR-14)
- Patch:anti-gaming 新增一条(AG-008):live 探针/gated real 测试有效性判据 = 实跑输出留存 + 回灌 fixture + 判定可复算,不适用变异证明;五处局部豁免改引用该条。地板本体不动。

**F5 — P2:terminal_hint 聚合被误述为"last-wins",实为"last-Some-wins";多 keyword 混合 shortfall 优先级未定义**(DR-11)
- Evidence:orchestrator.rs:384-385 `if outcome.terminal_hint.is_some() { … }`——None 不覆盖 Some。kw1=PartialFailure、kw2=Exhausted → 终值 NO_MORE,M6 落 SEARCH_EXHAUSTED 不告警——部分失败被枯竭标签掩盖,与 B2 冲突;无测试钉住。
- Patch:更正行文;M1-T4 加聚合优先级(PartialFailure > Exhausted,B2 推导)+ 混合形状测试,或 test-suite §8 增 justified gap(附单 keyword 占比证据)交读签。

**F6 — P3:T-010 页形状账本(20+20+10)与 M2(20+20+20)不一致**;Patch:统一为 20+20+10 或 §7 注明。

**F7 — P3:M2-T6 live RED 取证与 P-001"≤3 次"预算未对账**(RED+GREEN 两轮最多 6 次);Patch:写明"≤3 次/每轮",或 live 仅取 GREEN 冒烟、RED 由 mock 层承担。

其余检查项均通过:目标蒸馏、子问题按力切分、假设处置(交接零 ASSUMED;A005 drop 有书面推导)、重构最小性(三个常规方案均以 bedrock 理由拒绝)、重 gate 适用性、D-01~D-12 引用一致、R-012 闭环、M6 schema 列前置核查已实证可满足(glance_mind_rust schema.rs:838)。
