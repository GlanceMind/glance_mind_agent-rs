# Module Plan: M6 `scheduler/once-guard`

> 计划族:`docs/plans/jplan/2026-06-10-pagination-max-scan/`(Step 04 起草,2026-06-10)
> **仓库:worker 仓子目录 `/Users/jacksoom/programer/aihub/glance_mind_worker/glance_mind_scheduler`(`glance_mind_worker` 单一 git 仓;`glance_mind_scheduler/` 无独立 .git,Step 06 实证;独立 PR/提交流程;CI 工作流在 worker 仓根 `.github/workflows/`)**。依赖:M1(语义依赖:消费 agent 写入的 terminal_reason 值,C-004;代码无依赖;部署无硬顺序——读方容忍 NULL/未知值)。被依赖:root chain gate(T-040「无 WARN」半边)。
> 账本输入:requirements(R-010、R-011 scheduler 侧、R-013;R-008 重派语义保护)、invariants-failures(I-007;F-008)、test-suite(T-020~T-022)、anti-gaming(AG-001~AG-007 全局;**AG-013**;§3 末属性测试豁免)、production-dependencies(P-007)、framework(FR-002 CI 缺口、FR-004 测试模式)、cross-service-contracts(C-003 含 §2 读方枚举与子串约束、C-004 scheduler 读方、§3 契约不变)、non-code(N-002 scheduler 侧、N-003 降级)。
> **已裁决输入(`04-adjudications.md`,不再开放)**:D-03(agent 不停 campaign,区分由本模块闭环)、D-06(终名 `SEARCH_EXHAUSTED`)、D-07(P-007 = 最小 gated 测试;N-003 降级可选抽查)、D-08(WARN+指标不补派)、D-09(补 mutation CI)。

## 0. 模块目标(一句话)

关闭事故的 scheduler 半边:ONCE campaign 完结时读取 task 的 `terminal_reason`(C-004 新增读方,容忍 NULL/未知值),**枯竭欠扫 → `completed_reason='SEARCH_EXHAUSTED'`(不告警);非枯竭欠扫 → WARN 日志 + 运行结果指标(不补派、零状态副作用);扫满 → 现状 `ONCE_EXECUTED` 不变**——使 campaign 269 类「欠扫被静默标完成」从此可观测、可区分;并为本仓补上 mutation CI 门禁(D-09)。**写方零行为改动:`dispatch_task` 与预算路径不动(R-012/契约不变 §3)。**

## 1. 全模块强制约束(每个任务自动继承)

1. **AG-001 先红后绿**:每个新测试先以「正确的原因」失败;RED/GREEN 输出留存为任务完成证据。
2. **AG-002 断言不可变**:实现子 agent 不得修改断言来过测试;失败只能改生产代码或停下上报;例外须带 `ASSERTION-CHANGE-JUSTIFIED: <原因>` 重走 RED→GREEN。**既有 33 条 test_schedule_evaluator 断言一条不许动**(本计划设计已保证无需动,见 §2.2-a)。
3. **AG-003 禁绕过/伪造**:不得 `#[ignore]`、吞 `Result`、expected 改 actual、生产代码特判测试输入。
4. **AG-004 职责分离**:测试载荷与实现载荷分属不同子 agent 上下文。
5. **AG-005 过度 mock 禁令**:eval_once 决策与 completion 判定为被测对象本身,不得 mock;输入用内存构造(FR-004 既有 `make_campaign`/`make_task` 模式)。
6. **AG-006 先绿类测试**:标注「允许先绿」者须经变异/金丝雀证明有效。
7. **AG-007 如实报告**:本仓测试为纯确定性(无 live API),不适用上游漂移豁免;gated DB 测试红须先核对 `DATABASE_URL`。
8. **AG-013 变异门槛(本仓唯一变异强制,CI 落地前)**:M6 全部 diff 在仓内 `cargo mutants --in-diff` 下无 missed,或逐个书面豁免(预案见 §6.2);D-09 的 CI 工作流落地后成为永久强制。
9. **契约不变(cross-service-contracts §3)**:`dispatch_task` 写方零行为改动(lib.rs:313-383 仅允许加注释);不改任何跨服务消息/表形状;terminal_reason 读取**必须容忍 NULL/未知值**(滚动部署,§3.3);`SEARCH_EXHAUSTED` 含 `EXHAUST` 子串(C-003,测试钉死)。
10. **R-008/A007 重派语义保护**:eval_once 对 `failed` 的一次重派分支(schedule_evaluator.rs:120-127)不得改动;campaign 121 回归注释(L128-131,pending 不当 completed)不得削弱。
11. **属性测试豁免(anti-gaming §3 末)**:eval_once/completion 输入域有限(status × process_count↔max_count × terminal_reason 形状),T-020~T-022 表驱动穷举即可,**不引入 proptest**。

