# Traceability Compile — 分片报告:M1 + M2(P0 面)

> Compiler 分片执行(2026-06-10)。输入:`plans/modules/m1-pagination-core.md`、`plans/modules/m2-facebook-p0.md`、`03-split.md` §3/§5、`04-adjudications.md`、`handoffs/07-to-08.md`、`constraints/testing-constraints.md`、ledgers/{requirements, test-suite, anti-gaming-test-quality, invariants-failures, production-dependencies, providers}.md(M1/M2 相关行)。
> 已按 handoff §Compiler 须知预先排除误判面:允许先绿三形态、M3 RED 重分类(本分片不涉)、D3 六行、显式开放项 ≠ placeholder。

## 检查项逐条裁定

### 1. 覆盖映射完整性(03-split §3 M1/M2 每个 ID → 计划 §5/§7 认领任务 → 任务正文实际存在)— **FAIL(1 处动作缺失)**

**PASS 部分(全量核对)**:
- 03-split §3 归 M1 的全部 ID —— R-007/R-008/R-009/R-011(agent)/R-012(自查)/R-001(解耦机制语境)、I-001~I-005/I-008/I-009、F-001~F-005(语义/机制)/F-009、T-002/T-003/T-004/T-030~T-034、C-001/C-002(agent)/C-004(agent)、P-006、N-002(agent)、FR-001/FR-003/FR-005、AG-001~AG-007/AG-010~AG-012/AG-020~AG-024、A005(drop)/A006/A007、B2/B4 —— 均在 M1 §5 表认领,且每条对应任务正文(M1-T0~T7)实际含测试名/动作。Step 07 新增行(DR-01/02/03/10/11、D-13、D-10/D-14①/DR-07/08、legacy 桥接)亦全部在册。
- 03-split §3 归 M2 的全部 ID —— R-001(fb)/R-002、T-001(fb)/T-010/T-015/T-016/T-017/T-040/T-050/T-054、P-001/P-005、PV-001、I-006、F-001~F-006/F-009(证据侧)、FR-003/FR-005、AG 族 —— 均在 M2 §7 表认领并有任务正文;M2 §7 末覆盖自查与 03-split §3 点名一致。
- shared 条目双侧齐:R-011(M1-T5 / N-002 scheduler 归 M6)、C-002(M1-T5 / M6)、C-004(M1-T6 pin / M6 读方)、F-003(M1-T2+T4 / M2-T2.2+T4.1 / 区分值归 M6)、I-006(M2-T6 + root)。

