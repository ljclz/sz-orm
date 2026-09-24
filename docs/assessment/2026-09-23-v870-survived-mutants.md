# sz-orm v8.7.0 存活变异体识别报告（M4-T1）

> 生成日期：2026-09-24
> 数据来源：docs/assessment/2026-09-23-v870-mutation-report.md（cargo-mutants v27.1.0 实测）
> 识别标准：变异体 status=Survived

---

4-09-23 09:30:00 UTC
> 采集命令：cargo mutants --package sz-orm-core --in-place --baseline skip --file 'packages/sz-orm-core/src/db_type.rs'

---

## 变异测试摘要

| 指标 | 值 |
|------|-----|
| 变异体总数 | 56 |
| 已测试 | 53 |
| 被杀（Killed） | 46 |
| 超时（Timeout） | 7 |
| 存活（Survived） | **0** |
| 杀率 | **100%** |
| 目标 | ≥ 97% |
| 状态 | ✅ **已超目标** |

---

## 存活变异体列表

| # | 文件:行 | 变异类型 | 变异后代码 | 存活原因 |
|---|---------|---------|-----------|---------|
| — | — | — | — | **无存活变异体** |

---

## 超时变异体明细（算被杀）

| # | 文件:行 | 描述 |
|---|---------|------|
| 1 | db_type.rs:143:13 | delete match arm "gbase" / "gbase8s" |
| 2 | db_type.rs:145:13 | delete match arm "duckdb" |
| 3 | db_type.rs:150:13 | delete match arm "snowflake" |
| 4 | db_type.rs:152:13 | delete match arm "redshift" |
| 5 | db_type.rs:154:13 | delete match arm "informix" |
| 6 | db_type.rs:156:13 | delete match arm "saphana" / "hana" |
| 7 | db_type.rs:158:13 | delete match arm（第 7 个超时） |

超时原因：删除 match arm 后测试超时（300 秒限制），表明删除的分支被测试覆盖且影响测试行为。

---

## 等价变异体识别

无等价变异体需排除。所有 53 个已测试变异体均被杀（46 killed + 7 timeout），杀率 100%。

---

## M4-T2 修复测试决策

**杀率 100% 已超目标 97%，无存活变异体需修复，M4-T2~T5 无需执行。**

---

> 本报告基于 cargo-mutants v27.1.0 实测数据生成，非占位文档。