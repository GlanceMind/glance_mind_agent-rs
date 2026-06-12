# Patch Note: Step 07 批 D — M6 计划(2026-06-10)

> 文件:`plans/modules/m6-once-guard.md`。实现:独立子 agent(9 项,DONE);spec 审:✅ 9/9(含 python 实证 DR-21 三形状判定);质量审:✅(2 Minor 已修:烟测 SQL 执行方式、金丝雀伴随红声明)。

## 关闭的 findings 与闭合理由

| Finding | 改动 | 复审方 |
|---|---|---|
| DR-06(P1,M6 侧) | 「独立 git 仓」全文改述为 worker 仓子目录(实证);M6-T4 workflow 与 M6-T5/AG-013 预检统一 `--relative` 剥前缀;`runs-on: [self-hosted, front]`;**哨兵判据 Found N mutants,N≥1**两处——变异门禁空转风险关闭 | Dependency / Test-Gate |
| DR-12(P2,M6 侧) | T1.11/T1.12 改手工金丝雀(completed→Dispatch / failed→Skip,各注「既有断言伴随红属预期,观察对象=新增测试」),预设豁免论证删除 | Test-Gate |
| DR-21(P2) | 枯竭判定改「冒号前 token 精确比较」三处同步(§2.1/真值表/GREEN `splitn(2,':')`);防未来 code 前缀碰撞;T1.5/1.6 语义不变 | Protocol |
| SM#F6 / protocol#4(P3) | §2.1「已知边界」登记:fallback 写序竞态(修法属 agent 仓,转 root backlog)+ completed+NULL 主路径可达(WARN 良性噪声分诊注) | Failure-Recovery |
| SM#F7(P3) | completed_reason 写方措辞改「worker 仓应用层唯一写方+系统层另有写方」+ mark_campaign_completed 幂等注 | State-machine |
| F-04 / protocol#3 / TG-08(P3) | 部署前烟测 SQL(含 psql 执行方式)、migrations 空虚真注、N-007 三段实证引用 | Plan-Integrator |

## 验收强化

M6-T4/T5 双哨兵(N==0 即配置失败)使 D-09 门禁不可能静默空转;金丝雀程序对「既有 33 断言零触碰」约束的表观张力已用伴随红声明消解。
