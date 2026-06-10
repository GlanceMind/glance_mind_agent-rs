# Module Plan: M5 `agent-rs/instagram-p2`

> 计划族:`docs/plans/jplan/2026-06-10-pagination-max-scan/`(Step 04 起草,2026-06-10)。仓库:本仓。
> 依赖:M1(D1/D2/D4);**分支选择 gated on V1**(C-005;判定标准 = assumptions.md V1 行)。样板:M2/M3(软依赖)。
> 账本输入:R-001(ig 行)、R-006、V1、T-001(ig 行)/T-014/T-053、PV-005、P-004、C-005、N-001、AG 全局、FR-003、R-012。
> 已裁决输入:D-01(若走翻页分支,必须 PaginationLoop)、D-02(hint 可控性消费)、D-04(cap ig=50)、D-12(V1 维持计划内验证)。

## 0. 模块目标(一句话)

V1 验证(TikHub instagram `general_search` V3/V2 请求端是否接受分页 token)为**首任务**;按判定走两条预起草分支之一:**分支 A** 同构翻页(PaginationLoop + token 转发)或 **分支 B** 单页 + 如实上报 `NO_MORE_POSSIBLE_DATA`(欠量时 shortfall=Exhausted,**不得静默 COMPLETED**——R-006 红线);两分支都先解除 strategy `v.min(50)` 截断(instagram.rs:99);R-012 零形状改动。

## 1. 全模块强制约束

与 M2 §1 同文继承 + M3 §1 两条补充。另:
1. **分支互斥**:执行期按 V1 判定**只实现一条分支**;未选分支的任务标记 NOT-TAKEN 并在完成报告记录判定依据(不算未完成)。
2. **V1 判定写回义务**:判定结论 + 证据(响应/文档引文)必须写回 `ledgers/assumptions.md` V1 行与 `ledgers/cross-service-contracts.md` C-005,否则模块不得标完成。

## 2. 设计

### 2.1 V1 判定标准(assumptions.md V1 行,原文采纳)

