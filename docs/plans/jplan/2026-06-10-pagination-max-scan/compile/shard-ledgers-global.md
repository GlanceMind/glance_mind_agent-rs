# Step 08 Traceability Compile — Shard: 账本全局完整性 + 第一性/假设处置(2026-06-10)

> 分片执行者:只读核查;输入 = 01-first-principles.md、03-split.md、全部 ledgers/*.md(11)、04-adjudications.md(D-01~D-15)、reviews/autoplan/summary.md、reviews/domain/summary.md + 7 个 domain 评审原文(抽查)、handoffs/07-to-08.md、plans/root.md + m1~m6 全部模块计划(现行版)、patches/07-batch{A..F}(closure rationale 抽查)、constraints/testing-constraints.md。

## 逐项裁定

### 1. 需求全覆盖 — **PASS**

- R-001~R-013 + V1 全部在 03-split §3.1 有 owner;与 requirements.md owner 列一致(账本用 Step-02 模块队列名 platform-strategies/pagination-core/once-guard,账本头部已声明「Step 03 可重排但 ID 不变」,§3.1 为权威重排表,无冲突)。
- Shared 行双侧标注:R-001(按平台分行,机制归 M1;五平台行分别被 M2-T1 / M3-T1 / M4-T1 / M5-T2 认领,F-07 的 ig 行悬空已闭合于 m5 §5)✅;R-011(M1-T5 agent 侧 + M6-T2 scheduler 侧)✅;R-007/R-008(M1 语义 + M2 集成证据,两侧 §5/§7 映射表互指)✅;R-012(root 终审 + 六模块各自收尾任务自查)✅。
- Origin 列:全部 `derived` 或 `incident`,**零 `assumed`** ✅。
- V1:owner = M5 首任务(M5-T1),阻塞面仅 R-006 分支(C-005/D-12),03-split §3.1 一致 ✅。

### 2. 假设处置 — **PASS**(附 1 条非阻塞观察,见 MINOR-2)

- 每行 disposition ∈ {keep, keep-justified, drop, ASSUMED→DERIVED 改写}:B1~B5 keep;A001/A002/A010 keep-justified;A003 ASSUMED→DERIVED 改写;A005 **drop**(带替代推导);A006~A009/A011/A012 keep ✅。交接零 ASSUMED 残留 ✅。
- A005 drop 作为范围变更显式:01 §6「关键降级」整段 + §7「Conventional approaches NOT taken」第一条 + 03-split M1 行「cursor 不过 trait,A005 drop」 + m1 §1.10 防漂移条款 + manifest 模块队列文案——四处一致 ✅。
- A007 注(DR-22):「每 tick 一次重派、跨 tick 无上限」与 invariants-failures F-001/F-007/§3 重试行、root §2.3/RT-1、N-005 改述一致 ✅。观察:m1 §2 D3 表(L119)与 m6 §1.10/T1.12、requirements R-008 行仍用「一次重派」短语(无 per-tick 限定)——在 eval_once 单次调用域内该读法准确、无语义矛盾,但 DR-22 patch 方向曾点名 M1-D3/M6 §1.10 → 登记 MINOR-2。

### 3. 第一性重构 — **PASS**

- Bedrock 集:01 §7 Reconstruction Note 列 B1~B5(各带来源)✅;§6 Why-Ladder 分类与 assumptions.md 对账一致 ✅。
- 被拒常规方案 ≥1:实为三个(trait 扩展 cursor / scheduler offset 多 task 续扫 / scheduler 自动补派),各带 bedrock 引用的拒绝理由(B2/B3/B4)✅;与 03-split §7 五条拆分决策记录(per-platform lane、M1 抽共享契约、reddit+twitter 合并、V1 并入 M5、M6 独立)无矛盾,first-principles 评审原文确认「三个常规方案均以 bedrock 理由拒绝」✅。
- 反作弊地板未被重分类/弱化:constraints/testing-constraints.md 七条硬规则完整;anti-gaming §1 AG-001~AG-008 完整。**AG-008 为补类别非弱化 AG-006**:防滥用句在场——「确定性测试不得借此类别逃避 AG-006;类别归属由计划文本显式标注并经 Test-Gate Reviewer 复核」(anti-gaming L17)✅;且 AG-008 自带三判据(输出留存/回灌 fixture/判定可复算)。「允许先绿」三形态(AG-006 变异 / AG-006 手工金丝雀 / AG-008 探针)在 m1 §4、m2 §5、m4 §4、m6 §4 清单逐条归类,与 handoff 07-to-08 Compiler 须知一致 ✅。

### 4. 失败矩阵闭合 — **PASS**

- I-001~I-009、F-001~F-010 每行验证列均指向实存测试/任务。下钻 6 条(验证列 → 计划任务名):
  1. I-002(含 Step 07 新增 (e) 枚举)→ PT-2/T-031 = **M1-T2**(harness 冻结规范 + 单测 7)✅
  2. I-006 → T-054 = **M2-T6.2 `real_facebook_paged_db_conservation`** + root §2.1/§2.4 双证终审 ✅
  3. I-007 → **M6-T1.11 `decision_unchanged_for_completed_underscan`** + M6-T2.1~3(纯观测字段)✅
  4. F-005 → **M1-T2 单测 4** + **M2-T2.4 / M2-T3.2-3.3**(请求级 + 复位,D-15 语义)✅
  5. F-008 → **M6-T1.4(枯竭不告警)+ M6-T1.1(欠扫告警)** 双向 ✅
  6. F-010 → **M1-T3 测试 7 `partial_constructor_rejects_empty_contents`** + **M2-T2.9 `date_filter_empty_delivery_with_429_is_err`** ✅(F-010 为 Step 07 新增行,03-split §3.6 表未列属预期——handoff 07-to-08 已显式声明「新增行 owner 以各计划 §5 现行表为准」,owner 在册)
- §3 横切行:进展定义(DR-09 → D1/M2 §2.2-b 过滤后口径,owner = M1/M2)✅;重试(DR-22 裁决落账,F-001/F-007 改述 + root RT-1)✅;可观测(agent 侧 = F-02 GREEN 规格落 M3-T2/M4-T2/T3/M5-T3-A;facebook 既有循环日志现状覆盖,Step 05 F-02 patch 范围如此裁定;scheduler 侧 = M6-T2 结构化 WARN+指标)✅。

### 5. 测试账本 — **PASS**

- T-001~T-054 全分配:03-split §3.3 与各模块 §5/§7 映射表逐 ID 对账(T-001 五平台分行→M2/M3/M4/M5;T-002~004/T-030~034→M1;T-010/015/016/017/040/050/054→M2;T-011/051→M3;T-012/013/052→M4;T-014/053→M5;T-020~022→M6)无遗漏、无双重唯一-owner 冲突 ✅。
- §8 justified gaps 五条各有理由,含 Step 07 新增两条(facebook 实现级 PT gap = TG-07/D-01 派生,防线 = 既有 mock 套件 + M2-T2.8 + M2-T3 金丝雀;F-010 构造器独立 mock gap = D1 类型级禁止 + M1-T3.7 + 变异门禁)✅。
- T-030~T-034 ↔ M1 PT 套件:T-030~033 落 M1-T2(src/pagination.rs)、T-034 落 M1-T5(redis.rs PT-5),与 AG-020~AG-024 生成器要点一一对应 ✅。

### 6. 生产依赖 / provider — **PASS**(附 1 条非阻塞观察,见 MINOR-3)

- P-001~P-007:每行有真实路径 + 凭据 gate + 成本/幂等控制;P-001/P-002 含 Step 06 实证纠偏(现状 panic → M1-T0 后 env-skip + CI opt-in);P-006 部分 inapplicability 带证据(映射纯函数 + 路径零改动 + 升级触发条款);P-007 经 D-07 落为 M6-T3 最小 gated 测试 ✅。
- PV-001~PV-005 mock+real 双侧齐备(mock 载体 + gate 测试逐项对应 T-010~017/T-050~053),fixture 回灌控制在册 ✅。
- D-14 口径三处一致:production-deps 结论(L20「D-14:M1-T0 守卫 + mutation workflow 移除 live key」)/ providers 控制汇总(L19 同口径)/ AG-012 预检文本(m1-T7 L377、m2-T7 L250「预检须在 live key 未设环境执行(D-14)」;M3-T4/M4-T5/M5-T4 以「同 M2-T7 形状」继承)——三处语义一致,与 RT-4(workflow 去 key)+ M1-T0(env/CI 守卫)双修结构吻合 ✅。观察:anti-gaming 账本 AG-012 行本体未携带该条款(不矛盾,登记 MINOR-3)。

### 7. 非代码例外 — **PASS**(附 1 条非阻塞观察,见 MINOR-4)

- N-001(文档核查)/ N-002(双仓注释,评审读签)/ N-003(人工 SQL,conditional)/ N-004(DONE,grep 实证)/ N-005(F-007 文档化,Cross-Service 读签)/ N-006(GitHub 手工设置,gh api 输出留存)/ N-007(YAML 交付物,三段实证)——每条 genuinely non-code(验收不靠测试断言)且判定标准可操作(证据落点 + 阻塞面明确)✅。
- 观察:N-003 状态列「CONDITIONAL(Step 04 裁决)」未追注 D-07 已裁(降级为可选抽查);语义已由 M6-T3 验收注 + M6 §5 行承接,无执行歧义(登记 MINOR-4)。

### 8. 跨服务契约 — **PASS**

- C-001(写方 lib.rs:354-355 / 读方 redis.rs:438-465 / 验证 = M1-T5.3 向后兼容断言)✅;C-002(M1-T5 clamp+注释 + M6-T2 注释,N-002 双仓,Step 06 Cross-Service 复核位)✅;C-003(O1 五仓读方枚举 §2 + 子串约束 → M6-T1.10 钉子 + M6-T2 db.rs 注释 + M6-T3 落库证据)✅;C-004(M1-T6 六串契约 pin + M6-T1.2/1.3 NULL/未知容忍 + DR-21 冒号前 token 精确比较三处同步)✅;C-005(M5-T1,判定写回义务 §1.2)✅——写方/读方/验证列与计划任务一致。
- §3 四条不变约束逐条有守卫任务:①零形状改动 → root §2.1 终审 + M1-T7/M2-T7/M3-T4/M4-T5/M5-T4/M6-T5 diff 自查;②不重命名 terminal_reason 字符串 → M1-T6 契约 pin 测试 + root §2.2;③scheduler 容忍 NULL/未知 → M6-T1.2/1.3 + root §2.2/§3 滚动部署条款;④completed_reason 子串白名单 → M6-T1.10 `contains("EXHAUST")` 显式断言 + D-06 ✅。

### 9. 评审闭环 — **FAIL(minor:1 条 P3-Info 无闭合证据)**

- Autoplan F-01~F-05:F-01→升级 DR-08→M1-T0 ✅;F-02→M3-T2/M4-T2/M4-T3/M5-T3-A 各一行 GREEN 日志规格(逐文件核实)✅;F-03→root §3「每模块独立分支/PR,diff 基线 = main」✅;F-04→M6-T5 命令块第 4 段部署前烟测 SQL + 验收判据 6 ✅;F-05→M2-T4 装配注 ✅。
- Domain P1×8(DR-01~08)、P2×14(DR-09~22 + F-02/F-03):逐条在 patches/07-batch{A..F} 有改动记录,并在 m1/m2/m3/m4/m5/m6/root/账本现行文本中逐一核实落位(DR-05 的 4×429/requests==5、DR-06 的 --relative+双哨兵、DR-07 的 M1-T0+RT-4、DR-17 的 T2.8 五形状+测试 12/13、DR-21 的冒号 token 三处等均实文在场)✅。patches/ 六文件齐(A~F),各含双审记录;无 P0/P1 遗留 ✅。
- P3 批:F-06(fp)/F-07/F-08/F-09/F-10/F-11、TG-07~TG-11、FR#5/#6、SM#F6/F7/F9、protocol#3/#4、fp#F7(live RED 预算口径 → prod-deps 结论「仅 GREEN 一轮取证」句)均核实闭合 ✅。
- **唯一缺口:state-machine#F8(Info)「StopReason 同页多信号优先序未定义」无闭合证据**——summary P3 行点名「D2 注 + 组合信号单测」,但 batch A patch note 未列、m1 §2 D2 无「ReachedMaxCount 优先」注、M1-T2 无组合信号单测,grep「优先序/组合信号/ReachedMaxCount 优先」计划族零命中。与 manifest「P3 批全部闭合」陈述不符。终态级后果已被 PT-4 合取兜住(评审原文自述),故为 minor。
- **Patch 建议(SM#F8)**:m1 §2 D2 增一句冻结注「同页多信号(达量 ∧ cursor 缺失/重复/空页)优先序:`ReachedMaxCount` 优先(达量即正常完结,不产出 Exhausted——与 PT-4 合取一致)」+ M1-T2 增一条组合信号单测(末页恰好达量且 `next_cursor=None` → `Stop(ReachedMaxCount)`、`shortfall_for==None`),并在 M2-T2.8 对齐域注明该形状;或在 patches/补遗中登记显式 deferral 理由并修正 manifest「全闭合」措辞。

### 10. 裁决一致性 — **PASS**(抽样 5 处)

1. **D-04** 冻结表(fb=20/tiktok=20/reddit=100/twitter=100/ig=50/unknown=20):m1 §2 D4 = root §1 = M1-T5.2 五平台 clamp 期望 = M1-T5.5 表驱动断言,四处一致 ✅。
2. **D-06** `SEARCH_EXHAUSTED`:C-003 子串约束 / R-013 / M6-T1.10 / M6-T3 / root §1 / manifest Open Blockers,取值与子串论证一致 ✅。
3. **D-13** 任务级 max_count:M1-T4.8(K=2,60 vs 50 RED)/ root §2.6 / I-001 任务级表述 / invariants §3 口径,一致 ✅。
4. **D-15** 空页计数改生产行:m1 §2 D2 注 / I-002(e) / M2 §2 两类生产行改动声明 / M2-T2.11 killing 测试 / M2-T3.3 复位措辞,一致 ✅。
5. **D-01** facebook 特例豁免:04-adjudications 直接指令(M3~M5 仍接 PaginationLoop)/ m3 §1.1、m4 §1、m5 已裁决输入的 D-01 绑定 / anti-gaming ※1(TG-07)+ test-suite §8 gap 登记 / root §1 D2 消费形态,一致且豁免理由不可迁移性被显式记载 ✅。
- 另核:D-07/D-08/D-09 ↔ M6 任务清单一致 ✅。观察:FR-002/AG-013 行「是否补 CI 由 Step 04 评审定(推荐补)」为 D-09 前陈旧指针(裁决方向与推荐一致,无矛盾,归 MINOR-4)。

### 11. 无 placeholder — **PASS**

- 全族 grep 零 TODO/TBD/FIXME。显式开放项均有登记位:V1(C-005 + N-001 + T-053/P-004 + D-12 + M5-T1)、M3 per-request count「待执行期复核」(M3 现状核对差异表 #3 + handoff 07-to-08 开放项)、N-006 required-check(root RT-2.2)、RT-2.1 helper 提升 backlog(root RT-2.1 + M3 §6.3)、user-videos 双循环归并 backlog(M3 §2.2)、SM#F6 fallback 写序竞态(M6 §2.1 已知边界 → root backlog)✅。M3「迁移改写 + 既有测试撞红处置段(ASSERTION-CHANGE-JUSTIFIED,测试上下文执行)」为 handoff 声明的有意设计,非缺口 ✅。

## 非阻塞观察(MINOR;建议随 root 阶段一次性微补)

| # | 内容 | 建议 patch |
|---|---|---|
| MINOR-1 | = 第 9 项 FAIL 本体(SM#F8),见上方 patch 建议 | D2 优先序注 + M1-T2 组合信号单测,或登记显式 deferral |
| MINOR-2 | DR-22 改述残留:m1 §2 D3 行(L119)、m6 §1.10 与 T1.12、requirements R-008 行仍用「一次重派」无 per-tick 限定(单次 eval_once 域内读法准确,无语义矛盾;但 DR-22 patch 方向点名过 M1-D3/M6 §1.10) | 三处补「(每 tick)」限定词;requirements 行按只增惯例补注 |
| MINOR-3 | anti-gaming 账本 AG-012 行未携带 D-14「须在 live key 未设环境执行」条款(计划侧 M1-T7/M2-T7 已携带,实际执行无歧义) | AG-012 行尾补该半句 |
| MINOR-4 | 账本陈旧指针:FR-002/AG-013「是否补 CI 由 Step 04 评审定」已被 D-09 裁(补);N-003 状态「CONDITIONAL(Step 04 裁决)」已被 D-07 裁(降级可选) | 各补一条「→ 已裁,见 04-adjudications D-09/D-07」追注(只增不改) |

## 分片裁定

**PASS(有保留)— 10/11 项 PASS;第 9 项(评审闭环)minor FAIL:state-machine#F8(Info 级)无闭合证据,与「P3 全闭合」陈述不符,但无终态级后果(PT-4 合取已兜)、不阻塞 compile。** 账本侧全局完整性成立:需求/不变量/失败模式/测试/依赖/provider/契约/非代码例外全部有 owner 且指向实存任务;假设零 ASSUMED、A005 drop 显式;反作弊地板完整未弱化;裁决引用抽样无矛盾。建议:SM#F8 微补丁(或显式 deferral 登记)+ MINOR-2~4 三处措辞追注,可与 root 验收阶段合并处理,无需回退步骤。
