# Traceability Compile — Shard M3/M4/M5(平台计划)

> Compiler 分片执行者,2026-06-10。输入:`plans/modules/{m3,m4,m5}-*.md`、`03-split.md` §3/§5、`plans/root.md` §1、`ledgers/{requirements,test-suite,providers,production-dependencies,assumptions,non-code-exceptions}.md`、`04-adjudications.md`、`handoffs/07-to-08.md`、`patches/07-batchF-platforms.md`。
> 须知项已按 07-to-08 §Compiler 须知对待:M3「迁移到共享契约」改写 + RED 重分类(PR #5 消解 → 金丝雀)+「待执行期复核」= 有意设计/显式开放项;M5 分支互斥 NOT-TAKEN;V1 OPEN 仅 gate M5 分支。

## 逐项裁定

### 1. 03-split §3 归属 ID 全认领且任务实存 — **PASS**

- M3 行(03-split §3.3/§3.1/§3.4):R-001(tiktok)/R-003、T-001(tiktok)/T-011/T-051、PV-002、P-002 → M3 §5 映射表逐条认领,自查行 ✅;另认领 DR-10/DR-18(Step 07 patch 新增,见 batchF)。
- M4 行:R-001(reddit/twitter)/R-004/R-005、T-001 两行/T-012/T-013/T-052、PV-003/PV-004、P-003 → M4 §5 全认领,自查行 ✅。
- M5 行:R-001(ig)/R-006/V1、T-001(ig)/T-014/T-053、PV-005、P-004、C-005、N-001 → M5 §5 全认领,自查行 ✅(R-001 ig 行为 batchF F-07 闭合项,已入 §5)。
- F-004/F-005「M3/M4/M5 各自循环复制」义务(03-split §3.2)→ M3-T2 测试 4/5、M4-T2 测试 3/4、M4-T3 测试 3/4~8、M5-T3-A 测试 3/4 实存。
- **抽样下钻(10 条,均验证任务文本实存且断言具体)**:
  | ID | 落点 | 实存证据 |
  |---|---|---|
  | R-003/T-011 | M3-T2 测试 1 | offset 序列 0,20,40 + count=20 捕获断言 |
  | T-051/P-002 | M3-T3 | ≤3 HTTP 上界 + 第二页 fixture 回灌 + cursor 语义结论义务 |
  | PV-002(mock) | M3-T2 载荷头 | 形状取自 `tests/fixtures/tiktok/search_travel_us.json`,禁捏造 |
  | DR-18 | M3-T2 测试 10 | 非数字 cursor → Exhausted,真 RED(`left: None, right: Some(Exhausted)`) |
  | R-004/T-012 | M4-T2 测试 1/2 | after=c2/c3 转发捕获断言 + hasNextPage=false → Exhausted |
  | R-005/T-013 | M4-T3 测试 1 | `cursor=` 转发(现状 twitter.rs:161-163 从不设,真 RED) |
  | T-052/P-003 | M4-T4 | 双平台第二页 + DR-16 分支式写死断言 + DR-20 对账 |
  | V1/N-001/T-053/C-005 | M5-T1 | 文档核查 + ≤4 探测 + 双账本写回义务 |
  | R-006(单页分支) | M5-T3-B 测试 1 | 欠量 → `Some(Exhausted)`,R-006 红线钉子 |
  | T-014 | M5-T3-A 测试 1 / T3-B 测试 1 | 分支互斥,各有主断言 |

### 2. 每任务 RED/GREEN/验收/反作弊四件套 + M3 RED 重分类 — **PASS(带 2 条 minor FAIL)**

- M3-T1/T2/T3、M4-T1~T4、M5-T1/T2:四件套齐(收尾 gate 任务 M3-T4/M4-T5/M5-T4 为命令序列 + 反作弊形态,属 gate 任务合法形状)。
- M3 RED 重分类逐条核:
  - M3-T1 测试 1/2:消解原因(cap 行已在 main 删除,`git show 4ab34ca` 实证)+ 金丝雀程序具体(临时恢复 `.min(20)` 须红)✅;测试 5 AG-006 由 AG-012 覆盖 ✅;测试 3 仍真 RED(无 PAGE_SIZE extra)✅。
  - M3-T2 测试 1:消解子断言(请求数 3)标金丝雀且程序具体(循环体改单次调用须使期望 3 实得 1 变红);count=固定 20 子断言如实标「待执行期复核」(显式开放项,非 placeholder)✅。测试 7/9 允许先绿带 AG-006/AG-012 归属 ✅。测试 11(DR-10)如实记录「PR #5 现状任意错误即 Err → 迁移前先绿」+ 金丝雀(把 500 加入 Partial 触发集须红)✅——无「悄悄先绿」。
  - M3-T3 / M4-T4 / M5-T1:AG-008 探针类允许先绿,书面理由义务交 Test-Gate Reviewer,均显式声明 ✅。
- **minor FAIL 2a**:M5-T3-A 与 M5-T3-B 缺「最终验收」行(M3-T2/M4-T2/M4-T3 同形任务均有「同上 + `cargo build --all-features`」)。**Patch 建议**:两任务 GREEN 命令行后各加一句「**最终验收**:同上 + `cargo build --all-features`」。
- **minor FAIL 2b**:M4 §4 有聚合「允许先绿」清单(逐条归属),M3 §4 / M5 §4 无同形聚合清单(逐条标注散在任务文本与 M3 现状核对「RED 预期改写」段,实质齐全,但与 07-to-08 须知「清单在各计划 §5/完成判据」的形状不一致,增加 Test-Gate 逐条核归属成本)。**Patch 建议**:M3 §4 末尾补一行聚合清单(T1 测试 1/2/5;T2 测试 1 子断言/7/9/11;T3 探针),M5 §4 同(T1 探针;T2 None 缺省;T3-A 测试 4 两条;T3-B 测试 3/4)。

### 3. Real gates(T-051/T-052/T-053)控制四要素 — **PASS**

| Gate | 凭据 env | 调用预算(=HTTP 请求上界,DR-19) | 只读/清理 | 回灌 + 对账 |
|---|---|---|---|---|
| T-051(M3-T3) | `TIKHUB_API_KEY`(real_api_test.rs:3-4 既有约定) | ≤3,零重试保证调用数=请求数 ✅ | 只读无清理 ✅ | `search_travel_us_page2.json` 回灌 + cursor 推进语义结论写 fixture 旁注;不符 → 停下上报 ASSERTION-CHANGE-JUSTIFIED + root 知会(= M3-T3 对账义务)✅ |
| T-052(M4-T4) | `TIKHUB_API_KEY` | 每平台 ≤3(与 PV-003/004「同 PV-002 ≤3」一致),零重试 ✅ | 只读无清理 ✅ | 两平台第二页回灌 + **DR-20 逐字段对账** mock fixture;不符走 ASSERTION-CHANGE + root 知会 ✅;DR-16 twitter 重复 cursor 分支式写死断言(执行期零断言改动)✅ |
| T-053(M5-T1) | `TIKHUB_API_KEY` | ≤4(P-004),零重试;文档明确可省至 2 ✅ | 只读无清理 ✅ | 两页原始 JSON 回灌 `tests/fixtures/instagram/`(PV-005);T3-A mock 形状取自回灌样本 = 对账闭环 ✅ |

预算口径与 production-dependencies P-002/P-003/P-004、providers PV-002~005 行一致;DR-19 的 429 测试侧(mock `Retry-After: 0` + 4 请求口径)在 M3-T2 测试 6/7、M4-T2 测试 5/6、M4-T3 4~8、M5-T3-A.4/T3-B.3 全部落地。

### 4. Mock 形状来源声明诚实 — **PASS(带 1 条 minor FAIL)**

- M3-T2:「取自 `tests/fixtures/tiktok/search_travel_us.json` 字段集,禁凭空捏造」= 既有 fixture ✅(与 PV-002 行一致)。
- M4-T2/T3:「取自 `reddit_types.rs`/`twitter_types.rs` serde 定义 + 评论侧既有请求样板,**标注待回灌确认**——仓内无真实样本,Step 06 实证;M4-T4 回灌后逐字段对账」✅ 诚实(不冒充真实样本)。
- M5-T3-A:「fixture 形状取自 T-053 回灌样本,禁凭空捏造」✅。
- **minor FAIL 4a**:M5-T3-B 测试载荷未声明 mock 形状来源(T3-A 有,T3-B 无;§5 表 PV-005 mock 侧归「T3-x」含 T3-B)。**Patch 建议**:T3-B 测试载荷头加「(mock-HTTP,fixture 形状取自 T-053 回灌样本,禁凭空捏造)」。

### 5. V1 判定标准/写回义务/分支互斥闭环;N-001 在册 — **PASS**

- 判定标准:M5 §2.1 原文采纳 assumptions.md V1 行(「非错误且内容异于首页 → 分支 A;4xx 或相同首页 → 分支 B」),两文本逐句一致 ✅;模糊结果保守判 B 并留原始证据(M5-T1 反作弊声明)✅。
- 写回义务:M5 §1.2「不写回 assumptions.md V1 行 + C-005 则模块不得标完成」+ §2.4 状态面 + T1 验收命令含两账本 diff ✅。
- 分支互斥:§1.1 NOT-TAKEN 机制(只实现一条、未选分支记录判定依据,不算未完成)+ §5 表「T-014 → M5-T3-A 或 T3-B 按 V1」闭环 ✅;DR-10 在分支 B 显式注明不适用(形状不存在)✅。
- V1 OPEN 仅 gate M5 分支(D-12;M5-T2 分支无关可并行)✅;N-001 在 non-code-exceptions.md 行 7,状态 OPEN(M5-T1 前置),与 M5-T1 步骤 1 对齐 ✅。

### 6. M3 既有测试处置段 + user-videos backlog — **PASS**

- M3-T2 末「既有测试处置」段在册:`f3_empty_second_page_terminates_at_20` / `r6a_empty_first_page_returns_empty_vec` 迁移撞红预告 + ASSERTION-CHANGE-JUSTIFIED 理由模板 + 修订/删除两条合法路径(测试硬规则 §1(b)/(c))+ **修订由测试子 agent 在测试上下文执行,实现子 agent 不得单方面删改** + 修订版先红后绿留证据 ✅(与全局测试硬规则的职责分离条款一致)。
- user-videos 双循环共存:M3 §2.2 边界条目(`UserVideoPageFetcher` 维持 PR #5 私有循环、归并登记 backlog、不阻塞 M3、`tikhub_user_videos_pagination.rs` 不受影响)✅;07-to-08 开放项清单同步在册 ✅。

### 7. PAGE_SIZE 载体表述与 root §1 D4 一致 — **PASS**

- root §1 D4:「hint 按平台可控性消费——tiktok 经 `extra_keys::PAGE_SIZE` 下发(M3 §2.1 载体,一并冻结);facebook/reddit/twitter/instagram(V3/V2)上游无单页参数,文档化豁免」。
- M3 §2.1/§2.3:仅 tiktok 消费,形状提请 root 冻结(root 已冻结,闭环);M5 豁免限定句「除非 V1 证据显示上游接受单页参数且经 root 修订豁免表」= root §1 头部修订路径(变更须经本计划修订 + Cross-Service Reviewer)的实例,不矛盾 ✅。
- M4 §2.1:「PAGE_SIZE extra 不读(D-02 豁免,代码注释固化)」+ M4-T1 GREEN 含豁免注释 ✅;M5 §2.2:V3/V2 无 count 参数(client.rs:710-770 实证)→ 豁免注释 ✅。
- D-04 cap 引用:M3 tiktok=20、M4 reddit=100/twitter=100、M5 ig=50,均与冻结表一致 ✅。

### 8. 无 placeholder;无改弱断言/skip 手段;D-01/D-02/D-04 引用一致 — **PASS**

- 唯二「占位/待定」均为显式开放项:M3 per-request count 语义「待执行期复核」(07-to-08 明示非 placeholder);M5-T3-A token 参数名「占位语义,V1 实测定」且登记 §6.1 开放问题 + 停下上报路径 ✅。
- 达成手段核查:全部撞红处置走修生产代码或 ASSERTION-CHANGE-JUSTIFIED 流程(M3-T2 处置段、M3-T3/M4-T4 对账);无新增 skip/ignore 形态;real gate 用 env-gate skip(既有合法模式,非绕过失败);各任务反作弊声明含「不得修改断言」「不得绕开 PaginationLoop(D-01 红线,含保留 PR #5 手写逻辑不迁移)」「不得特判 mock」✅。
- D-01 引用一致:三计划头部均「新循环必须接 PaginationLoop;facebook 豁免不可迁移」= 04-adjudications 直接指令原文 ✅。D-02:M3「tiktok 有 count 参数可下发」/M4「两平台实证无单页参数,同 fb 豁免」/M5「V3/V2 无 count 参数豁免」与 D-02 裁决及 root D4 措辞一致 ✅。D-04 同上项 7 ✅。

### 9. 模块独立验证命令与 03-split §5 一致 — **PASS**

- M3 §4 vs 03-split §5 M3 行:判据同(T-001 tiktok/T-011 green + RED 证据;T-051 输出 + fixture 回灌);命令 `cargo test --lib strategies::tiktok adapters::tikhub` 为靶向细化,全量 `cargo test` + mutants 由 M3-T4 承载,为超集非冲突 ✅。迁移改写产生的偏差(部分 RED 证据形态从「红」改「金丝雀输出」)已在 M3 现状核对/「RED 预期改写」段与 07-to-08 须知中显式说明 ✅;mutants 判据(03-split §5「mutants 无 missed」)在 M3 §4 + M3-T4 在册 ✅。
- M4 §4 vs §5 M4 行(「同 M3 形式」):靶向命令 + real 命令 + AG-012,且含聚合允许先绿清单,一致 ✅。
- M5 §4 vs §5 M5 行:V1 写回 + 所选分支 T-014 green + RED + T-053 ≤4 调用 + AG-012,逐项一致 ✅。

## FAIL 清单(全部 minor,不阻断)

| # | 位置 | 缺口 | Patch 建议 |
|---|---|---|---|
| 2a | M5-T3-A / M5-T3-B | 缺「最终验收」行(同形任务 M3-T2/M4-T2/T3 均有) | 各加「**最终验收**:同上 + `cargo build --all-features`」 |
| 2b | M3 §4 / M5 §4 | 无 M4 式聚合「允许先绿」清单(逐条标注实质齐全,但与 07-to-08 须知的落位形状不一致) | M3 §4 补:T1 测试 1/2/5;T2 测试 1 子断言/7/9/11;T3 探针。M5 §4 补:T1 探针;T2 缺省;T3-A 测试 4 两条;T3-B 测试 3/4 |
| 4a | M5-T3-B 测试载荷头 | 未声明 mock 形状来源(T3-A 已声明) | 加「mock 形状取自 T-053 回灌样本,禁凭空捏造(PV-005)」 |

## 抽样清单

R-003/T-011(M3-T2.1)、T-051/P-002(M3-T3)、PV-002 mock(M3-T2 载荷头)、DR-18(M3-T2.10)、R-004/T-012(M4-T2.1/2)、R-005/T-013(M4-T3.1)、T-052/P-003+DR-16/DR-20(M4-T4)、V1/N-001/T-053/C-005(M5-T1)、R-006 单页钉子(M5-T3-B.1)、T-014 双分支(M5-T3-A.1/T3-B.1)。

## 分片裁定

**PASS(条件通过)** — 9 项检查 6 项干净 PASS、3 项 PASS 带 minor FAIL(2a/2b/4a,合计 3 条,均为表述/落位补齐,零设计缺口、零反作弊缺口、零账本失认领)。M3「迁移到共享契约」改写、RED 重分类(全部带消解原因 + 具体金丝雀程序)、M5 分支互斥与 V1 写回闭环均符合 07-to-08 已知有意设计;real gates 三件(T-051/052/053)控制四要素齐备且与 providers/production-dependencies 账本逐行一致。建议 3 条 minor patch 在 root 收口或下一 patch 批次顺手落,不阻塞 compile。
