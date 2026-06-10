# Module Plan: M2 `agent-rs/facebook-p0`

> 计划族:`docs/plans/jplan/2026-06-10-pagination-max-scan/`(Step 04 起草,2026-06-10)
> 仓库:`glance_mind_agent_rs`(本仓)。依赖:**M1**(D1~D4 共享接口,已定形于 `plans/modules/m1-pagination-core.md` §2,待 root 冻结)。被依赖:M3/M4/M5(样板软参照)、M6(语义:消费 agent 写入的 terminal_reason 值)。
> 账本输入:requirements(R-001 fb 行、R-002;R-012 约束)、invariants-failures(I-006;F-001~F-006/F-009 证据侧)、test-suite(T-001 fb 行、T-010、T-015、T-016、T-017、T-040、T-050、T-054)、anti-gaming(AG-001~AG-007/AG-010~AG-012 全局、AG-020~AG-023 引用适用性)、production-dependencies(P-001、P-005)、providers(PV-001)、framework(FR-003 mock 样板扩展、FR-005)、cross-service-contracts(§3 契约不变)。

## 0. 模块目标(一句话)

关闭 P0 生产事故面(campaign 269 / task 4285):**解除 facebook strategy 层 `v.min(20)` 总量截断**(事故根因,`src/strategies/facebook.rs:131`),把 M1 的欠交付契约(D1)接入 facebook 适配器路径——facebook adapter override `fetch_by_keyword_with_outcome` 返回带 `shortfall` 的 `FetchOutcome`,达成「`max_count=50` 上游充足时实际扫 50/COMPLETED;上游枯竭/中途失败/防环时如实上报 `NO_MORE_POSSIBLE_DATA` / `COMPLETED_WITH_PARTIAL_ERRORS`」。**对外零形状改动(R-012):不动 schema/migration/预算函数/Redis 协议形状/对外 DTO。**

## 1. 全模块强制约束(每个任务自动继承,不再逐条复抄全文)

1. **AG-001 先红后绿**:每个新测试必须先以「正确的原因」失败;RED 输出与 GREEN 输出都留存为任务完成证据(粘贴入任务完成报告)。
2. **AG-002 断言不可变 / 实现者不得修改断言来过测试**:本计划各任务定义的 RED 测试是契约。实现子 agent 测试失败时只能改生产代码或停下上报;唯一例外是期望本身错误,须带 `ASSERTION-CHANGE-JUSTIFIED: <原因>` 并重走 RED→GREEN。
3. **AG-003 禁绕过/伪造**:不得新增 `#[ignore]`、不得吞 `Result`/`unwrap_or_default()` 掩盖失败、不得把 expected 改成 actual、生产代码不得特判测试输入(如识别 mock 注入/特定 query 而走特殊分支)。
4. **AG-004 职责分离**:subdriven 执行时,每个任务的「测试载荷」(写测试 + 必要的类型骨架)与「实现载荷」(让测试转绿)分属**不同子 agent 上下文**;实现上下文不得改测试文件断言。
5. **AG-005 过度 mock 禁令**:只 mock 外部依赖(RapidAPI HTTP 上游、AI/progress/repo 测试替身);不得 mock 被测对象本身(facebook 适配器翻页、strategy cap 计算、orchestrator shortfall 映射)。
6. **AG-006 先绿类回归测试**:凡本计划标注「允许先绿」的测试,必须经变异门禁证明有效(AG-012 预检中对应代码被变异时该测试变红),否则视为无效证据。
7. **AG-007 如实报告**:CI Rust Test Gates 捆绑 live-API 测试,PR 红可能是 Facebook 上游漂移(端点/字段)而非本改动;验收时须区分,不得以此为由弱化断言。real gates(T-050/T-054)默认不入 `cargo test`,失败先核对凭据/上游可用性再判定。
8. **变异门槛(AG-010~AG-012)**:M2 全部 diff 在本地预检 `cargo mutants --in-diff` 下**无 missed mutants**,或对每个 missed 写书面豁免(说明为何不可测/等价变异,入 PR 描述,Step 06 Test-Gate Reviewer 复核)。**事故根因行(`facebook.rs:131` cap 截断 + 适配器 shortfall 判定)不接受静默豁免**。
9. **R-012 零形状改动**:本模块 diff 不得触及 `migrations/`、`src/schema.rs`、`src/db/schema.rs`、预算存储过程调用形状、`src/protocol_gen/`(协议结构体)、任何对外 API DTO。不触发 db-migration-guard / api-contract-guard。
10. **D1~D4 消费契约不重设计(M1 冻结)**:`FetchShortfall`/`FetchOutcome`/`fetch_by_keyword_with_outcome`/`PaginationLoop`/`platform_page_cap`/`page_size_hint`/D3 orchestrator 映射表均按 M1 §2 引用,M2 **只消费不改形状**;`platform_page_cap("facebook")==20` 为冻结值(M1 §2 D4),修订须经 root。

## 2. M2 消费的 M1 共享接口(只读引用,不在 M2 重定义)

> 全部定义见 `plans/modules/m1-pagination-core.md` §2(D1~D4)。下面仅摘 M2 任务直接调用的形状与语义锚点,**形状以 M1/root 冻结版为准**。