带首页 token(V3 `next_max_id`/`rank_token`;V2 `pagination_token`)重发:**非错误且内容异于首页 → 支持翻页(分支 A);4xx 或返回相同首页 → 单页能力(分支 B)**。方法 = (a) N-001 文档核查(https://api.tikhub.io/docs OpenAPI;文档明确即可直接判定)+ (b) T-053 探测(≤4 次调用,P-004)。

### 2.2 分支 A:同构翻页(V1=支持)

循环形态同 M3 §2.2,差异:cursor = V3 `media_grid.next_max_id`(`has_more==Some(false)`/缺失 → None)或 V2 `pagination_token`;token 经 client 层新增请求参数(`max_id`/`pagination_token`,**参数名以 V1 实测/文档为准**);PAGE_SIZE extra:V3/V2 端点无 count 参数(client.rs:710-770 实证)→ hint 豁免注释(D-02,同 fb/reddit/twitter)。V3 主路径 + V2 fallback 的翻页只做主路径(fallback 翻页无账本要求,欠量时如实 Exhausted)。

### 2.3 分支 B:单页 + 如实上报(V1=不支持)

适配器 override `fetch_by_keyword_with_outcome`:单次调用后 `delivered < options.count` → `shortfall = Some(Exhausted)`(上游单页即枯竭语义);达量 → None;零进展错误 → Err。**这是事故反模式的 instagram 版钉子:欠量绝不静默 COMPLETED。**

### 2.4 状态面

| 状态键 | 写方 | 读方 |
|---|---|---|
| `FetchOutcome.shortfall`(instagram) | instagram 适配器 override(T3-A 或 T3-B) | orchestrator(M1-T4) |
| V1 判定记录 | M5-T1(写回 assumptions/C-005) | root 验收、Step 06 评审 |

## 3. 任务清单

> T1(V1,gated)→ T2(strategy,分支无关)→ T3-A **或** T3-B(按判定)→ T4 收尾。

---

### M5-T1 V1 验证:N-001 文档核查 + T-053 探测(首任务,gated)

**覆盖 ID**:V1、N-001、T-053、P-004、C-005、PV-005(探测即 real gate)。
**文件**:`tests/real_api_test.rs`(探测测试)+ 账本写回(`ledgers/assumptions.md` V1 行、`ledgers/cross-service-contracts.md` C-005)。

**Gate 控制**:`TIKHUB_API_KEY`;**≤4 = HTTP 请求上界**(P-004:V3 首页+token 重发、必要时 V2 同对;DR-19:探测用零重试调用,确保「调用数 = 请求数」,预算按 HTTP 请求计);只读无清理。
**步骤与载荷**:
1. N-001:核查 TikHub OpenAPI 文档两端点 query 参数表(截图/引文存档);**文档明确支持或排除 → 直接判定,探测调用数可省至 2**。
2. T-053 探测测试 `real_instagram_general_search_pagination_probe`:V3 首页 → 记录 `next_max_id`/`rank_token`/`has_more` 实测值 → 带 token 重发 → 按 §2.1 标准判定;输出(两页原始 JSON)回灌 `tests/fixtures/instagram/`(PV-005)。
3. 判定结论 + 证据写回两账本(分支 A/B 二选一,显式记录)。

**RED/GREEN 说明**:探测型任务,无 RED→GREEN 语义(它产出**判定事实**而非回归断言);测试断言仅「调用成功 + 判定逻辑可复算」(如实记录两种合法结局);**允许先绿**,书面性质说明交 Test-Gate Reviewer。
**验收命令**:`TIKHUB_API_KEY=… cargo test --test real_api_test real_instagram_general_search_pagination_probe -- --nocapture` + 两账本 diff 含写回。
**反作弊声明**:判定不得「按希望的分支」倾向解读;模糊结果(如 token 接受但内容相同)按标准判为分支 B(保守),并记录原始证据。

---

### M5-T2 strategy 截断解除(T-001 ig 行;分支无关)

**覆盖 ID**:R-006(count 语义前提)、T-001(ig)、D-02 豁免注释。依赖:无(可与 T1 并行)。
**文件**:`src/strategies/instagram.rs`(99 行 + 测试)。

**测试载荷**(镜像 M4-T1 四条形状):max_videos=137 → `count == 137`(RED:`left: 50, right: 137`);max_videos=30 → 30;None → 20(现状缺省,**允许先绿** AG-006);D-02 豁免注释落 build_search_options。
**GREEN**:`map(|v| v as u32)` + 注释。命令:`cargo test --lib strategies::instagram`。**最终验收**:同上 + `cargo build --all-features`。
**反作弊声明**:不得修改断言;不得引入新上限。

---

### M5-T3-A 分支 A:翻页循环(PaginationLoop)+ D1 override(T-014-A;V1=支持时执行)

**覆盖 ID**:R-006(翻页分支)、T-014、PV-005(mock 侧)、I-001~I-004/F-001/F-002/F-004/F-005(ig 实例)、D-01、FR-003。依赖:M5-T1(判定=支持)、M5-T2。
**文件**:`src/adapters/instagram.rs`(循环 + override + 测试)、`src/tikhub/client.rs`(V3 search 加 token 参数;参数名以 V1 实测为准)。

**测试载荷**(mock-HTTP,fixture 形状取自 T-053 回灌样本,禁凭空捏造;镜像 M3-T2 形状):
1. `paginates_tokens_until_count`:多页 token 链 → 达量、shortfall=None;捕获请求断言第 2+ 请求带 token(T-014-A 主断言)。
2. `exhausted_when_has_more_false`/token 缺失 → `Some(Exhausted)`。
3. `repeated_token_stops`(F-004)→ `Some(Exhausted)`。
4. 空页上限(F-005)/ partial(F-002)/ 零进展 Err(F-001,**允许先绿** AG-006)/ 达量 None / 既有 `search()` 回归(**允许先绿** AG-006)。**DR-19**:partial 与零进展两条 429 测试规定 mock 429 带 `Retry-After: 0` 且按 tikhub client 重试语义排队(RateLimited max_retries=3,error.rs:140-153)——同一页 429 = 4 次 HTTP 请求,请求数期望按 4 计。
5. `hard_error_with_progress_is_err`(DR-10,同 M3 形状):第 1 页有进展 + 第 2 页 HTTP 500(硬错误)→ 整体 `Err`(**PartialFailure 触发集仅 RateLimited,M1 D2 冻结**)。**预期 RED**:现状单次调用返回首页 Ok(无 override,shortfall=None)→ `expected Err, got Ok(..)`。
**预期 RED**:测试 1 捕获请求数 1 且无 token 参数;测试 2 `left: None, right: Some(Exhausted)`;测试 5 `expected Err, got Ok(..)`。
**GREEN 命令**:`cargo test --lib adapters::instagram`。**GREEN 规格补充(F-02)**:循环终止时记一条结构化日志:platform、accepted_count、StopReason/Partial(可观测性横切,Step 05 F-02)。**反作弊声明**:同 M3-T2(D-01 红线、禁特判 mock)。

---

### M5-T3-B 分支 B:单页 + 如实上报 override(T-014-B;V1=不支持时执行)

**覆盖 ID**:R-006(单页分支)、T-014、PV-005(mock 侧)、F-001(实例)。依赖:M5-T1(判定=不支持)、M5-T2。
**文件**:`src/adapters/instagram.rs`(override + 测试)。

**测试载荷**:
1. `single_page_underdelivery_reports_exhausted`(T-014-B 主断言/R-006 红线):mock 单页 20 条、count=50 → `contents.len()==20`、`shortfall == Some(Exhausted)`。
2. `single_page_reaching_count_no_shortfall`:单页 ≥ count → 截取 count 条、`None`。
3. `zero_progress_error_is_err`(F-001):首调即 429 → `Err`(**允许先绿** AG-006)。**DR-19**:mock 429 带 `Retry-After: 0` 且按重试语义排队(RateLimited max_retries=3,error.rs:140-153)——429 = 4 次 HTTP 请求,请求数期望按 4 计。
4. `legacy_search_unchanged`(回归;**允许先绿** AG-006)。
> DR-10 注:分支 B 为单页语境,「中途硬错误(有进展 + 后续页 500)」形状不存在,`hard_error_with_progress_is_err` 钉子**不适用**(仅 T3-A 承担)。
**预期 RED**:测试 1 `left: None, right: Some(Exhausted)`(默认方法无 override)。
**GREEN 命令**:`cargo test --lib adapters::instagram`。**反作弊声明**:不得修改断言;欠量上报 Exhausted 是 R-006 的规格本体,不得改为 None/COMPLETED 语义。

---

### M5-T4 模块收尾 gate

**覆盖 ID**:AG-010~AG-012、R-012(自查)、FR-005(引用)。依赖:T1~T3(所选分支)。
**命令序列**(同 M3-T4 形状):`cargo test --lib` / `cargo test` → AG-012 预检(**ig cap 行与 override 判定分支不接受静默豁免**)→ R-012 自查零命中 → V1 写回核验(两账本 diff)→ 证据归档 → `rust-verify-change`。
**反作弊声明**:gate 红 = 回对应任务修生产代码。

## 4. 完成判据与独立验证(03-split §5 M5 行)

V1 判定记录写回 assumptions.md V1 行 + C-005;所选分支 T-014 green + RED 证据;T-053 输出留存(≤4 调用)。命令:`TIKHUB_API_KEY=… cargo test --test real_api_test real_instagram_general_search_pagination_probe -- --nocapture`;`cargo test --lib strategies::instagram adapters::instagram`;AG-012 预检。

## 5. 账本覆盖映射

| ID | 任务 | 备注 |
|---|---|---|
| V1 / N-001 / T-053 / P-004 / C-005 | M5-T1 | 判定写回义务 §1.2 |
| R-006 | M5-T2(前提)+ M5-T3-A 或 T3-B(分支本体) | 分支互斥 §1.1 |
| R-001(ig 行) | M5-T2 | cap 行删除 + T-001 ig 断言 |
| T-001(ig) | M5-T2 | — |
| T-014 | M5-T3-A 或 T3-B | 按 V1 |
| PV-005 | M5-T1(real+回灌)+ T3-x(mock 侧) | — |
| I-00x/F-00x(ig 实例) | M5-T3-A(分支 A 时);分支 B 时实例面收窄为 F-001 + R-006 钉子 | 机制归 M1 |
| DR-10(有进展硬错误 → Err,ig 实例) | M5-T3-A(测试 5 `hard_error_with_progress_is_err`);分支 B 不适用(单页语境,形状不存在,T3-B 已注明) | 触发集仅 RateLimited(M1 D2 冻结);Step 07 patch |
| AG-010~012 / R-012 / FR-005 | M5-T4 | — |
| AG-001~007 / AG-020~023 / D-01/D-02/D-04 | §1/§2 继承 | — |

> 自查:03-split M5 行全部 ID(R-006/V1、T-001 ig/T-014/T-053、PV-005、P-004、C-005、N-001)已认领。✅

## 6. 开放问题

1. 分支 A 的 token 请求参数名(`max_id` vs `pagination_token` vs `rank_token` 组合)以 V1 实测/文档为准——T3-A 任务文本中参数名为占位语义,实现期由 T-053 证据定;若与计划假设冲突,走停下上报。
2. V2 fallback 路径翻页不做(§2.2);若 Step 06 评审认为必要,作为后续增量(非本计划账本要求)。
3. **Step 07 patch 记录见 `patches/07-batchF-platforms.md`**(F-07、DR-10/DR-19、F-02)。
