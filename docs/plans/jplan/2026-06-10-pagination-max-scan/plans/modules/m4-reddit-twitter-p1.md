# Module Plan: M4 `agent-rs/reddit-twitter-p1`

> 计划族:`docs/plans/jplan/2026-06-10-pagination-max-scan/`(Step 04 起草,2026-06-10)。仓库:本仓。
> 依赖:M1(D1/D2/D4;**D-01 绑定:两平台 content 路径新循环必须接 `PaginationLoop`**)。样板:M2/M3(软依赖)。
> 账本输入:R-001(reddit/twitter 行)/R-004/R-005、T-001(两行)/T-012/T-013/T-052、PV-003/PV-004、P-003、I-001~I-004(实例侧)、F-001/F-002/F-004/F-005(实例)、AG 全局、FR-003、R-012。
> 已裁决输入:D-01、D-02(**reddit/twitter 搜索端点无单页大小参数——实证 `RedditSearchParams{query,safe_search,allow_nsfw,after}`、`TwitterSearchParams{keyword,search_type,cursor}`——hint 同 facebook 文档化豁免,不读 PAGE_SIZE extra**)、D-04(cap reddit=100/twitter=100)。

## 0. 模块目标(一句话)

解除 reddit(reddit.rs:100 `min(100)`)/ twitter(twitter.rs:123 `min(100)`)strategy 总量截断,把两平台 content 搜索路径从「单次调用 + take(count)」改为**评论侧既有 cursor 模式的镜像移植**(reddit:`after`/`pageInfo.hasNextPage`,评论侧样板 reddit.rs:292-330;twitter:`cursor`/`next_cursor`,样板 twitter.rs:331-363),循环接 `PaginationLoop`,override `fetch_by_keyword_with_outcome` 经 D1 上报;R-012 零形状改动。

## 1. 全模块强制约束

与 M2 §1 同文继承(AG-001~007、AG-010~012、R-012、契约不变 §3、D1~D4 不重设计)+ M3 §1 两条补充(D-01 绑定;AG-020~023 实例化引用,不新增 PT)。两平台证据形态一致(同构镜像移植),合并一模块(03-split §7.3)。

## 2. 设计

### 2.1 循环形态(两平台同构)

```text
pl = PaginationLoop::new(options.count)
cursor = None
loop:
  resp = client.<search>_with_retry(params.with_cursor/after(cursor?))
  items = 提取条目;ids = 条目 id 列表
  next = 平台终止信号归一化:
    reddit : pageInfo.has_next_page==true 时 Some(end_cursor),否则 None
    twitter: data.next_cursor(缺失→None;重复值由状态机 CursorLoop 兜)
  out = pl.accept_page(&ids, next);收集 newly_accepted
  Continue{cursor} → 转发;Stop(reason) → shortfall = pl.shortfall_for(&reason)
错误:有进展可恢复错误 → PartialFailure{message};零进展 → Err(F-001)
```
- 单页大小由上游决定(无参数);PAGE_SIZE extra 不读(D-02 豁免,代码注释固化)。
- 既有公开 `search()` 签名不变;override 走带 shortfall 路径。
- twitter 既有 `is_search_result_tweet` 过滤保持:**喂入状态机的 ids = 过滤后条目**(达量计数以可用条目为准,与 facebook date-filter 同理)。

### 2.2 状态面

| 状态键 | 写方 | 读方 |
|---|---|---|
| `FetchOutcome.shortfall`(reddit/twitter) | 各适配器 override(M4-T2/T3) | orchestrator(M1-T4) |

## 3. 任务清单

> T1 → T2/T3(互相独立,可并行)→ T4 gated → T5 收尾。T1~T3 确定性,不需凭据。

---

### M4-T1 两平台 strategy 截断解除(T-001 reddit/twitter 行)

**覆盖 ID**:R-001(reddit/twitter)、T-001(两行)、D-02(豁免注释)。
**文件**:`src/strategies/reddit.rs`、`src/strategies/twitter.rs`(各 cap 行 + 测试)。

