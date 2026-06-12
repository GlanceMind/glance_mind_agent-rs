# Module Plan: M3 `agent-rs/tiktok-p1`

> 计划族:`docs/plans/jplan/2026-06-10-pagination-max-scan/`(Step 04 起草,2026-06-10)。仓库:本仓。
> 依赖:M1(D1/D2/D4 消费;**D-01 绑定:tiktok 新循环必须接 `PaginationLoop`**,facebook 豁免不可迁移)。样板:M2(任务结构/AG 模板,软依赖)。
> 账本输入:R-001(tiktok 行)/R-003、T-001(tiktok)/T-011/T-051、PV-002、P-002、I-001~I-004(实例化证据侧)、F-001/F-002/F-004/F-005(tiktok 实例)、AG 全局、FR-003、R-012。
> 已裁决输入:D-01(必须 PaginationLoop)、D-02(hint 按可控性消费——**tiktok 上游有 count 参数,hint 可下发**)、D-04(cap tiktok=20)。

## 现状核对(2026-06-10,Step 07)

main 上已合并 **PR #5「fix(tikhub): paginate TikTok scan to configured max_scan_count (was capped at 20)」(commit 4ab34ca,2026-06-09)**。基于 `git show origin/main` 实证核对:

**PR #5 改了什么**(`git show 4ab34ca --stat`):`src/strategies/tiktok.rs`(±8 行)、`src/adapters/tikhub.rs`(+532 行)、`tests/real_api_test.rs`(±40,新增 `test_tiktok_adapter_paginates_real`,无 key 自动 skip)、新增 `tests/tikhub_search_pagination.rs`(mockito)与 `tests/tikhub_user_videos_pagination.rs`,以及其自带 jplan 计划族文档(`docs/plans/jplan/2026-06-09-tiktok-scan-pagination/`,含 RED/GREEN 证据)。

