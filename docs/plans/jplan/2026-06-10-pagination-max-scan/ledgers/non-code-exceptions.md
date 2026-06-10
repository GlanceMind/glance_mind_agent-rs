# Non-Code Verification Exception Ledger(Step 02)

> 规则:仅收「genuinely non-code」的工作项 —— 其验收不靠测试,而靠书面证据/文档/人工核查。每条写明方法、判定标准、证据落点、阻塞面。代码可验证的工作一律不入本表(它们在 test-suite.md / production-dependencies.md)。

| ID | 工作项 | 方法 | 判定标准 / 证据落点 | 阻塞面 | 状态 |
|----|--------|------|----------------------|--------|------|
| N-001 | **V1 文档侧核查**:TikHub OpenAPI 文档中 `/api/v1/instagram/v3/general_search` 与 `/v2/general_search` 接受的 query 参数(是否有 `max_id`/`pagination_token`/`rank_token`) | 查 https://api.tikhub.io/docs 的 OpenAPI spec(仓内无 vendored spec,Step 01 已 find 验证为空) | 文档明确列出/排除分页参数 → 直接判定;文档不明 → 落到代码侧探测 T-053(P-004,不在本表)。结论与文档截图/引文写回 `assumptions.md` V1 行与 C-005 | 仅 R-006 分支选择(P2);不阻塞 P0/P1 | OPEN(Step 04 instagram 模块前置) |
| N-002 | **跨服务字段语义文档固化**(C-002):`search_limit`=页大小提示、`search_offset`=仅观测,写入两仓代码注释与契约说明(scheduler lib.rs dispatch 处、agent redis.rs 映射处) | 文档/注释撰写;非测试可验 | 两仓注释存在且与 C-002 措辞一致;Step 06 Cross-Service Contract Reviewer 复核;PR diff 可见 | 不阻塞实现;与 R-011 同 PR 落地 | OPEN(Step 04 任务) |
| N-003 | **P-007 补偿控制(若 scheduler 不建 real-DB gate)**:部署后人工 SQL 验证 `gm_campaigns.completed_reason` 区分值落库(枯竭欠扫场景) | 现网/预发 SQL 抽查(参照 `glance_mind_worker/docs/playbooks/` 既有剧本形式) | SQL 输出留存于模块计划完成证据;查得 R-013 区分值 ≥1 行或构造用例验证 | 仅作为 P-007 的 fallback;Step 04 若决定建最小 gated 测试则本条作废 | CONDITIONAL(Step 04 裁决) |
| N-004 | **O1 读方枚举**(`gm_campaigns.completed_reason` 全读方核查) | 跨仓 grep 实证(glance_mind_rust / glance_mind_admin / glance_mind_front / gm-e2e / glance_mind_worker) | 证据表已落 `cross-service-contracts.md` §2;裁决 CLOSED-SAFE | 曾阻塞 R-013 裁决 | **DONE(2026-06-10,本步完成)** |
| N-005 | **RT-1:F-007 task 重派重扫语义文档化** | 文档撰写 | 文档存在 + Step 06 Cross-Service Reviewer 读签;按 DR-22 修正后的真实语义撰写(每 tick 重派无上限) | root 验收 | OPEN(root 阶段) |
| N-006 | **M6 mutation CI 工作流设为受保护分支 required check(GitHub 手工配置)** | 仓库设置操作 | 设置截图或 `gh api` 输出留存(命令示例:`gh api repos/{owner}/{repo}/branches/<受保护分支>/protection/required_status_checks \| jq '.contexts[]' \| grep mutation-scheduler`,输出留存于 root 验收清单) | 无(跟进项) | OPEN |
| N-007 | **M6-T4 mutation workflow YAML 交付物** | 非测试交付物三段实证 | YAML 静态校验 + 本地等价命令实跑输出 + M6 PR 上工作流实跑可见(按 m6-once-guard.md M6-T4 原文) | M6 完成判据 | OPEN(M6 执行期) |

## 边界说明

- V1 的代码侧探测(T-053)与全部翻页/终态/防御逻辑均为代码可验证工作,不入本表。
- 本计划无其他纯文档/纯运维交付物;若 Step 04 起草中出现(如 runbook 更新),须回填本表。