**测试载荷(每平台镜像四条,测试子 agent 先写)**:
1. `count_carries_total_max_videos`:max_videos=150 → `options.count == 150`(选 150 > 旧 cap 100,RED 才暴露截断)。
2. `count_arbitrary_total_not_capped`:max_videos=237 → `237`。
3. `count_below_legacy_cap_unchanged`:max_videos=50 → `50`(回归;**允许先绿 + AG-006(AG-012 覆盖,cap 行在 diff 内)**)。
4. `missing_max_videos_default_unchanged`:None → reddit `25` / twitter `20`(现状缺省;**允许先绿**,AG-006 由 AG-012 覆盖)。

**预期 RED**:测试 1 `assertion 'left == right' failed: left: 100, right: 150`;测试 2 `left: 100, right: 237`。
**GREEN**:两处改 `map(|v| v as u32)` + R-001 注释 + D-02 豁免注释(「上游搜索端点无单页参数,page_size_hint 本平台不消费」)。命令:`cargo test --lib strategies::reddit strategies::twitter`。
**最终验收**:同上 `-- --nocapture` + `cargo build --all-features`。
**反作弊声明**:实现者不得修改断言;不得引入任何新总量上限。

---

### M4-T2 reddit content 路径 after 翻页(PaginationLoop)+ D1 override(T-012)

**覆盖 ID**:R-004、T-012、I-001~I-004/F-001/F-002/F-004/F-005(reddit 实例)、PV-003(mock 侧)、FR-003、D-01。依赖:M4-T1。
**文件**:`src/adapters/reddit.rs`(循环 + override + 测试;mock-HTTP helper 按 FR-003 模块内复制)。

**测试载荷(mock 响应形状取自 `reddit_types.rs` serde 定义 + 评论侧既有请求样板,**标注待回灌确认**——仓内无 reddit/twitter 搜索响应真实样本,Step 06 实证;M4-T4 回灌后逐字段对账)**:
1. `paginates_after_until_count`:3 页(各含若干 posts,hasNextPage=true/true/false,endCursor=c2/c3)、count 跨页 → 达量或全收;**捕获请求断言第 2/3 请求转发 after=c2/c3**(T-012 主断言)。
2. `exhausted_when_has_next_false`:2 页后 hasNextPage=false 且未达 count → `Some(Exhausted)`。
3. `repeated_end_cursor_stops`(F-004):endCursor 重复 → 终止、`Some(Exhausted)`、不发额外请求。
4. `empty_pages_stop_at_limit`(F-005):3 连空页(hasNextPage=true,cursor 递进)→ `Some(Exhausted)`。
5. `partial_failure_with_progress`(F-002):第 2 页 429 → 首页条目保留、`Some(PartialFailure{..})` 含 "rate"。**DR-19**:mock 429 响应必须带 `Retry-After: 0`,且按 tikhub client 重试语义排队——RateLimited 默认延迟 60s × max_retries=3(error.rs:140-153),不带 `Retry-After: 0` 会实睡 ~180s 撞 mutants 300s 预算;同一页 429 = 4 次 HTTP 请求,**请求数期望按 4 计**。
6. `zero_progress_error_is_err`(F-001):第 1 页即 429 → `Err`(**允许先绿**,AG-006)。**DR-19 同测试 5**:`Retry-After: 0` + 重试排队,请求数期望按 4 计。
7. `shortfall_none_when_reached`:达量 → `None`。
8. `legacy_search_unchanged`:既有 `search()` 同 mock 返回与 outcome.contents 一致(回归;**允许先绿**,AG-006)。
9. `hard_error_with_progress_is_err`(DR-10,同 M3 形状):第 1 页有进展 + 第 2 页 HTTP 500(硬错误)→ 整体 `Err`(**PartialFailure 触发集仅 RateLimited,M1 D2 冻结**;硬错误不得降级为 Partial)。**预期 RED**:现状单次调用返回首页 Ok(shortfall=None)→ `expected Err, got Ok(..)`。

**预期 RED**(现状单次调用 + take):测试 1 捕获请求数 1≠3 且 `after` 从未转发(`assertion failed: requests[1].contains("after=c2")`);测试 2 `left: None, right: Some(Exhausted)`;测试 9 `expected Err, got Ok(..)`。
**GREEN 命令**:`cargo test --lib adapters::reddit`。**GREEN 规格补充(F-02)**:循环终止时记一条结构化日志:platform、accepted_count、StopReason/Partial(可观测性横切,Step 05 F-02)。**最终验收**:同上 + `cargo build --all-features`。
**反作弊声明**:不得修改断言;不得手写终止逻辑绕开 PaginationLoop(D-01);不得特判 mock。

