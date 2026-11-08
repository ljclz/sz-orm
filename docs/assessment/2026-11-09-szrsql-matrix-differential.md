# sz-orm 全功能矩阵 × SzRSQL 差分评测报告（2026-11-09）

> 依据提示词：[2026-11-09-szrsql-matrix-differential-prompt.md](2026-11-09-szrsql-matrix-differential-prompt.md)
> 差分基准：本机 PostgreSQL 18.4（`127.0.0.1:5432/sz_orm_test`）
> 被测：SzRSQL `D:\www\test\鲜视达\rust\szrsql` 基线 commit `f937e474`（2026-11-08 22:43 +0800，`feat(codec): add decode_cols and deserialize_cols for multi-column partial decoding`；工作区另有 `crates/szrsql-sql/src/executor.rs` 未提交改动，属并行会话，本评测只读未触碰）
> sz-orm 侧：main `077ddd75` 起步，本批新增提交见 §5
> 运行环境：szrsql 6543（`--auth-mode trust --connection-idle-timeout 0 --data-dir D:/tmp/szrsql-matrix`），对拍脚本在 `D:\tmp\m{1,2,4b,56,7,9*,m9b_unicode,m9c_indf,m9d_probe}_*.py`（不入仓库，符合提示词坑位规范）

## 0. 结论速览

| 能力块 | 结论 | 差分结果 |
|--------|------|---------|
| M1 QueryBuilder 生成 SQL | **通过** | 12/12 与 PG18 逐行一致（聚合/NULL 排序/join/子查询/union/case） |
| M2 表达式层（typed_ast） | **通过** | 12/12 一致（ILIKE/DISTINCT ON/CAST/字符串函数/数学函数/Extract/coalesce/greatest） |
| M3 迁移与 schema_sync | **基本通过**（1 缺陷） | DDL 变体全通；advisory lock 函数缺失 → [sz-orm 侧已修复降级] |
| M4 事务深化 | **P0 上游缺陷** | RR/Serializable 非真快照语义（出现不可重复读） |
| M5 连接池行为 | **缺陷** | DISCARD ALL/RESET ALL 解析不支持（session 复位路径不可用） |
| M6 批量与流式 | **通过**（1 缺陷） | generate_series 10 万行/1000 批 INSERT/游标 DECLARE+FETCH 全通；`EXECUTE` 被防火墙拦截（SQL 级 PREPARE/EXECUTE 不可用，协议级 prepared 正常） |
| M7 多租户与 N+1 | **通过** | SET 会话变量/错误后连接恢复一致；`current_setting()`/`SHOW` 返回空串（与 PG 不同但非阻断） |
| M8 RETURNING/写后读 | **基本通过**（1 缺陷） | INSERT/UPDATE/DELETE..RETURNING 全通；`lastval()` 未实现 |
| M9 边缘 SQL（bug 猎场） | **P0 上游缺陷 ×2** | IS [NOT] DISTINCT FROM 规划期误裁列 → ColumnNotFound；bool↔int cast 拒绝；int 溢出不报错；numeric 除法精度差 |

**sz-orm 集成缺口修复：1 项**（迁移锁降级，§4.1）。**szrsql 上游缺陷：8 项**（§2，可反馈级别 4 项）。**文档措辞问题：2 项**（§3）。

---

## 1. 矩阵逐块记录（两库对照）

### M1 QueryBuilder 生成 SQL —— 12/12 通过

载荷：group/having/聚合遇 NULL、NULL 排序四型（ASC/DESC/NULLS FIRST/NULLS LAST）、inner/left join、标量/IN 子查询、UNION/UNION ALL、DISTINCT/CASE。
方法：同一数据集（6 行含 NULL）双库执行逐行 diff（`D:\tmp\m1_diff.py`）。
全部结果集与错误面一致。**优于预期**：NULLS FIRST/LAST、聚合遇 NULL/空集语义与 PG 完全一致。

### M2 表达式层 —— 12/12 通过

载荷：ILIKE、DISTINCT ON、CAST（`CAST`/`::` 双语法）、Lower/Upper/Trim/Concat/Substring、round/floor/ceil/power/mod、Extract(year/month/day)、coalesce/nullif/greatest/least、length/replace/position。
`D:\tmp\m2_diff.py` 全绿。PG 特化分支（Szrsql 纳入 PG 家族 match 后）真实可执行。

### M3 迁移与 schema_sync

