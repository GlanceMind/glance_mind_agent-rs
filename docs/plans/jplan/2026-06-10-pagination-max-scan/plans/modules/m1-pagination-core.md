# Module Plan: M1 `agent-rs/pagination-core`

> 计划族:`docs/plans/jplan/2026-06-10-pagination-max-scan/`(Step 04 起草,2026-06-10)
> 仓库:`glance_mind_agent_rs`(本仓)。依赖:无(首发模块)。被依赖:M2~M5(共享构造)、M6(terminal_reason 语义,C-004)。
> 账本输入:requirements(R-007/R-008/R-009/R-011 agent/R-012;R-001 解耦机制语境)、invariants(I-001~I-005/I-008/I-009;F-001/F-002 语义层/F-003/F-004/F-005 机制/F-009)、test-suite(T-002~T-004、T-030~T-034、§8 gaps)、anti-gaming(AG-001~AG-007、AG-010~AG-012、AG-020~AG-024)、framework(FR-001/FR-003/FR-005)、contracts(C-001/C-002/C-004 agent 侧、§3 契约不变)、prod-deps(P-006)、non-code(N-002 agent 侧)、assumptions(A005 drop/A006/A007/B2/B4)。

## 0. 模块目标(一句话)

在 agent-rs 内建立「适配器内翻页到 max_count」的**共享核心机制**:可属性测试的翻页状态机(纯逻辑,与 async I/O 解耦)、fetch 路径暴露欠交付原因(exhausted / partial-failure,trait 不传 cursor)、orchestrator 终态映射、redis 字段语义(`search_limit`=页提示、`search_offset`=仅观测)、proptest 基建(FR-001)——为 M2~M5 平台模块和 M6(scheduler 读 terminal_reason)提供被冻结的公共契约。**对外零形状改动(R-012):不动 schema/migration/预算函数/Redis 协议形状/对外 DTO。**

## 1. 全模块强制约束(每个任务自动继承,不再逐条复抄全文)

1. **AG-001 先红后绿**:每个新测试必须先以「正确的原因」失败;RED 输出与 GREEN 输出都留存为任务完成证据(粘贴入任务完成报告)。
2. **AG-002 断言不可变 / 实现者不得修改断言来过测试**:本计划各任务定义的 RED 测试是契约。实现子 agent 测试失败时只能改生产代码或停下上报;唯一例外是期望本身错误,须带 `ASSERTION-CHANGE-JUSTIFIED: <原因>` 并重走 RED→GREEN。
3. **AG-003 禁绕过/伪造**:不得新增 `#[ignore]`、不得吞 `Result`/`unwrap_or_default()` 掩盖失败、不得把 expected 改成 actual、生产代码不得特判测试输入(如识别 mock 注入而走特殊分支)。
4. **AG-004 职责分离**:subdriven 执行时,每个任务的「测试载荷」(写测试 + 必要的类型骨架)与「实现载荷」(让测试转绿)分属**不同子 agent 上下文**;实现上下文不得改测试文件断言。
5. **AG-005 过度 mock 禁令**:只 mock 外部依赖(gateway/DB/Redis);不得 mock 被测对象本身(翻页状态机、orchestrator 映射、redis 映射函数)。
6. **AG-006 先绿类回归测试**:凡计划标注「允许先绿」的测试,必须经变异门禁证明有效(AG-012 预检中对应代码被变异时该测试变红),否则视为无效证据。
7. **AG-007 如实报告**:CI Rust Test Gates 捆绑 live-API 测试,PR 红可能是 Facebook/Twitter 上游漂移而非本改动;验收时须区分,不得以此为由弱化断言。
8. **变异门槛(AG-010~AG-012)**:M1 全部 diff 在本地预检 `cargo mutants --in-diff` 下**无 missed mutants**,或对每个 missed 写书面豁免(说明为何不可测/等价变异,入 PR 描述,Step 06 Test-Gate Reviewer 复核)。
9. **R-012 零形状改动**:本模块 diff 不得触及 `migrations/`、`src/schema.rs`、`src/db/schema.rs`、预算存储过程调用形状、`src/protocol_gen/`(协议结构体)、任何对外 API DTO。不触发 db-migration-guard / api-contract-guard。
10. **A005 已 drop(防漂移)**:`ContentGateway` trait **不传 cursor / has_more**;只暴露「欠交付原因」。cursor/offset/seen-set 全部留在适配器与共享状态机内部。

## 2. 共享接口决策(本计划定形,root 冻结;变更须回到 root)

> M2~M5/M6 的消费契约。Step 04 起草 M2 起即按此引用;评审通过后由 `plans/root.md` 冻结。

### D1 欠交付原因形状(R-007,A005-compliant)

`src/ports/content_gateway.rs` 新增(不改既有方法签名,**不破坏既有 5 个适配器**):

```rust
/// 取数欠交付原因(per R-007;cursor 不过 trait,A005 drop)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchShortfall {
    /// 上游枯竭:has_more=false / cursor 缺失 / 空页上限 / 重复 cursor(F-003/F-004/F-005)
    Exhausted,
    /// 翻页中途失败但已有部分进展(F-002/F-006);message 仅用于 terminal_reason,入库前必经脱敏
    PartialFailure { message: String },
}

/// fetch 路径返回载体
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub contents: Vec<Content>,
    /// None = 足量交付 或 适配器未提供欠交付信息(未迁移平台的滚动兼容语义)
    pub shortfall: Option<FetchShortfall>,
}

// trait 新增默认方法(默认包装既有 fetch_by_keyword,shortfall=None):
async fn fetch_by_keyword_with_outcome(
    &self, keyword: &KeywordType, options: &SearchOptions,
) -> GatewayResult<FetchOutcome> {
    Ok(FetchOutcome { contents: self.fetch_by_keyword(keyword, options).await?, shortfall: None })
}
```

**构造不变量(DR-01,冻结)**:`PartialFailure` ⇒ `contents` 非空;零可交付进展(过滤后为空)的失败一律走 `Err`(F-001 语义),不得构造「空 contents + PartialFailure」的 `FetchOutcome`。`FetchOutcome::partial` 构造器须断言/归一化此不变量(空 contents 时 panic/Err 或归一化为错误——断言形式实现期定,语义固定);RED 载体 = M1-T3 测试 7。

语义:`shortfall=None` 时 orchestrator 行为与现状完全一致(滚动迁移:M3~M5 未接通前各平台不回归)。M2~M5 各平台适配器 override 此方法。

