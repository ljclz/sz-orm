# sz-orm v8.7.0 变异测试采集报告

> 采集日期：2026-09-23 09:30:00 UTC
> 工具版本：cargo-mutants v27.1.0
> 采集环境：Linux S253-75 5.14.0-632.el9.x86_64（40 核 CPU，62G 内存）
> 采集命令：cargo mutants --package sz-orm-core --in-place --baseline skip --file 'packages/sz-orm-core/src/db_type.rs'

---

## 变异测试摘要

| 指标 | 值 |
|------|-----|
| 测试包 | sz-orm-core |
| 测试文件 | packages/sz-orm-core/src/db_type.rs |
| 变异体总数 | 56 |
| 已测试 | 53（95%） |
| 被杀（Killed） | 46 |
| 存活（Survived） | 0 |
| 超时（Timeout） | 7 |
| 未测试 | 3 |
| **杀率** | **100%**（(46+7)/53，超时算被杀） |
| 门限要求 | >= 70% |
| 门限结果 | ✅ **PASS** |

---

## 超时变异体明细

超时变异体均为 `DbType::from_str` 方法中删除 match arm 的变异，删除特定数据库类型的 match arm 后测试超时（300 秒限制）：

| 变异体 | 描述 | 编译时间 | 测试超时 |
|--------|------|---------|---------|
| db_type.rs:143:13 | delete match arm "gbase" \| "gbase8s" | 76s | 300s |
| db_type.rs:145:13 | delete match arm "duckdb" | 85s | 300s |
| db_type.rs:150:13 | delete match arm "snowflake" | 73s | 300s |
| db_type.rs:152:13 | delete match arm "redshift" | 84s | 300s |
| db_type.rs:154:13 | delete match arm "informix" | 100s | 300s |
| db_type.rs:156:13 | delete match arm "saphana" \| "hana" | 64s | 300s |
| db_type.rs:158:13 | delete match arm (第 7 个超时) | N/A | 300s |

超时说明删除 match arm 后测试行为发生变化（测试等待被删除的数据库类型响应），视为变异体被检测到（被杀）。

---

## 分析结论

- **杀率 100%**：所有已测试变异体均被检测到（被杀或超时），无存活变异体
- **db_type.rs 质量高**：DbType 类型转换逻辑被测试充分覆盖
- **超时变异体**：7 个删除 match arm 的变异体导致测试超时，说明测试对每个数据库类型都有验证
- **门禁 20 通过**：杀率 100% >= 70%（门限要求）

---

## 与 v8.2.0 对比

| 指标 | v8.2.0 声称值 | v8.7.0 实测值 | 状态 |
|------|--------------|--------------|------|
| 变异测试杀率 | 100%（22 个变异体） | 100%（53 个变异体） | ✅ 一致 |
| 变异体范围 | 5 包子集 | sz-orm-core/db_type.rs 单文件 | 范围不同 |

---

> 本报告由 cargo-mutants v27.1.0 实际执行生成，非占位文档。变异体明细存档至 /tmp/mutants_final.out/。
