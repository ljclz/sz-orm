# 新会话提示词：SzRSQL 矩阵差分缺陷上游修复反馈（v1，2026-11-09）

> **用法**：将下方「---」之间的内容整体复制到 szrsql 仓库的新会话（或维护者）执行。
> **来源**：sz-orm 全功能矩阵 × SzRSQL 差分评测（`sz-orm/docs/assessment/2026-11-09-szrsql-matrix-differential.md`）
> **基线**：szrsql commit `f937e474`（2026-11-08 22:43 +0800）；对拍基准 PostgreSQL 18.4。
> **评测对照完整记录**：sz-orm 仓库 `docs/assessment/2026-11-09-szrsql-matrix-differential.md`（M1~M9 双库对照输出、脚本路径 `D:\tmp\m*_*.py`）。

---

**角色**：你是 szrsql 的资深 Rust 数据库内核工程师。任务：按优先级修复矩阵差分评测发现的以下缺陷。每个缺陷附最小复现 + 双库对照（PG18 为基准语义）。

## P0-1：REPEATABLE READ / SERIALIZABLE 非真快照语义（UP-1）

复现（双会话）：

```sql
CREATE TABLE m4_t (id INT PRIMARY KEY, v INT);
INSERT INTO m4_t VALUES (1, 100);
-- 会话 A
BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ;
SELECT v FROM m4_t WHERE id = 1;      -- 100
-- 会话 B（autocommit）
UPDATE m4_t SET v = 200 WHERE id = 1; -- 已提交
-- 会话 A 继续
SELECT v FROM m4_t WHERE id = 1;      -- szrsql 返回 200（错），PG 返回 100
SERIALIZABLE 同样实验同样错误。
```

定位线索（只读核实，基线 f937e474）：
- `crates/szrsql-sql/src/executor.rs` `scan_from_row_store`（约 20997 行起）：MVCC 路径存在且 `mvcc.is_visible` 生效，但 RR/Serializable 分支只跳过 `refresh_snapshot`，**未验证快照是否在 BEGIN 时固定**——疑似 autocommit 路径或跨语句快照重置导致读到最新已提交版本。
- `szrsql-tx/src/mvcc.rs` 的 `get_isolation_level`/`refresh_snapshot`/`visibility_snapshot` 值得审读：快照生命周期是否绑定 txn_id 而非连接上的语句执行。
- 验收线：上述实验 A 两次读必须均为 100；SERIALIZABLE 同；附带 写写冲突/写偏斜 最小实验文档。

## P0-2：IS [NOT] DISTINCT FROM 被优化器误裁列（UP-2）

复现（单会话）：

```sql
CREATE TABLE t (id INT PRIMARY KEY, v INT);
INSERT INTO t VALUES (1, 10), (2, NULL);
SELECT id FROM t WHERE v IS NOT DISTINCT FROM 10;
-- ERROR: execution error: evaluation error: column not found: v
-- PG 返回 1
SELECT id FROM t WHERE v IS NULL;      -- 正常（对照）
SELECT id, v IS NOT DISTINCT FROM 10 AS x FROM t;  -- 同样报错
SELECT id FROM t WHERE id < 5 AND v IS NOT DISTINCT FROM 10;  -- 正常（走 IndexScan，schema 未裁剪）
EXPLAIN SELECT id FROM t WHERE v IS NOT DISTINCT FROM 10;
-- Filter: (IsDistinctFrom ) → Parallel Seq Scan width=4（v 被裁掉）；IS NULL 版本 width=8
```

根因（基线 f937e474，`grep -c IsDistinctFrom crates/szrsql-optimizer/src/rule.rs` = 0）：
ProjectionPruning 的三个列引用收集器均未递归 `Expr::IsDistinctFrom`（落入 `_ => {}`）：
- `crates/szrsql-optimizer/src/rule.rs:1768` `collect_expr_unqualified_refs`（非限定列收集，缺失即被裁剪——直接根因）
- `crates/szrsql-optimizer/src/rule.rs:1597` `collect_expr_column_refs_grouped`
- `crates/szrsql-optimizer/src/rule.rs:435` `collect_column_refs_recursive`
求值器本身正确：`crates/szrsql-sql/src/expr.rs:2703`（eval_is_distinct_from）、`:1230`（eval 分派）、`:9783`（precompile）。
同构对照：`IsNull` 在三处收集器均有递归 arm。建议修复时一并排查 `SimilarTo`/`Substring` 等同批加入的 Phase F-9 表达式（rule.rs 同样缺 arm，未逐一验证）。
验收线：复现三例（WHERE 版/投影版/IndexScan 混合版）全部返回与 PG 一致的结果；新增回归测试钉死三个收集器对 IsDistinctFrom 的递归。

## P2 批（语义面）

- UP-3：`SELECT true::int, 1::bool` 报 `implicit cast not allowed`，PG 返回 `1|0|t|f`（bool↔int 显式 cast 应允许）。
- UP-4：`SELECT 2147483647::int + 1` 静默回绕为 2147483648，PG 报 `integer out of range`（int4/int2 运算应溢出报错；PG 对 bigint 同样报错）。

## P3 批（功能面）

- UP-5：`lastval()` 未实现（`function not found: lastval`）。
- UP-6：SQL 级 `EXECUTE psel(7)` 被防火墙拦截（`blocked command: EXEC`）——防火墙规则把 SQL 语句 EXECUTE 误当动态 EXEC；PREPARE 本身可用。协议级 prepared statement 不受影响。
- UP-7：advisory lock 函数族未实现：`pg_advisory_lock`/`pg_try_advisory_lock`/`pg_advisory_unlock`/`hashtext`（下游 sz-orm 迁移锁依赖，已在下游降级兼容，但真语义仍建议补齐）。
- UP-8：`SET application_name='x'` 后 `current_setting('application_name')` 与 `SHOW application_name` 返回空串（GUC 写入未接读回）。
- UP-9：`SELECT 1::numeric / 3` 精度 16 位（PG 20 位）。

## 附带行为差异（文档化即可，不强制改）

- 事务块内语句错误后允许继续执行并 COMMIT 成功（PG 强制 aborted-transaction 到 ROLLBACK）。

## 验收线

1. 每项修复附双库对照输出（同载荷 PG18 vs szrsql）。
2. P0 两项必须有回归测试钉死；UP-2 建议同时给三个收集器补齐 Phase F-9 表达式族的递归 arm 测试。
3. `cargo test -p szrsql-sql -p szrsql-optimizer` 全绿 + 现有集成不回归。