### D2 翻页状态机(R-009;FR-001 ② 纯逻辑与 async I/O 解耦)

新文件 `src/pagination.rs`(facebook.rs:522-644 既有循环为语义样板,M1 **不改** facebook.rs;M2 接通):

```rust
pub const MAX_EMPTY_PAGES: u32 = 3;          // 对齐 facebook.rs:28 MAX_EMPTY_CURSOR_HOPS

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    ReachedMaxCount,        // 达量
    UpstreamExhausted,      // next_cursor 缺失(适配器把 has_more=false 归一化为 None)
    CursorLoop,             // 重复 cursor(F-004)
    EmptyPageLimit,         // 连续空页达上限(F-005)
}

#[derive(Debug, PartialEq, Eq)]
pub enum PageDecision {
    Continue { cursor: String },
    Stop(StopReason),
}

pub struct PaginationLoop { /* max_count, seen_ids, seen_cursors, accepted, empty_streak */ }

impl PaginationLoop {
    pub fn new(max_count: usize) -> Self;
    /// 喂入一页(条目 id 列表 + 下一页 cursor),返回「新接受的 id 子集」与决策。
    /// 去重(I-003):seen id 不重复接受;接受数严格 ≤ max_count(I-001,页内截断)。
    pub fn accept_page(&mut self, item_ids: &[String], next_cursor: Option<String>) -> PageOutcome;
    pub fn accepted_count(&self) -> usize;
    /// 终止原因 → 欠交付原因映射(I-004 接线;达量→None;枯竭族 且 accepted<max → Exhausted)
    pub fn shortfall_for(&self, stop: &StopReason) -> Option<FetchShortfall>;
}

pub struct PageOutcome {
    pub newly_accepted: Vec<String>,
    pub decision: PageDecision,
}
```

**`empty_streak` 语义(DR-03,冻结)**:`empty_streak` 按 `newly_accepted.is_empty()` 递增(**非**按原始 item_ids 长度)——重复内容页连发与字面空页同等计入,保证页数上界 ≈ ⌈max_count/页⌉ + MAX_EMPTY_PAGES(活性,B4)。钉子测试 = M1-T2 单测 7。