## 2. 设计(本计划定形;消费契约不重设计)

### 2.1 消费的跨服务契约(只读引用)

- **C-004 读方**:`gm_crawler_tasks.terminal_reason`,写方 = agent(值集 6 个 code 已由 M1-T6 契约 pin),持久化形状 `"CODE: message"`(`as_terminal_message`);scheduler 以**取冒号前 token(trim 后)精确比较 `== "NO_MORE_POSSIBLE_DATA"`** 判定枯竭(防未来 code 前缀碰撞,Step 06 protocol#1;T1.5 裸 code 无冒号形状容忍不变——取冒号前 token 在无冒号时即取整串;T1.6 非前缀防误判不变,且精确比较天然满足 T1.6);NULL/未知值 → 按「非枯竭」处理,不 panic、不跳过完结。
- **M1 D3 前提(D-03 已裁决)**:agent 在有进展+枯竭时只写 terminal_reason、不停 campaign——ONCE 完结理由区分**由本模块唯一实现**。
- **C-003**:`completed_reason` 列为自由文本,唯一取值敏感读方 = gm-e2e 子串白名单;`SEARCH_EXHAUSTED` 含 `EXHAUST` 直接通过。

#### 已知边界(登记,不在 M6 范围)

**① 降级环境 fallback 写序竞态(SM#F6)**:agent legacy 路径(`postgres.rs:2169-2186`)先置 `completed` 再补写 `terminal_reason`——M6 在窗口内读 NULL → 误写 `ONCE_EXECUTED` 且不可纠正(completed+NULL 落入 `OnceExecuted+WARN` 格,结构化 WARN 含 terminal_reason 可分诊)。主路径(`postgres.rs:2044-2069`)无此窗口。修法(fallback 写序调换:terminal_reason 先写)属 agent 仓改动,登记 root backlog,不在 M6 范围。

**② completed+NULL 在新 agent 主路径下亦可达(protocol#4)**:`postgres.rs:2044-2069` Pending/Running 转移主动把 terminal_reason 置 NULL;`Completed|Failed` else 分支设状态但不写 terminal_reason——M6 读 NULL → `OnceExecuted+WARN` 含良性噪声;结构化 WARN 字段(terminal_reason 原值为 NULL)可分诊根因。M6-T2 WARN 语义注已引用此边界。

### 2.2 M6 局部设计裁决

**a) 决策枚举不加载荷;新增并行纯函数 `eval_once_completion`。**
既有 33 条测试断言 `DispatchDecision::MarkCompleted`(单元变体)。给变体加字段将迫使全部既有断言改写(AG-002 高危面)。故:`evaluate`/`eval_once` 返回形状**完全不变**;新增
```rust
pub enum OnceCompletionReason { OnceExecuted, SearchExhausted }
impl OnceCompletionReason { pub fn as_str(&self) -> &'static str /* "ONCE_EXECUTED" | "SEARCH_EXHAUSTED" */ }
pub struct OnceCompletion { pub reason: OnceCompletionReason, pub underscan_warning: bool }
/// completed 终态 task 的完结判定(纯函数;调用方 = lib.rs MarkCompleted 分支)
pub fn eval_once_completion(task: &CrawlerTaskEntity) -> OnceCompletion
```
真值表(穷举,T-020/T-021/T-022 载体;`exhausted` := `terminal_reason` 取冒号前 token(trim 后)精确比较 `== "NO_MORE_POSSIBLE_DATA"`,NULL→false;DR-21;`underscan` := `process_count < max_count`):

| underscan | exhausted | reason | underscan_warning | 关联 |
|---|---|---|---|---|
| false | 任意 | OnceExecuted | false | T-022(扫满即正常,即使带 NO_MORE) |
| true | true | SearchExhausted | false | T-021 / F-008(枯竭不告警) |
| true | false(含 NULL/未知值) | OnceExecuted | **true** | T-020(campaign 269 形状落此格) |

**b) 「指标」形态(R-010,本仓无 metrics 基建)**:`SchedulerRunResult` 新增 `pub underscan_warning_campaign_ids: Vec<i32>`(附加字段,既有消费方 main.rs 不破坏)+ 结构化 `warn!`(campaign_id、task_id、process_count、max_count、terminal_reason)。可观测、可测试、零新依赖;升级 prometheus 留作扩展点(R-010 原文保留)。

**c) lib.rs 接线提取可测缝**:`fn apply_once_completion(completion: &OnceCompletion, campaign_id: i32, task: &CrawlerTaskEntity, result: &mut SchedulerRunResult) -> &'static str`——发 WARN、记指标、返回 reason 字符串;MarkCompleted 分支变为 `let reason = apply_once_completion(...); db::mark_campaign_completed(pool, id, reason)`。db 调用 glue 本身(≤3 行)若产生不可测变异,走书面豁免(§6.2),端到端由 P-007 gated 测试兜底。