DDL 变体（pgsql 直连执行，`[[ignore]]` 外另附 sz-orm 矩阵测试 matrix_m3_ddl_variants 通过）：
- CREATE TABLE / PRIMARY KEY / NOT NULL：✅
- 普通索引 / 部分索引（`WHERE age > 18`）/ 表达式索引（`upper(name)`）：✅
- `COMMENT ON TABLE/COLUMN`：✅
- `ALTER TABLE ADD COLUMN` / `ALTER COLUMN SET DEFAULT`：✅
- FK（REFERENCES ... ON DELETE CASCADE）：✅
- `CREATE UNIQUE INDEX IF NOT EXISTS`：✅
- 8 层嵌套 savepoint 语义（2 层 ROLLBACK TO + RELEASE 实测）：✅（matrix_m3b_savepoint_stack 通过）

缺陷：**advisory lock 函数缺失**（`pg_advisory_lock`/`pg_try_advisory_lock`/`hashtext` 均 `function not found`）→ 触发 sz-orm 迁移锁路径 Err 上抛，`Migrator::migrate()` 整体失败。定性：**双方交界缺陷**——szrsql 缺函数是上游事实，但 sz-orm 在「方言声明支持锁而后端探测失败」时不应让 migrate 整体失败（rows 为空已有降级分支，Err 路径却没有）。sz-orm 侧已修复（§4.1），同时登记为 szrsql 上游功能缺口（[UP-7]，P3）。
迁移锁双进程并发真语义无法在 szrsql 验证（函数缺失），如实记录「未验证」。

### M4 事务深化 —— P0

复现实验（`D:\tmp\m4_isolation_v3.py` + `m4b_isolation.py`，双库同流程）：

```
会话 A: BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ;
        SELECT v FROM m4_t WHERE id=1;          -- 100
会话 B: UPDATE m4_t SET v=200 WHERE id=1;       -- (autocommit)
会话 A: SELECT v FROM m4_t WHERE id=1;          -- ?
szrsql:  200  → 非快照稳定（ReadCommitted 行为）
PG18:    100  → 快照稳定（真 RR）
SERIALIZABLE 同样实验：szrsql 200 / PG18 100
```

语法面：`START TRANSACTION ISOLATION LEVEL ... , READ ONLY`、`SET SESSION CHARACTERISTICS AS ...` 均接受（隔离级别字面量被解析）；`SET TRANSACTION READ WRITE` 在事务块外静默成功（PG18 报 WARNING）——即**隔离级别是仅接受语句、非真语义**。savepoint/COMMIT/ROLLBACK/panic 安全回滚在既有集成测试中通过。
MVCC 代码存在（`scan_from_row_store` 走 `mvcc.is_visible`），但 SELECT 快照未按事务级固定。**这是数据正确性类缺陷，可反馈级别**（[UP-1]）。

### M5 连接池行为 —— 缺陷 ×2

- `DISCARD ALL`（sz-orm session_reset 默认路径之一）：`unsupported statement: Discard { object_type: ALL }`；SEQUENCES/PLANS/TEMP 同样不支持。
- `RESET ALL`：sqlparser 解析错误（`Expected: an SQL statement, found: RESET`）。
- 连接池 acquire/release 往返（matrix 测试 + 既有 pool 测试）：✅。
- 中途 kill szrsql 二进制的池恢复实验：**未执行**——评测中途 szrsql 进程曾静默退出一次（日志无 panic 记录，重启后继续），恰构成一次真实断连观察：sz-orm 侧重连即恢复，未复现池挂死；正式的 kill-recover 矩阵用例因属稳定性长测未纳入本批，如实登记未验证。

### M6 批量与流式 —— 通过（1 缺陷）

- `generate_series(1,100000)` 计数：✅ 与 PG18 一致
- 1000 行批量 INSERT（`INSERT...SELECT generate_series`）：✅
- 服务端游标 DECLARE/FETCH/CLOSE（含分次 FETCH 3+2）：✅（matrix_m6_batch_and_cursor 通过）
- SQL 级 `PREPARE`：✅ 接受；`EXECUTE psel(7)`：❌ `SQL blocked by firewall: blocked command: EXEC`（[UP-6] P3，防火墙规则误伤合法 EXECUTE 语句；sqlx 协议级 prepared statement 不受影响，既有参数化测试通过）
- >10 万行流式读稳定性：列为超范围长测，未执行，如实登记。