**FAIL-1:F-009「兼容自查」动作在 M2-T7 正文缺失。**
- 证据:M2-T7 覆盖 ID 列「F-009(兼容自查)」;invariants-failures.md F-009 行写「postgres.rs 零 diff 自查(M1-T7/**M2-T7**)」;但 M2-T7 命令块第 3 条只查 `migrations/ src/schema.rs src/db/schema.rs src/protocol_gen/`,**没有** M1-T7 拥有的 `git diff main...HEAD -- src/adapters/postgres.rs` 零改动自查行;M2-T7 验收判据 3 也只引 R-012。账本声明的动作在认领任务正文不存在。
- Patch 建议(确切):在 `plans/modules/m2-facebook-p0.md` M2-T7 命令块第 3 段追加一行:
  ```bash
  git diff main...HEAD -- src/adapters/postgres.rs   # 期望:零改动(F-009:新终态走既有值,fallback 路径不触及;real-DB 主路径证据 = M2-T6 T-054)
  ```
  并把验收判据 3 改为「第 3 条 diff 自查零命中(R-012 + F-009 postgres.rs 零改动)」。

### 2. 每个 code/behavior/integration/provider 任务:RED 先行 + 确切预期 RED 失败信息 + GREEN 命令 + 最终验收命令 + 反作弊声明 — **FAIL(M2-T2 三条测试无 RED 可能且未归类允许先绿)**

**PASS 部分**:M1-T1~T6、M2-T1、M2-T4、M2-T5 每任务四件套齐全且 RED 文本具体到断言(抽样下钻见末节);M1-T0 为基建型(零断言改动),以前后输出对照为证据,属 handoff 认可的有意设计;M1-T7/M2-T7 为 gate 任务(无新测试),反作弊声明在;M2-T6 为 AG-008 探针(见检查项 3);全部 15 个任务均有反作弊声明。

**FAIL-2:M2-T2 测试 1/10/13 在计划自己声明的 RED 基线下必然先绿,但既无 RED 预言、也不在 M2 §5「允许先绿」清单。**
- 证据:M2-T2 的 RED 基线 = 「仅有 M1 默认方法、facebook 未 override」。该状态下 D1 默认方法包装既有 `fetch_by_keyword`(facebook 既有循环已翻页到 count、转发 cursor、非 RateLimited 错误整体 Err——R-002/F-002 账本实证),因此:
  - 测试 1 `fetch_outcome_reaches_count_no_shortfall`:期望 50 条 + `shortfall.is_none()` + cursor 转发 —— 默认方法状态下三者全成立 → 先绿;
  - 测试 10 `hard_error_with_progress_is_err`:期望整体 `Err` —— 既有循环对非 429 错误本就 `return Err`(facebook.rs:577 先例,F-002 账本注)→ 先绿;
  - 测试 13 `candidates_inner_exhaustion_not_leaked`:期望 `shortfall == None` —— 默认方法恒 None → 先绿。
  M2-T2「预期 RED 失败信息」仅列测试 2/5/6/8(测试 3/4/9/11/12 可由同形状推出真红),测试 1/10/13 无 RED 路径;M2 §5 允许先绿清单(M2-T1.4、M2-T2.7、M2-T3.1~4、M2-T4.3、T-050/T-054)不含此三条 → 按 §5「每个非允许先绿测试有 RED 证据」要求,执行期将无法如实交付,构成诱导补做伪 RED 或如实报告冲突。
- Patch 建议(确切):在 `plans/modules/m2-facebook-p0.md` 做两处修改:
  1. M2-T2 测试 1/10/13 各加标注「**允许先绿(AG-006)**」,并写明金丝雀变异对象(手工金丝雀形态,程序同 DR-12:只动生产代码、RED 输出留存后还原):
     - T2.1:临时在 override 内 `contents.truncate(20)`(或恢复循环 20 截断)→ 本测试须红;
     - T2.10:临时把非 RateLimited 错误分支收敛为 `PartialFailure` → 本测试须红;
     - T2.13:临时把内层 `fetch_page_posts_paginated` 枯竭外泄为整体 `Some(Exhausted)` → 本测试须红。
     (三者对应代码均在 M2 diff 内,AG-012 预检亦应覆盖;金丝雀为预检不生成对应变异时的兜底。)
  2. M2 §5 允许先绿清单同步扩为「M2-T1.4、M2-T2.1、M2-T2.7、M2-T2.10、M2-T2.13、M2-T3.1~4、M2-T4.3(+T-050/T-054 探针)」。
- 附注(不计 FAIL,执行期注意):M1-T5 测试 3 `non_positive_search_limit_falls_back_to_none` 在「骨架字段(默认 None)已补、映射未实现」状态下亦先绿;其 RED 依据 = 字段缺失时整批 `error[E0609]` 编译失败(M1-T5 RED 节已写),与 M1-T1 的「依赖即交付物,编译失败为正确 RED」同口径,勉强成立——执行期取证须以 E0609 输出为该测试的 RED 证据,不得跳过。

### 3. 「允许先绿」逐条归属三形态 + 证明义务具体 — **PASS**(现行清单内逐条核过;FAIL-2 的三条是「应入册而未入册」,归检查项 2)

| 条目 | 形态 | 证明义务 | 裁定 |
|---|---|---|---|
| M1-T3.1 | AG-006-diff | 默认方法在 diff 内,AG-012 变异须被本测试抓住 | PASS |
| M1-T4.4 / M1-T4.6 | AG-006-diff | fetch_content/process_keyword 重写在 M1 diff 内 | PASS |
| M1-T5.4(PT-5) | AG-006-diff + D-05 书面豁免预案 | 豁免文本具体(T-003c+PT-5 联合钉死),Test-Gate Reviewer 复核点已写 | PASS |
| M1-T6 | AG-006 手工金丝雀 | 变异对象具体(`"NO_MORE_POSSIBLE_DATA"`→`"X"`),程序写明只动生产串、留存后还原 | PASS |
| M2-T1.4 | AG-006-diff | cap 行在 diff 内 | PASS |
| M2-T2.7 | AG-006 手工金丝雀(DR-12) | 变异对象具体(`truncate(10)`) | PASS |
| M2-T3.1~3.4 | AG-006 手工金丝雀 | 四条各有变异对象(注释 seen-cursor break / `MAX_EMPTY_CURSOR_HOPS` 3→999 / 移除 RateLimited-partial 分支) | PASS |
| M2-T4.3 | AG-006-diff | 义务已声明;注意点:该测试走 mock gateway+orchestrator,而 orchestrator 改动在 M1 diff——同分支顺序执行时 `git diff main...HEAD` 含 M1 改动,可杀;若 M1 已合 main 后单跑 M2 预检,建议执行期按 M2-T3 同款金丝雀补证(P3 备注,不构成 FAIL) | PASS(带备注) |
| T-050 / T-054 | AG-008 探针 | M2-T6 注明「有效性判据按 AG-008(实跑输出留存 / fixture 回灌 / 判定可复算)」,三判据引用齐;AG-008 防滥用条款(确定性测试不得借道)未被违反 | PASS |

### 4. 变异门槛:数值门槛 + 确切命令 + 不可豁免清单在册 — **PASS**

- 门槛数值:M1 §1.8 / M2 §1.8「无 missed mutants 或逐个书面豁免(入 PR 描述,Test-Gate Reviewer 复核)」;与 AG-011 一致。
- 确切命令:M1-T7 / M2-T7 均为 `git diff main...HEAD > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff -- --all-features --test-threads=1`(= AG-012 原文);并含「预检须在 live key 未设环境执行(D-14)」防 live 相乘。
- 不可豁免清单:M2 §1.8「事故根因行(facebook.rs:131 cap 截断 + 适配器 shortfall 判定)不接受静默豁免」+ M2-T7 验收 2「三条循环(search/page/candidates)的 shortfall 出口各有 killing 测试,不接受静默豁免」(DR-17d)。CI 真·强制层(AG-010,mutation-rust.yml 已存在)被两计划引用。

### 5. 属性测试:核心逻辑有 PT 或 justified gap — **PASS**

- 共享翻页状态机:PT-1~PT-4(T-030~T-033,M1-T2)对应 I-001~I-004,生成器要点逐条对齐 AG-020~AG-023(含 PT-2 Step 07 可满足改写 + harness 冻结、PT-3 计划期修正,handoff 已认可);PT-5(T-034,M1-T5)对应 I-008/AG-024。
- redaction 属性化冒烟(M1-T1)补强 I-009。
- facebook 实现级循环无 PT:justified gap 在 test-suite §8 行 73(D-01 保留手写循环;防线 = 既有 8+ mock 测试 + M2-T2.8 逐形状对齐 + M2-T3 金丝雀),与检查清单要求的登记位置完全一致;F-010 构造器独立 mock gap 亦在 §8 行 74。

### 6. mock + real 双 gate — **PASS**

- 每 feature 确定性 mock:M1 全部任务为 mock/纯逻辑(文件/测试名/命令齐);M2 mock 面 = strategy 单测(M2-T1)+ 适配器 mock-HTTP(M2-T2/T3,扩展 facebook.rs:972-1075 样板,零新依赖 FR-003)+ orchestrator MockContentGateway(M2-T4/T5)。
- real gate 控制齐:
  - P-001/T-050:凭据 `FACEBOOK_RAPIDAPI_KEY`、≤3 调用、只读无清理、证据 = 实跑输出留存;429 计费重试预算注(DR-05)在。
  - P-005/T-054:`DATABASE_URL` + GitHub Actions 受限 skip(既有模式 271-289)、清理沿既有 task 隔离模式、证据 = consumed 守恒 + terminal_reason 落库可查。
  - PV-001:mock 侧 + real 输出回灌 fixture 控制(禁凭空捏造字段、标注来源 commit/日期)在 M2-T6。
  - P-006:部分 inapplicability 维持有证据(映射纯函数 + 不改入队/出队),且 M1 §5 写明触及则升级 real gate 的上报义务。
- gate 不入默认 `cargo test`;M1-T0(D-10/D-14①)把 live-API panic 噪声修为 env-skip + CI opt-in,零断言改动。

### 7. 状态键一写多读 — **PASS**

M1 D5 与 M2 §3 两表交叉核对:`TaskConfig.page_size_hint` 唯一写方 = redis.rs(M1),M2 显式声明自己只是读方;`FetchOutcome.shortfall` 写方 = 各平台适配器(facebook 写方 = M2-T2 override),读方 = orchestrator(M1-T4);`gm_crawler_tasks.terminal_reason` 写方 = agent postgres adapter(既有、值集不变 C-004),M6 为新增读方;`SearchOptions.count` 写方 = strategy,读方 = 适配器既有 `reached_post_limit`;`PaginationLoop` 内部状态不出模块(A005)。无重复写方,无写方漂移。

### 8. 模块独立验证命令存在且与 03-split §5 一致 — **PASS**

- M1 §4 vs 03-split §5 M1 行:`cargo test --lib pagination orchestrator redis content_gateway mock_gateway progress_tracker`(= 03-split「cargo test 指定测试名」的落实)+ mutants 命令逐字符一致。
- M2 §5 vs 03-split §5 M2 行:`cargo test --lib strategies::facebook adapters::facebook orchestrator` + mutants + 两条 gated 命令(facebook_real_api_test / facebook_real_db_test)一致;M2 在 db gate 命令上多带 `FACEBOOK_RAPIDAPI_KEY`(T-054 需 live 抓取喂库,属补全非偏离);AG-012 预检双方均在。

### 9. 无 placeholder;无任务以改弱断言/skip 为达成手段 — **PASS**

- grep `TODO|TBD|待定|待确认|占位|placeholder|FIXME` 于两计划:零命中。「断言形式实现期定但语义固定」(M1-T3.7)与「若 T-050 实测…执行期补」(M2 §6.3)为显式标注开放项,per handoff 不算 placeholder。
- skip/ignore 全部出现均为:AG-003 禁令原文、反作弊声明(「不得改用 ignore」)、M1-T0 env-gate 守卫(D-10/D-14 裁决:零断言改动、与既有 real-db 模式同构,属 gate 对齐非测试弱化)。无任务以改弱断言/加 skip 为达成手段;ASSERTION-CHANGE-JUSTIFIED 通道仅出现在合法场景(D-04 冻结表修订、AG-002 例外条款)。

### 10. D-01~D-15 裁决引用一致(M1/M2 范围)— **PASS**

| 裁决 | 计划内引用 | 一致性 |
|---|---|---|
| D-01(保留手写循环) | M2 §6.1 状态更新 + §1.10 + M2-T2 实现裁决 + T2.8 对齐断言强制项;test-suite §8 gap 行同步 | ✅(M3~M5 不可迁移注亦在 M2 §6 引用) |
| D-02(fb hint no-op) | M2 §6.3 + M1 脚注 F-08(消费名单以 root §1 D4 为准) | ✅ |
| D-03(不停 campaign) | M1 §6.1 / M2 §6.2;M1-T4.1 与 M2-T4.1「未调用 stop_campaign_gracefully」断言 | ✅ |
| D-04(cap 冻结表) | M1 §2 D4 取值 = D-04 表逐值相等(fb20/tt20/rd100/tw100/ig50/unknown20);M2 §6.4 继承 | ✅ |
| D-05(PT-5 豁免预案) | M1 §6.3 + M1-T5.4 豁免文本原文 | ✅ |
| D-10 / D-14① | M1-T0(并入 M1,提前于变异预检;D-14② 归 root,不在本分片) | ✅ |
| D-11(+增补) | M2-T5 RED 基线 =「含 M1、不含 M2 的 commit」+ worktree 方式;双金丝雀程序在 04-adjudications 增补,M2-T5 经 D-11 引用 | ✅ |
| D-13(任务级 max_count) | M1-T4.8 + GREEN 实现段 remaining 语义 | ✅ |
| D-15(空页计数改生产行) | M2 §2 D2 条目②(三处生产行)+ M2-T2.11 killing 测试 + M2-T3.3 复位语义;M1 D2 DR-03 注「对齐归 M2」 | ✅ |
| D-06~D-09、D-12 | M6/M5 域,不在本分片范围 | —(未见 M1/M2 误引) |

## 抽样下钻清单(ID → 任务 → 测试名 → RED 文本,14 条 ≥ 10)

| # | ID | 任务 | 测试名 | 预期 RED 文本(计划原文) |
|---|---|---|---|---|
| 1 | T-002/F-003 | M1-T4 | `exhausted_maps_to_no_more_possible_data` | `assertion failed: reason.starts_with("NO_MORE_POSSIBLE_DATA"), got "COMPLETED: Task completed successfully"` |
| 2 | T-003/C-001 | M1-T5 | `search_limit_becomes_page_size_hint` / `search_limit_clamped_to_platform_cap` | `left: None, right: Some(7)`;clamp 未实现 `left: Some(500), right: Some(20)` |
| 3 | T-004/I-009 | M1-T4.5 | `partial_failure_message_is_redacted` | `assertion failed: reason.contains("[REDACTED]")` |
| 4 | T-030/I-001 | M1-T2 PT-1 | `prop…accepted_count <= max_count` | `Test failed: assertion failed: loop.accepted_count() <= max_count; minimal failing input: ...` |
| 5 | T-031/I-002 | M1-T2 PT-2 | 终止性 + 枚举完备 | `assertion failed: steps <= seq.len() + 1 && matches!(last_decision, Stop(_)); minimal failing input: ...` |
| 6 | DR-03 | M1-T2 单测 7 | `repeated_content_pages_stop_at_empty_limit` | `left: Continue { cursor: "c3" }, right: Stop(EmptyPageLimit)` |
| 7 | DR-10 | M1-T3.6 | `non_rate_limited_error_with_progress_is_err` | `assertion failed: matches!(result, Err(_)); got Ok(FetchOutcome { contents: [..20..], shortfall: Some(PartialFailure { .. }) })` |
| 8 | DR-01 | M1-T3.7 | `partial_constructor_rejects_empty_contents` | `assertion failed: constructor rejects/normalizes empty contents; got FetchOutcome { contents: [], … }` |
| 9 | D-13 | M1-T4.8 | `two_keywords_share_task_level_max_count` | `assertion 'left == right' failed: left: 60, right: 50` |
| 10 | C-004(agent) | M1-T6 | `task_terminal_reason_codes_are_cross_service_contract` | 允许先绿;金丝雀:`"NO_MORE_POSSIBLE_DATA"`→`"X"` 须红(输出留存后还原) |
| 11 | R-001(fb)/T-001(fb) | M2-T1 | `search_count_is_total_not_capped_at_20` | `assertion 'left == right' failed: left: 20, right: 50`(137 行:`left: 20, right: 137`) |
| 12 | F-003(fb) | M2-T2.2 | `fetch_outcome_exhausted_when_cursor_null_before_count` | `left: None, right: Some(Exhausted)` |
| 13 | F-002/F-006(fb) | M2-T2.5 | `fetch_outcome_partial_failure_with_progress` | `left: None, right: Some(PartialFailure { .. })`;请求数 `1 + 4 == 5`(DR-05) |
| 14 | T-040(事故重演) | M2-T5 | `incident_269_shape_only_20_available_reports_no_more` | `assertion failed: reason.starts_with("NO_MORE_POSSIBLE_DATA"), got "COMPLETED: ..."`(RED 基线 = 含 M1 不含 M2 commit,D-11) |

四级链(ID→任务→测试名→RED 文本)全部贯通;14 条中 12 条为真 RED,2 条(#10 + T-050/T-054 探针)为合法允许先绿形态。

## 分片裁定

**分片裁定:FAIL;FAIL 项数:2**

| # | 检查项 | FAIL 内容 | Patch 落点 |
|---|---|---|---|
| FAIL-1 | 检查项 1(覆盖映射) | F-009「postgres.rs 零 diff 自查」在 M2-T7 覆盖 ID 与 invariants 账本均声明,但 M2-T7 命令块/验收判据缺该动作 | M2-T7 命令块第 3 段补 `git diff main...HEAD -- src/adapters/postgres.rs` 一行 + 验收判据 3 增注 F-009 |
| FAIL-2 | 检查项 2(RED 先行)/波及 3 | M2-T2 测试 1(`fetch_outcome_reaches_count_no_shortfall`)、10(`hard_error_with_progress_is_err`)、13(`candidates_inner_exhaustion_not_leaked`)在计划声明的 RED 基线下必然先绿,既无 RED 预言又不在允许先绿清单 | 三条标注「允许先绿(AG-006)」+ 各写金丝雀变异对象(truncate / 硬错误收敛 partial / 内层枯竭外泄);M2 §5 允许先绿清单同步扩列 |

两项均为局部 patch 可闭合,不动任何共享接口(D1~D4)、不动账本 owner 映射结构。
