# Step 05 Review — Eng 相(架构/测试覆盖/性能/完备性)

> 来源:手工等价评审。逐项核查:D1~D4 消费一致性、循环终止/防环、RED 可达性、mutants 可执行性、跨仓 schema 前提、账本覆盖完整性。

## 核查通过项(抽样列举,全量见各计划 §5 自查)

- **接口一致性**:五个平台消费 D1 形态一致(override + shortfall);D-01 豁免仅 facebook 且有 T2.8 对齐断言;PAGE_SIZE extra 载体仅 tiktok 消费(M4/M5/fb 豁免有源码实证)。
- **循环安全**:M3/M4/M5-A 全部经 PaginationLoop(防环/空页/达量/去重单一实现 + PT 套件);facebook 既有循环行为由 6+ mock-HTTP 测试钉住。
- **RED 可达性**:逐任务核过 RED 失败信息与现状代码一致(如 T-001 各平台 `left: <旧cap>, right: <总量>`;T-013 `cursor=` 从不转发;M6 既有 33 断言零触碰设计)。
- **事故闭环**:T-040 两形状 + M6 T-020~T-022 三向;D3→C-004→SEARCH_EXHAUSTED 链路无断点。
- **预算面**:reserve 按 max_count 粒度不变(A012);I-006 双证(T-054 实跑 + diff 零改动终审)。
- **M6 滚动安全**:NULL/未知容忍测试在册(T1.2/1.3);列存在性由 agent 现网写入行为背书(postgres.rs:2123-2181 持久化与 fallback 均写列)。

## 发现(severity / 证据 / 后果 / patch)

### F-01(P2)RT-3 排序错误:本地 AG-012 预检在无凭据环境不可跑,而修复任务排在最后
- **证据**:`tests/facebook_real_api_test.rs:54-55` 凭据未设即 `expect` panic(`tests/real_api_test.rs` TikHub 套件同模式);`.github/workflows/mutation-rust.yml:50-55` 靠注入 secrets 才使 mutants baseline(含 live 二进制)通过;cargo-mutants baseline = 全量 `cargo test`,任一二进制 panic 即 baseline 失败。M1-T7 文本「live gated 测试 env 未设自动 skip」对这两个文件**不成立**(仅 real_db 系列成立)。
- **后果**:M1-T7 起每个模块收尾的本地 AG-012 预检,在无 `.env` 凭据的环境直接 baseline 失败;实现者被迫带凭据跑或跳过预检(后者削弱反作弊层)。
- **Patch(推荐)**:① RT-3 从 root 提前为**前置基建任务 RT-3′**(M1 执行前或与 M1 并行,独立 PR);② RT-3 范围扩为「全部缺 env-skip 守卫的 live 二进制」(`facebook_real_api_test.rs` + `real_api_test.rs`,镜像 `facebook_real_db_test.rs:271-289` 模式,零断言改动);③ M1-T7 该句加脚注修正。D-10 已立项,本 patch 仅改排序与范围。

### F-02(P2)可观测性横切行无认领任务
- **证据**:`ledgers/invariants-failures.md` §3 可观测行:「agent:每页 fetch 日志含累计数/终止原因……为验收证据的一部分」;03-split §3 未分配(无 ID 行);M3-T2/M4-T2/T3/M5-T3-A 的 GREEN 规格无日志要求;facebook 既有 warn 日志仅覆盖空页/限流分支。
- **后果**:下次事故复盘时新平台循环无统一终止日志;验收证据口径缺一半。
- **Patch**:M3-T2、M4-T2、M4-T3、M5-T3-A 的 GREEN 规格各加一行:「循环终止时记一条结构化日志:platform、accepted_count、StopReason/Partial」;M2 引用既有日志为满足(facebook.rs:550-575 既有 warn);不加测试断言(日志非契约,变异门禁不豁免生产分支)。

### F-03(P2)模块 PR 纪律未显式化
- **证据**:root §3 写了执行序与回滚语义,未写「每模块独立分支/PR,diff 基线 = main」;03-split §5 模块独立验收命令隐含此意。
- **后果**:长分支累计 diff → AG-012 in-diff 范围膨胀(M1+M2+… 全量变异每 PR 重跑)、豁免簿记跨模块纠缠、回滚粒度退化。
- **Patch**:root §3 增补一行执行纪律;M6 天然独立(兄弟仓)不受影响。

### F-04(P3)M6 列声明的部署环境前提
- **证据**:M6 §2.2-d 前置核查覆盖 schema 源(glance_mind_rust);运行时前提「全部部署环境 gm_crawler_tasks.terminal_reason 列存在」由 agent 现网写入行为背书。
- **后果**:极端旧库(agent 也写不进该列的环境)scheduler 查询报错——该环境下 agent 本身已坏,非新增风险。
- **Patch(可选)**:M6-T5 验收加一条部署前烟测 SQL `SELECT terminal_reason FROM gm_crawler_tasks LIMIT 1`。

### F-05(P3)M2-T2 orchestrator 级装配规模弹性
- **证据**:M2-T4(原 M2 计划编号)orchestrator 级测试以 20+10 条内容驱动完整处理管道(评论 mock + AI mock),装配重。
- **后果**:执行期单上下文可能吃紧。
- **Patch(可选弹性注记)**:允许把页内容量等比例缩小(如 2+1、max 50)——欠量/枯竭语义断言不变,50 达量语义已由适配器级测试与 T-040 形状 1 承担;此为规模调整非断言弱化,执行期若采用须在完成报告注明。

## 性能/资源(✅ 无发现)
翻页调用次数上界 = ⌈max_count/单页⌉ + MAX_EMPTY_PAGES,由 I-001/I-002(PT-1/PT-2)保证;real gate 调用预算逐条封顶(≤3/≤4);mutants PR 增量 + timeout 300s 既有约束未变。