### M7 多租户与 N+1 —— 通过（1 措辞项）

- `SET application_name`：✅；`current_setting()`/`SHOW application_name`：返回**空串**（PG18 返回 'psql'）——GUC 读回路径未接（[UP-8] P3）。
- 语句级错误后连接继续可用：✅（matrix_m7_session_and_error_recovery 通过）。
- 错误后事务块内继续执行：szrsql 允许 `SELECT` 继续 + COMMIT 成功（PG18 会 abort 事务强制 ROLLBACK）——宽松方向差异，登记为文档措辞/行为差异项（§3 D-2）。
- `connection_tenant`/`N1QueryDetector` 为 sz-orm 侧组件（提示词 M7 定性「纯 sz-orm 侧」），矩阵测试确认 szrsql 后端下不回归（既有 szrsql 集成 8 项 + 矩阵 11 项全过）。

### M8 RETURNING/写后读 —— 基本通过

- INSERT/UPDATE/DELETE .. RETURNING：✅（matrix_m8_returning 通过）
- `lastval()`：❌ `function not found: lastval`（[UP-5] P3）。sz-orm active_model save 走 RETURNING 回填主键，不受影响；`nested_active_model::get_last_insert_id` 硬编码 `SELECT LAST_INSERT_ID()`——该路径对 PG18 同样失败（PG 无此函数），属**历史遗留的 sz-orm 缺陷**（与 szrsql 无关，nested_save 生产零调用、仅 mock 测试覆盖），登记为遗留问题不属本批修复面（见 §5 遗留风险）。

### M9 边缘 SQL —— bug 猎场收获

`D:\tmp\m9_diff.py`（29 项）+ UTF-8 复验（`m9b_unicode.py`/`m9c_indf.py`/`m9d_probe.py`）：

| # | 载荷 | szrsql | PG18 | 定性 |
|---|------|--------|------|------|
| X2/X3 | `WHERE val IS NOT DISTINCT FROM 40` / `cat IS NOT DISTINCT FROM NULL` | **ERROR: column not found: val** | 正常返回 | [UP-2] **P0** |
| X10 | `SELECT true::int, 1::bool` | **ERROR: implicit cast not allowed** | `1\|0\|t\|f` | [UP-3] P2 |
| X27 | `SELECT 2147483647::int + 1` | 返回 2147483648（静默溢出） | ERROR: integer out of range | [UP-4] P2 |
| X29 | `SELECT 1::numeric / 3` | `0.3333333333333333`（16 位） | `0.33333333333333333333`（20 位） | [UP-9] P3 |
| 其余 18 项 | 三值 NULL/隐式转换/保留字引号/Unicode 标识符/空串 vs NULL/60 参数 | 与 PG18 一致 | — | 通过 |

中文/Unicode：首轮对拍 PG18 侧报 `invalid byte sequence 0xb1` 为**我方 GBK 控制台编码假象**，改用 stdin UTF-8 注入后双库一致（教训：差分工具自身先过编码关）。

### [UP-2] 深度定位（可反馈级别核心证据）

最小复现：

```sql
CREATE TABLE t (id INT PRIMARY KEY, v INT);
INSERT INTO t VALUES (1, 10), (2, NULL);
SELECT id FROM t WHERE v IS NOT DISTINCT FROM 10;   -- ERROR: column not found: v
SELECT id FROM t WHERE v IS NULL;                    -- OK（对照）
SELECT id FROM t WHERE id < 5 AND v IS NOT DISTINCT FROM 10;  -- OK（对照：id 走 IndexScan，schema 未被裁剪）
EXPLAIN SELECT id FROM t WHERE v IS NOT DISTINCT FROM 10;
-- Filter: (IsDistinctFrom )  → Parallel Seq Scan (width=4，仅 id —— v 已被裁剪)
-- 对照 IS NULL 版本 Scan width=8（v 保留）
```

定位线索（只读核实）：
- 求值器实现完好：`crates/szrsql-sql/src/expr.rs:2703`（`eval_is_distinct_from` NULL 语义正确）、`crates/szrsql-sql/src/expr.rs:1230`（eval 分派）、`crates/szrsql-sql/src/expr.rs:9783`（precompile 递归）。
- 根因在优化器 ProjectionPruning 的三个列引用收集器**均未递归 `Expr::IsDistinctFrom`**（落入 `_ => {}`）：
  - `crates/szrsql-optimizer/src/rule.rs:1768`（`collect_expr_unqualified_refs`——非限定列收集，缺失即被裁剪）；
  - `crates/szrsql-optimizer/src/rule.rs:1597`（`collect_expr_column_refs_grouped`）；
  - `crates/szrsql-optimizer/src/rule.rs:435`（`collect_column_refs_recursive`）。