**d) C-004 读取接线 = schema 列声明 + 实体字段(非 migration)**:scheduler `src/schema.rs` `gm_crawler_tasks` 表宏新增 `terminal_reason -> Nullable<Text>`;`entity.rs::CrawlerTaskEntity` 新增 `pub terminal_reason: Option<String>`。**前置核查(实现者义务)**:在 `glance_mind_rust/crates/db/src/schema.rs` 确认该列已存在于 DB schema;若不存在 → BLOCKED 上报(**严禁**在任何仓创建 migration——R-012)。`NewCrawlerTask` 不加此字段(scheduler 永不写它,单写方 = agent)。

### 2.3 状态面(step-04 action 8)

| 状态键 | 唯一写方 | 声明读方 | M6 变化 |
|---|---|---|---|
| `gm_crawler_tasks.terminal_reason` | agent postgres adapter(既有) | **scheduler eval_once_completion(M6 新增读方)** | 新增读;容忍 NULL/未知 |
| `gm_campaigns.completed_reason` | **worker 仓应用层唯一写方**(scheduler `mark_campaign_completed`);系统层面 DB 过程/主后端另有写方(C-003 白名单 BUDGET/FINALIZED 取值为证;SM#F7) | 透传/展示读方(C-003 §2 枚举);gm-e2e 子串白名单 | 新增取值 `SEARCH_EXHAUSTED`(列自由文本,无形状变化) |
| `SchedulerRunResult.underscan_warning_campaign_ids` | `apply_once_completion`(M6) | main.rs 日志循环 / 未来 metrics 导出 | 新增附加字段 |
| `gm_crawler_tasks.{search_limit,search_offset}` | scheduler `dispatch_task`(既有,**零行为改动**) | agent(limit 作页提示,M1);运维(offset 观测) | 仅 N-002 注释 |

## 3. 任务清单

> 执行序 = 编号序(T1 核心判定 → T2 接线 → T3 gated 证据 → T4 CI → T5 收尾;T3/T4 可并行)。
> 每任务 1-3 文件(类型骨架行不计)、单个全新实现者上下文可完成;测试/实现分上下文(AG-004)。
> T1/T2 为确定性测试(内存构造,FR-004 模式),不需凭据;T3 为 `DATABASE_URL` gate;T4 为 CI 工作流(非测试交付物,验收 = CI 实跑)。
> **仓布局说明(DR-06 实证)**:`glance_mind_scheduler/` 为 `glance_mind_worker` 单一 git 仓的子目录(无独立 .git);所有提交/PR 均走 worker 仓流程;cargo-mutants `--in-diff` 须在子目录内搭配 `--relative` 剥前缀,否则包根路径不匹配导致 0 变异体恒绿(空转)。

---

### M6-T1 eval_once_completion 真值表 + terminal_reason 读取骨架(T-020/T-021/T-022)

**覆盖 ID**:R-010(判定逻辑)、R-013(区分值判定)、I-007(决策不变断言)、F-008(双向)、T-020、T-021、T-022、C-003(子串钉子)、C-004(scheduler 读取 + NULL 容忍)、FR-004。依赖:无(可立即起做)。
**文件**:`src/schedule_evaluator.rs`(新类型 + 函数 + 既有 eval_once **零改动**)、`src/test_schedule_evaluator.rs`(新测试;`make_task` 补 `terminal_reason: None` 字段初始化——结构体字面量补全,**非断言改动**)。类型骨架(测试载荷一并落):`src/schema.rs` `gm_crawler_tasks` 宏加 `terminal_reason -> Nullable<Text>`、`src/entity.rs` 加 `pub terminal_reason: Option<String>`(§2.2-d;前置核查列已存在,否则 BLOCKED)。

**测试载荷(测试子 agent 先写;表驱动,沿 make_task 模式扩一个 `make_completed_task(process, max, terminal: Option<&str>)` helper)**:

1. `underscan_completed_normally_warns`(T-020 主形状 = campaign 269):`(20, 50, Some("COMPLETED: Task completed successfully"))` → `reason == OnceExecuted`、`underscan_warning == true`。
2. `underscan_null_terminal_warns`(NULL 容忍,滚动部署/老 task):`(20, 50, None)` → `OnceExecuted`、`warning == true`(不 panic)。
3. `underscan_unknown_terminal_value_warns`(未知值容忍):`(20, 50, Some("FUTURE_CODE_X: whatever"))` → `OnceExecuted`、`warning == true`。
4. `exhausted_underscan_no_warn_search_exhausted`(T-021 / F-008 正向):`(20, 50, Some("NO_MORE_POSSIBLE_DATA: upstream exhausted"))` → `reason == SearchExhausted`、`warning == false`。
5. `exhausted_bare_code_no_colon_still_detected`(形状容忍):`(20, 50, Some("NO_MORE_POSSIBLE_DATA"))` → `SearchExhausted`。
6. `exhausted_mention_not_prefix_is_not_exhausted`(防误判,前缀非子串):`(20, 50, Some("COMPLETED: saw NO_MORE_POSSIBLE_DATA upstream"))` → `OnceExecuted`、`warning == true`。
7. `full_scan_no_warn_once_executed`(T-022):`(50, 50, Some("COMPLETED: ..."))` → `OnceExecuted`、`warning == false`(**允许先绿**仅当实现先行——本计划测试先写、函数骨架 `todo!()`,故实为 RED;若执行序导致先绿,按 AG-006 经 AG-013 变异证明)。
8. `full_scan_with_no_more_still_once_executed`(真值表边角):`(50, 50, Some("NO_MORE_POSSIBLE_DATA: x"))` → `OnceExecuted`、`warning == false`(扫满即正常)。
9. `over_delivery_no_warn`(边界 `>=`):`(51, 50, None)` → `OnceExecuted`、`warning == false`。
10. `completion_reason_strings_are_contract`(C-003 钉子):`OnceExecuted.as_str() == "ONCE_EXECUTED"`、`SearchExhausted.as_str() == "SEARCH_EXHAUSTED"`、且 `assert!("SEARCH_EXHAUSTED".contains("EXHAUST"))`(gm-e2e 子串白名单约束显式入册)。
11. `decision_unchanged_for_completed_underscan`(I-007 / R-010「不补派」):`evaluate(once_campaign, Some(&underscan_completed_task), now) == DispatchDecision::MarkCompleted`(**不是** `Dispatch`——防御分支零派发;**手工金丝雀验证(DR-12)**:eval_once 零改动,预检不会生成对应变异(Step 06 TG-02);须临时把 `eval_once` `completed` 分支改为返回 `Dispatch` → 本测试须红;还原后复绿;只动生产代码,输出留存。书面豁免仅当金丝雀不可行时方可作为替代)。注:临时改动期间既有 33 条断言中关联用例亦会同步变红——属预期伴随(eval_once 行为改变覆盖全部用例);观察目标 = 本测试变红即确认有效;还原后全部 33+新增测试须复绿,输出留存。
12. `failed_retry_semantics_untouched`(R-008/A007 回归钉):`failed` task → `Dispatch`(既有一次重派语义;**手工金丝雀验证(DR-12)**:临时把 `eval_once` `failed` 分支改为返回 `Skip` → 本测试须红;还原后复绿;只动生产代码,输出留存。书面豁免仅当金丝雀不可行时方可作为替代)。注:临时改动期间既有 33 条断言中关联用例亦会同步变红——属预期伴随(eval_once 行为改变覆盖全部用例);观察目标 = 本测试变红即确认有效;还原后全部 33+新增测试须复绿,输出留存。

**预期 RED 失败信息**(骨架 `todo!()` 下):测试 1~10 `panicked at src/schedule_evaluator.rs:<line>: not yet implemented`;实现中途断言型失败示例:测试 4 `assertion 'left == right' failed: left: OnceExecuted, right: SearchExhausted`、测试 1 `assertion 'left == right' failed: left: false, right: true`(warning 位)。RED 证据 = `cargo test`(仓内)本组全红输出。

**GREEN(实现子 agent)**:按 §2.2-a 真值表实现;**精确比较用取冒号前 token(DR-21)**:`terminal_reason.as_deref().map(|r| r.splitn(2, ':').next().map(|t| t.trim()) == Some("NO_MORE_POSSIBLE_DATA")).unwrap_or(false)`(无冒号时 splitn 取整串,T1.5 裸 code 容忍成立;精确比较天然满足 T1.6 防误判;禁止改为 `contains` 或 `starts_with`)。命令(scheduler 仓内):`cargo test schedule_evaluator`。
**最终验收命令**:`cargo test`(全 33+12 绿;既有 33 条零断言改动,`git diff -- src/test_schedule_evaluator.rs` 仅含新增测试与 make_task 字段补全)。

**反作弊声明**:实现者不得修改断言来过测试;尤其测试 6 的「前缀非子串」语义不得改为 `contains`(那会把任何提及 NO_MORE 的消息误判为枯竭——F-008 误报面)。

---

### M6-T2 lib.rs 接线:MarkCompleted 消费 completion + WARN/指标 + N-002 注释

**覆盖 ID**:R-010(WARN+指标落地)、R-013(区分值写入路径)、R-011(scheduler 侧)、I-007(零副作用)、N-002(scheduler 侧注释)、C-002(注释措辞)。依赖:M6-T1。
**文件**:`src/lib.rs`(MarkCompleted 分支 + `apply_once_completion` + `SchedulerRunResult` 字段 + N-002 注释)、`src/db.rs`(仅注释:mark_campaign_completed 处 C-003 取值契约注;**函数体零改动**)。

**测试载荷(测试子 agent 先写;lib.rs `#[cfg(test)]` 或 test 模块,纯内存——`apply_once_completion` 不触 DB)**:

1. `apply_warns_and_returns_once_executed`:`OnceCompletion{OnceExecuted, warning:true}` + 空 `SchedulerRunResult` → 返回 `"ONCE_EXECUTED"`、`result.underscan_warning_campaign_ids == vec![campaign_id]`。
2. `apply_exhausted_no_warning_returns_search_exhausted`:`{SearchExhausted, warning:false}` → 返回 `"SEARCH_EXHAUSTED"`、`underscan_warning_campaign_ids` 为空。
3. `apply_full_scan_no_warning`:`{OnceExecuted, warning:false}` → 返回 `"ONCE_EXECUTED"`、ids 为空。
4. `run_result_default_has_no_warnings`:`SchedulerRunResult::default()`(或既有构造)→ 新字段为空 vec(附加字段向后兼容)。

**预期 RED 失败信息**:`apply_once_completion` 未实现 → `error[E0425]: cannot find function`(编译失败即正确 RED:被引入物 = 接线函数本身);骨架后:测试 1 `assertion 'left == right' failed: left: [], right: [1]`。

**GREEN(实现子 agent)**:实现 §2.2-c;MarkCompleted 分支把硬编码 `"ONCE_EXECUTED"` 替换为 `apply_once_completion(&eval_once_completion(task), ...)` 的返回值(`last_task` 在作用域内,completed 分支必有 `Some(task)`);WARN 含 campaign_id/task_id/process_count/max_count/terminal_reason。**WARN 语义注(§2.1 protocol#4/SM#F6)**:部分 WARN 根因是「terminal_reason 为 NULL 的良性噪声」(fallback 写序竞态或主路径 Completed else 分支不写 reason),而非真欠扫;结构化字段 terminal_reason 原值已含在 WARN 中,运维可按此分诊。并落注释:
- lib.rs dispatch_task `search_limit`/`search_offset` 写入处(N-002/C-002 措辞):`// 跨服务契约(C-002):search_limit = 单页大小提示(agent clamp 到平台页上限);search_offset = 仅观测字段,agent 不读。两字段形状不变、不删(读方:agent-rs redis.rs to_domain_task_config)。`
- db.rs mark_campaign_completed 处:`// C-003:completed_reason 为自由文本契约;新值须含 BUDGET/EXHAUST/FINALIZED/ONCE_EXECUTED 之一为子串(gm-e2e 白名单)。当前取值:ONCE_EXECUTED | SEARCH_EXHAUSTED(D-06)。`
命令:`cargo test`(仓内)。
**幂等性注(SM#F7)**:`mark_campaign_completed` 为按 id 的幂等 `UPDATE`(set status/completed_reason/updated_at),对已完结 campaign 重复调用无害——实现期确认无需 status guard 即维持现状,不引入额外条件分支。
**最终验收命令**:`cargo test` + `git diff -- src/db.rs`(仅注释行)+ `git diff -- src/lib.rs` 中 dispatch_task 区段(L313-383)仅注释。

**反作弊声明**:实现者不得修改断言来过测试;不得为简化接线而改动 `dispatch_task` 行为或 `db.rs` 函数体(契约不变 §3 红线)。

---

### M6-T3 P-007 最小 real-DB gate:SEARCH_EXHAUSTED 落库可查(D-07)

**覆盖 ID**:P-007、R-013(持久化证据)、C-003(real 侧)。依赖:M6-T2(取值已接线;本测试直接驱动 db 层,逻辑上仅依赖 db.rs 既有函数)。
**文件**:`tests/real_db_completed_reason_test.rs`(scheduler 仓新建;本仓首个 real-DB gated 测试,gate 模式镜像 agent 仓 `facebook_real_db_test.rs:271-289`)。

**Gate 控制**:`DATABASE_URL` 未设 / `GITHUB_ACTIONS` 下未显式 `RUN_REAL_DB_TESTS` → 显式 skip + `eprintln!`(不 panic、不 `#[ignore]`);本地/CI DB,无外部成本;幂等:测试自建专用 campaign 行(name 含唯一标记如 `__m6_p007__<uuid>`),断言后清理删除(`DELETE WHERE id`),失败路径亦清理(guard/finally 模式)。

**测试载荷(测试子 agent 先写)** — `mark_campaign_completed_persists_search_exhausted`:
1. 直插最小 campaign 行(diesel,字段沿 `make_campaign` 默认值集;若 NOT NULL 字段集致直插过重,允许改为「单连接事务内直插」并在完成报告说明,但断言不变)。
2. 调 `db::mark_campaign_completed(&pool, id, "SEARCH_EXHAUSTED")`。
3. 查回该行断言:`status == "COMPLETED"`、`completed_reason == Some("SEARCH_EXHAUSTED")`、`updated_at` 已更新(非 NULL)。
4. 清理删除该行。

**RED/GREEN 说明**:`mark_campaign_completed` 为既有参数化函数,本测试**允许先绿**(live gate;AG-006 论证:gated 测试不入变异运行集,其价值 = prod-shape DB 的 D-07 自动化证据,取代 N-003 人工 SQL;确定性对应物 = T1.10 字符串契约 + T2 接线测试)。书面理由随完成报告交 Test-Gate Reviewer。
**验收命令**:`DATABASE_URL=… cargo test --test real_db_completed_reason_test -- --nocapture`(实跑输出留存)。N-003 降级登记:部署后人工 SQL 抽查改为**可选**运维步骤(写入 worker 仓 playbook 由运维自决,不入本计划验收)。

**反作弊声明**:实现者不得修改断言;DB 不可用时如实 skip 上报,不得伪造实跑输出。

---

### M6-T4 mutation CI 工作流(D-09;worker 仓根)

**覆盖 ID**:FR-002(缺口关闭)、AG-013(永久化)、AG-010(镜像)。依赖:无(可与 T3 并行)。
**文件**:`/Users/jacksoom/programer/aihub/glance_mind_worker/.github/workflows/mutation-scheduler.yml`(新建;镜像 agent-rs `.github/workflows/mutation-rust.yml` 形状)。

**交付物规格(非测试交付物;step-04 非代码例外处理,验收=实证)**:
- 触发:`pull_request` 且 `paths: ['glance_mind_scheduler/**']`。
- **`runs-on: [self-hosted, front]`(镜像 agent 仓 mutation workflow,Step 06 实证)**。
- 步骤:checkout(fetch-depth: 0)→ Rust toolchain → `cargo install cargo-mutants --locked --version 24.11.0`(与 agent 仓 pinned 同版)→ **`git diff --relative=glance_mind_scheduler origin/${{ github.base_ref }}...HEAD -- glance_mind_scheduler > /tmp/scheduler-pr.diff`(--relative 剥去 `glance_mind_scheduler/` 前缀,使 cargo-mutants 包根路径匹配成立;缺少 --relative 则 0 变异体恒绿)** → `cd glance_mind_scheduler && cargo mutants --in-diff /tmp/scheduler-pr.diff --timeout 300 --annotations=github -- --test-threads=1`。
- 失败语义:missed mutants → 非零退出 → 红(与 agent 仓一致);是否设为受保护分支 required check = 仓库设置,登记给 root 验收清单(GitHub 手工配置一次)。

**验收(替代 RED→GREEN,非代码交付物)**:
1. YAML 静态校验(`actionlint` 若可用,否则 `python -c "import yaml,sys; yaml.safe_load(open(...))"`)。
2. **本地等价命令实跑**(= AG-013 同款):在 `glance_mind_scheduler/` 目录内对 M6 diff 跑 `cargo mutants --in-diff`(须用 `--relative`)取得与 CI 将执行的一致结果,输出留存。
3. **最终验收 = M6 PR 上该工作流实跑可见且结果与本地预检一致**(绿,或 missed 与豁免清单一致)。
4. **哨兵判据**:对含 `src/` 改动的 diff,mutants 输出须含 `Found N mutants` 且 **N ≥ 1;N==0 即门禁配置失败**(空转检测哨兵),输出留存。N==0 时须先修正 `--relative` 配置再重跑,不得以「无变异体」为由视为通过。
- 账本引用:N-007(三段实证已入 non-code-exceptions.md)。

**反作弊声明**:不得通过 paths 过滤排除被改动的源码目录、调低 timeout 至变异无法运行、或 `continue-on-error: true` 使门禁形同虚设(任何此类配置 = 伪造通过,AG-003)。

---

### M6-T5 模块收尾 gate:AG-013 变异预检 + 契约面自查 + 全量回归

**覆盖 ID**:AG-013、AG-010~AG-012(本仓适配)、契约不变 §3(自查)、I-007(diff 终查)。依赖:M6-T1~T4。
**文件**:无新生产代码。

**命令序列(scheduler 仓内;全部输出留存)**:

```bash
# 1. 全量确定性回归(本仓测试均不需凭据;T3 gated 测试 env 未设自动 skip)
cargo test

# 2. AG-013 本地变异预检(CI 落地前的强制证据;门槛:无 missed 或书面豁免)
# 须在 glance_mind_scheduler/ 子目录内执行;--relative 剥去子目录前缀使包根路径匹配(缺少则 0 变异体空转)
git diff --relative main...HEAD > /tmp/scheduler-pr.diff
cargo mutants --in-diff /tmp/scheduler-pr.diff -- --test-threads=1

# 3. 契约面 diff 自查(期望:仅注释/声明级改动)
git diff main...HEAD -- src/db.rs            # 期望:仅注释(mark_campaign_completed 函数体零改动)
git diff main...HEAD -- src/schema.rs        # 期望:仅 gm_crawler_tasks 新增 terminal_reason 列声明(无 migration)
git diff main...HEAD --stat -- migrations/   # 期望:零命中(本仓亦不得有;基线状态 = 该目录不存在(空虚真),证据包注明——protocol#3/Observation 3)
# dispatch_task 区段(写方零行为改动):
git diff main...HEAD -- src/lib.rs | grep -A2 -B2 "^[+-]" | grep -i "dispatch_task" || true  # 人工核对仅注释

# 4. 部署前烟测 SQL(F-04:列存在性验证,须在部署目标 DB 执行)
# 执行方式:`psql $DATABASE_URL -c "SELECT terminal_reason FROM gm_crawler_tasks LIMIT 1;"`(DATABASE_URL 与 M6-T3 gate 同一 DB = 部署目标环境;由实现者或运维在部署目标主机执行)
# SELECT terminal_reason FROM gm_crawler_tasks LIMIT 1;
# 期望:查询成功返回(列存在);若报「column does not exist」→ BLOCKED 上报,禁止部署
```

**验收判据**:
1. `cargo test` 全绿;既有 33 条断言零改动(`git diff -- src/test_schedule_evaluator.rs` 复核)。
2. mutants 无 missed;预案豁免(§6.2 db-glue)若触发,豁免文本入 PR 描述交 Test-Gate Reviewer。**哨兵判据**:对含 `src/` 改动的 diff,mutants 输出须含 `Found N mutants` 且 **N ≥ 1;N==0 即门禁配置失败**(空转检测哨兵),输出留存。
3. 自查三条符合期望;`SEARCH_EXHAUSTED` 子串断言(T1.10)在册。
4. 每任务 RED→GREEN 证据齐备;T1.11/T1.12 金丝雀证明输出留存(DR-12);T3 书面理由交 Test-Gate Reviewer;T4 的 CI 实跑证据(或 PR 待开时的本地等价输出 + 跟进项)。
5. 提交/PR 走 worker 仓流程(DR-06 实证:`glance_mind_scheduler/` 为 `glance_mind_worker` 单一 git 仓子目录,独立 PR/提交流程);agent 仓的 `rust-verify-change` 不适用于此仓,以本节命令序列为等价守卫。
6. **部署前烟测**:`SELECT terminal_reason FROM gm_crawler_tasks LIMIT 1` 在部署目标 DB 执行成功(列存在,F-04);若列不存在 → BLOCKED 上报,禁止部署。

**反作弊声明**:本任务不得以任何形式使 gate 变绿;gate 红 = 回到对应任务修生产代码。

---

## 4. 模块完成判据与独立验证(03-split §5 M6 行)

- T-020~T-022 green(T-020/T-021 带 RED 证据;T-022 按 T1.7 实际 RED 或 AG-006 变异证明);本地 mutants 无 missed 输出留存(AG-013);P-007 gated 测试实跑输出留存(D-07);N-002 scheduler 侧注释落地;mutation CI 工作流落地(D-09)。
- T1.11/T1.12 金丝雀证明:各自手工改动生产代码使测试先红、还原后复绿,输出留存(DR-12;eval_once 零改动场景变异预检不覆盖,金丝雀为唯一有效证明手段)。
- 独立验证命令(在 `glance_mind_scheduler/` 子目录内执行):`cargo test`;`git diff --relative main...HEAD > /tmp/scheduler-pr.diff && cargo mutants --in-diff /tmp/scheduler-pr.diff -- --test-threads=1`(**--relative 必须**,否则 0 变异体空转);gated:`DATABASE_URL=… cargo test --test real_db_completed_reason_test -- --nocapture`。
- 事故闭环判据(与 M2 合并):M2 T-040 两形状 + 本模块 T-021/T-022 = 「欠扫可区分(SEARCH_EXHAUSTED)/ 异常欠扫可观测(WARN)/ 扫满不误报」三向钉死;T-040「无 WARN」半边由 T-022 + T1.11 承担。

## 5. 账本覆盖映射(traceability;M6 全部认领 ID → 任务)

| 账本 ID | 认领任务 | 备注 |
|---|---|---|
| R-010 | M6-T1(判定)+ M6-T2(WARN+指标落地) | 强度 = D-08 已裁决(不补派,保留升级点) |
| R-011(scheduler 侧) | M6-T2(N-002 注释) | agent 侧归 M1-T5 |
| R-013 | M6-T1(判定)+ M6-T2(写入)+ M6-T3(落库证据) | 终名 = D-06 `SEARCH_EXHAUSTED` |
| I-007 | M6-T1.11(决策不变)+ M6-T2(apply 零状态副作用,测试 1~3 仅观测字段) | — |
| F-008 | M6-T1.4(枯竭不告警)+ M6-T1.1(欠扫告警)双向 | — |
| T-020 | M6-T1.1/1.2/1.3(+ T2.1 接线侧) | — |
| T-021 | M6-T1.4/1.5/1.6(+ T2.2) | — |
| T-022 | M6-T1.7/1.8(+ T1.11) | 先绿时 AG-006 |
| C-003 | M6-T1.10(子串钉子)+ M6-T2(db.rs 注释)+ M6-T3(real 证据)+ M6-T5(自查) | — |
| C-004(scheduler 侧) | M6-T1(读取骨架 + NULL/未知容忍测试 1.2/1.3) | agent 侧 pin 归 M1-T6 |
| P-007 | M6-T3 | D-07:gated 测试;N-003 降级可选 |
| FR-002 | M6-T4 | CI 缺口关闭(D-09) |
| FR-004 | M6-T1(make_task/表驱动模式沿用) | — |
| N-002(scheduler 侧) | M6-T2 | — |
| N-003 | 降级登记(M6-T3 验收注) | D-07 后非验收必需 |
| AG-013 | M6-T5(本地预检)+ M6-T4(永久化) | — |
| AG-001~AG-007 | §1 全任务继承 | — |
| R-008/A007(保护) | §1.10 + M6-T1.12 回归钉 | 非 owner,保护性引用 |
| 契约不变 §3 | §1.9 + M6-T5 自查 | — |

> 自查:03-split §3 点名的 M6 全部 ID —— R-010/R-011(scheduler)/R-013、I-007、F-008、T-020~T-022、C-003/C-004(scheduler)、P-007、FR-002/FR-004、N-002/N-003 —— 均已认领。✅

**Step 07 patch 备注(DR-06/DR-12/DR-21 等)**:
- DR-06:头部仓性质改为「worker 仓子目录」;§3 引言补仓布局说明;M6-T4 diff 步骤加 `--relative=glance_mind_scheduler` 并补 `runs-on: [self-hosted, front]`;M6-T5 预检命令加 `--relative`;§4 独立验证命令同步。
- DR-12:T1.11/T1.12「允许先绿+AG-013 预检证明」改为手工金丝雀(临时改生产代码→红→还原→绿,输出留存);§4 完成判据同步。
- DR-21:§2.1 C-004、§2.2-a 真值表 exhausted 定义、M6-T1 GREEN 指引均改为「取冒号前 token 精确比较 == NO_MORE_POSSIBLE_DATA」。
- SM#F6+protocol#4:§2.1 末新增「已知边界(登记)」两条竞态/噪声说明;M6-T2 WARN 语义处补引用注。
- SM#F7:§2.3 `completed_reason` 写方改为「worker 仓应用层唯一写方;系统层面另有写方」;M6-T2 补幂等性注。
- F-04+protocol#3+N-007:M6-T5 命令块补部署前烟测 SQL;migrations 零命中注补「空虚真」说明;M6-T4 验收引用 N-007;§5 补本条备注。
- 哨兵判据:M6-T4 验收第 4 条、M6-T5 验收判据第 2 条各加 N≥1 哨兵要求。

## 6. 开放问题 / 预案

1. **`SchedulerRunResult` 新增字段的下游消费**:main.rs 当前仅日志循环;若 worker 仓其他模块解构该 struct(非 `..` 模式),实现期编译即暴露,逐处补 `underscan_warning_campaign_ids: vec![]` 初始化(机械修复,无断言面)。
2. **AG-013 豁免预案(db-glue)**:`apply_once_completion` 返回值传给 `db::mark_campaign_completed` 的 ≤3 行 glue 若产生 missed mutants(如错误信息字符串变异),以「P-007 gated 测试覆盖端到端 + glue 无分支逻辑」为书面豁免,交 Test-Gate Reviewer。
3. **required check 设置**:T4 工作流是否设为受保护分支 required check 属 GitHub 仓库设置(手工一次),登记 root 验收清单跟进,不阻塞 M6 完成。
4. **terminal_reason 列存在性前置核查**(§2.2-d):若 `glance_mind_rust` schema 中列不存在(与 C-004 证据冲突)→ BLOCKED 上报,严禁建 migration。
5. **Step 07 patch 记录**:见 `patches/07-batchD-m6.md`(本文件 §5 备注节已内联摘要;patches/ 目录为跨步骤追溯索引)。
