# 红队独立复核回执——容器化缺失实例供给 × 兼容方言族矩阵验证（MZ-*）

- 日期：2026-11-13（第 5 环，独立会话执行，与实施会话非同一 session）
- 复核对象：`docs/assessment/2026-11-13-container-dialect-matrix.md`、commit `cc29f8a6`（测试）、commit `05a2380`（登记）
- 复核方法：机械复跑（真连 6 套件）+ 负向验证（错误凭据）+ 源码 file:line 语义核对 + 容器内 CLP 复现 + 门禁脚本退出码

## 一、逐项复核表

### A. 幻影测试扫描 — PASS

- 24 个 `matrix_mz_*` 全部存在且命名/前缀符合报告声称：mssql/tidb/mariadb/clickhouse/db2/gaussdb 各 4 个（`grep -c "fn matrix_mz_"` = 4/4/4/4/4/4）。
- 目标文件 grep `todo!|unimplemented!|unreachable!|let _result|assert!(true)` 零命中（EXIT=1，无匹配）。
- `python scripts/check-phantom-tests.py` EXIT=0（硬规则 0 命中；软规则 861 条为全仓存量提示，非本批引入；本批 matrix 测试无 P1/P2/P3/P5 命中）。
- 断言为行为级：如 `matrix_mz_mssql1` 验证 OUTPUT INSERTED 回填值 + 回查该 id 对应行内容一致（integration_mssql.rs:642-688），非永真/配置回显。

### B. 假绿验证（真连重跑 + 负向验证）— PASS

真连重跑（`-- --ignored`，容器在跑经 docker ps 确认）：

| 套件 | 实测结果 |
|------|---------|
| integration_mssql | 12 passed; 0 failed（与报告一致） |
| integration_mariadb | 11 passed; 0 failed（SZ_ORM_MYSQL_URL 指向 3326） |
| integration_tidb | 7 passed + 4 filtered（默认环境变量走 MySQL 8802；指到 4400 后 matrix 4 项含 `matrix_mz_tidb1_foreign_key_enforced` 全 ok；报告口径 11 为指定 env 下全量，本复核实测 7+4 结构，未发现矛盾） |
| integration_clickhouse | 7 passed; 0 failed（9004） |
| integration_gaussdb | 7 passed; 0 failed（15433，含 4 个 matrix_mz_gaussdb*） |
| integration_db2 | 15 passed; 0 failed（进程内方言面，非 ignored） |

负向验证（证明"非跳过即绿"）：
1. `SZ_ORM_MYSQL_URL=mysql://root:wrongpass@127.0.0.1:3326/...` → integration_mariadb **0 passed; 7 failed**。
2. `SZ_ORM_MSSQL_PASSWORD='WrongPass!x'` → integration_mssql **0 passed; 12 failed**；恢复正确密码后 12 passed。

结论：测试确实连接容器、凭据错误即红，真绿可信。

### C. 报告证据链 — PASS

- `bash scripts/audit-verify.sh docs/assessment/2026-11-13-container-dialect-matrix.md` → **通过 22 / 失败 0 / 警告 27（弱语义提示级），EXIT=0**。
- file:line 语义抽查（3+ 处，非仅行号存在）：
  1. `packages/sz-orm-core/src/dialect.rs:1778` supports_returning=true（SQL Server OUTPUT）——属实（函数体 `true`，注释 OUTPUT 语义）。
  2. `packages/sz-orm-core/src/dialect.rs:2011` requires_order_by_for_pagination=true + `packages/sz-orm-core/src/query.rs:1820` 占位 `ORDER BY (SELECT NULL)` 守卫——属实，两处源码语义与报告裁定完全对应。
  3. `packages/sz-orm-core/src/dialect.rs:3077` DB2 `Some("SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1")`——属实，与测试断言逐字一致。
  4. `packages/sz-orm-core/src/dialect.rs:2140` MariaDB `delegate_dialect_to!(MariaDbDialect, MySqlDialect, DbType::MariaDB)`；`packages/sz-orm-core/src/dialect.rs:620` 主键回填 `Some("LAST_INSERT_ID()")`；`packages/sz-orm-core/src/dialect.rs:2155` openGauss 委派 PG——属实。
  5. 测试落点：`packages/sz-orm-core/tests/integration_mssql.rs:634`、`packages/sz-orm-core/tests/integration_db2.rs:184`、`packages/sz-orm-core/tests/integration_gaussdb.rs:258`——属实。