---

### M4-T3 twitter content 路径 cursor 翻页(PaginationLoop)+ D1 override(T-013)

**覆盖 ID**:R-005、T-013、I-001~I-004/F-001/F-002/F-004/F-005(twitter 实例)、PV-004(mock 侧)、FR-003、D-01。依赖:M4-T1;与 M4-T2 互相独立。
**文件**:`src/adapters/twitter.rs`(循环 + override + 测试;twitter.rs:456 已有 helper 可扩展)。

**测试载荷(镜像 M4-T2 各条,差异点;mock 响应形状取自 `twitter_types.rs` serde 定义 + 评论侧既有请求样板,**标注待回灌确认**——仓内无 twitter 搜索响应真实样本,Step 06 实证;M4-T4 回灌后逐字段对账)**:
1. `paginates_cursor_until_count`:断言第 2/3 请求转发 `cursor=`(T-013 主断言:search_params 现状从不设 cursor,twitter.rs:161-163)。
2. `exhausted_when_next_cursor_missing`:next_cursor 缺失 → `Some(Exhausted)`。
3. `repeated_next_cursor_stops`(F-004):**twitter 已知会返回重复 cursor**,状态机 CursorLoop 兜底 → `Some(Exhausted)`。
4~8. 空页上限 / partial / 零进展 / 达量 None / 既有 search 回归(同 M4-T2 形状;6/8 **允许先绿** + AG-006;**DR-19**:5/6 两条 429 测试同 M4-T2 规定——mock 429 带 `Retry-After: 0` + 按重试语义排队,同一页 429 = 4 次 HTTP 请求,请求数期望按 4 计)。
9. `filtered_items_drive_counting`:含非 tweet 类型条目的页(`is_search_result_tweet` 过滤)→ 计数以过滤后为准(达量判定不被噪声条目充数)。
10. `hard_error_with_progress_is_err`(DR-10,同 M3 形状):第 1 页有进展 + 第 2 页 HTTP 500 → 整体 `Err`(**PartialFailure 触发集仅 RateLimited,M1 D2 冻结**)。**预期 RED**:现状单次调用返回首页 Ok → `expected Err, got Ok(..)`。

**预期 RED**:测试 1 `assertion failed: requests[1].contains("cursor=")`(现状从不转发);测试 2 `left: None, right: Some(Exhausted)`;测试 9 过滤前计数实现错误时 `left: 50, right: 47` 类;测试 10 `expected Err, got Ok(..)`。
**GREEN 命令**:`cargo test --lib adapters::twitter`。**GREEN 规格补充(F-02)**:循环终止时记一条结构化日志:platform、accepted_count、StopReason/Partial(可观测性横切,Step 05 F-02)。**最终验收**:同上 + `cargo build --all-features`。
**反作弊声明**:同 M4-T2;过滤计数语义(测试 9)不得弱化为「按原始条目计数」。

---

### M4-T4 T-052 real gate:reddit/twitter 真实第二页 + fixture 回灌(P-003)

**覆盖 ID**:T-052、P-003、PV-003/PV-004(real + 回灌)、AG-007。依赖:M4-T2/T3。
**文件**:`tests/real_api_test.rs`(扩展)、`tests/fixtures/{reddit,twitter}/`(第二页回灌)。