**逐条结论**:
1. **cap 行已除** ✅:`strategies/tiktok.rs:96` 的 `map(|v| v.min(20) as u32)` 已改为 `map(|v| v.max(0) as u32)`(count 改总量语义,注释明示「must NOT clamp」);`max_videos_per_search()` 仍返回 20,注释改为「per-PAGE size」。
2. **翻页循环已建** ✅:`adapters/tikhub.rs` 新增私有 `paginate_videos(target, page_size_cap, fetcher)` + `VideoPageFetcher` trait;`SearchPageFetcher` 走 offset 翻页(`with_offset(cursor)`,next = `resp.data.cursor`,`has_more==1` 继续),另有 `UserVideoPageFetcher`(max_cursor 路径,超出 M3 §2.2 边界但不冲突);`search`/`fetch_by_keyword` 路径均已走循环;含 proptest 套件(adapter 模块内)。
3. **未接 `PaginationLoop`** ❌(核对前预判成立:PR #5 是不经 M1 接口的独立修复):循环为 adapter 私有手写——自带 `seen: HashSet<aweme_id>` 去重、`max_pages = target/cap + 2` 守卫、**首个空页即停**、cursor 缺失即停、`collected.truncate(target)`。
4. **未消费 D1** ❌:无 `fetch_by_keyword_with_outcome` override,欠交付静默(无 shortfall 上报)。
5. **未消费 D4/PAGE_SIZE extra** ❌:单页 cap 硬编码 20;每请求 `count = min(remaining, 20)`(非固定 page_size)。
6. **错误语义与 D2/F-002 不一致** ❌:循环中任意页错误经 `?` 直接整体 `Err`,无「有进展 → PartialFailure」分支。

**差异清单(PR #5 实现 vs M1 D1/D2 契约)与 M3 任务范围改写**——M3 任务范围从「新建循环」改写为「**把既有 PR #5 修复迁移到共享契约(PaginationLoop + D1 override + D4 载体)上**」,账本 ID 认领不变,仅达成路径变化:
| # | PR #5 现状 | M1 契约要求 | M3 迁移动作 |
|---|---|---|---|
| 1 | 手写终止逻辑(seen-set / 首个空页即停 / max_pages 守卫) | `PaginationLoop::accept_page` 驱动(D-01 红线) | 替换为状态机驱动;空页语义以 D2 冻结的 EmptyPageLimit(连续空进展页计数)为准 |
| 2 | 无 override / 无 shortfall | D1 `fetch_by_keyword_with_outcome` + `shortfall_for` | M3-T2 新增(该侧 RED 全部保留) |
| 3 | cap 硬编码 20,count=min(remaining,cap) | PAGE_SIZE extra 消费,固定 page_size 下发 | M3-T1 测试 3 / M3-T2 测试 8 RED 保留;per-request count 语义以 D2/D4 为准,**T-051 实测复核上游对 count<20 的行为(待执行期复核)** |
| 4 | 任意错误 → Err | 有进展可恢复错误 → PartialFailure(触发集仅 RateLimited);硬错误 → Err | F-002 测试 RED 保留;DR-10 钉子见 M3-T2 |
| 5 | 无重复 cursor 检测(重复页靠去重 + max_pages 兜) | 状态机 CursorLoop | M3-T2 测试 4 RED 保留(现状会发第 3 请求) |

**RED 预期改写**:已被 main 修复的行为(总量不截断、多页请求本身)对应测试改标「**允许先绿 + AG-006 金丝雀(临时恢复旧行为——如重新加 `.min(20)` 或砍掉循环——须红)**」,并注明「RED 前提已被 PR #5 消解」;shortfall/PAGE_SIZE/PartialFailure/CursorLoop 侧 RED 依旧成立(main 无此行为)。逐测试落点见 M3-T1/M3-T2 任务文本。不确定处(测试 1 的 per-request count=20 断言对 main 的红绿、上游接受任意 count 值)已标注待执行期复核。

## 0. 模块目标(一句话)

tiktok strategy `v.min(20)` 截断与适配器 offset 翻页循环**已由 PR #5 在 main 落地**(见上方现状核对);M3 的剩余目标 = **把该独立修复迁移到共享契约上**:循环主体改接 `PaginationLoop`(D-01),override `fetch_by_keyword_with_outcome` 经 D1 上报欠交付原因,接通 D4 PAGE_SIZE 载体(单页≤20 上游硬限,types.rs:297;`has_more=0`/cursor 缺失 → 枯竭归一化);R-012 零形状改动。

## 1. 全模块强制约束

与 M2 计划 §1 同文继承(AG-001~AG-007、AG-010~AG-012、R-012、契约不变 §3、D1~D4 不重设计),此处不复抄;补充两条:
1. **D-01 绑定**:循环主体必须用 `PaginationLoop::accept_page` 驱动、`shortfall_for` 映射;不得手写 seen-cursor/空页计数(那是状态机职责)。
2. **AG-020~AG-023 实例化引用**:M3 不新增 PT 套件;循环不变量由 M1 PT-1~PT-4 承担,tiktok 侧为确定性 mock 实例(T-011)。

## 2. 设计

### 2.1 页大小载体(M3 定形,**提请 root 冻结**;M4/M5 复用判定见各计划)

`SearchOptions` 无单页字段(count 已改总量语义)。M3 新增共享 extra 键 `extra_keys::PAGE_SIZE`(`"page_size"`,json number):strategy 从 `config.page_size_hint` 写入(存在时;clamp 到 `platform_page_cap("tiktok")=20`),适配器读取为单页请求 count(缺省 20)。这是 D4「单页大小另行传递」的具体载体,**形状由 root 冻结**;reddit/twitter/facebook 等无上游单页参数的平台不读此键(D-02)。

### 2.2 适配器循环(search 路径;Search/Hashtag keyword)

```text
pl = PaginationLoop::new(options.count)          // 总量
page_size = extra PAGE_SIZE 或 20(SearchParams::with_count 再 clamp 20)
offset = 0
loop:
  resp = client.search_videos_with_retry(params{keyword, offset, count: page_size, ...})
  ids = extract_videos(resp).aweme_id 列表
  next = (resp.data.has_more == Some(1)).then(|| resp.data.cursor)::to_string   // 不做本地合成游标(DR-18:offset+len 合成与归一化 Exhausted 矛盾)
  out = pl.accept_page(&ids, next)               // 去重/达量/防环/空页全在状态机
  收集 newly_accepted 对应 Content
  Continue{cursor} → offset = cursor.parse()
  Stop(reason)     → break;shortfall = pl.shortfall_for(&reason)
错误分支:有进展(accepted>0)的可恢复错误 → PartialFailure{message};零进展 → return Err(F-001)
```
- `has_more` 缺失/0 与 `cursor` 缺失均归一化为 `next=None`(→ UpstreamExhausted);**cursor 非数字/parse 失败 → next=None(Exhausted)**(DR-18);上游返回重复 cursor → 状态机 CursorLoop;空页 has_more=1 连发 → EmptyPageLimit。next offset 以响应 `cursor` 字段为准(T-051 实测语义回灌确认,P-002 证据项)。
- **边界**:UserId/SecUserId(user-videos)与 ContentId 路径不在 M3 范围(无账本行;欠交付语义维持默认 shortfall=None,滚动兼容)。user-videos 路径(`UserVideoPageFetcher`)维持 PR #5 私有循环不动,与 search 路径 PaginationLoop 并存;后续归并登记为 backlog(无账本要求,不阻塞 M3);`tikhub_user_videos_pagination.rs` 测试集不受本次迁移影响。
- 既有公开 `search()` 签名不变(包装循环、丢弃 shortfall);override `fetch_by_keyword_with_outcome` 走带 shortfall 路径。

### 2.3 状态面

| 状态键 | 写方 | 读方 |
|---|---|---|
| `options.extra["page_size"]` | TiktokStrategy::build_search_options(M3-T1) | tikhub 适配器循环(M3-T2);仅 tiktok 消费(root §1 D4/D-02);M5 已豁免——除非 V1 证据显示上游接受单页参数且经 root 修订豁免表 |
| `FetchOutcome.shortfall`(tiktok) | tikhub 适配器 override(M3-T2) | orchestrator(M1-T4 既有读方) |

## 3. 任务清单

> T1→T2 顺序;T3 gated;T4 收尾。测试/实现分上下文(AG-004);T1/T2 确定性,不需凭据。

---

### M3-T1 strategy 截断解除 + PAGE_SIZE 下发(T-001 tiktok 行)

**覆盖 ID**:R-001(tiktok)、T-001(tiktok)、D4 消费(tiktok 可控分支)。
**文件**:`src/strategies/tiktok.rs`(96 行 + 测试)、`src/strategies/mod.rs`(extra_keys::PAGE_SIZE 一行)。

**测试载荷(测试子 agent 先写)**:
1. `count_carries_total_max_videos`:max_videos=50 → `options.count == 50`(**允许先绿 + AG-006 金丝雀(临时恢复 `.min(20)` 须红);RED 前提已被 PR #5 消解**——cap 行已在 main 删除,见现状核对)。
2. `count_arbitrary_total_not_capped`:max_videos=137 → `137`(防换一个硬上限糊弄;**允许先绿 + AG-006 金丝雀,RED 前提已被 PR #5 消解**,同上)。
3. `page_size_extra_from_hint_clamped`:`page_size_hint=Some(7)` → `extra["page_size"] == 7`;`Some(500)` → `20`(clamp 到 cap)。
4. `no_hint_no_page_size_extra`:hint=None → extra 无 `page_size` 键(适配器默认 20)。
5. `missing_max_videos_default_unchanged`:None → `count == 10`(现状缺省;**允许先绿**,AG-006 由 AG-012 覆盖)。

**预期 RED**(按现状核对改写):测试 1/2 的 RED 前提(`left: 20`)已被 PR #5 消解,改走「允许先绿 + AG-006 金丝雀」;仍须 RED 的仅测试 3:`extra 无键 → assertion failed: options.extra.get("page_size").is_some()`(main 无 PAGE_SIZE extra,实证)。
**GREEN**:count 总量语义已在 main(`map(|v| v.max(0) as u32)`,PR #5);本任务剩余 GREEN = hint→PAGE_SIZE 写入(clamp 20)+ R-001/D4 注释。命令:`cargo test --lib strategies::tiktok`。
**最终验收**:同上 `-- --nocapture` + `cargo build --all-features`。
**反作弊声明**:实现者不得修改断言来过测试;不得把 min(20) 换成任何其它总量上限。

---

### M3-T2 适配器 offset 翻页循环(PaginationLoop)+ D1 override(T-011)

**覆盖 ID**:R-003、T-011、I-001~I-004(tiktok 实例)、F-001/F-002/F-004/F-005(实例)、PV-002(mock 侧)、FR-003、D-01。依赖:M3-T1。
**文件**:`src/adapters/tikhub.rs`(循环 + override + 测试;mock-HTTP helper 按 FR-003 自 client.rs:1788 同构复制入测试模块)。

**测试载荷(测试子 agent 先写;mock-HTTP 响应形状取自 `tests/fixtures/tiktok/search_travel_us.json` 字段集,禁凭空捏造)**:
1. `paginates_offsets_until_count`(T-011 主断言):3 页(20+20+10,has_more=1/1/0,cursor=20/40/—)、count=50、page_size=20 → 50 条、shortfall=None;**捕获请求断言 offset 序列 0,20,40 且 count=20**。(「请求数 3 / offset 推进」的 RED 前提已被 PR #5 消解 → 该子断言允许先绿 + AG-006 金丝雀(临时将循环体改为单次调用(仅发 offset=0 首请求后直接 break,不转发 cursor)须使请求数断言(期望 3,实得 1)变红);「每请求 count=固定 20」与 main 的 `min(remaining,20)` 不同——第 3 请求 main 发 count=10——该子断言对 main 的红绿**待执行期复核**,以 D2/D4 契约为准。)
2. `exhausted_when_has_more_zero`:2 页(20+10,第 2 页 has_more=0)、count=50 → 30 条、`Some(Exhausted)`。
3. `cursor_missing_normalized_exhausted`:has_more=1 但 cursor 缺失 → `Some(Exhausted)`(归一化)。
4. `repeated_cursor_stops`(F-004):第 2 页返回与第 1 页相同 cursor → 终止、`Some(Exhausted)`、不发第 3 请求。
5. `empty_pages_stop_at_limit`(F-005):3 连空页(has_more=1,cursor 递进)→ 终止、`Some(Exhausted)`。
6. `partial_failure_with_progress`(F-002):第 1 页 20 条 + 第 2 页 HTTP 429 → 20 条、`Some(PartialFailure{..})` message 含 "rate"(小写)。**DR-19**:mock 429 响应必须带 `Retry-After: 0`,且 mock 按 TikHub 重试语义排队——RateLimited 走 max_retries=3 → 同一页共 4 次 HTTP 请求(error.rs:140-153);**请求数期望按 4 计**(不带 `Retry-After: 0` 会实睡默认延迟,撞测试预算)。
7. `zero_progress_error_is_err`(F-001):第 1 页即 429 → `Err(..)`(**允许先绿**:错误透传现状;AG-006 由 AG-012 覆盖)。**DR-19 同测试 6**:`Retry-After: 0` + 重试排队,首页 429 = 4 次请求后 Err,请求数期望按 4 计。
8. `page_size_extra_consumed`:extra page_size=7 → 捕获请求 count=7。
9. `legacy_search_unchanged`:既有 `fetch_by_keyword`(无 outcome)同 mock 下返回条数与 outcome.contents 一致(回归;**允许先绿**,AG-006)。
10. `non_numeric_cursor_normalized_exhausted`(DR-18):mock 第 1 页 has_more=1 但返回非数字 cursor(parse 失败)→ 终止、`Some(Exhausted)`(归一化,不 panic 不重发)。**预期 RED**:main 的 cursor 为 i64 反序列化语义/无 shortfall → `left: None, right: Some(Exhausted)`。
11. `hard_error_with_progress_is_err`(DR-10):第 1 页 20 条有进展 + 第 2 页 HTTP 500(硬错误)→ 整体 `Err`(**PartialFailure 触发集仅 RateLimited,M1 D2 冻结**;硬错误不得降级为 Partial)。**RED 说明**:守的是「过宽实现把硬错误并入 Partial」——迁移实现若触发集过宽则红(`expected Err, got Ok(PartialFailure)`);注:PR #5 现状任意错误即 Err,迁移前该测试先绿 → 按允许先绿 + AG-006 金丝雀(临时把 500 加入 Partial 触发集须红),RED 前提部分被 PR #5 消解,如实记录。

**预期 RED**(按现状核对改写;main 已有 PR #5 私有循环,但无 PaginationLoop/override/PAGE_SIZE):测试 1 的「请求数 1≠3」前提已被 PR #5 消解(见测试 1 内标注);仍须 RED:测试 2/3/10 `left: None, right: Some(Exhausted)`(main 无 shortfall 上报);测试 4 main 无重复 cursor 检测(会发第 3 请求,靠 max_pages 兜)→ 请求数断言红;测试 5 main 首个空页即停(≠ D2 EmptyPageLimit 连续计数语义)→ 行为断言红;测试 6 main 任意错误整体 Err → `expected Ok(PartialFailure), got Err`;测试 8 main 忽略 extra,count=min(remaining,20) → `left: 20, right: 7`。
**GREEN 命令**:`cargo test --lib adapters::tikhub`。**GREEN 规格补充(F-02)**:循环终止时记一条结构化日志:platform、accepted_count、StopReason/Partial(可观测性横切,Step 05 F-02)。**最终验收**:同上 + `cargo build --all-features`。
**反作弊声明**:实现者不得修改断言/请求序列期望;不得绕开 `PaginationLoop` 手写终止逻辑(D-01 红线——**含保留 PR #5 的 `paginate_videos` 手写 seen-set/空页/max_pages 逻辑不迁移**);不得特判 mock base_url。

**既有测试处置(PR #5 测试集,迁移撞红预告)**:`tests/tikhub_search_pagination.rs::f3_empty_second_page_terminates_at_20` 与 `r6a_empty_first_page_returns_empty_vec` 编码的是 PR #5 私有循环的旧语义(首空页即停),与 D2 EmptyPageLimit(连续 3 空页)不兼容——迁移后这两条**必然变红**。处置路径:按 `ASSERTION-CHANGE-JUSTIFIED: D2 EmptyPageLimit 冻结——旧语义由 PR #5 私有循环实现,迁移后不再适用` 修订(f3 的 mock 改为 3 连空页形状以对齐 EmptyPageLimit)或整条删除并说明(测试硬规则 §1(b)/(c));修订版测试须先红后绿并留证据;**修订由测试子 agent 在测试上下文完成,实现子 agent 不得单方面删改**。

---

### M3-T3 T-051 real gate:offset 第二页实测 + fixture 回灌(P-002/PV-002 real 侧)

**覆盖 ID**:T-051、P-002、PV-002(real + 回灌)、AG-007。依赖:M3-T2。
**文件**:`tests/real_api_test.rs`(扩展)、`tests/fixtures/tiktok/`(第二页 fixture 回灌)。

**Gate 控制**:`TIKHUB_API_KEY`(real_api_test.rs:3-4 既有约定);**≤3 = HTTP 请求上界**(DR-19:探针用无重试调用或 `with_retry_config` 零重试,确保「调用数 = 请求数」,预算按 HTTP 请求计);只读无清理;AG-007 漂移区分。
**测试载荷** — `real_tiktok_search_second_page_offset`:第 1 次调用 offset=0 记录 has_more/cursor 实测值;第 2 次调用 offset=cursor 实测值 → 断言第二页非空且 aweme_id 集合 ≠ 首页(允许部分重叠,断言不全同);两页原始响应回灌 `tests/fixtures/tiktok/search_travel_us_page2.json`(及必要时 page1 刷新),**cursor 推进语义(=下一 offset?)的实测结论写入 fixture 旁注释与完成报告**——若与 M3-T2 实现假设不符,停下上报走断言修正流程(ASSERTION-CHANGE-JUSTIFIED + root 知会),不得静默改 mock。
**RED/GREEN 说明**:live 契约探针,**允许先绿**(书面理由同 M2-T6/M5-T1 探针性质说明模式,交 Test-Gate Reviewer;AG-008 探针类;AG-007 漂移区分保留)。
**验收命令**:`TIKHUB_API_KEY=… cargo test --test real_api_test real_tiktok_search_second_page_offset -- --nocapture`(输出留存)。
**反作弊声明**:不得修改断言;live 红先区分上游漂移(贴原始响应)。

---

### M3-T4 模块收尾 gate

**覆盖 ID**:AG-010~AG-012、R-012(自查)、FR-005(引用)。依赖:T1~T3。
**命令序列**(同 M2-T7 形状,输出留存):`cargo test --lib` / `cargo test`(AG-007 注记)→ `git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1`(无 missed 或书面豁免;**tiktok.rs:96 cap 行与循环终止分支不接受静默豁免**)→ R-012 自查(migrations/schema/protocol_gen 零命中;`src/adapters/redis.rs`、`src/ports/` 零改动)→ RED→GREEN 证据归档 → `rust-verify-change`。
**反作弊声明**:gate 红 = 回对应任务修生产代码。

## 4. 完成判据与独立验证(03-split §5 M3 行)

T-001(tiktok)/T-011 green + RED 证据;gated T-051 输出 + 第二页 fixture 回灌;mutants 无 missed。命令:`cargo test --lib strategies::tiktok adapters::tikhub`;`TIKHUB_API_KEY=… cargo test --test real_api_test -- --nocapture`;AG-012 预检。
允许先绿清单:M3-T1.1/T1.2(PR #5 消解,金丝雀)、T1.5(AG-012)、M3-T2.1 子断言(金丝雀)、T2.7(书面理由)、T2.9(AG-006)、T2.11(金丝雀)、M3-T3(AG-008 探针类;AG-007 漂移区分);其余测试均须 RED。

## 5. 账本覆盖映射

| ID | 任务 | 备注 |
|---|---|---|
| R-001(tiktok)/T-001(tiktok) | M3-T1 | — |
| R-003 / T-011 | M3-T2 | offset 递进、has_more 终止、达量终止 |
| T-051 / P-002 / PV-002(real) | M3-T3 | cursor 语义实测回灌 |
| PV-002(mock) | M3-T2 | fixture 形状来源既有样本 |
| I-001~I-004 / F-001/F-002/F-004/F-005(实例) | M3-T2 | 机制归 M1 |
| DR-18(非数字 cursor 归一化) | M3-T2(测试 10 `non_numeric_cursor_normalized_exhausted`) | Step 07 patch |
| DR-10(有进展硬错误 → Err,tiktok 实例) | M3-T2(测试 11 `hard_error_with_progress_is_err`) | 触发集仅 RateLimited(M1 D2 冻结);Step 07 patch |
| FR-003 | M3-T2(helper 同构复制;**第三次复制已发生 → 见本计划 §6.3 / root RT-2.1**) | — |
| AG-010~012 / R-012 / FR-005 | M3-T4 | — |
| AG-001~007 / AG-020~023 | §1 继承/引用 | — |

> 自查:03-split M3 行全部 ID(R-001 tiktok/R-003、T-001 tiktok/T-011/T-051、PV-002、P-002)已认领。✅

## 6. 开放问题

1. **PAGE_SIZE extra 键形状提请 root 冻结**(§2.1);仅 tiktok 消费(root §1 D4/D-02);M5 已豁免——除非 V1 证据显示上游接受单页参数且经 root 修订豁免表。
2. **cursor 推进语义假设**(响应 cursor = 下一 offset)以 T-051 实测为准;不符时走修正流程(M3-T3)。
3. **mock helper 第三次复制**:提请 root 裁决一次性提升至 `src/testing`(后续重构任务,不阻塞 M3)。
4. **Step 07 patch 记录见 `patches/07-batchF-platforms.md`**(现状核对/PR #5 迁移改写、DR-10/DR-18/DR-19、F-02/F-08/F-09)。