- **D1 欠交付契约**:`FetchShortfall::{Exhausted, PartialFailure{message}}`、`FetchOutcome{contents, shortfall}`、trait 默认方法 `fetch_by_keyword_with_outcome`(默认 shortfall=None)。**M2-T2 = facebook 适配器 override 此方法**,把内部翻页的终止/失败原因翻译成 `shortfall`。
- **D2 翻页状态机**:`src/pagination.rs::PaginationLoop`(`accept_page`、`StopReason::{ReachedMaxCount,UpstreamExhausted,CursorLoop,EmptyPageLimit}`、`shortfall_for`、`MAX_EMPTY_PAGES=3`)。**M2 裁决(§6.1,a:循环形态与生产行改动面)**:facebook 适配器**保留既有手写循环**(facebook.rs:522-644,已被既有 mock-HTTP 测试证明),M2 不重写为 `PaginationLoop`;生产行改动限两类:① 循环出口/失败分支**接线 shortfall**;② **三循环的空页计数从 `page_count == 0` 改为本页新增(去重后)数 == 0**(facebook.rs:546-558 三处;D-15 已裁决,对齐 M1 D2 `empty_streak` 冻结语义:按 `newly_accepted.is_empty()` 递增,重复内容页与字面空页同等计入)。**(b:接线语义)**:达量→None、枯竭族→Exhausted、`RateLimited` 且**过滤后交付集非空**→PartialFailure(**进展定义 = 进入 `FetchOutcome.contents` 的过滤后条目数**,invariants §3/D1 冻结,DR-09 统一口径——**不**按原始 posts 计)。语义须与 `PaginationLoop::shortfall_for` 表一致(M2-T2 断言对齐)。
- **D3 orchestrator 终态映射**:`fetch_content` 改用 `fetch_by_keyword_with_outcome`、`process_keyword` 按 shortfall 设 `KeywordProcessOutcome.terminal_hint`(M1-T4 已落)。**M2 不在 orchestrator 重复实现映射**;T-015/T-016/T-017/T-040 是该映射对 facebook 适配器 override 的**端到端实例化证据**(orchestrator.rs:39-61 `KeywordProcessOutcome`/L344-423 聚合/L668-682 fetch_content,M1-T4 接线点)。
- **D4 页大小语义**:`TaskConfig.page_size_hint: Option<u32>`(redis 映射写入,clamp 到 `platform_page_cap("facebook")=20`)。**M2-T1**:facebook strategy `build_search_options` 消费 `page_size_hint` 作**单页 count**,`options.count` 改传**总量** `max_videos`(不再 `v.min(20)`);删 `facebook.rs:131` 截断。
- **MockContentGateway 分页注入(M1-T3)**:`add_search_pages` / `set_page_error_at` —— M2 orchestrator 级集成测试(T-015/T-016/T-040)直接复用,**不打真实上游**(AG-005:mock 的是上游数据,不是被测映射)。
- **facebook 适配器级 mock-HTTP 样板(FR-003)**:`spawn_mock_http_server(_with_capture)` / `MockHttpResponse::json` / `test_post`(facebook.rs:972-1075)—— M2-T2/T3 适配器级测试扩展此样板,**不引入 wiremock/mockito 新依赖**。

## 3. 状态面(step-04 action 8:一写多读)

| 状态键 | 唯一写方 | 声明读方 |
|---|---|---|
| `SearchOptions.count`(facebook,= 总量 max_videos) | `FacebookStrategy::build_search_options`(M2-T1) | facebook 适配器循环 `reached_post_limit`(facebook.rs:518-520,既有读方) |
| facebook 单页请求 count(= page_size_hint,默认 20) | `FacebookStrategy::build_search_options`(M2-T1;**注**:facebook RapidAPI 端点按 cursor 翻页,无显式 page-size 入参——见 §6.3,单页大小由上游决定,hint 仅用于 `options.count` 不被 hint 误截) | facebook 适配器(若上游不支持 page-size 参数,则 hint 不下发,仅 R-012 合规记录) |
| `FetchOutcome.shortfall`(facebook) | `FacebookAdapter::fetch_by_keyword_with_outcome`(M2-T2) | `orchestrator::process_keyword`(读方在 M1-T4) |
| `gm_crawler_tasks.terminal_reason`(facebook 路径落库) | agent postgres adapter(既有,值集不变,C-004) | scheduler eval_once(M6 读方,容忍 NULL/未知) |

> **注**:M2 不新增任何「状态键的新写方」到既有 Redis/DB 形状;`page_size_hint` 写方在 M1(redis.rs)。M2 只是 facebook strategy 侧的**读方**(消费 hint)+ facebook adapter 侧 shortfall 的**写方**(D1 默认方法 override)。

## 4. 任务清单

> 执行序 = 编号序(T1 strategy 解截断 → T2 适配器 shortfall override → T3 适配器级防环/枯竭样板 → T4 orchestrator 端到端集成 → T5 事故重演 → T6 real gates → T7 收尾)。
> 每任务 1-3 文件、单个全新实现者上下文可完成;「测试载荷」与「实现载荷」分上下文(AG-004)。
> T1~T5、T7 为**确定性测试(mock / 纯逻辑),不需任何 live 凭据**;**T6(real gates)独立 gate,需凭据,不入默认 `cargo test`**。

---

### M2-T1 strategy 解除 `min(20)` 总量截断 + 消费 page_size_hint(T-001 fb 行、R-001 fb)

**覆盖 ID**:R-001(fb 行)、T-001(fb 行)、AG-001~AG-006。依赖:M1-T5(`page_size_hint` 字段 + clamp;若 M2 先于 M1 执行,实现者须停下上报序错)。
**文件**:`src/strategies/facebook.rs`(L131 cap 行 + 测试模块 L247+)。

**测试载荷(测试子 agent 先写;`src/strategies/facebook.rs` `#[cfg(test)]` 内)**:

1. `search_count_is_total_not_capped_at_20`(T-001 fb 主断言):`TaskConfig` `max_videos=50` 的 facebook keyword-mode config → `build_search_options(...).count == 50`(**不是 20**)。
2. `search_count_equals_max_videos_for_various_totals`:`max_videos ∈ {25, 50, 137}` → `options.count` 分别 `== 25/50/137`(钉死「count = 总量」语义,防实现者把 20 换成另一个硬上限)。
3. `page_size_hint_does_not_cap_total_count`:config 带 `page_size_hint=Some(20)`、`max_videos=50` → `options.count == 50`(hint 是单页提示,**不截总量**;I-001 语境)。
4. `missing_max_videos_defaults_unchanged`:`max_videos=None` → `options.count == 10`(现状默认值回归保护,facebook.rs:131 末 `unwrap_or(10)` 不变)。(**允许先绿 + AG-006:AG-012 覆盖,cap 行在 diff 内**)

**预期 RED 失败信息**(现状 `let count = config.max_videos.map(|v| v.min(20) as u32)` 仍在):
- 测试 1:`assertion 'left == right' failed: left: 20, right: 50`;
- 测试 2(max_videos=137):`left: 20, right: 137`。
RED 证据 = `cargo test --lib strategies::facebook` 中测试 1/2/3 红、测试 4 绿的输出。

