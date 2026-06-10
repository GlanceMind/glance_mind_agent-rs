# LLM API Boundary Coverage Ledger(Step 02)

## 裁决:INAPPLICABLE(带证据)

本计划(翻页到 max_count + 欠交付原因 + scheduler 防御校验)不触碰任何 LLM API 边界:

- 改动面为内容取数路径(strategies / content-gateway 适配器 / orchestrator 终态映射 / redis 映射)与 scheduler eval_once;LLM 调用(AI 评论生成,DeepSeek)发生在内容处理的下游环节,本计划不修改其请求/响应/流式/工具负载/重试/脱敏任何一项。
- `01-first-principles.md` §7 gate findings 已将 api-contract-guard 标 INAPPLICABLE;同理 LLM 边界无新增行。
- 唯一邻接点:翻页后单 task 处理内容条数上升(20→50),AI 评论生成调用次数随之上升 —— 这是**既有逐条处理路径的次数变化,非边界形状变化**;成本影响已由预算模型覆盖(reserve 本就按 max_count=50 预留,I-006/R-012)。

## 重评触发条件

若 Step 04 起草中出现以下任一情况,本账本必须补全完整边界行:
1. 修改 AI 评论生成的批量/并发策略;
2. 修改任何 LLM 请求/响应结构或错误映射;
3. 在欠交付原因路径中引入 LLM 生成的文本(如 AI 总结失败原因)。