- 该 crate 全文 0 处 `IsDistinctFrom`（`grep -c` = 0），Phase F-9 加入该表达式时未同步优化器。同构对照：`IsNull` 在三处收集器均有递归 arm，故 `IS NULL` 正常。
- 触发链：pgwire 会话优化管线 `crates/szrsql-protocol/src/pgwire/session.rs:7028/7471`（`ProjectionPruning::apply`）→ Scan schema 被裁至投影列 → Filter 逐行 eval 时 `v` 列不存在。
- 旁证：`id < 5 AND v IS NOT DISTINCT FROM 10` 走 IndexScan（IndexSelection 保留完整谓词、schema 未裁剪）即返回正确结果 1；`v IS NOT DISTINCT FROM 10 AND id < 5` 走 Filter+ParallelScan 即失败——失败与否完全由走没走 ProjectionPruning 裁剪决定。
- 推论：所有「仅出现在谓词中、不在投影中」且收集器缺 arm 的表达式类型都有同类风险（`Substring`/`SimilarTo` 在 rule.rs 同样缺 arm，未逐一验证，建议上游一并排查）。

---

## 2. szrsql 上游缺陷清单（只读，未改其任何文件）

| ID | 分级 | 缺陷 | 证据 |
|----|------|------|------|
| UP-1 | **P0** | REPEATABLE READ / SERIALIZABLE 非真快照语义：其他会话已提交的 UPDATE 在 RR 事务内可见（不可重复读） | §1 M4；`m4_isolation_v3.py` 双库对照输出 |
| UP-2 | **P0** | IS [NOT] DISTINCT FROM 在 WHERE/投影中被 ProjectionPruning 误裁列 → `column not found` | §1 M9/§0 定位线索；`m9c_indf.py` 最小复现 |
| UP-3 | P2 | bool↔int cast 全拒绝（`true::int`/`1::bool`），PG 为允许 | `m9_diff.py` X10 |
| UP-4 | P2 | int 溢出静默回绕（`2147483647::int + 1` = 2147483648），PG 报错 | `m9_diff.py` X27 |
| UP-5 | P3 | `lastval()` 未实现 | `m56_diff.py` M8-returning |
| UP-6 | P3 | SQL 级 `EXECUTE psel(...)` 被防火墙拦截（PREPARE 可用） | `m56_diff.py` M6-prepared |
| UP-7 | P3 | advisory lock 函数族（pg_advisory_lock/pg_try_advisory_lock/pg_advisory_unlock/hashtext）未实现 | §1 M3；sz-orm 侧已降级兼容 |
| UP-8 | P3 | `current_setting()`/`SHOW` 返回空串（GUC 读回未接） | `m7_setting_probe.py` |
| UP-9 | P3 | numeric 除法精度 16 位（PG 20 位） | `m9_diff.py` X29 |

反馈提示词：[2026-11-09-szrsql-matrix-bugs-feedback-prompt.md](2026-11-09-szrsql-matrix-bugs-feedback-prompt.md)（覆盖 UP-1/UP-2 两个可反馈级别 + P2/P3 项附批量复现清单）。

## 3. 双方文档措辞问题

- D-1：szrsql「PG 语义」宣称需限定范围——事务隔离级别（UP-1）与 advisory lock（UP-7）当前不满足 PG 语义，建议文档明示「隔离级别语句语法兼容，快照语义 RR/SERIALIZABLE 未实现」。
- D-2：szrsql 事务内语句错误后允许继续执行并 COMMIT 成功（PG 强制 aborted-transaction）。宽松方向、不产生错误数据，但依赖「错误即 abort」语义的应用会感知差异，建议文档登记。

## 4. sz-orm 集成缺口修复记录

### 4.1 迁移锁探测失败降级（本批修复，commit `6ecfc96`）