**GREEN(实现子 agent)**:把 `facebook.rs:131` 改为
```rust
// R-001: options.count = 总量目标(max_videos),不被单页上限截断(事故根因 v.min(20) 删除)。
// 单页大小由 page_size_hint 提示(clamp 到 platform_page_cap=20,见 redis.rs/M1);
// 总量翻页由适配器循环(search_posts_paginated)消费 options.count 达成。
let count = config.max_videos.map(|v| v as u32).unwrap_or(10);
```
(若 config 暴露 `page_size_hint` 且 facebook 上游支持单页 size 入参,另行下发——见 §6.3;**本任务不要求下发 hint 到 HTTP,只要求 `options.count` = 总量**。)
命令:`cargo test --lib strategies::facebook`。

**最终验收命令**:`cargo test --lib strategies::facebook -- --nocapture` + `cargo build --all-features`。

**反作弊声明**:实现者不得修改断言来过测试;尤其**不得把 `v.min(20)` 换成 `v.min(50)` 或其它硬上限**(测试 2 用 137 钉死任意总量)——总量截断本身是事故根因,任何 cap 都违反 R-001。

---

### M2-T2 facebook 适配器 override `fetch_by_keyword_with_outcome` + shortfall 接线(R-002、F-002/F-003/F-006、PV-001 mock 侧)

**覆盖 ID**:R-002、T-010(达量形状)、F-002、F-003、F-004、F-005、F-006、I-004(facebook 实例)、PV-001(mock-HTTP 侧)、AG-001~AG-006。依赖:M1-T2(StopReason/shortfall_for 语义)、M1-T3(D1 类型 + 默认方法)、M2-T1(count 语义)。
**文件**:`src/adapters/facebook.rs`(L518-644 循环出口接线 + L805-830 `ContentGateway` impl 加 override + 测试模块 L972+ 扩展)。

**测试载荷(测试子 agent 先写;facebook.rs `#[cfg(test)]`,扩展既有 mock-HTTP 样板)**:

1. `fetch_outcome_reaches_count_no_shortfall`(R-002 / T-010 / PV-001 充足):mock-HTTP 3 页(**20+20+10** 个 `test_post`,T-010 统一形状,与 test-suite 账本一致;前两页带 `cursor`,第 3 页 `cursor:null`)、`options.count=50` → `outcome.contents.len() == 50`、`outcome.shortfall.is_none()`。**并**断言请求序列(用 `spawn_mock_http_server_with_capture`)第 2/3 请求带前页 cursor(`cursor=...`,沿 facebook.rs:1198-1201 样板)。(**允许先绿 + AG-006 金丝雀**:RED 基线下默认方法包装既有循环已具备该行为;金丝雀 = 测试 1 临时 truncate 交付集,须使本测试红,还原复绿,输出留存)
2. `fetch_outcome_exhausted_when_cursor_null_before_count`(F-003):mock 2 页(20+10,第 2 页 `cursor:null`)、`options.count=50` → `contents.len() == 30`、`shortfall == Some(FetchShortfall::Exhausted)`。
3. `fetch_outcome_cursor_loop_maps_exhausted`(F-004):第 2 页返回与第 1 页相同 `cursor`、未达 count → `shortfall == Some(Exhausted)`(seen-cursor 终止按枯竭语义,facebook.rs:564-566 既有 break 现接 shortfall)。
4. `fetch_outcome_empty_page_limit_maps_exhausted`(F-005):连续 3 空页(cursor 各异)、未达 count → `shortfall == Some(Exhausted)`(`MAX_EMPTY_CURSOR_HOPS=3` 出口接 shortfall)。
5. `fetch_outcome_partial_failure_with_progress`(F-002 / F-006 / DR-05):第 1 页 20 条、第 2 页 mock **排队 4 个 429 响应(各带 `retry-after: 0`)**(适配器 `request_json` 对 429 自动重试 ×3,共 4 次请求,facebook.rs:27/154-172;mock 样板 = facebook.rs:1406-1437 `test_governor_rate_limiter_applies_to_retries`)、`options.count=50` → `contents.len() == 20`、`shortfall == Some(FetchShortfall::PartialFailure{message})` 且 `message`(小写后)含 `"rate"`(facebook.rs:569-576 既有 RateLimited 且有进展 break 现接 PartialFailure)。请求数期望(若本测试用 capture 样板)= `1 + 4 == 5`。
6. `fetch_zero_progress_rate_limit_is_err`(F-001 语义边界):第 1 页即 429(零进展;mock 排队 4×429、各带 `retry-after: 0`,DR-05 同形)→ `fetch_by_keyword_with_outcome` 返回 `Err(GatewayError::RateLimited{..})`(零进展走错误路径,**不**包装成 PartialFailure;facebook.rs:577 `Err(err) => return Err(err)` 路径)。
7. `default_fetch_by_keyword_still_returns_contents`(回归):既有 `fetch_by_keyword`(无 outcome)对同 mock 3 页(20+20+10)仍返回 50 条 Vec(override 不破坏既有方法;**允许先绿**,AG-006·**手工金丝雀程序(DR-12)**:临时让 `fetch_by_keyword` 返回截断 Vec(如 `truncate(10)`)→ 本测试须红;只动生产代码、RED 输出留存后还原)。
8. `shortfall_matches_pagination_loop_semantics`(对齐契约,D-01 强制项;**对齐域声明(DR-17a):覆盖无 date-filter 子空间的五形状**):对「达量 / cursor 缺失 / cursor 环 / 空页上限 / **重复内容页**」五形状,断言 facebook 适配器产出的 `shortfall` 与同形状喂入 `PaginationLoop::{accept_page→shortfall_for}` 的结果**逐一相等**(防 facebook 私自定义与 M1 不一致的枯竭语义)。date-filter 子空间的对齐覆盖由测试 9/12 承担。
9. `date_filter_empty_delivery_with_429_is_err`(DR-01 M2 侧 / D1 构造不变量):options 带 `START_DATE`/`END_DATE`,第 1 页 20 条**全部在日期范围外**(过滤后交付 0),第 2 页排队 4×429(`retry-after: 0`)→ 整体 **`Err(GatewayError::RateLimited{..})`**(D1 构造不变量,M1 §2 冻结:`PartialFailure` ⇒ `contents` 非空;零可交付进展(过滤后为空)的失败一律走 `Err`,不得包成 PartialFailure)。
10. `hard_error_with_progress_is_err`(DR-10 M2 侧):第 1 页 20 条、第 2 页 HTTP 500 → 整体 `Err`(PartialFailure 触发集冻结 = 仅 `GatewayError::RateLimited`,M1 D2/DR-10 冻结;其余错误即使有进展也整体 `Err`)。(**允许先绿 + AG-006 金丝雀**:RED 基线下默认方法包装既有循环已具备该行为;金丝雀 = 测试 10 临时把硬错误收敛为 PartialFailure,须使本测试红,还原复绿,输出留存)
11. `repeated_content_pages_stop_via_empty_limit`(D-15 / DR-03 fb 对齐):第 1 页 20 条内容,随后 3 连页返回**与第 1 页相同内容**(cursor 各异)→ 终止、`shortfall == Some(Exhausted)`、**不发第 5 请求**(`requests.len() == 4`;空页计数按「本页新增(去重后)数 == 0」递增,D-15 生产行改动的 killing 测试)。
12. `date_filter_shortfall_uses_filtered_count`(DR-17b):形状 A——raw 50 条、过滤后 30 条、cursor 链尽、`options.count=50` → `contents.len() == 30`、`shortfall == Some(Exhausted)`;形状 B——过滤后达 count → `shortfall == None`(达量/枯竭判定按**过滤后交付计数**,与 `reached_post_limit` 的 date-filter 后判定语义一致)。
13. `candidates_inner_exhaustion_not_leaked`(DR-17c):`search_type=pages`(两级 candidates 循环),candidate1 内页枯竭(仅少量),candidate2 内页补足达 count → `shortfall == None`(内层 `fetch_page_posts_paginated` 的枯竭不得外泄为整体 shortfall)。(**允许先绿 + AG-006 金丝雀**:RED 基线下默认方法包装既有循环已具备该行为;金丝雀 = 测试 13 临时让内层枯竭外泄为 Some(Exhausted),须使本测试红,还原复绿,输出留存)

