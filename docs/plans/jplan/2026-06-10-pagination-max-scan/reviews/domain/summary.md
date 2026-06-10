# Step 06 Domain Review — Summary(2026-06-10)

> 7 个领域评审(First-principles / Protocol / State-machine / Test-gate / Dependency / Failure-Recovery / Plan-Integrator)以并行只读子 agent 执行,各自独立上下文;原始 findings 全文见本目录各文件。本文件做严重度归一(映射到 P0~P3)、跨评审去重与路由裁定。
> **路由裁定:存在 P1 ×8 → Step 07 patch-iterate(P0/P1 不得延迟)。无 P0。**

## P1(must-patch;含跨评审交叉确认标注)

| ID | 发现 | 来源(交叉确认) | Patch 方向 |
|---|---|---|---|
| DR-01 | **D3 映射缺行**:`contents 空 + PartialFailure` 可达(date-filter 全滤除 + 第 2 页 429),落入零结果路径 → 瞬态限流被报成 NO_MORE_POSSIBLE_DATA 并停 campaign(违反 I-004,绕过 D-03);M6 随之误写 SEARCH_EXHAUSTED 不告警 | failure-recovery#1 + state-machine#F1(**双确认**) | M1 D1 冻结构造不变量「PartialFailure ⇒ contents 非空;零可交付进展一律 Err」+ 构造器归一化 + M1-T3/M2-T2 各补 RED 测试;invariants-failures 补行 |
| DR-02 | **PT-2 属性按字面不可满足**(空/短序列合法无 Stop),且 harness「注入页耗尽」语义未定义——制造「永远红且禁改断言」死局 | first-principles#F1 + test-gate#TG-04(**双确认**) | M1-T2 写死 harness 规范(页耗尽 = 补 `(空,None)`)+ 属性改写为可满足形式(`len+1` 步内 Stop / 标记子类必 Stop),RED 文本同步 |
| DR-03 | **循环活性缺口**:「非空但零新进展页 + fresh cursor」不触发任何终止条件 → 适配器无界发请求(违反 B4 成本上界);`empty` 按原始条数还是 `newly_accepted` 计未定义,facebook 现状按原始条数(facebook.rs:546-558) | first-principles#F2 | D2 定义 `empty_streak` 按 `newly_accepted.is_empty()` 递增;M1-T2 补「重复内容页 ×3 → Stop(EmptyPageLimit)」单测;I-002 枚举补 (e);M2-T2.8 形状表补此形状并裁决 facebook 侧对齐方式 |
| DR-04 | **多 keyword 预算超界**:reserve = max_count×单价(与 keyword 数无关),解 cap 后每 keyword count=max_videos 跨 keyword 累加 → consume 可达 K×max_count > reserve;I-001「单 task ≤ max_count」多 keyword 下不成立,T-054 守恒断言抓不到 | state-machine#F2 | root §2 三选一裁决:实证单 keyword / orchestrator 跨 keyword 传 remaining + 任务级测试 / 书面接受;建议补 K=2 确定性形状 |
| DR-05 | **M2 429 期望与既有重试循环矛盾**:`request_json` 对 429 自动重试 ×3(共 4 请求,facebook.rs:27/154-172,既有测试断言 requests==4),M2-T2.5 单响应 / M2-T3.4 断言 requests==2 → 不可达绿,唯一出路 = 删生产重试或改断言(AG-002 死局) | dependency#F3 | M2-T2.5/T3.4 改为排队 4×429(`retry-after: 0`)、期望 requests==5;RED 文本修正;T-050 预算注「429 触发 4 次计费重试」 |
| DR-06 | **scheduler 变异门禁空转族**:① scheduler 实为 worker 仓**子目录**(`.git` 不存在,实证),非「独立 git 仓」;② `git diff` 产出 `glance_mind_scheduler/src/...` 前缀路径,`cd` 子目录后 `--in-diff` 前缀不匹配 → 0 变异体恒绿;③ root §4 chain gate 还把 agent 仓 diff 喂给 scheduler mutants(且 `cd` 路径不可达);④ gated 段漏 M6-T3 命令 | dependency#F5 + test-gate#TG-01 + plan-integrator#F-06 + first-principles#F3(**四重确认**) | M6-T4/T5/AG-013/root §4 统一改 `git -C … diff --relative=glance_mind_scheduler`;增加哨兵判据「Found N mutants, N≥1,N==0 即配置失败」;root §4 scheduler 独立 diff 文件 + 补 M6-T3 gated 命令;M6 头部改「worker 仓子目录、独立 PR 流程」;M6-T4 补 `runs-on` |
| DR-07 | **mutation CI 每变异体打付费 live 调用**:mutation-rust.yml 注入 TIKHUB/FACEBOOK key(L48-56/105-114),mutants 全量 cargo test 含 live 二进制;live-API 文件无 `GITHUB_ACTIONS` opt-in 守卫(real-DB 有);计划新增 5+ live 测试放大 → 单 PR 数百次计费调用 + 变异判定混入上游抖动 | dependency#F1 | 横切任务:live-API 测试文件加 `GITHUB_ACTIONS && !RUN_REAL_API_TESTS → skip` 守卫(零断言改动)与/或 mutation workflow 移除 live key;AG-012 预检文本注明「须在 live key 未设环境执行」;P-001~P-004/providers 控制汇总补注 |
| DR-08 | **RT-3 排序升级**(原 Step 05 F-01 P2 → P1):无凭据环境 M1-T7 起 mutants baseline 直接 abort;有凭据环境落入 DR-07;且「凭据未设 → skip」在 CI 恒有凭据下永不触发,须并入 DR-07 的 CI opt-in 条款 | dependency#F2(升级 F-01) | RT-3 提升为 M1 前置任务(M1-T0),范围 = facebook_real_api_test + real_api_test(+twitter_real_api_test)env-skip + CI opt-in;P-001 措辞改「现状 panic,RT-3 修复后 skip」 |