**同页多信号优先序(SM#F8,冻结)**:`ReachedMaxCount` 优先于一切枯竭信号——同页同时达量且 cursor 缺失/重复/空页时取 `ReachedMaxCount`(而非 `UpstreamExhausted`/`CursorLoop`/`EmptyPageLimit`)。终态级等价由 PT-4(T-033)合取保证:达量序列下 `shortfall_for` 永不产出 `Exhausted`,本注消除 T2.8 对齐测试输入歧义。

适配器侧用法(M2~M5 消费契约):async 循环里每拿到一页调 `accept_page`,按 `newly_accepted` 收集内容;`Continue{cursor}` 才发下一请求;循环内捕获可恢复错误(**= 仅 `GatewayError::RateLimited` 且已有进展,见下方 DR-10 冻结**)时按 `FetchShortfall::PartialFailure` 收尾(facebook.rs:569-576 先例语义)。

**PartialFailure 触发集冻结(DR-10)**:**PartialFailure 触发集 = `GatewayError::RateLimited`(与 facebook.rs:569-576 先例一致);其余错误即使有进展也整体 `Err`**(已收集未落库内容丢弃属预期,I-005 仅保护已落库进展)。各平台不得扩大此集合;扩大须经 root 修订。钉子测试 = M1-T3 测试 6。

> 注(DR-03/D-15):facebook 既有循环(facebook.rs:546-558,空页按原始条数计)的同形状对齐归 M2 —— D-15 已裁决改生产行(空页计数改为「本页新增(去重后)数 == 0」)。

### D3 orchestrator 终态映射(R-007/R-008,T-002)

`fetch_content` 改用 `fetch_by_keyword_with_outcome`;`process_keyword` 按 shortfall 设置 `KeywordProcessOutcome.terminal_hint`:

| fetch 结果 | 终态(经既有 L398-431 聚合) | 关联 |
|---|---|---|
| contents 非空 + shortfall=None | `COMPLETED`(现状不变) | T-002 |
| contents 非空 + Exhausted | `NO_MORE_POSSIBLE_DATA`(terminal_hint) | F-003, I-004 |
| contents 非空 + PartialFailure | `COMPLETED_WITH_PARTIAL_ERRORS`(terminal_hint,message 经 `TaskTerminalReason` 构造器脱敏) | F-002, I-005, I-009 |
| contents 空 + (Exhausted 或 None) | 既有零结果路径不变:`stop_campaign_gracefully` + `no_more_possible_data`(orchestrator.rs:470-513) | 现状回归保护 |
| fetch 返回 Err(零进展) | 既有失败路径不变:`fail_task` + `terminal_reason_for_error`(F-001;保留 eval_once failed 一次重派语义(每 tick 一次、跨 tick 无上限,DR-22),A007) | R-008 |
| contents 空 + PartialFailure | **不可达**(D1 构造不变量保证,DR-01);防御性处理 = 按零进展错误路径 `fail_task`(若违例出现;载体 = M1-T4 测试 9) | DR-01, F-001 |

多 keyword 聚合更正(DR-11):现状实为 **last-Some-wins**(None 不覆盖 Some,orchestrator.rs:384-385 实证),非 last-wins。**聚合优先级规则(新增,B2)**:多 keyword 混合 shortfall 时 **PartialFailure > Exhausted**(部分失败不得被枯竭标签掩盖)——实现于 M1-T4(terminal_hint 合并处),钉子测试 = M1-T4 测试 7。
**设计裁决(给 root/评审)**:contents 非空 + Exhausted 时 agent **不**调用 `stop_campaign_gracefully`,只记 terminal_reason —— ONCE campaign 的完结理由区分由 scheduler M6 消费 terminal_reason 实现(C-004);避免 agent 越权改 campaign 状态。

### D4 redis 字段语义(R-011 agent 侧,C-001/C-002)

- `src/domain/entities.rs` `TaskConfig` 新增内部字段 `page_size_hint: Option<u32>`(`#[serde(default)]`;**内部域模型,非协议形状**——`protocol_gen::TaskConfig` 零改动,R-012 合规)。
- `src/adapters/redis.rs` `to_domain_task_config`:`search_limit >= 1` → `page_size_hint = Some(min(search_limit, platform_page_cap))`;`search_limit <= 0` → `None`(平台默认页大小)。`search_offset` 任何路径不读(I-008)。
- `src/pagination.rs` 提供 `pub fn platform_page_cap(platform: &str) -> u32`,取值来源 = 各 strategy 现行 cap 与上游硬约束:facebook 20(strategies/facebook.rs:131)、tiktok 20(TikHub 硬上限,tikhub/types.rs:297)、reddit 100(reddit.rs:100)、twitter 100(twitter.rs:123)、instagram 50(instagram.rs:99)、未知平台 20(保守)。M2~M5 若有上游证据可在各自模块修订(经 root)。
- `page_size_hint` 的消费(strategy `build_search_options` 用作单页 count)归 M2~M5;M1 只交付字段+clamp+注释。[^f08]

[^f08]: **脚注 F-08**:消费名单以 root §1 D4(D-02 修订)为准:仅 tiktok 经 `extra_keys::PAGE_SIZE` 下发;fb/reddit/twitter/ig 文档化豁免。

### D5 状态面(step-04 action 8:一写多读)

| 状态键 | 唯一写方 | 声明读方 |
|---|---|---|
| `TaskConfig.page_size_hint` | `redis.rs::to_domain_task_config`(M1) | 各平台 strategy(M2~M5)[^f08] |
| `FetchOutcome.shortfall` | 各平台适配器(M2~M5;默认方法=None) | `orchestrator::process_keyword`(M1) |
| `gm_crawler_tasks.terminal_reason` | agent postgres adapter(既有,值集不变,C-004) | scheduler eval_once(M6 新增读方,容忍 NULL/未知值) |
| `PaginationLoop` 内部 seen_ids/seen_cursors | 状态机自身(M1) | 不出模块(A005:不过 trait) |

## 3. 任务清单

> 执行序 = 编号序(T0 为 live-gate 测试基建前置,D-14①;T1 为 FR-001 前置;T2→T3→T4 有类型依赖;T5/T6 可与 T4 并行;T7 收尾)。
> 每任务 1-3 文件、单个全新实现者上下文可完成;「测试载荷」与「实现载荷」分上下文(AG-004)。
> 所有测试均为确定性测试(mock / 纯逻辑),**不需要任何 live 凭据**;real gates(T-050~T-054)归 M2~M5。

---

### M1-T0 live 测试 env/CI 守卫对齐(原 root RT-3 提前,D-14①)

**覆盖 ID**:D-10、D-14①、DR-07、DR-08。依赖:无(必须先于 M1-T7 变异预检;建议最先执行)。
**文件**:`tests/facebook_real_api_test.rs`、`tests/real_api_test.rs`、`tests/twitter_real_api_test.rs`(仓内实存,一并)。

**改动(只加 gate 前置,零断言改动)**:
1. 增加与 `tests/facebook_real_db_test.rs:271-289` 同构守卫:凭据未设 → 显式 skip + `eprintln!`(替代现状「凭据未设即 panic-fail」,P-001 既述行为对齐)。
2. **并增加 `GITHUB_ACTIONS && !RUN_REAL_API_TESTS → skip`**(D-14①:CI 恒有凭据,仅 env-skip 在 CI 永不触发;此条款使 mutation/常规 CI 默认不打 live,opt-in 显式开启)。
3. **零断言改动**:任何断言触碰即违规(AG-002;本任务只允许在测试函数入口加 gate 前置返回)。

**RED/GREEN 说明**:基建型(无新断言),以前后输出对照为证据 —— 改动前:无凭据环境 `cargo test --test facebook_real_api_test` panic/fail 输出;改动后:同环境全 skip(eprintln 留痕)+ 退出码 0。两份输出均留存入任务完成报告。

**验收**:
1. 无凭据环境 `cargo test --test facebook_real_api_test`(及 `real_api_test`、`twitter_real_api_test`)全 skip 不 panic;
2. 有凭据环境(且非 CI 或已设 `RUN_REAL_API_TESTS`)行为不变;
3. diff 审查:零断言行变更(只允许 gate 前置 + eprintln)。

**反作弊声明**:本任务是 gate 守卫对齐,不是测试弱化;不得借机改动/删除任何断言、不得新增 `#[ignore]`。

---

### M1-T1 proptest 基建引入(FR-001 前置)

**覆盖 ID**:FR-001(①③④)、AG-020(基建)、I-009(属性形态补强)、B5。
**文件**:`Cargo.toml`(dev-dep)、`src/ports/progress_tracker.rs`(测试模块内新增冒烟属性测试)。

**RED 测试(测试子 agent 先写)** — `src/ports/progress_tracker.rs` `#[cfg(test)]` 内:

```rust
use proptest::prelude::*;

proptest! {
    /// 冒烟属性:任意输入下 redaction 输出有界且敏感值不泄漏(I-009 既有逻辑的属性化)
    #[test]
    fn prop_redaction_bounded_and_no_secret_leak(
        prefix in ".{0,80}", secret in "[A-Za-z0-9_-]{8,40}", suffix in ".{0,80}"
    ) {
        let msg = format!("{prefix} api_key={secret} {suffix}");
        let reason = TaskTerminalReason::provider_failure(&msg);
        prop_assert!(reason.message.chars().count() <= 500);
        prop_assert!(!reason.message.contains(&secret));
    }
}
```

**预期 RED 失败信息**:依赖未引入,`cargo test --lib prop_redaction` 编译失败:
`error[E0432]: unresolved import proptest`(或 `use of undeclared crate or module 'proptest'`)。
这是本任务「正确的失败原因」:被引入物 = 依赖本身;属性断言的有效性由 AG-012 变异预检兜底(redact 函数被变异时本测试须变红)。

**GREEN(实现子 agent)**:`Cargo.toml` `[dev-dependencies]` 加 `proptest = "1"`(实现时核对 crates.io 最新 1.x;不锁 patch 版本,FR-001 注)。命令:`cargo test --lib prop_redaction_bounded_and_no_secret_leak`。

**最终验收命令**:`cargo test --lib prop_redaction_bounded_and_no_secret_leak -- --nocapture`(通过)+ `cargo build --all-features`(无破坏)。

**附加要求(FR-001 ③④)**:
- 若本套件或后续 PT 套件在 AG-012 预检(单变异 timeout 300s)下超时,允许对**具体测试**用 `ProptestConfig { cases: 64, .. }` 显式下调并在代码注释 + 任务完成报告写明理由;**不得以下调为名删/弱断言**。
- proptest 首次失败会生成 `proptest-regressions/` 种子文件:**必须随 PR 入库**(已核实 `.gitignore` 无排除项);不得删除已入库种子。

**反作弊声明**:实现者不得修改断言来过测试;失败只能改生产代码/依赖配置或停下上报。

---

### M1-T2 翻页核心状态机 + 确定性单测 + PT-1~PT-4(T-030~T-033)

**覆盖 ID**:R-009、I-001、I-002、I-003、I-004(机制层)、F-004/F-005(机制)、T-030、T-031、T-032、T-033、AG-020~AG-023、A011(语境)。依赖:M1-T1。
**文件**:`src/pagination.rs`(新建:实现 + `#[cfg(test)]` 单测 + proptest 套件)、`src/lib.rs`(挂模块,一行)。

**测试载荷(测试子 agent 先写;同时落「类型骨架」= D2 全部公开类型签名 + 方法体 `todo!()`,使测试可编译)**:

确定性单测(各断言即契约):
1. `reaches_max_count_and_stops`:max=50,喂 3 页 id(20+20+20,无重复)→ 第 3 页 `newly_accepted.len()==10`、`accepted_count()==50`、decision==`Stop(ReachedMaxCount)`、`shortfall_for(..)==None`。
2. `upstream_exhausted_when_cursor_missing`:max=50,2 页(20+10)第 2 页 `next_cursor=None` → `Stop(UpstreamExhausted)`、`shortfall_for==Some(Exhausted)`、`accepted_count()==30`。
3. `cursor_loop_detected`:第 2 页返回与第 1 页相同 cursor → `Stop(CursorLoop)`、shortfall==`Some(Exhausted)`(F-004,按枯竭语义)。
4. `empty_page_limit`:连续 3 个空页(cursor 各异)→ 第 3 空页 `Stop(EmptyPageLimit)`(F-005,MAX_EMPTY_PAGES=3 对齐 facebook 样板);中间插入非空页则计数复位(第 4 页空不触发)。
5. `duplicate_ids_not_double_counted`:两页含相同 id 集 → 第 2 页 `newly_accepted` 为空、`accepted_count()` 不变(I-003)。
6. `partial_failure_mapping`:`shortfall_for` 不接受 PartialFailure 类 StopReason(枚举不含之);适配器错误路径映射由 M1-T3 的 `FetchOutcome::partial` 构造器测试覆盖 —— 本测试断言 `shortfall_for(&ReachedMaxCount)==None` 与三个枯竭族 → `Some(Exhausted)` 的完整表。
7. `repeated_content_pages_stop_at_empty_limit`(DR-03):max=50,3 连页内容与第 1 页相同(cursor 各异)→ 第 3 重复页 `Stop(EmptyPageLimit)`(`empty_streak` 按 `newly_accepted.is_empty()` 计,重复内容页 = 空进展页,D2 冻结语义)。
8. `combined_signal_prefers_reached_max`(SM#F8):max=20,喂 1 页 20 条唯一 id + `next_cursor=None`(末页恰好达量)→ `decision==Stop(ReachedMaxCount)`、`shortfall_for(..)==None`。**预期 RED**:实现若取 `UpstreamExhausted`(先检 cursor 缺失再检达量),则 `assertion 'left == right' failed: left: Stop(UpstreamExhausted), right: Stop(ReachedMaxCount)`;达量优先序须在 `accept_page` 内显式实现(SM#F8 冻结语义)。

proptest 套件(生成器要点按 AG-020~AG-023;页序列建模 `Vec<(Vec<String>, Option<String>)>`):
- **T-030/PT-1**:`max_count ∈ 1..=200`,页数 `0..=20`,每页条数 `0..=25`,id 取 `[a-d][0-9]{0,2}` 小空间 → 驱动循环至 Stop,`prop_assert!(loop.accepted_count() <= max_count)`。
- **T-031/PT-2(Step 07 DR-02 改写为可满足形式)**:cursor 取 `[a-c]{1,2}` 小字母表(高概率制造环)+ 空页连发。**harness 规范(冻结)**:驱动器逐页喂入注入序列,产生 `Stop` 即停;**注入页耗尽 = 上游枯竭,驱动器自动补一页 `(空, None)`**。断言:① `accept_page` 调用次数 ≤ 序列长度 + 1;② 必在 `序列长度 + 1` 步内产生 `Stop(_)`;③ StopReason ∈ {ReachedMaxCount, UpstreamExhausted, CursorLoop, EmptyPageLimit}(枚举完备性断言)。
- **T-032/PT-3**:id 小空间制造碰撞 → `accepted_count == min(Stop 前实际被喂入页中的唯一 id 数, max_count)`——harness 与 PT-2 同款:产生 Stop 即停,Stop 后页不喂入、不计入唯一 id 基数。本属性形式为计划期修正(Step 07 质量审 Issue 2),非执行期断言变更;修正原因 = DR-03 冻结后小 id 空间高碰撞序列会合法地提前 Stop(EmptyPageLimit)。
- **T-033/PT-4(双向)**:`shortfall_for(stop)==Some(Exhausted)` **当且仅当** stop ∈ {UpstreamExhausted, CursorLoop, EmptyPageLimit} 且 `accepted < max_count`;达量序列(生成器保证唯一 id ≥ max_count 且 cursor 链足够)下永不产出 Exhausted。

**预期 RED 失败信息**(骨架 `todo!()` 下):每个测试 `panicked at src/pagination.rs:<line>: not yet implemented`;实现中途的断言型失败示例:`assertion 'left == right' failed: left: 0, right: 50`(单测 1)、`assertion 'left == right' failed: left: Continue { cursor: "c3" }, right: Stop(EmptyPageLimit)`(单测 7,若 `empty_streak` 误按原始条数计则重复内容页永不 Stop)、`Test failed: assertion failed: loop.accepted_count() <= max_count; minimal failing input: ...`(PT-1)、`Test failed: assertion failed: steps <= seq.len() + 1 && matches!(last_decision, Stop(_)); minimal failing input: ...`(PT-2,harness 补 `(空, None)` 后仍未 Stop 或超步数)。RED 证据取「全部 9+ 条测试失败」的 `cargo test pagination` 输出。

**GREEN 命令**:`cargo test --lib pagination`(单测 + PT 全绿)。
**最终验收命令**:`cargo test --lib pagination -- --nocapture` + 留存 `proptest-regressions/`(若生成)入库。

**反作弊声明**:实现者不得修改断言/生成器范围来过测试(收窄生成器 = 改弱断言,同等禁止);PT 失败的最小反例须在完成报告中说明修复方式。本 PT-2 属性形式为计划期修正(Step 07 DR-02,删除原「任意有限序列必在序列长度步内产生 Stop」的不可满足措辞),非执行期断言变更。

---

### M1-T3 ContentGateway 欠交付契约 + MockContentGateway 分页注入(FR-003 载体)

**覆盖 ID**:R-007(契约面)、R-001(解耦机制接口形状语境)、A005(drop 裁决落地)、FR-003、I-004(接线)、F-003(机制)、DR-01(构造不变量)、DR-10(触发集冻结)。依赖:M1-T2(类型)。
**文件**:`src/ports/content_gateway.rs`(D1 类型 + 默认方法)、`src/testing/mock_gateway.rs`(分页注入)。

**测试载荷(测试子 agent 先写;含 D1 类型骨架使编译通过)**:

1. `default_method_preserves_legacy_semantics`(content_gateway.rs 测试模块):用一个仅实现既有 4 方法的最小测试 gateway 调 `fetch_by_keyword_with_outcome` → `outcome.shortfall.is_none()` 且 contents 与 `fetch_by_keyword` 一致。**允许先绿**(契约文档型,AG-006:AG-012 预检中默认方法被变异须由本测试抓住)。
2. `mock_gateway_paged_injection_exhausted`(mock_gateway.rs 测试模块):新增注入 API `add_search_pages(keyword, pages: Vec<Vec<Content>>)`(页间隐式 cursor 由 mock 内部生成;mock override `fetch_by_keyword_with_outcome`,**内部用 `PaginationLoop` 驱动** —— 共享构造 dogfooding,AG-005 合规:mock 的是上游数据,不是状态机)。**legacy 桥接(冻结)**:`add_search_pages` 同时使 legacy `fetch_by_keyword` 返回各页 flatten 后截断到 `options.count` 的结果(保证 RED 期 orchestrator 走旧方法时行为可预言)。注入 2 页(20+10)、`options.count=50` → `contents.len()==30`、`shortfall==Some(FetchShortfall::Exhausted)`。
3. `mock_gateway_paged_injection_reaches_count`:注入 3 页(20+20+20)、count=50 → `contents.len()==50`、`shortfall==None`。
4. `mock_gateway_page_failure_with_progress`:新增 `set_page_error_at(page_idx, MockError)`;第 2 页注入 RateLimit、第 1 页已有 20 条 → `shortfall==Some(PartialFailure{..})` 且 message 含 `"rate"`(大小写不敏感);第 1 页即错(零进展)→ 整体 `Err(GatewayError::RateLimited{..})`(F-001 语义:零进展走错误路径)。
5. `fetch_outcome_constructors`:`FetchOutcome::complete/exhausted/partial` 三构造器字段断言(partial 携带 message)。
6. `non_rate_limited_error_with_progress_is_err`(DR-10):`set_page_error_at` 第 2 页注入**非 429** 错误(provider/network 类 MockError)、第 1 页已有 20 条 → 整体 `Err`(**不得**收敛为 PartialFailure;触发集冻结 = 仅 `GatewayError::RateLimited`)。
7. `partial_constructor_rejects_empty_contents`(DR-01):空 contents + partial → 构造器 panic/Err 或归一化为错误(断言形式实现期定但语义固定:违例形状「空 contents + Some(PartialFailure)」不得作为合法 `FetchOutcome` 产出)。
8. `legacy_fetch_sees_flattened_pages`(legacy 桥接冻结):注入 2 页(20+10)、count=50 → legacy `fetch_by_keyword` 返回 30 条;count=25 → 25 条。预期 RED(桥接未实现时 legacy 返回空/独立 map):`assertion 'left == right' failed: left: 0, right: 30`。

**预期 RED 失败信息**:
- 注入 API 未实现(骨架 `todo!()`):`panicked at src/testing/mock_gateway.rs:<line>: not yet implemented`;
- mock 未 override 默认方法时测试 2 的断言失败:`assertion 'left == right' failed: left: None, right: Some(Exhausted)`;
- 测试 4 零进展分支:`assertion failed: matches!(result, Err(GatewayError::RateLimited { .. }))`;
- 测试 6(DR-10):实现若把任意错误+进展收敛为 partial → `assertion failed: matches!(result, Err(_)); got Ok(FetchOutcome { contents: [..20..], shortfall: Some(PartialFailure { .. }) })`;
- 测试 7(DR-01):构造器无校验时 → `assertion failed: constructor rejects/normalizes empty contents; got FetchOutcome { contents: [], shortfall: Some(PartialFailure { .. }) }`(以实跑「构造器静默产出违例形状」的断言失败输出为准记录)。

**GREEN 命令**:`cargo test --lib content_gateway && cargo test --lib mock_gateway`。
**最终验收命令**:同上 + `cargo build --all-features`(确认既有 5 个平台适配器零改动仍编译——默认方法不破坏)。

**反作弊声明**:实现者不得修改断言来过测试;不得在生产 gateway 代码中特判 mock 输入。

---

### M1-T4 orchestrator 终态映射 + 页失败语义 + 脱敏(T-002、T-004)

**覆盖 ID**:R-007、R-008、T-002、T-004、I-004、I-005、I-009、F-001/F-002(语义层)、F-003(映射)、A007、B2、DR-11(聚合优先级)、D-13(任务级 max_count)、DR-01(防御性路径)。依赖:M1-T3。
**文件**:`src/orchestrator.rs`(`fetch_content`/`process_keyword`/终态聚合 + 测试模块)。

**测试载荷(测试子 agent 先写;orchestrator.rs `#[cfg(test)]` 内,复用既有 MockRepository/MockCommentGateway/MockAiAnalyzer 装配样板,gateway 用 M1-T3 的分页注入 MockContentGateway)**:

1. `exhausted_maps_to_no_more_possible_data`(T-002a / F-003):mock 注入 2 页(20+10)、max_videos=50 → `process_task` 后经 `progress_tracker.get_task(id)` 断言 `terminal_reason` 以 `"NO_MORE_POSSIBLE_DATA"` 开头,且 task 为 completed、30 条进入处理管道(`contents_processed==30`)。**并断言未调用 `stop_campaign_gracefully`**(MockRepository 调用计数;D3 设计裁决)。
2. `partial_failure_maps_to_completed_with_partial_errors`(T-002b / F-002 / I-005):第 2 页注入失败、第 1 页 20 条 → terminal_reason 以 `"COMPLETED_WITH_PARTIAL_ERRORS"` 开头、task completed、`contents_processed==20`(已落进展保留)。
3. `full_delivery_maps_to_completed`(T-002c):3 页喂满 50 → terminal_reason 以 `"COMPLETED"` 开头且**不含** `"PARTIAL"`、不含 `"NO_MORE"`。
4. `zero_progress_failure_maps_to_failed`(F-001 / R-008):第 1 页即不可恢复错误 → task failed、terminal_reason 以 `"PROVIDER_FAILURE"` 开头(既有路径回归保护;**允许先绿**,AG-006 变异证明)。
5. `partial_failure_message_is_redacted`(T-004 / I-009):页失败注入 message 含 `api_key=sk-test-secret-123` → terminal_reason 含 `"[REDACTED]"`、不含 `"sk-test-secret-123"`(新 partial 路径必须走 `TaskTerminalReason` 构造器,不得手拼字符串绕过脱敏)。
6. `empty_first_page_keeps_campaign_stop_behavior`(现状回归):零结果搜索 → `stop_campaign_gracefully` 被调用、terminal_reason 为 NO_MORE_POSSIBLE_DATA(**允许先绿**,AG-006)。
7. `mixed_shortfall_partial_wins_over_exhausted`(DR-11):K=2,kw1=PartialFailure、kw2=Exhausted → terminal_reason 以 `"COMPLETED_WITH_PARTIAL_ERRORS"` 开头(D3 聚合优先级:PartialFailure > Exhausted,B2)。
8. `two_keywords_share_task_level_max_count`(D-13):K=2、max_videos=50、kw1 注入可给 30、kw2 注入 ≥30 → 任务总处理 50、kw2 实取 20(`contents_processed==50`;remaining 跨 keyword 传递)。
9. `defensive_empty_contents_partial_failure_fails_task`(DR-01b 防御):以字面构造(绕过 `FetchOutcome::partial` 构造器)注入「空 contents + Some(PartialFailure)」违例形状 → task failed(`fail_task` 零进展错误路径),terminal_reason **不**以 `"COMPLETED_WITH_PARTIAL_ERRORS"` 开头。注:该形状经 D1 构造不变量在构造层拦截(M1-T3 测试 7 为其载体),本测试是 orchestrator 侧的防御性兜底(D3 第六行)。

**预期 RED 失败信息**(orchestrator 仍走旧 `fetch_by_keyword`、无 shortfall 消费时):(RED 预言依赖 M1-T3 legacy 桥接已落地——orchestrator RED 期走旧 `fetch_by_keyword`)
- 测试 1:`assertion failed: reason.starts_with("NO_MORE_POSSIBLE_DATA"), got "COMPLETED: Task completed successfully"`;
- 测试 2:`assertion failed: reason.starts_with("COMPLETED_WITH_PARTIAL_ERRORS"), got "COMPLETED: ..."`(或零进展旧语义下 got `"PROVIDER_FAILURE: ..."`,以实跑输出为准记录);
- 测试 5:`assertion failed: reason.contains("[REDACTED]")`;
- 测试 7(DR-11):初始 RED(orchestrator 未消费 shortfall):got `"COMPLETED: ..."`;映射接通后、优先级未实现的中途态:got `"NO_MORE_POSSIBLE_DATA: ..."`(last-Some-wins 下 kw2 的 Exhausted 覆盖);以实跑输出为准记录;
- 测试 8(D-13):现状每 keyword 独立 count=50 → 总 60:`assertion 'left == right' failed: left: 60, right: 50`;
- 测试 9(DR-01b):无防御分支时违例形状被当作正常 partial → `assertion failed: task is failed; got completed with "COMPLETED_WITH_PARTIAL_ERRORS: ..."`。

**GREEN(实现子 agent)**:按 D3 表实现 —— `fetch_content` 返回 `FetchOutcome`;`process_keyword` 把 shortfall 映射进 `KeywordProcessOutcome.terminal_hint`(Exhausted→`TaskTerminalReason::no_more_possible_data()`;PartialFailure→`TaskTerminalReason::completed_with_partial_errors(message)`)。
**任务级 max_count 语义(D-13)**:`process_keyword` 循环间传递 remaining = `max_videos − 已累计 contents`,后续 keyword 的 `options.count = remaining`,remaining ≤ 0 时跳过剩余 keyword——任务总处理数 ≤ max_count(I-001 任务级)。
**terminal_hint 合并优先级(DR-11)**:terminal_hint 合并处实现 PartialFailure > Exhausted(D3 聚合优先级规则;None 仍不覆盖 Some)。
命令:`cargo test --lib orchestrator`。
**最终验收命令**:`cargo test --lib orchestrator -- --nocapture` + `cargo test`(全套件无回归)。

**反作弊声明**:实现者不得修改断言来过测试;尤其不得通过让 mock 返回「恰好 50 条」来绕开映射逻辑(AG-003 特判禁令)。

---

### M1-T5 redis 字段语义:search_limit 页提示 + clamp、search_offset 行为等价(T-003、T-034/PT-5)

**覆盖 ID**:R-011(agent 侧)、I-008、T-003、T-034、AG-024、C-001、C-002(agent 侧)、P-006、N-002(agent 侧)、A006。依赖:M1-T1(proptest);与 M1-T4 无依赖,可并行。
**文件**:`src/domain/entities.rs`(`page_size_hint` 字段)、`src/adapters/redis.rs`(映射 + clamp + N-002 注释 + 测试)、`src/pagination.rs`(`platform_page_cap`,若 M1-T2 未含则此处补)。

**测试载荷(测试子 agent 先写)** — redis.rs 测试模块(沿既有 task JSON 样板,redis.rs:709 形状):

1. `search_limit_becomes_page_size_hint`(T-003a):tiktok task JSON `search_limit=7` → `to_domain_task_config(...).page_size_hint == Some(7)`。
2. `search_limit_clamped_to_platform_cap`(T-003b):tiktok `search_limit=500` → `Some(20)`;facebook 500 → `Some(20)`;reddit 500 → `Some(100)`;twitter 500 → `Some(100)`;instagram 500 → `Some(50)`(D4 取值表,5 平台逐一断言)。
3. `non_positive_search_limit_falls_back_to_none`(T-003c):`search_limit=0` 与 `-3` → `page_size_hint == None`(老 task / 缺省语义,C-001 向后兼容)。
4. `prop_search_offset_never_changes_task_config`(T-034/PT-5,proptest):`offset in any::<i32>()`,同一 task JSON 仅 search_offset 不同 → 两次 `to_domain_task_config` 的 `serde_json::to_value(..)` 全等(行为等价;TaskConfig 无 PartialEq,以 JSON 全等代理)。**允许先绿**(现状已忽略 offset;本测试是 I-008 的回归钉子,AG-006:AG-012 预检中映射函数被注入「读 offset」类变异时须变红 —— 若 mutants 不生成该类变异,以「测试 3+4 联合钉死字段语义」为书面豁免记录)。
5. `platform_page_cap_table`(pagination.rs 测试):5 平台 + 未知平台 `"unknown" → 20` 的完整表驱动断言。

**预期 RED 失败信息**:
- `page_size_hint` 字段不存在 → `error[E0609]: no field 'page_size_hint' on type TaskConfig`(骨架补字段后):测试 1 `assertion 'left == right' failed: left: None, right: Some(7)`;
- 测试 2 clamp 未实现:`left: Some(500), right: Some(20)`;
- 测试 5:`panicked at 'not yet implemented'`(骨架)。

**GREEN(实现子 agent)**:实现 D4;并在 redis.rs 映射处与 entities.rs 字段上落 **N-002 agent 侧注释**(措辞与 C-002 一致):
```
/// 跨服务契约(C-002):`search_limit` = 单页大小提示(clamp 到平台页上限,见 platform_page_cap);
/// `search_offset` = 仅观测字段,agent 不读、不参与取数。两字段形状不变、不删(scheduler 写方:lib.rs dispatch_task)。
```
命令:`cargo test --lib redis && cargo test --lib pagination::tests::platform_page_cap_table`。
**最终验收命令**:`cargo test --lib redis -- --nocapture` + diff 自查:`src/protocol_gen/` 零改动(R-012)。

**反作弊声明**:实现者不得修改断言来过测试;clamp 表期望值如与上游证据冲突,停下上报(走 root 修订),不得改测试凑数。

---

### M1-T6 C-004 agent 侧契约 pin:TaskTerminalReason 序列化字符串不可重命名

**覆盖 ID**:C-004(agent 侧)、AG-006、契约不变约束 §3.2。依赖:无(可最先并行)。
**文件**:`src/ports/progress_tracker.rs`(测试模块)。

**测试载荷(测试子 agent 先写)**:

```rust
/// C-004 契约 pin:scheduler(M6)将按字符串读取 terminal_reason;以下 6 个 code 字符串
/// 自本计划起为跨服务契约,不得重命名(cross-service-contracts.md C-004/§3.2)。
#[test]
fn task_terminal_reason_codes_are_cross_service_contract() {
    assert_eq!(TaskTerminalReason::completed().code, "COMPLETED");
    assert_eq!(TaskTerminalReason::completed_with_partial_errors("e").code, "COMPLETED_WITH_PARTIAL_ERRORS");
    assert_eq!(TaskTerminalReason::no_more_possible_data().code, "NO_MORE_POSSIBLE_DATA");
    assert_eq!(TaskTerminalReason::provider_failure("e").code, "PROVIDER_FAILURE");
    assert_eq!(TaskTerminalReason::cancelled("m").code, "CANCELLED");
    assert_eq!(TaskTerminalReason::internal_error("e").code, "INTERNAL_ERROR");
    // M6 读方解析依赖 "CODE: message" 形状(as_terminal_message)
    assert_eq!(TaskTerminalReason::no_more_possible_data().as_terminal_message()
        .split(':').next().unwrap(), "NO_MORE_POSSIBLE_DATA");
}
```

**RED/GREEN 说明**:本测试**允许先绿**(契约钉子;现状即正确)。AG-006 有效性证明:AG-012 预检若触及 progress_tracker.rs(M1-T1 会触及该文件)则由 mutants 验证;若 diff 未覆盖 code 字符串行,以「一次性手工金丝雀」补证:临时将 `"NO_MORE_POSSIBLE_DATA"` 改为 `"X"` 跑测试确认变红(输出留存),再还原 —— 该金丝雀仅动生产字符串、不动断言,无需 ASSERTION-CHANGE 标记。
**GREEN/验收命令**:`cargo test --lib task_terminal_reason_codes_are_cross_service_contract`。

**反作弊声明**:实现者不得修改断言;若未来确需改值,属跨仓契约变更,须经 root + Cross-Service Reviewer。

---

### M1-T7 模块收尾 gate:变异预检 + R-012/F-009 自查 + 全量回归

**覆盖 ID**:AG-010、AG-011、AG-012、R-012(模块侧自查)、F-009(兼容自查)、AG-007、FR-005(引用:无新框架待办)、B4。依赖:M1-T1~T6 全部完成。
**文件**:无新生产代码(只跑命令 + 写证据;若预检发现 missed mutants,修复归对应任务的生产代码/补测试,不得弱化断言)。

**命令序列(全部输出留存为模块完成证据)**:

```bash
# 1. 全量确定性回归(不需凭据;live gated 测试自动 skip —— M1-T0 落地后成立;预检须在 live key 未设环境执行(D-14))
cargo test

# 2. 本地变异预检(AG-012;门槛 AG-011:无 missed 或逐个书面豁免)
git diff main...HEAD > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1

# 3. R-012 / F-009 / 契约面 diff 自查(期望全部零命中)
git diff main...HEAD --stat -- migrations/ src/schema.rs src/db/schema.rs src/protocol_gen/
git diff main...HEAD -- src/adapters/postgres.rs   # 期望:零改动(F-009 fallback 路径未触及;
                                                    # 新终态走既有值,postgres.rs:2123-2181 兼容性由 M2 T-054 实证)
```

**验收判据**:
1. `cargo test` 全绿(若 CI 上 live-API 套件红,按 AG-007 区分上游漂移并在报告注明,不得弱化断言)。
2. mutants 无 missed;任何 missed → 修生产代码/补测试断言重跑,或写书面豁免(等价变异论证)入 PR 描述。
3. 三条 diff 自查零命中;`page_size_hint` 仅存在于 domain 实体(非协议)。
4. 每任务的 RED→GREEN 证据齐备(失败输出 + 通过输出各一份)。
5. 提交/PR 前跑 `rust-verify-change` 流程(项目级守卫)。

**反作弊声明**:本任务不得以任何形式(skip、删测试、放宽断言)使 gate 变绿;gate 红 = 回到对应任务修生产代码。

---

## 4. 模块完成判据与独立验证(03-split §5 M1 行)

- proptest 套件(T-030~T-033 in `src/pagination.rs`、T-034 in `src/adapters/redis.rs`)+ T-002(orchestrator 三向映射)+ T-003(redis clamp)+ T-004(脱敏)全部 green,且每个非「允许先绿」测试有 RED 证据;「允许先绿」测试(M1-T3.1、M1-T4.4、M1-T4.6、M1-T5.4、M1-T6)有 AG-006 变异/金丝雀证明。
- trait/映射改动经本地变异预检无 missed(M1-T7)。
- 独立验证命令:`cargo test --lib pagination orchestrator redis content_gateway mock_gateway progress_tracker` 与 `git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1`。
- 共享接口(D1~D4)写入本计划 §2,交 root 冻结;M2 起草以 §2 为消费契约。

## 5. 账本覆盖映射(traceability;M1 全部认领 ID → 任务)

| 账本 ID | 认领任务 | 备注 |
|---|---|---|
| R-007 | M1-T3(契约面)+ M1-T4(映射) | 集成证据 T-016 归 M2 |
| R-008 | M1-T4 | 失败注入集成证据 T-015 归 M2 |
| R-009 | M1-T2 | per-platform 终止断言归 T-011~T-014(M3~M5) |
| R-011(agent) | M1-T5 | scheduler 侧注释归 M6 |
| R-012 | M1-T7(模块自查;owner=root) | 终审归 root |
| R-001(解耦机制语境) | M1-T3 + M1-T5(接口形状) | 各平台 cap 行删除归 M2~M5 |
| I-001 / I-002 / I-003 | M1-T2(PT-1/PT-2/PT-3) | 平台实例化证据归 M2~M5 |
| I-004 | M1-T2(PT-4)+ M1-T3 + M1-T4 | F-003 实例证据归 M2(T-016) |
| I-005 | M1-T4 | 集成证据 T-015 归 M2 |
| I-008 | M1-T5(T-003 + PT-5) | — |
| I-009 | M1-T4(T-004)+ M1-T1(属性补强) | — |
| F-001 / F-002 | M1-T4(语义层) | 注入证据归 M2(T-015) |
| F-003 | M1-T2 + M1-T4(机制/映射) | 区分值归 M6(R-013) |
| F-004 / F-005 | M1-T2(机制;PT-2 + 单测 3/4) | 集成样板归 M2(T-017) |
| F-009 | M1-T7(零触及自查) | real-DB 证据归 M2(T-054) |
| T-002 | M1-T4 | — |
| T-003 | M1-T5 | — |
| T-004 | M1-T4 | — |
| T-030 / T-031 / T-032 / T-033 | M1-T2 | — |
| T-034 | M1-T5 | — |
| C-001 | M1-T5(测试 3 向后兼容断言) | scheduler 写方零改动前提 |
| C-002(agent) | M1-T5(clamp 实现 + N-002 注释) | scheduler 侧归 M6 |
| C-004(agent) | M1-T6 | scheduler 读方归 M6 |
| P-006 | M1-T5(确定性映射测试;不改入队/出队 → 维持部分 inapplicability) | 若执行期触及入队/出队代码须升级 real gate 并上报 |
| N-002(agent) | M1-T5 | — |
| FR-001 | M1-T1 | — |
| FR-003 | M1-T3(mock 注入载体,复用既有模式、零新依赖) | — |
| FR-005 | M1-T7(引用:无待办) | — |
| AG-001~AG-007 | §1 全任务继承 + 各任务声明 | — |
| AG-010~AG-012 | M1-T7 | — |
| AG-020~AG-023 | M1-T2 | — |
| AG-024 | M1-T5 | — |
| A005(drop) | §1.10 + M1-T3(trait 无 cursor) | 防漂移裁决 |
| A006 / A007 | M1-T5 / M1-T4(裁决落地) | — |
| B2 / B4 | M1-T4 / M1-T2+T7(语境) | bedrock,无独立任务 |
| D-10 / D-14① / DR-07 / DR-08 | M1-T0(live env/CI 守卫对齐) | 基建型,零断言改动;前后输出对照为证据 |
| DR-01(构造不变量) | M1-T3(测试 7 `partial_constructor_rejects_empty_contents`)+ M1-T4(测试 9 防御兜底) | Step 07 patch;D1 冻结 + D3 第六行 |
| DR-02(PT-2 可满足改写) | M1-T2(PT-2/T-031 改写 + harness 规范) | 计划期修正,非执行期断言变更 |
| DR-03(empty_streak 语义) | M1-T2(单测 7 `repeated_content_pages_stop_at_empty_limit`) | facebook 对齐归 M2(D-15) |
| SM#F8(优先序) | M1-T2(单测 8 `combined_signal_prefers_reached_max`) | ReachedMaxCount 优先;D2 冻结注 |
| DR-10(触发集冻结) | M1-T3(测试 6 `non_rate_limited_error_with_progress_is_err`) | 仅 RateLimited;扩大须经 root |
| DR-11(聚合更正+优先级) | M1-T4(测试 7 `mixed_shortfall_partial_wins_over_exhausted`) | last-Some-wins 更正;PartialFailure > Exhausted |
| D-13(任务级 max_count) | M1-T4(测试 8 `two_keywords_share_task_level_max_count`) | I-001 任务级;remaining 跨 keyword 传递 |
| 冻结语义(legacy 桥接,Step 07 质量审) | M1-T3(测试 8 `legacy_fetch_sees_flattened_pages`) | flatten + 截断到 options.count;RED 50→30 |

## 6. 开放问题 / 提请 root·评审裁决

> **状态更新(2026-06-10)**:本节三条已经用户裁决(功能优先),见 `04-adjudications.md` —— §6.1=D-03(采纳)、§6.2=D-04(冻结)、§6.3=D-05(采纳预案)。Step 06 评审否决须附功能性反证并经用户确认。

1. **D3 裁决复核**:contents 非空 + Exhausted 时不调 `stop_campaign_gracefully`(只记 terminal_reason,campaign 级完结交 M6)。与既有零结果路径(会停 campaign)存在行为差,理由已记于 §2 D3;请 Step 06 Concurrency/Cross-Service Reviewer 确认。
2. **platform_page_cap 取值表**(D4)来源为各 strategy 现行 cap;M2~M5 若有上游证据修订,须经 root 更新冻结表(M1 测试期望值随之走 ASSERTION-CHANGE-JUSTIFIED 流程)。
3. **PT-5 变异豁免预案**(M1-T5.4):若 cargo-mutants 不生成「读 search_offset」类变异,按计划记书面豁免;Test-Gate Reviewer 复核豁免文本。
4. **Step 07 patch 记录**:DR-01/02/03/10/11/D-13 已落(另含 D-14①/DR-07/DR-08 → M1-T0、F-08 脚注),详见 `patches/07-batchA-m1.md`。