**预期 RED 失败信息**(adapter 尚未 override 默认方法时,默认方法返回 `shortfall=None`):
- 测试 2:`assertion 'left == right' failed: left: None, right: Some(Exhausted)`;
- 测试 5:`assertion 'left == right' failed: left: None, right: Some(PartialFailure { .. })`;
- 测试 6(若实现者错误地把零进展也包成 PartialFailure):`assertion failed: matches!(result, Err(GatewayError::RateLimited { .. }))`;
- 测试 8(默认方法 shortfall=None 时):五形状中除达量外,逐形状 `left: None, right: Some(Exhausted)` 类不等断言失败。
RED 证据 = 上述测试在「仅有 M1 默认方法、facebook 未 override」状态下的失败输出。

**GREEN(实现子 agent)**:在 `impl ContentGateway for FacebookAdapter` 内 override
```rust
async fn fetch_by_keyword_with_outcome(
    &self, keyword: &KeywordType, options: &SearchOptions,
) -> GatewayResult<FetchOutcome> { ... }
```
内部复用既有 `search()` 分发,但循环须把**终止原因/失败分支**翻译为 `shortfall`:
- 达 `options.count`(`reached_post_limit` 命中)→ `shortfall = None`;
- `cursor==None` / seen-cursor 重复 / 空页达上限,且 `contents.len() < options.count`(`contents` = **过滤后交付集**长度,DR-09 口径)→ `Some(Exhausted)`;
- `RateLimited` 且**过滤后交付集非空**(进入 `FetchOutcome.contents` 的过滤后条目数 > 0,DR-09)→ `Some(PartialFailure{message})`;
- `RateLimited` 且**过滤后交付集为空** → `return Err(...)`(含 raw 非空但全被 date-filter 滤除的形状,测试 9 钉死)。
**实现裁决(§6.1)**:保留既有手写循环,在 `search_posts_paginated`/`fetch_page_posts_paginated`/`fetch_posts_from_search_candidates` 三处出口返回 `(Vec<Content>, Option<FetchShortfall>)`(或等价载体),由 override 汇总;**语义须与 `PaginationLoop::shortfall_for` 表一致**——任务须加一条断言对齐测试(测试 8,见测试载荷区)。**shortfall 只由最外层循环的最终出口决定**;内层 `fetch_page_posts_paginated` 的枯竭不得外泄为整体 shortfall(测试 13 钉死)。

命令:`cargo test --lib adapters::facebook`。

**最终验收命令**:`cargo test --lib adapters::facebook -- --nocapture` + `cargo build --all-features`(确认既有 4 个平台适配器 + facebook 既有 `fetch_by_keyword` 零破坏)。

**反作弊声明**:实现者不得修改断言来过测试;不得在适配器内特判 mock query/host 走特殊分支(AG-003);零进展失败不得被静默包成 PartialFailure(测试 6 钉死)。

---

### M2-T3 facebook 适配器级翻页样板巩固:防环/空页/RateLimited 适配器单测(T-017、F-004/F-005/F-006 适配器侧)

**覆盖 ID**:T-017、F-004、F-005、F-006(适配器单测层)、PV-001(防环子项)、AG-001~AG-006。依赖:M2-T2。
**文件**:`src/adapters/facebook.rs`(测试模块,扩展 facebook.rs:1204-1311 既有 empty-page/cursor 样板)。

> **边界澄清**:M2-T2 测试 3/4/5 已断言 `shortfall` 映射;**M2-T3 专注适配器循环的「请求级行为」**(请求次数、cursor 转发停止、空页计数复位)——与既有 `test_search_posts_continues_across_empty_page_when_cursor_advances`(facebook.rs:1204)同层,补齐 F-004/F-005 的请求级断言,避免与 T-002 的语义级混淆。

**测试载荷(测试子 agent 先写;用 `spawn_mock_http_server_with_capture`)**:

1. `cursor_loop_stops_and_does_not_refetch`(F-004 请求级):第 2 页返回与第 1 页相同 cursor → 适配器**不发第 3 个相同 cursor 请求**(`requests.len() == 2`),收集首两页内容。
2. `empty_page_streak_stops_at_three`(F-005 请求级):mock 连续返回 3 个空页(cursor 各异 c2/c3/c4)→ 适配器在第 3 空页后停止(`requests.len() == 3`,不发第 4 请求);断言总收集 0 条且(经 T-002 路径)shortfall=Exhausted。
3. `empty_streak_resets_on_nonempty_page`(F-005 复位):空页 → 非空页(20 条)→ 空页 序列、count=50 → 计数在**本页有新增(去重后)**时复位(D-15 语义),不在第 2 空页误停(`requests.len()` 反映继续翻页直到真正终止)。
4. `rate_limited_after_progress_stops_with_collected`(F-006 请求级):第 1 页 20 条、第 2 页**排队 4 个 429 响应(各带 `retry-after: 0`)** → `requests.len() == 1 + 4 == 5`、返回首页 20 条(与 M2-T2 测试 5 同源但断言请求次数:**重试耗尽(4 次)后进入 partial 分支,不无限重试**)。

