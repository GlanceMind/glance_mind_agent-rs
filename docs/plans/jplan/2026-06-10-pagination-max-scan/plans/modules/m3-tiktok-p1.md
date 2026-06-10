# Module Plan: M3 `agent-rs/tiktok-p1`

> 计划族:`docs/plans/jplan/2026-06-10-pagination-max-scan/`(Step 04 起草,2026-06-10)。仓库:本仓。
> 依赖:M1(D1/D2/D4 消费;**D-01 绑定:tiktok 新循环必须接 `PaginationLoop`**,facebook 豁免不可迁移)。样板:M2(任务结构/AG 模板,软依赖)。
> 账本输入:R-001(tiktok 行)/R-003、T-001(tiktok)/T-011/T-051、PV-002、P-002、I-001~I-004(实例化证据侧)、F-001/F-002/F-004/F-005(tiktok 实例)、AG 全局、FR-003、R-012。
> 已裁决输入:D-01(必须 PaginationLoop)、D-02(hint 按可控性消费——**tiktok 上游有 count 参数,hint 可下发**)、D-04(cap tiktok=20)。

## 0. 模块目标(一句话)

解除 tiktok strategy `v.min(20)` 截断(strategies/tiktok.rs:96),在 tikhub 适配器**新建 offset 翻页循环并接 `PaginationLoop`**(单页≤20 上游硬限,types.rs:297;`has_more=0`/cursor 缺失 → 枯竭),override `fetch_by_keyword_with_outcome` 经 D1 上报欠交付原因;R-012 零形状改动。

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
  next = (resp.data.has_more == Some(1)).then(|| resp.data.cursor 或 offset+len)::to_string
  out = pl.accept_page(&ids, next)               // 去重/达量/防环/空页全在状态机
  收集 newly_accepted 对应 Content
  Continue{cursor} → offset = cursor.parse()
  Stop(reason)     → break;shortfall = pl.shortfall_for(&reason)