- 位置：`packages/sz-orm-core/src/migration.rs` `acquire_migration_lock` 循环内 `conn.query(&try_sql)` 的 Err 分支（新增 match 降级，warn 消息「迁移锁语句执行失败，降级为无锁迁移」@ [packages/sz-orm-core/src/migration.rs:679](../../packages/sz-orm-core/src/migration.rs#L679)）。
- 修复：Err → `tracing::warn!` + `return Ok(None)`（降级无锁迁移），与既有「rows 为空跳过串行化」分支语义对齐；多实例并发由部署侧串行化的既有约定不变。
- 测试：新增 `r7_lock_query_error_degrades_to_unlocked_migration`（`migration.rs` tests，模拟 Szrsql 缺函数路径：DDL 正常执行、不尝试 release 未持有的锁）。`cargo test -p sz-orm-core --lib` 3272 passed。

### 4.2 审查后明确**不属于** szrsql 集成缺口的项

- `nested_active_model::get_last_insert_id` 硬编码 `LAST_INSERT_ID()`：对真 PG 同样失败，历史遗留（生产零调用、仅 mock 覆盖），不属本批（§5）。
- `DbType::Szrsql` 方言分支（`dialect.rs:1760/2844/4877`，`any_driver.rs:129`）委托 PG 族完整，无缺 arm。

## 5. 支持面扩张结论（宁窄勿宽）

- **维持**：v10.13.2 既有声明「`DbType::Szrsql` + PG 方言族委托 + `szrsql://` scheme + 8 项真连集成测试」不变。
- **新增可信声明**：QueryBuilder 同型 SQL（M1 12 项）、表达式层（M2 12 项）、DDL 变体、RETURNING、游标、10 万行级 generate_series、批量 INSERT、错误恢复——在 szrsql 与 PG18 行为一致（矩阵 14 项测试，11 真连通过 + 3 项上游缺陷留档 `#[ignore]`）。
- **不可声明**（防幻影）：「szrsql 支持可序列化/可重复读事务」「szrsql 支持 advisory lock 迁移锁」「szrsql 支持 session 复位（DISCARD/RESET）」——均不成立或未验证。
- 遗留风险：①`nested_active_model` 的 `LAST_INSERT_ID()` 历史缺陷（PG 族后端均不可用）；②M5 kill-recover 长测与 M6 >10 万行流式长测未执行；③szrsql 并行会话高频变动，缺陷复现以其基线 `f937e474` 为准。

## 6. 验收清单执行记录

1. `cargo check --workspace --all-targets` ✅（1m21s）；`cargo clippy --workspace --all-targets -- -D warnings` ✅；fmt 逐包 72/72 ✅
2. `cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored` → **11 passed, 3 failed（均为 `#[ignore="szrsql 上游 ..."]` 留档项，真连复现上游缺陷本身，符合提示词验收形态）**
3. `cargo test -p sz-orm-sqlx --test integration_szrsql -- --ignored` → **8 passed** 不回归
4. 差分对拍：M1/M2/M9 双库对照输出（§1 各节脚本路径）；M4 双会话并发对照；M5/M6/M8 双库对照——无凭单库行为断言项
5. `cargo test --workspace`（PYTHONHOME 注入）→ **605 套件 0 failed，15,893 passed**（日志 D:\tmp\ws_test2.log）；G9 `--strict` 46/46 Safe + 自测 15/15 ✅ / G12 ✅ / G14 ✅ / G15 ✅（PHANTOM-1 0 / 接线 4/4）/ G16 ✅ / G17 ✅ / G19 ✅ / G24 ✅ / G26 ✅ / G27 ✅ / G28 ✅ / G30 ✅ / G31 ✅ / G32 ✅ / G33 ✅ / G35 ✅（0 死链）/ G36 ✅
6. bug 三分类清单：§2（上游 9 项）/ §4（集成缺口 1 项修复）/ §3（文档措辞 2 项），均有双库输出 + file:line

## 7. 六环闭环状态

1. 实施提示词：`2026-11-09-szrsql-matrix-differential-prompt.md`（先行落盘，commit 077ddd75）
2. 本报告：本文档；audit-verify 自证见文末
3. 实施批：integration_szrsql_matrix.rs（`ac36909`）+ 迁移锁降级（`6ecfc96`）+ fmt（后续提交）
4. 复评：门禁逐项退出码 + 矩阵测试 EXIT 码见 §6；缺陷项复现脚本双库输出留档 D:\tmp
5. 红队复核：待独立会话执行（`background-task` 启新 session）
6. 登记：CHANGELOG/AGENTS.md 更新随本批提交