**预期 RED 失败信息**:若实现者(M2-T2)在接 shortfall 时**误改了循环的请求触发条件**(如把 seen-cursor break 删除导致重复请求),测试 1 `assertion 'left == right' failed: left: 3, right: 2`;空页计数实现错误时测试 2 `left: 4, right: 3`;T3.4 请求数期望按 5 计(适配器对 429 自动重试 ×3)。
> **注**:这些请求级行为**现状已正确**(facebook.rs 既有循环)。故测试 1~4 **允许先绿(AG-006)**:其价值是「为 M2-T2 改动钉住既有循环不变量」——必须经 AG-012 预检证明有效(变异 seen-cursor/empty-hop 判定时这些测试须变红)。**AG-006 证明 = 手工金丝雀(只动生产代码、输出留存后还原)**:T3.1 = 临时注释 seen-cursor break → 须红;T3.2/3.3 = 临时改 `MAX_EMPTY_CURSOR_HOPS` 判定(3→999)→ 须红;T3.4 = 临时移除 RateLimited-partial 分支 → 须红。书面豁免仅当金丝雀也不可行且附实际说明。

**GREEN 命令**:`cargo test --lib adapters::facebook`(T-017 组)。
**最终验收命令**:`cargo test --lib adapters::facebook -- --nocapture`。

**反作弊声明**:实现者不得修改断言/请求次数期望来过测试;「允许先绿」测试若 AG-012 预检证明无效(变异不变红),须补强断言而非降低期望。

---

### M2-T4 strategy 解截断 + D3 映射的 facebook 形状实例(T-010、T-015、T-016;MockContentGateway 装配)

**覆盖 ID**:T-010、T-015、T-016、R-007(facebook 集成证据)、R-008(facebook 集成证据)、I-005、I-006(进度守恒证据侧,确定性)、F-001/F-002/F-003(集成证据)、AG-001~AG-006。依赖:M1-T4(orchestrator shortfall 消费已落)、M2-T2(facebook override)。
**文件**:`src/orchestrator.rs`(`#[cfg(test)]`,复用既有 MockRepository/MockCommentGateway/MockAiAnalyzer 装配 + M1-T3 的分页注入 `MockContentGateway`)。

> **边界澄清**:D3 映射逻辑由 **M1-T4** 实现并单测(用 MockContentGateway 的通用 shortfall 注入)。**M2-T4 是 facebook 语境的实例化**:用 facebook strategy + 注入 facebook 形状分页数据,证明「strategy 解截断(T1)+ adapter shortfall(T2)+ orchestrator 映射(M1-T4)」端到端贯通。**不得在 orchestrator 重写映射**(若映射有缺陷,回 M1-T4 修)。**真实 adapter override 不在本任务测试路径**(mock gateway 直接产 shortfall);override 的集成覆盖由 M2-T2 全套 + 测试 8 对齐断言承担(§7 注明)。

**测试载荷(测试子 agent 先写)**:

1. `facebook_exhausted_maps_no_more_possible_data`(T-016 / F-003):MockContentGateway 注入 facebook 2 页(20+10,枯竭)、facebook strategy、`max_videos=50` → `process_task` 后经 `progress_tracker.get_task(id)` 断言 `terminal_reason` 以 `"NO_MORE_POSSIBLE_DATA"` 开头、task completed、`contents_processed == 30`。并断言**未调用** `stop_campaign_gracefully`(MockRepository 调用计数;D3 设计裁决,M1 §6.1)。
2. `facebook_page2_failure_maps_partial_errors`(T-015 有进展 / F-002):第 1 页 20 条、第 2 页注入失败、`max_videos=50` → terminal_reason 以 `"COMPLETED_WITH_PARTIAL_ERRORS"` 开头、task completed、`contents_processed == 20`(已落进展保留)。
3. `facebook_page1_failure_maps_failed`(T-015 零进展 / F-001 / R-008):第 1 页即不可恢复错误 → task **failed**、terminal_reason 以 `"PROVIDER_FAILURE"` 开头(既有失败路径回归;**允许先绿**,AG-006 变异证明)。
4. `facebook_full_delivery_maps_completed`(达量基线):注入 3 页(**20+20+10**)满 50 → terminal_reason 以 `"COMPLETED"` 开头且**不含** `"PARTIAL"`、不含 `"NO_MORE"`、`contents_processed == 50`。

**预期 RED 失败信息**:**RED 取证点 = pre-M2-T1 基线**(strategy cap 截断 → count=20 提前达量):测试 1 got `COMPLETED` + contents_processed==20(而非 NO_MORE_POSSIBLE_DATA + 30);测试 2 同理(got `COMPLETED` + contents_processed==20,而非 COMPLETED_WITH_PARTIAL_ERRORS + 20 有进展路径)。
RED 证据 = 在『M1 已在、**M2-T1/T2 均未落地**(即 pre-M2 基线 commit,与 M2-T5 RED 基线相同)』状态下的失败输出。

**GREEN(实现子 agent)**:确认注入 gateway 实现 facebook 形状的 `fetch_by_keyword_with_outcome`(复用 M1-T3 `MockContentGateway::add_search_pages`/`set_page_error_at`),无需改 orchestrator 生产代码(D3 已在 M1-T4)。若测试暴露 orchestrator 缺陷,**回 M1-T4 修**并在报告注明跨模块依赖。命令:`cargo test --lib orchestrator`。

**最终验收命令**:`cargo test --lib orchestrator -- --nocapture` + `cargo test`(全套件无回归)。

**反作弊声明**:实现者不得修改断言来过测试;不得通过让 mock 恰好返回「50 条 / 30 条」来绕开 shortfall 映射(AG-003);`contents_processed` 守恒断言不得弱化为 `>= N`。

> **装配注(F-05)**:装配吃紧时允许页内容量等比缩小(形状与断言语义不变,完成报告注明;50 达量语义由 M2-T2 测试 1 与 T-040 形状 1 承担)。

---