**Gate 控制**:`TIKHUB_API_KEY`;**每平台 ≤3 = HTTP 请求上界**(P-003;DR-19:探针用无重试调用或零重试配置,确保「调用数 = 请求数」,预算按 HTTP 请求计);只读无清理;AG-007 漂移区分。
**测试载荷**:`real_reddit_search_second_page_after` / `real_twitter_search_second_page_cursor` —— 各:首页取 endCursor/next_cursor → 带 cursor 重发 → reddit:断言第二页非错误、内容前进(id 集合不全同);原始响应回灌 fixture(PV 控制:此后 mock 字段修订必须取自回灌样本)。**twitter 重复 cursor 实测(DR-16,分支式写死断言,执行期零断言改动)**:`if second_cursor == first_cursor { 记录证据(F-004 现实依据)+ 仅断言响应非错误,PASS } else { 断言 id 集合不全同 }`——两路径均为计划契约。**对账义务(DR-20)**:回灌后逐字段对账 M4-T2/T3 的 mock fixture;不符 → 停下上报,mock 修订走 ASSERTION-CHANGE-JUSTIFIED + root 知会(同 M3-T3 模式)。
**RED/GREEN 说明**:live 契约探针,**允许先绿**(书面理由交 Test-Gate Reviewer)。
**验收命令**:`TIKHUB_API_KEY=… cargo test --test real_api_test real_reddit_search_second_page_after real_twitter_search_second_page_cursor -- --nocapture`(输出留存)。
**反作弊声明**:不得修改断言;漂移如实区分上报。

---

### M4-T5 模块收尾 gate

**覆盖 ID**:AG-010~AG-012、R-012(自查)、FR-005(引用)。依赖:T1~T4。
**命令序列**(同 M2-T7/M3-T4 形状,输出留存):`cargo test --lib` / `cargo test` → AG-012 预检(无 missed 或书面豁免;**两条 cap 行与两循环终止分支不接受静默豁免**)→ R-012 自查零命中(migrations/schema/protocol_gen;redis.rs/ports 零改动)→ 证据归档 → `rust-verify-change`。
**反作弊声明**:gate 红 = 回对应任务修生产代码。

## 4. 完成判据与独立验证(03-split §5 M4 行)

T-001(reddit/twitter)/T-012/T-013 green + RED 证据;gated T-052 输出 + 两平台第二页 fixture 回灌;mutants 无 missed。命令:`cargo test --lib strategies::reddit strategies::twitter adapters::reddit adapters::twitter`;`TIKHUB_API_KEY=… cargo test --test real_api_test -- --nocapture`;AG-012 预检。
「允许先绿」清单(书面理由交 Test-Gate Reviewer):M4-T1 测试 3(AG-006 由 AG-012 覆盖,cap 行在 diff 内)/ 测试 4;M4-T2 测试 6/8;M4-T3 测试 6/8;M4-T4 live 探针。

## 5. 账本覆盖映射

| ID | 任务 | 备注 |
|---|---|---|
| R-001(reddit/twitter)/ T-001(两行) | M4-T1 | — |
| R-004 / T-012 / PV-003(mock) | M4-T2 | after/hasNextPage 镜像移植 |
| R-005 / T-013 / PV-004(mock) | M4-T3 | cursor/next_cursor 镜像移植 |
| T-052 / P-003 / PV-003·004(real) | M4-T4 | 双平台第二页实测 |
| I-001~I-004 / F-001/F-002/F-004/F-005(实例) | M4-T2 + M4-T3 | 机制归 M1 |
| DR-10(有进展硬错误 → Err,reddit/twitter 实例) | M4-T2(测试 9)+ M4-T3(测试 10)`hard_error_with_progress_is_err` | 触发集仅 RateLimited(M1 D2 冻结);Step 07 patch |
| DR-16(twitter 重复 cursor 分支断言)/ DR-20(mock 形状待回灌对账) | M4-T4 | Step 07 patch |
| FR-003 | M4-T2/T3(helper 复用/复制) | 提升裁决已登记 root(M3 §6.3) |
| AG-010~012 / R-012 / FR-005 | M4-T5 | — |
| AG-001~007 / AG-020~023 / D-02 豁免 | §1 继承 + M4-T1 注释 | — |

> 自查:03-split M4 行全部 ID(R-001 两行/R-004/R-005、T-001 两行/T-012/T-013/T-052、PV-003/PV-004、P-003)已认领。✅

## 6. 开放问题

1. twitter live 重复 cursor 行为以 T-052 实测为准(M4-T4 已改 DR-16 分支式写死断言,两路径均为计划契约);影响仅证据描述,不影响 F-004 设计。
2. helper 提升裁决随 M3 §6.3 走 root;M4 不重复登记。
3. **Step 07 patch 记录见 `patches/07-batchF-platforms.md`**(DR-10/DR-15/DR-16/DR-19/DR-20、F-02)。
