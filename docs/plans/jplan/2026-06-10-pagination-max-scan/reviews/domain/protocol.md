# Domain Review — Protocol / Cross-Service Contract Reviewer(2026-06-10,只读子 agent 原文)

> 归一化映射:Finding 1→DR-21(P2)、Finding 2→P3 批(与 plan-integrator F-08 同源)、Observation 3/4→P3 注记。结论:2 Low + 2 Info,无 High/Critical;契约设计面(C-001~C-005、冻结表、滚动部署、LLM INAPPLICABLE)判定扎实,引用源码事实逐条核实无误。

### Finding 1 — C-004 枯竭判定用裸 starts_with,留下未来 code 前缀碰撞陷阱(Low → DR-21)
- Evidence:M6 §2.1/T1 GREEN:`starts_with("NO_MORE_POSSIBLE_DATA")`;契约 §3.2 只禁重命名、不禁新增;无规则阻止未来第 7 个 code 以既有 code 为前缀(值集本身已含前缀对 COMPLETED / COMPLETED_WITH_PARTIAL_ERRORS,今日无害仅因 scheduler 不匹配 COMPLETED)。
- Consequence:未来以 NO_MORE_POSSIBLE_DATA 为前缀的新 code 被静默归类为枯竭(SEARCH_EXHAUSTED、不告警)——重新引入静默欠扫盲区。
- Patch(二选一,皆廉价):(a) M6-T1 GREEN 改为取冒号前 token 精确比较 `== "NO_MORE_POSSIBLE_DATA"`(T1.5 裸 code 容忍仍过,T1.6 不变);(b) root §1 C-004 行与契约 §3 加一句「新增 terminal_reason code 不得以任何既有 code 为前缀」。

### Finding 2 — M5 分支 A 复用 PAGE_SIZE 的计划文本矛盾(Low;= plan-integrator F-08)
- M3 §2.3/§6.1「M5 分支 A 可复用」vs root §1 D4 把 instagram 列入豁免、M5 §2.2 实证无 count 参数。Patch:M3 措辞改为「仅当 V1/T-053 证据显示上游接受单页 count 参数、经 root 修订豁免表后方可复用;否则 instagram 维持 D-02 豁免」。

### Observation 3(Info)— scheduler `migrations/` 零命中检查为空虚真
- `glance_mind_scheduler/migrations` 目录不存在;守卫仍有效(有人创建目录即触发),但证据包应注明基线状态 =「目录不存在」。

### Observation 4(Info)— 新 agent 下 completed+NULL terminal_reason 仍可达;WARN 含良性噪声
- Evidence:agent postgres.rs:2044-2069:Pending/Running 转移主动把 terminal_reason 置 NULL;`update_task_status` 的 Completed|Failed else 分支设状态但不动 terminal_reason。
- Consequence:M6 真值表 NULL→OnceExecuted+WARN 是安全缺省;运维须预期部分 WARN 根因是「完成路径绕过 reason 写入」而非真欠扫。结构化 WARN 已含 terminal_reason 可分诊。建议 M6-T2 WARN 语义注一行,无契约改动。

### 核查通过清单(全部带证据)
C-004 值集与形状(progress_tracker.rs:59-101,M1-T6 pin 测试可直接编译通过);terminal_reason 列存在(glance_mind_rust schema.rs:838 + migration 2026-05-13-120000);scheduler 读侧接线安全(as_select 名称选择,加列序安全;entity 不外序列化);NewCrawlerTask 排除 terminal_reason(单写方);MarkCompleted 分支 Some(task) 在作用域(lib.rs:159-178);写方冻结面(dispatch_task L355-356、mark_campaign_completed db.rs:112-131、lib.rs:170 硬编码行 = M6-T2 替换点);C-003 白名单逐字核实(test_10_status_transitions.py:215-219);completed_reason 双侧 Nullable<Text>;滚动部署双向安全;C-001/C-002 零形状改动成立(protocol_gen:156-158 现状字段、redis.rs 现状不读、page_size_hint 仅域模型无外序列化);PAGE_SIZE extra 无形状改动(entities.rs:806-808);D-02 豁免证据逐字核实(reddit_types.rs:357-362、twitter_types.rs:357-361);脱敏链路(构造器 + 500 字界 + 禁手拼);protocol_gen 零改动承诺无任务违反;LLM INAPPLICABLE 确认(改动面不触 LLM,重评触发器无一命中);D-02/D-06 全族一致(唯 M3 §2.3 例外即 Finding 2)。