### M2-T5 事故重演回归(campaign 269 形状)(T-040)

**覆盖 ID**:T-040、R-001/R-002/R-007(综合)、I-001(达量上界证据)、AG-001~AG-006。依赖:M2-T1、M2-T2、M2-T4。
**文件**:`src/orchestrator.rs`(`#[cfg(test)]`,与 M2-T4 同装配)或 `tests/`(若需更接近端到端,沿确定性 mock,不打真实上游)。

> T-040 是 **root chain gate 的最终验收项**(03-split §2 裁决):必须给出 mock 层两形状。本任务在 M2 内落地两形状的确定性回归,root 在 chain gate 复跑。

**测试载荷(测试子 agent 先写)**:

1. `incident_269_shape_sufficient_upstream_scans_50_completed`(事故修复证据):facebook strategy、`max_videos=50`、MockContentGateway 注入 ≥50 条可得(如 3 页 20+20+20)→ `contents_processed == 50`、terminal_reason 以 `"COMPLETED"` 开头、**不含** `"NO_MORE"`/`"PARTIAL"`;并断言**无 WARN 级欠扫告警**(若 orchestrator 有欠扫 warn 计数器/钩子则断言其未触发;无则以 terminal_reason 纯净为证)。
2. `incident_269_shape_only_20_available_reports_no_more`(如实上报证据):同 config,但上游仅 20 条可得(1 页 20 + cursor:null)→ `contents_processed == 20`、terminal_reason 以 `"NO_MORE_POSSIBLE_DATA"` 开头(**不得**静默 `COMPLETED`——这正是事故:扫 20 即标 COMPLETED 掩盖欠扫)。

**预期 RED 失败信息**(现状 strategy 截断 + 无 shortfall):
- 测试 1:`assertion 'left == right' failed: left: 20, right: 50`(现状只扫 20);
- 测试 2:`assertion failed: reason.starts_with("NO_MORE_POSSIBLE_DATA"), got "COMPLETED: ..."`(现状把欠扫标成 COMPLETED——事故本体)。
RED 证据 = 这两条在事故现状下的失败输出(直接重演 campaign 269)。**RED 基线 = 含 M1、不含 M2 的 commit**(M1 未合 main 时用分支上 M1 完成点 commit 建 worktree,D-11 增补)。

**GREEN 命令**:`cargo test --lib orchestrator::tests::incident_269`(M2-T1~T4 全绿后自动通过)。
**最终验收命令**:`cargo test --lib -- --nocapture incident_269` + 输出留存(交 root chain gate)。

**反作弊声明**:实现者不得修改断言来过测试;测试 2 的 `NO_MORE_POSSIBLE_DATA` 期望**编码了事故的正确终态**,不得改为 `COMPLETED`(那将复现事故本体,属 ASSERTION-CHANGE 严禁场景)。

---

### M2-T6 real gates:facebook 真实第二页 + real-DB 守恒(T-050、T-054、P-001、P-005、PV-001 real 侧、F-009、I-006)

**覆盖 ID**:T-050、T-054、P-001、P-005、PV-001(real 输出回灌 fixture)、F-009、I-006(real-DB 守恒)、AG-007。依赖:M2-T2(adapter override)、M2-T4(orchestrator 路径)。
**文件**:`tests/facebook_real_api_test.rs`(扩展;凭据 gate 模式见 L51-66)、`tests/facebook_real_db_test.rs`(扩展;real-DB gate 模式见 L271-289)。

> **gate 控制(本任务硬约束,不入默认 `cargo test`)**:
> - **凭据**:T-050 需 `FACEBOOK_RAPIDAPI_KEY`(未设 → `require_rapidapi_config` panic / 既有 skip 模式);T-054 需 `DATABASE_URL`(未设 / `GITHUB_ACTIONS` 无 `RUN_REAL_DB_TESTS` → `database_url()` 返回 None → skip,facebook_real_db_test.rs:271-289)。
> - **调用上限**:T-050 ≤3 次 RapidAPI 调用(P-001);只读,无清理。
> - **幂等/清理**:T-054 沿既有测试的 task/数据隔离与清理模式(`(task_id,video_id)` 唯一约束),不新增清理负担(P-005)。
> - **AG-007 live 漂移**:real 测试红时,先核对凭据 + 上游可用性,区分「Facebook 端点/字段漂移」与「本改动回归」,不得以漂移为由弱化断言。
> - **fixture 回灌(PV-001)**:T-050 实跑得到的真实第二页响应,**若**需固化为确定性 fixture(供 M2-T2 mock 用),字段必须取自实跑输出,**禁止凭空捏造**;新 fixture 入库并标注来源 commit/日期。

**测试载荷(测试子 agent 先写;均为 gated 测试)**:

1. `real_facebook_second_page_cursor_smoke`(T-050 / P-001):live keyword 搜索,`options.count` 设足以触发第二页(如 25),断言:返回总数 > 单页量(证明翻到第二页)或在响应中观察到第二页 cursor 被转发且第二页内容**非空且异于首页**(`contents` 去重后 > 首页量)。调用 ≤3 次。
2. `real_facebook_paged_db_conservation`(T-054 / P-005 / I-006 / F-009):real Facebook 抓取多批 → 经 `PostgresAdapter` 落库 → 断言 `consumed` 增量 `== 实际处理条数`(I-006 守恒)、`terminal_reason`(`NO_MORE_POSSIBLE_DATA` 或 `COMPLETED_WITH_PARTIAL_ERRORS` 或 `COMPLETED`)持久化可查(F-009:存储过程缺失降级环境下 fallback 路径写入,postgres.rs:2123-2181 兼容);`(task_id, video_id)` 唯一约束实测。

**预期 RED 失败信息**:
- T-050:在 facebook adapter 未正确翻第二页时,`assertion failed: second_page_contents 非空且异于首页`(具体文本以实跑为准记录);
- T-054:守恒被破坏时 `assertion 'left == right' failed: left: <consumed>, right: <processed>`;terminal_reason 未持久化时 `assertion failed: row.terminal_reason.is_some()`。
> **注**:T-050/T-054 为 **live 契约探针(AG-008 类):允许先绿**;有效性判据按 AG-008(实跑输出留存 / fixture 回灌 / 判定可复算),不要求 live RED 取证(本仓逻辑的 RED→GREEN 由 T1~T5 承担)。T-050 预算注:live 429 将触发适配器 4 次计费重试,预算按 HTTP 请求计(DR-05)。