错误分支:有进展(accepted>0)的可恢复错误 → PartialFailure{message};零进展 → return Err(F-001)
```
- `has_more` 缺失/0 与 `cursor` 缺失均归一化为 `next=None`(→ UpstreamExhausted);上游返回重复 cursor → 状态机 CursorLoop;空页 has_more=1 连发 → EmptyPageLimit。next offset 以响应 `cursor` 字段为准(T-051 实测语义回灌确认,P-002 证据项)。
- **边界**:UserId/SecUserId(user-videos)与 ContentId 路径不在 M3 范围(无账本行;欠交付语义维持默认 shortfall=None,滚动兼容)。
- 既有公开 `search()` 签名不变(包装循环、丢弃 shortfall);override `fetch_by_keyword_with_outcome` 走带 shortfall 路径。

### 2.3 状态面

| 状态键 | 写方 | 读方 |
|---|---|---|
| `options.extra["page_size"]` | TiktokStrategy::build_search_options(M3-T1) | tikhub 适配器循环(M3-T2);root 冻结后 M5 分支 A 可复用 |
| `FetchOutcome.shortfall`(tiktok) | tikhub 适配器 override(M3-T2) | orchestrator(M1-T4 既有读方) |

## 3. 任务清单

> T1→T2 顺序;T3 gated;T4 收尾。测试/实现分上下文(AG-004);T1/T2 确定性,不需凭据。

---

### M3-T1 strategy 截断解除 + PAGE_SIZE 下发(T-001 tiktok 行)

**覆盖 ID**:R-001(tiktok)、T-001(tiktok)、D4 消费(tiktok 可控分支)。
**文件**:`src/strategies/tiktok.rs`(96 行 + 测试)、`src/strategies/mod.rs`(extra_keys::PAGE_SIZE 一行)。

**测试载荷(测试子 agent 先写)**:
1. `count_carries_total_max_videos`:max_videos=50 → `options.count == 50`。
2. `count_arbitrary_total_not_capped`:max_videos=137 → `137`(防换一个硬上限糊弄)。
3. `page_size_extra_from_hint_clamped`:`page_size_hint=Some(7)` → `extra["page_size"] == 7`;`Some(500)` → `20`(clamp 到 cap)。
4. `no_hint_no_page_size_extra`:hint=None → extra 无 `page_size` 键(适配器默认 20)。
5. `missing_max_videos_default_unchanged`:None → `count == 10`(现状缺省;**允许先绿**,AG-006 由 AG-012 覆盖)。

**预期 RED**:测试 1 `assertion 'left == right' failed: left: 20, right: 50`;测试 2 `left: 20, right: 137`;测试 3 `extra 无键 → assertion failed: options.extra.get("page_size").is_some()`。
**GREEN**:`let count = config.max_videos.map(|v| v as u32).unwrap_or(10);` + hint→PAGE_SIZE 写入(clamp 20)+ R-001/D4 注释。命令:`cargo test --lib strategies::tiktok`。
**最终验收**:同上 `-- --nocapture` + `cargo build --all-features`。
**反作弊声明**:实现者不得修改断言来过测试;不得把 min(20) 换成任何其它总量上限。

---

### M3-T2 适配器 offset 翻页循环(PaginationLoop)+ D1 override(T-011)

**覆盖 ID**:R-003、T-011、I-001~I-004(tiktok 实例)、F-001/F-002/F-004/F-005(实例)、PV-002(mock 侧)、FR-003、D-01。依赖:M3-T1。
**文件**:`src/adapters/tikhub.rs`(循环 + override + 测试;mock-HTTP helper 按 FR-003 自 client.rs:1788 同构复制入测试模块)。

**测试载荷(测试子 agent 先写;mock-HTTP 响应形状取自 `tests/fixtures/tiktok/search_travel_us.json` 字段集,禁凭空捏造)**:
1. `paginates_offsets_until_count`(T-011 主断言):3 页(20+20+10,has_more=1/1/0,cursor=20/40/—)、count=50、page_size=20 → 50 条、shortfall=None;**捕获请求断言 offset 序列 0,20,40 且 count=20**。
2. `exhausted_when_has_more_zero`:2 页(20+10,第 2 页 has_more=0)、count=50 → 30 条、`Some(Exhausted)`。
3. `cursor_missing_normalized_exhausted`:has_more=1 但 cursor 缺失 → `Some(Exhausted)`(归一化)。
4. `repeated_cursor_stops`(F-004):第 2 页返回与第 1 页相同 cursor → 终止、`Some(Exhausted)`、不发第 3 请求。
5. `empty_pages_stop_at_limit`(F-005):3 连空页(has_more=1,cursor 递进)→ 终止、`Some(Exhausted)`。
6. `partial_failure_with_progress`(F-002):第 1 页 20 条 + 第 2 页 HTTP 429 → 20 条、`Some(PartialFailure{..})` message 含 "rate"(小写)。
7. `zero_progress_error_is_err`(F-001):第 1 页即 429 → `Err(..)`(**允许先绿**:错误透传现状;AG-006 由 AG-012 覆盖)。
8. `page_size_extra_consumed`:extra page_size=7 → 捕获请求 count=7。
9. `legacy_search_unchanged`:既有 `fetch_by_keyword`(无 outcome)同 mock 下返回条数与 outcome.contents 一致(回归;**允许先绿**,AG-006)。

**预期 RED**(现状单次调用、无 override):测试 1 `left: 20, right: 50` 且捕获请求数 1≠3;测试 2 `left: None, right: Some(Exhausted)`;测试 8 `left: 10, right: 7`(现状 count 来自 options.count 截断链)。
**GREEN 命令**:`cargo test --lib adapters::tikhub`。**最终验收**:同上 + `cargo build --all-features`。
**反作弊声明**:实现者不得修改断言/请求序列期望;不得绕开 `PaginationLoop` 手写终止逻辑(D-01 红线);不得特判 mock base_url。

---

### M3-T3 T-051 real gate:offset 第二页实测 + fixture 回灌(P-002/PV-002 real 侧)

**覆盖 ID**:T-051、P-002、PV-002(real + 回灌)、AG-007。依赖:M3-T2。
**文件**:`tests/real_api_test.rs`(扩展)、`tests/fixtures/tiktok/`(第二页 fixture 回灌)。

**Gate 控制**:`TIKHUB_API_KEY`(real_api_test.rs:3-4 既有约定);**≤3 次调用**;只读无清理;AG-007 漂移区分。
**测试载荷** — `real_tiktok_search_second_page_offset`:第 1 次调用 offset=0 记录 has_more/cursor 实测值;第 2 次调用 offset=cursor 实测值 → 断言第二页非空且 aweme_id 集合 ≠ 首页(允许部分重叠,断言不全同);两页原始响应回灌 `tests/fixtures/tiktok/search_travel_us_page2.json`(及必要时 page1 刷新),**cursor 推进语义(=下一 offset?)的实测结论写入 fixture 旁注释与完成报告**——若与 M3-T2 实现假设不符,停下上报走断言修正流程(ASSERTION-CHANGE-JUSTIFIED + root 知会),不得静默改 mock。
**RED/GREEN 说明**:live 契约探针,**允许先绿**(书面理由同 M2-T4 模式,交 Test-Gate Reviewer)。
**验收命令**:`TIKHUB_API_KEY=… cargo test --test real_api_test real_tiktok_search_second_page_offset -- --nocapture`(输出留存)。
**反作弊声明**:不得修改断言;live 红先区分上游漂移(贴原始响应)。

---

### M3-T4 模块收尾 gate

**覆盖 ID**:AG-010~AG-012、R-012(自查)、FR-005(引用)。依赖:T1~T3。
**命令序列**(同 M2-T7 形状,输出留存):`cargo test --lib` / `cargo test`(AG-007 注记)→ `git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1`(无 missed 或书面豁免;**tiktok.rs:96 cap 行与循环终止分支不接受静默豁免**)→ R-012 自查(migrations/schema/protocol_gen 零命中;`src/adapters/redis.rs`、`src/ports/` 零改动)→ RED→GREEN 证据归档 → `rust-verify-change`。
**反作弊声明**:gate 红 = 回对应任务修生产代码。

## 4. 完成判据与独立验证(03-split §5 M3 行)

T-001(tiktok)/T-011 green + RED 证据;gated T-051 输出 + 第二页 fixture 回灌;mutants 无 missed。命令:`cargo test --lib strategies::tiktok adapters::tikhub`;`TIKHUB_API_KEY=… cargo test --test real_api_test -- --nocapture`;AG-012 预检。

## 5. 账本覆盖映射

| ID | 任务 | 备注 |
|---|---|---|
| R-001(tiktok)/T-001(tiktok) | M3-T1 | — |
| R-003 / T-011 | M3-T2 | offset 递进、has_more 终止、达量终止 |
| T-051 / P-002 / PV-002(real) | M3-T3 | cursor 语义实测回灌 |
| PV-002(mock) | M3-T2 | fixture 形状来源既有样本 |
| I-001~I-004 / F-001/F-002/F-004/F-005(实例) | M3-T2 | 机制归 M1 |
| FR-003 | M3-T2(helper 同构复制;**第三次复制已发生 → 按 M2 §6.4 登记 root 裁决提升 `src/testing`**) | — |
| AG-010~012 / R-012 / FR-005 | M3-T4 | — |
| AG-001~007 / AG-020~023 | §1 继承/引用 | — |

> 自查:03-split M3 行全部 ID(R-001 tiktok/R-003、T-001 tiktok/T-011/T-051、PV-002、P-002)已认领。✅

## 6. 开放问题

1. **PAGE_SIZE extra 键形状提请 root 冻结**(§2.1);M5 分支 A 复用。
2. **cursor 推进语义假设**(响应 cursor = 下一 offset)以 T-051 实测为准;不符时走修正流程(M3-T3)。
3. **mock helper 第三次复制**:提请 root 裁决一次性提升至 `src/testing`(后续重构任务,不阻塞 M3)。
