# MySQL 方言矩阵批红队复核回执（环 5）

> **复核对象**：commit 206f562（矩阵套件 + G1/G2/G4 修复）+ 报告 `docs/assessment/2026-11-11-mysql-dialect-matrix.md`
> **复核者**：独立 Agent session（非报告作者 session），仓库工作区零写入
> **结论**：**放行**。阻断级缺陷 0，建议级 4（全部采纳处置，见文末）

## 逐项判定

| # | 复核项 | 判定 | 关键证据 |
|---|--------|------|---------|
| 1 | 报告三分类真实性 | PASS | G1 `packages/sz-orm-core/src/query.rs:1732` render_eq_fragment 实存；8 处参数化调用点 grep 精确命中 2262/2269/2377/2384/2426/2433/2831/2838；内联路径 1809/1817/1850/1858 逐行核对实存。G2 `packages/sz-orm-core/src/dialect.rs:494` AddIndex/DropIndex/FK 约束名 quote（diff 证实）；ClickHouse 同类修复在 2339。G4 `dialect.rs:312` MySQL max_identifier_len=64、trait 默认 63（dialect.rs:66）、`dialect.rs:51` quote_checked 接线 `sql_safety.rs:23` |
| 1a | G1 非 Null 参数化路径不破坏 | PASS | render_eq_fragment 非 Null 返回 `(" = ?", has_param=true)` → 仅 has_param 时 push 参数，占位符/参数对齐保持；真库侧 MY6 空串 eq 命中 1 行、MY4 `where_eq("name","b2")` 命中 id=2，非 Null 参数化端到端 |
| 2 | 修复正确性对抗审查 | PASS | ① `grep '" = ?"'` 残留仅 render_eq_fragment 本体 + 测试断言；② OrEq/OrNe 保留 `OR ` 前缀，has_or 分组对 Null/非 Null 均不受影响；③ `build_update_with_params` SET 子句未被触碰（MySQL SET 绑 NULL 合法）；④ keyset 走 `> ?`/`< ?` 无 Eq；⑤ having 仅渲染 Having 变体无遗漏；⑥ typed_ast Eq 为强类型 DSL（Value::Null 不可表达），非 G1 范围 |
| 3 | 测试真实性（门禁 30 口径） | PASS | 7 测试全部真连行为断言（行序/行数/往返值），无永真断言/名义差分/丢弃结果；MY4 探针双向（65535 接受 + 65538 拒绝）；MY2 双向（64 放行 + 65 拒绝）；MY5 GROUP_CONCAT 期望值由活库 `@@group_concat_max_len` 现算 |
| 4a | 矩阵真连复跑 | PASS | `cargo test -p sz-orm-core --test integration_mysql_matrix -- --ignored` → **7 passed; 0 failed**（1.01s） |
| 4b | audit-verify 复跑 | PASS | 12 通过 / 0 失败 / 24 警告（弱语义提示级） |
| 5 | 提交范围 | PASS | `git show 206f562 --stat`：仅 4 文件（dialect.rs/query.rs/sql_safety.rs/integration_mysql_matrix.rs），无并行会话文件 |

## 发现清单

**阻断级：无。**

**建议级（4 项，处置如下）**：

1. `where_in` 含 `Value::Null` 残留语义边界（`IN (?)` 绑 NULL 恒不匹配，与 G1 同族静默空结果）→ **采纳**：登记为报告建议项 S4，留后续批次评估，不阻断本批（G1 声明范围仅 Eq/Ne，报告未声称覆盖 In）。
2. 提交信息「新增 8 测试」笔误（实际 7）→ **采纳**：不改写历史提交；CHANGELOG 条目按实际 7 测试记入并注明笔误。
3. audit-verify 警告数 24 vs 报告记录 23 → **采纳**：报告 §7 更新为终态 24（系 §7 行文提及历史行号 4794 产生的跳过提示）。
4. MY1 多键排序注释表述有误（断言本身正确）→ **采纳**：注释已修正（登记 commit）。

另：报告自登的 S2/S3 边界（迁移/权限模块 validate_identifier 维持 63、select() ASCII 白名单）归类准确，属诚实登记而非隐瞒。