**GREEN / 验收命令**:
```bash
FACEBOOK_RAPIDAPI_KEY=… cargo test --test facebook_real_api_test real_facebook_second_page_cursor_smoke -- --nocapture
DATABASE_URL=… FACEBOOK_RAPIDAPI_KEY=… cargo test --test facebook_real_db_test real_facebook_paged_db_conservation -- --nocapture
```

**反作弊声明**:实现者不得修改断言来过测试;不得为「让 CI 绿」而 `#[ignore]` 这两个 gated 测试(它们靠 env 缺失自然 skip,**不得**改用 ignore);live 漂移须按 AG-007 如实区分,不得弱化守恒/翻页断言。

---

### M2-T7 模块收尾 gate:变异预检 + R-012/F-009 自查 + 全量回归

**覆盖 ID**:AG-010、AG-011、AG-012、R-012(模块侧自查)、F-009(兼容自查)、AG-007、FR-005(引用:无新框架待办)。依赖:M2-T1~T6 全部完成。
**文件**:无新生产代码(只跑命令 + 写证据;若预检发现 missed mutants,修复归对应任务的生产代码/补测试,不得弱化断言)。

**命令序列(全部输出留存为模块完成证据)**:
```bash
# 1. 全量确定性回归(不需凭据;real gated 测试 M1-T0 落地后自动 skip;AG-012 预检须在 live key 未设环境执行(D-14))
cargo test

# 2. 本地变异预检(AG-012;门槛 AG-011:无 missed 或逐个书面豁免)
git diff main...HEAD > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1

# 3. R-012 / 契约面 diff 自查(期望全部零命中)
git diff main...HEAD --stat -- migrations/ src/schema.rs src/db/schema.rs src/protocol_gen/
# 期望:facebook strategy/adapter 改动不触及上述路径;page_size_hint 仅域模型(M1),M2 无新协议字段
git diff main...HEAD -- src/adapters/postgres.rs   # 期望:零改动(F-009 兼容自查,与 M1-T7 同款;fallback 路径未触及)
```

**验收判据**:
1. `cargo test` 全绿(若 CI 上 live-API 套件红,按 AG-007 区分 Facebook 上游漂移并在报告注明,不得弱化断言)。
2. mutants 无 missed;**事故根因行(facebook.rs:131 cap 删除、adapter shortfall 判定四分支)+ 三条循环(search/page/candidates)的 shortfall 出口各有 killing 测试**,不接受静默豁免;其余 missed → 修生产代码/补断言重跑或写等价变异书面豁免。
3. 第 3 条 diff 自查零命中(R-012)(含 F-009 postgres.rs 零改动)。
4. 每任务 RED→GREEN 证据齐备(确定性任务:失败输出 + 通过输出各一份;real gate 任务:有凭据环境的取证或显式补证记录)。
5. 提交/PR 前跑 `rust-verify-change` 流程(项目级守卫)。

**反作弊声明**:本任务不得以任何形式(skip、删测试、放宽断言)使 gate 变绿;gate 红 = 回到对应任务修生产代码。

---

## 5. 模块完成判据与独立验证(03-split §5 M2 行)

- **确定性部分**:T-001(fb)/T-010(= M2-T2 测试 1 达量 + M2-T4 端到端,3 页→50/COMPLETED)/T-015/T-016/T-017/T-040 全 green,每个非「允许先绿」测试有 RED 证据;「允许先绿」测试(M2-T1.4、M2-T2.1、M2-T2.7、M2-T2.10、M2-T2.13、M2-T3.1~4、M2-T4.3、T-050/T-054(AG-008 探针类))有 AG-006 变异/金丝雀证明。
- **gated 部分**:T-050/T-054 实跑输出留存(有凭据/有 DB 环境)。
- **变异**:M2 全 diff 本地 `cargo mutants --in-diff` 无 missed(或书面豁免;事故根因行不豁免)。
- **独立验证命令**:
  ```bash
  cargo test --lib strategies::facebook adapters::facebook orchestrator
  git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1
  # gated:
  FACEBOOK_RAPIDAPI_KEY=… cargo test --test facebook_real_api_test -- --nocapture
  DATABASE_URL=… FACEBOOK_RAPIDAPI_KEY=… cargo test --test facebook_real_db_test -- --nocapture
  ```

## 6. 开放问题 / 提请 root·评审裁决

> **状态更新(2026-06-10)**:本节四条已经用户裁决(功能优先),见 `04-adjudications.md` —— §6.1=D-01(采纳;注意 D-01 为 facebook 特例,M3~M5 新循环仍接 PaginationLoop)、§6.2=D-03(采纳)、§6.3=D-02(采纳,root 冻结 D4 时写入措辞)、§6.4=D-04(冻结)。Step 06 评审否决须附功能性反证并经用户确认。

### 6.1 facebook 翻页循环「不重写为 PaginationLoop」裁决(DECIDED-本计划,提请 root 复核)
M2 **保留 facebook.rs:522-644 既有手写循环**,只接线 shortfall,**不**替换为 M1 的 `PaginationLoop`。理由:既有循环已被 8+ 个 mock-HTTP 测试覆盖(facebook.rs:1168-1311),重写引入回归风险且无收益;M1 的 `PaginationLoop` 是纯逻辑契约/proptest 载体与 M3~M5 新循环的复用基础。**约束**:M2-T2.8 断言 facebook shortfall 语义与 `PaginationLoop::shortfall_for` **逐形状相等**,确保两实现语义一致(防漂移)。请 Step 06 Concurrency/Resource Reviewer 复核「两套循环、单一语义」是否可接受,或要求 M2 改接 `PaginationLoop`。

### 6.2 D3 裁决继承(contents 非空 + Exhausted 不调 stop_campaign_gracefully)
M2-T4.1 断言「未调用 `stop_campaign_gracefully`」继承 M1 §6.1 裁决(campaign 级完结交 M6 消费 terminal_reason)。与既有零结果路径(会停 campaign)的行为差由 M1 记录;M2 仅提供 facebook 集成证据。请 Cross-Service Reviewer 与 M6 完成判据一并确认。