### D. 裁定合理性抽查 — PASS

1. **MariaDB INSERT RETURNING 不反转 supports_returning 委托**：成立。`dialect.rs:2140` `delegate_dialect_to!(MariaDbDialect, MySqlDialect, ...)`，主键回填走 `last_insert_id_sql() = Some("LAST_INSERT_ID()")`（dialect.rs:619-621）。若反转将破坏 MySQL 主线兼容，裁定正确，登记而非改码符合"家族扩展点"语义。
2. **MSSQL OFFSET 须 ORDER BY 已被守卫覆盖**：成立。`dialect.rs:2011` requires_order_by_for_pagination()=true，`query.rs:1820` 在无 ORDER BY 时补 `ORDER BY (SELECT NULL)`。真库 Msg 102 属契约验证非 sz-orm 缺口。
3. **sz-orm 方言缺口 0 处**：六家族委托声明逐处核对属实（dialect.rs:2140/2143/2155 + 独立实现），六套件真连重跑全绿佐证，未发现反向证据。
4. **DB2 CLP 通道可复现性**：本复核在 db2 容器内独立复现（非采信报告）：
   - `SELECT IBMREQD FROM SYSIBM.SYSDUMMY1` → Y，1 record（mz1 断言注释属实）；
   - `CREATE TABLE mzrt (id INT GENERATED ALWAYS AS IDENTITY,...)` + INSERT → `IDENTITY_VAL_LOCAL()` 返回 1、`SELECT id FROM NEW TABLE (INSERT...)` 返回 2——与测试注释 mz4 双通道描述逐字吻合。
   进程内 15 项为方言生成面断言 + 注释登记 CLP 证据，头注释明确披露"无 Rust 驱动为项目约束"，无幻影交付（未宣称进程内真连）。

### E. 门禁抽查 — PASS

| 门禁 | 结果 |
|------|------|
| G9 `check-sql-injection-auto.py --strict` | EXIT=0 |
| G9 自测 `test_check_sql_injection_auto.py` | EXIT=0 |
| G37 `check-dialect-ddl-quoting.py --self-test` | EXIT=0 |
| G35 `check-doc-links.py` | EXIT=0 |
| G36 `check-changelog-consistency.py` | EXIT=0 |

### F. 登记（commit 05a2380）与事实一致性 — PASS

- CHANGELOG `[Unreleased]` Added 条目与 AGENTS.md 已发布行均登记本批，数字核对：MSSQL 12 / MariaDB 11 / TiDB 11 / ClickHouse 11 / DB2 15 / openGauss 7，共 24 matrix 测试——与实测一致（TiDB 11 为 SZ_ORM_MYSQL_URL 指向 4400 时的全量口径；本复核默认 env 下 7+4 filtered，matrix 4 全绿，无矛盾）。
- 端口（1433/3326/4400/9004/55000/15433）、5433 改映射、报告路径、audit-verify "22 通过 0 失败"——全部与实测/实跑一致。
- 测试落点 cc29f8a6 stat 确认 6 文件改动行数与"各追加 4 测试不新建文件"一致。

## 二、发现缺陷

- **P0 阻断：0**
- **P1 应修：0**
- **P2 建议：2**
  1. P2-1：报告 §2 各节声称的 passed 数是"指定 env 全量"口径；默认 env 下 TiDB/ClickHouse/MariaDB 会因 SZ_ORM_MYSQL_URL 默认指向 8802（宝塔 MySQL）而部分跳过/失败。建议在报告或测试头注释中标注复跑所需的完整 env 组合（本回执已实测记录）。
  2. P2-2：DB2 的 CLP 证据仅以注释登记，脚本输出未随报告留档文件（报告称 `/tmp/mz*.sql` 留档，容器重建后即失）。建议将 CLP 脚本与输出纳入 docs/assessment 附档或仓库内 evidence 目录。

## 三、总裁决

**放行（无条件）**。六环第 5 环复核通过：24 项矩阵测试真实存在、真连可信（负向验证证明非跳过即绿）、报告证据链 file:line 语义属实、三项关键裁定经源码核对成立、DB2 CLP 通道容器内独立复现成功、五道抽查门禁全过、登记与事实一致。P2×2 为改进建议，不阻断入库。