## P2(本批一并 patch;按主题归并)

| ID | 发现 | 来源 |
|---|---|---|
| DR-09 | 「进展」定义跨平台口径不一(facebook 过滤前 vs 其余过滤后)→ 统一为「进入 FetchOutcome.contents 的过滤后条目数」 | failure-recovery#2 |
| DR-10 | PartialFailure 触发集未冻结(「可恢复错误」无定义)→ 冻结为仅 `RateLimited`,各平台补「硬错误+有进展 → Err」钉子;F-002 账本行收窄 | failure-recovery#3 + state-machine#F4 |
| DR-11 | terminal_hint 聚合误述「last-wins」实为「last-Some-wins」;混合 shortfall(Partial+Exhausted)优先级未定义 → 更正 + 优先级(PartialFailure>Exhausted)或 justified gap | first-principles#F5 + state-machine 注 |
| DR-12 | AG-006「由 in-diff 预检证明」对钉零改动代码的先绿测试结构性不可行(M2-T3.1~4、M2-T2.7、M6-T1.11/12)→ 逐条改手工金丝雀(写明变异对象);豁免须附实际 missed 清单 | test-gate#TG-02 |
| DR-13 | M2-T4 RED 原因错置:MockContentGateway 装配下 facebook override 不在测试路径 → 改名实义(RED 取证点 = pre-M2-T1)或加真实 adapter 装配测试 | test-gate#TG-03 |
| DR-14 | live 契约探针类缺账本级例外类别 → anti-gaming 新增 AG-008(实跑输出留存 + 回灌 fixture + 判定可复算;不适用变异证明),五处局部豁免改引用 | first-principles#F4 |
| DR-15 | 簿记外先绿测试(M2-T1.4、M4-T1.3「RED 义务由 1/2 承担」)→ 统一标注「允许先绿+AG-006」入清单 | test-gate#TG-05 |
| DR-16 | M4-T4 twitter live 断言与容忍条款自相矛盾 → 计划内写死分支断言(cursor 相同/不同两路径) | test-gate#TG-06 |
| DR-17 | T2.8 对齐域仅无过滤子空间;candidates 两级循环聚合规则未写、page/candidates 路径无 shortfall 测试 → 补 date-filter 形状测试 + 聚合规则文字 + 三循环出口 killing 测试入不可豁免清单 | state-machine#F5 |
| DR-18 | M3 伪代码自相矛盾(`offset+len` 合成游标 vs 归一化 Exhausted);cursor parse 失败无出口 → 删合成、补 parse 失败归一化 + 测试 | state-machine#F3 |
| DR-19 | TikHub 429 重试默认睡 60s×3、计划 429 测试未规定 Retry-After/重试配置 → 撞 mutants 300s 预算;real 探针 ≤3 调用按 HTTP 计 → 统一规定 `Retry-After: 0`/零重试配置 | dependency#F4 |
| DR-20 | M4 fixture 来源声明不成立(仓内无 reddit/twitter 搜索样本)→ 改述「取自 serde 定义,待回灌确认」+ M4-T4 补 M3 式对账义务 | dependency#F6 |
| DR-21 | C-004 前缀碰撞陷阱(starts_with + 未禁前缀扩展)→ 精确比较冒号前 token 或冻结表加「新增 code 不得以既有 code 为前缀」 | protocol#1 |
| DR-22 | 「failed 一次重派」系误读(实为每 tick 重派、跨 tick 无上限)→ F-001/F-007/A007/M1-D3/M6 §1.10/RT-1 统一改述;重试上限登记 backlog | failure-recovery#4 |

