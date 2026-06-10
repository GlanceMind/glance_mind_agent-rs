# Patch Note: Step 07 批 F — M3/M4/M5 计划(2026-06-10)

> 文件:`plans/modules/{m3-tiktok-p1, m4-reddit-twitter-p1, m5-instagram-p2}.md`。实现:独立子 agent(DONE,含 git 实证现状核对);spec 审:✅(唯一 ❌ = 本文件曾悬空,现已落;现状核对结论经评审独立抽查 `git show 4ab34ca` 证实);质量审:✅(1 Important + 2 Minor 已修:PR #5 既有测试 f3/r6a 撞红处置段(ASSERTION-CHANGE-JUSTIFIED 流程,测试上下文执行)、user-videos 双循环共存 backlog 注、金丝雀表述具体化)。

## 重大现状发现(M3)

**main 已合 PR #5(4ab34ca,2026-06-09)**:tiktok cap 已除、适配器已建私有翻页循环(+532 行,含自带 proptest 与 mock/real 测试)。但该修复**绕开 M1 全部共享契约**:未接 PaginationLoop(手写 seen-set/首空页即停/max_pages 守卫)、无 D1 override(欠交付静默——事故的「静默」属性在 tiktok 仍在)、cap 硬编码(无 PAGE_SIZE 载体)、任意错误整体 Err(无 F-002 partial 语义)、无重复 cursor 检测。

**M3 任务范围改写**:「新建循环」→「**把 PR #5 修复迁移到共享契约**」(差异清单 5 行入 M3 头部);账本 ID 认领不变,仅达成路径变化。RED 重分类全部如实:已消解项(总量不截断、多页请求)标「允许先绿 + AG-006 金丝雀(临时恢复旧行为须红)」;契约侧 RED(shortfall/PAGE_SIZE/PartialFailure/CursorLoop/空页语义)依旧成立;不确定项(per-request count 语义)标待执行期复核。

## 关闭的 findings

| Finding | M3 | M4 | M5 |
|---|---|---|---|
| DR-18(P2) | 伪代码删合成游标 + 非数字 cursor→Exhausted + 测试 10 | — | — |
| DR-19(P2) | 429 测试 Retry-After:0 + 4 请求口径;T3 零重试 ≤3=HTTP 上界 | T2/T3 同 + 180s 撞预算警示;T4 零重试 | T-053 ≤4=HTTP 上界;T3-A.4/T3-B.3 同 |
| DR-10(P2 关联) | 测试 11(PR #5 现状先绿→金丝雀,如实) | T2.9 / T3.10 真 RED | T3-A.5 真 RED;T3-B 注明不适用 |
| DR-15(P2) | — | T1.3 删「RED 义务委托」改标允许先绿+AG-006 | — |
| DR-16(P2) | — | T4 twitter 分支式写死断言(执行期零断言改动) | — |
| DR-20(P2) | — | mock 形状改 serde 定义+待回灌;T4 对账义务 | — |
| F-07(P3) | — | — | R-001(ig 行) 认领闭合 |
| F-08/F-09(P3) | PAGE_SIZE 表述限定 + 死引用修正 | — | — |
| F-02(P2) | T2 日志行 | T2/T3 日志行 | T3-A 日志行 |

## 流程含义(交 root/Step 08)

PR #5 证明了「不冻结接口则各平台各修各的」的风险成立——欠交付静默在 tiktok 现状中原样存在。M3 迁移动作的验收以 M1 D1/D2 契约为准;per-request count 上游行为(count<20 接受性)由 T-051 实测定。