### 6.3 facebook 单页 page-size 入参可用性(影响 page_size_hint 下发)
D4 定义 `page_size_hint`(clamp 到 20),但 facebook RapidAPI `facebook-scraper3` 的搜索/页面帖子端点按 **cursor 翻页**,单页大小由上游决定,**未观察到显式 page-size 入参**(facebook.rs:309-322 请求构造仅含 query/cursor)。**M2-T1 因此只要求 `options.count` = 总量**(消除截断),**不**强制把 hint 下发到 HTTP。若 T-050 实跑发现上游支持单页 size 入参,M2 可在执行期补「hint→单页 count」下发(经 root 确认,不改 D4 形状)。当前裁决:hint 在 facebook 路径仅作「不被误当作总量上限」的语义保护,不下发。请 root/评审确认此 facebook 特例不违反 D4。

### 6.4 platform_page_cap("facebook")=20 冻结值
继承 M1 §6.2 冻结表(fb=20)。M2 未发现上游证据要求修订;若 T-050 实测单页上限 ≠ 20,走 ASSERTION-CHANGE-JUSTIFIED + root 修订流程,不在 M2 私改。

## 7. 账本覆盖映射(traceability;M2 全部认领 ID → 任务)

| 账本 ID | 认领任务 | 备注 |
|---|---|---|
| R-001(fb 行) | M2-T1 | 解耦机制接口形状归 M1;fb cap 行删除 + T-001 fb 断言归 M2 |
| R-002 | M2-T2(adapter 达量)+ M2-T4(端到端 50/COMPLETED) | T-010 实例 |
| R-007(fb 集成证据) | M2-T4(strategy 解截断 + D3 映射 facebook 形状实例) | 映射实现归 M1-T4;fb 实例化证据归 M2;真实 adapter override 集成覆盖由 M2-T2 + T2.8 承担 |
| R-008(fb 集成证据) | M2-T4(T-015 两分支) | 语义归 M1-T4 |
| R-012(模块自查) | M2-T7 | 终审归 root |
| I-004(fb 实例) | M2-T2 | 机制归 M1-T2;fb shortfall 对齐断言 M2-T2.8 |
| I-005 | M2-T4(T-015 有进展保留) | 集成证据 |
| I-006 | M2-T6(T-054 real-DB 守恒)+ root(R-012 diff) | shared;确定性守恒证据侧 M2-T4 contents_processed |
| I-001(达量上界 fb 证据) | M2-T2 / M2-T5 | 机制归 M1-T2(PT-1) |
| F-001(fb) | M2-T2.6 + M2-T4.3 | 零进展失败 |
| F-002(fb) | M2-T2.5 + M2-T4.2 | 有进展 partial |
| F-003(fb) | M2-T2.2 + M2-T4.1 | 枯竭→NO_MORE |
| F-004(fb) | M2-T2.3 + M2-T3.1 | cursor 环 |
| F-005(fb) | M2-T2.4 + M2-T3.2/3.3 | 空页上限 + 复位 |
| F-006(fb) | M2-T2.5 + M2-T3.4 | RateLimited 中途 |
| M2-T2.9(`date_filter_empty_delivery_with_429_is_err`) | M2-T2 | DR-01 M2 侧;D1 构造不变量:零可交付进展(过滤后为空)的失败走 Err |
| M2-T2.10(`hard_error_with_progress_is_err`) | M2-T2 | DR-10 M2 侧;PartialFailure 触发集冻结=仅 RateLimited |
| M2-T2.11(`repeated_content_pages_stop_via_empty_limit`) | M2-T2 | D-15/DR-03 fb 对齐;重复内容页按新增(去重后)==0 计空页;不发第 5 请求 |
| M2-T2.12(`date_filter_shortfall_uses_filtered_count`) | M2-T2 | DR-17b;达量/枯竭判定按过滤后交付计数(两形状 A/B) |
| M2-T2.13(`candidates_inner_exhaustion_not_leaked`) | M2-T2 | DR-17c;内层 fetch_page_posts_paginated 枯竭不得外泄整体 shortfall |
| F-009(fb 证据) | M2-T6(T-054) | fallback 兼容自查归 M1-T7;real-DB 证据归 M2 |
| T-001(fb 行) | M2-T1 | — |
| T-010 | M2-T2.1 + M2-T4.4 | 3 页→50/COMPLETED(20+20+10 形状) |
| T-015 | M2-T4.2 + M2-T4.3 | 两分支(有/零进展) |
| T-016 | M2-T4.1 | 枯竭映射 |
| T-017 | M2-T3 | 防环/空页(请求级)|
| T-040 | M2-T5 | 事故重演两形状(root chain gate 复跑) |
| T-050 | M2-T6 | real-API gate |
| T-054 | M2-T6 | real-DB gate |
| P-001 | M2-T6(T-050) | ≤3 调用、只读 |
| P-005 | M2-T6(T-054) | 守恒 + 持久化;fallback 兼容 |
| PV-001 | M2-T2(mock 侧)+ M2-T6(real 输出回灌 fixture 控制) | 禁凭空捏造字段 |
| AG-001~AG-007 | §1 全任务继承 + 各任务声明 | — |
| AG-010~AG-012 | M2-T7 | 事故根因行不接受静默豁免 |
| AG-020~AG-023(引用适用性) | M2-T2.8 引用(与 PaginationLoop 语义对齐)、M2-T5(达量上界实例) | PT 套件实现归 M1-T2 |
| FR-003 | M2-T2/T3(扩展既有 mock-HTTP 样板,零新依赖) | — |
| FR-005 | M2-T7(引用:无待办) | — |
| C-004(契约不变,引用) | §1.10 + M2-T6(terminal_reason 值集不变) | pin 归 M1-T6;M6 读方 |

> **覆盖自查**:handoff/03-split §3 点名的 M2 全部 ID —— R-001(fb)/R-002、T-001(fb)/T-010/T-015/T-016/T-017/T-040/T-050/T-054、PV-001、P-001/P-005、F-001~F-006/F-009(证据侧)、I-006 —— 均已认领(上表)。I-004/I-005/I-001 作为 M1 机制的 facebook 实例化证据一并落位。

> **Step 07 patch 记录**:见 `patches/07-batchC-m2.md`(去重副本删除、DR-12 金丝雀化、DR-13 M2-T4 改名实义、DR-15 允许先绿清单扩展、DR-17d 循环出口 killing 测试、TG-09 探针化、TG-10 M2-T7 命令注、TG-11 RED 基线注、F-05 装配注、§7 簿记同步)。
