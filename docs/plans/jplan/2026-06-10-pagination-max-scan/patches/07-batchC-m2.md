# Patch Note: Step 07 批 C — M2 计划(2026-06-10)

> 文件:`plans/modules/m2-facebook-p0.md`。实现:**两个接力实现者**(前者 10 项后撞会话限额中断,续作者补余 10 项 + 去重)+ spec 审返修 6 处 + 质量审 Minor 2 处;spec 审:首轮 ❌(5 处接力断面遗漏)→ 返修后逐字核验 ✅;质量审:✅(2 Minor 已修)。

## 关闭的 findings 与闭合理由

| Finding | 改动 | 复审方 |
|---|---|---|
| DR-05(P1) | T2.5/T3.4 改 4×429(retry-after: 0)、requests==5,与适配器自动重试现实一致(不可达绿死局消除) | Test-Gate / Dependency |
| DR-01 M2 侧(P1) | 测试 9 date-filter 全滤除+429 → 整体 Err(构造不变量实例) | Failure-Recovery |
| DR-09(P2) | §2.2-b 与 GREEN 全部改过滤后口径(含 Exhausted 分支 contents 注) | State-machine |
| DR-10 M2 侧(P2) | 测试 10 硬错误+有进展 → Err 钉子 | Failure-Recovery |
| D-15(P1 关联) | 空页计数改「本页新增==0」(三循环)+ 测试 11 重复内容页 killing + T3.3 复位措辞 | Concurrency |
| DR-17a~d(P2) | T2.8 五形状+RED+移位去重;测试 12 date-filter 形状;测试 13 candidates 不外泄 + 最外层出口规则;T7 三循环 killing 不可豁免 | State-machine |
| DR-12(P2) | T3.1~4 / T2.7 金丝雀程序(变异对象逐条写明),预设豁免句删除 | Test-Gate |
| DR-13(P2) | M2-T4 改名实义 + RED 取证点 = pre-M2 基线(与 T5 基线同 commit,质量审 Minor 已澄清) | Test-Gate |
| DR-15 / TG-09 / TG-10 / TG-11(P2/P3) | T1.4 允许先绿入清单;T-050/T-054 探针化(AG-008);T7 句修正;T5 基线句 | Test-Gate |
| F-11 / F-06 / F-05(P3) | 任务头 ID 补齐;T-010 形状 20+20+10 全文统一;弹性注记;§7 簿记同步 | Plan-Integrator |

## 接力质量记录

spec 审首轮发现的 5 处遗漏全部位于前任中断点附近(M2-T2 GREEN 区与 M2-T3 区)——「测试区已更新、实现规格区未跟进」的典型接力断面;返修后 grep 逐字核验(1+4==5 ×2、过滤后交付集、本页有新增、最外层出口、20+20+10 等)全部落位。

## 测试面变化

M2-T2 测试 1~13(新增 9~13 五条,全带 RED);允许先绿清单 = T1.4、T2.7、T3.1~4、T4.3 + T-050/T-054(AG-008 探针类),与 §5/各任务标注精确一致。