## P3(随批修补;编号沿用各评审)

plan-integrator F-07(R-001 ig 行悬空)/F-08+protocol#2(M3 §2.3 PAGE_SIZE 与 D4 矛盾)/F-09(M3 §5 死引用)/F-10(root D3「四行」实五行)/F-11(M2 任务头 ID 漏列 + T2.8 载荷归属);test-gate TG-07(facebook PT 覆盖虚标 → justified gap 登记)/TG-08(非代码例外回填 N-005~N-007)/TG-09(T-050 RED 不可满足 → 探针化;M3-T3 引用错)/TG-10(「env 未设自动 skip」句修正扩至 M2-T7/M3-T4/M4-T5)/TG-11(D-11 基线前提 + 双金丝雀程序写死);failure-recovery#5(F-009 验证描述改述)/#6(工作树 postgres.rs 污染 → root §3 干净基线前置);state-machine#F6(fallback 写序竞态:调换两条写序或文档登记)/#F7(completed_reason 写方措辞)/#F8(StopReason 优先序注释+单测)/#F9(0/1 边界显式接受声明入 root §2);protocol#3/#4(Info:migrations 目录不存在注记;completed+NULL 可达性注入 M6-T2 WARN 语义);test-suite T-010 形状不一致(first-principles#F6);M2-T6 live RED 预算口径(first-principles#F7)。

## 与 Step 05 patch queue 的合并

- F-01 → **升级并入 DR-08**(P1)。
- F-02(日志规格)、F-03(PR 纪律)、F-04、F-05 → 维持原级,随本批一并处理(F-03 与 TG-01 的 chain gate 变异判据改述合并)。

## 评审有效性说明

- 交叉确认强度:DR-06 四重独立确认、DR-01/DR-02 双重确认——并行盲评的预期收益兑现。
- 已裁决项 D-01~D-12:全部 7 评审无功能性反证否决;state-machine#F9 对 D-03 的 0/1 边界差显式声明「不否决」。
- 各评审「通过项清单」(检查执行证明)保留在各自文件,Step 08 traceability 可引用。
