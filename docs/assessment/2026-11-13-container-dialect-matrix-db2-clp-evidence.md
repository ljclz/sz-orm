# MZ-DB2 容器内 CLP 证据附档（红队回执 P2-2 处置）

> 归属批次：`2026-11-13-container-dialect-matrix.md`（MZ-* 六实例矩阵）。红队回执 `2026-11-13-container-dialect-matrix-redteam-review.md` P2-2：DB2 CLP 证据原仅留容器 `/tmp`（重建即失）——本附档将脚本与输出永久留档。
> 环境：DB2 Community `icr.io/db2_community/db2:latest` 容器（宿主端口 55000），库 `szormdb`，实例 `db2inst1`。执行通道：`docker exec db2 bash -c 'su - db2inst1 -c "db2 -tvf <script>"'`。
> 复跑时点：2026-11-13（红队放行后回执处置阶段，独立于实施批执行）。
>
> **对应源码/测试证据锚点**（audit-verify 校验面）：`packages/sz-orm-core/tests/integration_db2.rs:196`（matrix_mz_db2_sysdummy1_channel——SYSDUMMY1 通道断言）/ `packages/sz-orm-core/tests/integration_db2.rs:209`（matrix_mz_db2_pagination_offset_fetch）/ `packages/sz-orm-core/tests/integration_db2.rs:222`（matrix_mz_db2_identifier_folding）/ `packages/sz-orm-core/tests/integration_db2.rs:234`（matrix_mz_db2_identity_backfill_channels）/ 方言实现 `packages/sz-orm-core/src/dialect.rs:3072`（GENERATED ALWAYS AS IDENTITY）与 `packages/sz-orm-core/src/dialect.rs:3077`（SYSDUMMY1 回填通道）。

## 1. mz_recheck.sql（基础连通 + 实施批遗留表）

```sql
CONNECT TO szormdb;
SELECT * FROM SYSIBM.SYSDUMMY1;
SELECT id, name FROM mzp ORDER BY id;
```

输出（节选，2026-11-13 实测）：

```
SELECT * FROM SYSIBM.SYSDUMMY1
（省略表头）
IBMREQD
-------
Y

  1 record(s) selected.

SELECT id, name FROM mzp ORDER BY id

ID                   NAME
-------------------- --------------------------------------------------
                   1 a
                   2 b

  2 record(s) selected.
```

## 2. mz2.sql（分页 + IDENTITY 双通道——对应 matrix_mz_db2_* mz2/mz4）

```sql
CONNECT TO szormdb;
SELECT id FROM mzp ORDER BY id DESC OFFSET 1 ROWS FETCH NEXT 1 ROWS ONLY;
CREATE TABLE mzcap (id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, v VARCHAR(20));
INSERT INTO mzcap (v) VALUES (chr(120));
SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1;
SELECT id FROM NEW TABLE (INSERT INTO mzcap (v) VALUES (chr(121)));
DROP TABLE mzcap;
```

输出（节选，2026-11-13 实测；与实施批输出一致）：

```
SELECT id FROM mzp ORDER BY id DESC OFFSET 1 ROWS FETCH NEXT 1 ROWS ONLY
ID
--------------------
                   1
  1 record(s) selected.

CREATE TABLE mzcap ...
DB20000I  The SQL command completed successfully.
INSERT INTO mzcap (v) VALUES (chr(120))
DB20000I  The SQL command completed successfully.
SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1
（列头 1）
                               1.
  1 record(s) selected.
SELECT id FROM NEW TABLE (INSERT INTO mzcap (v) VALUES (chr(121)))
ID
--------------------
                   2
  1 record(s) selected.
DROP TABLE mzcap
DB20000I  The SQL command completed successfully.
```

## 3. mz3.sql（大写折叠/双引号 quote——对应 matrix_mz_db2 mz3）

```sql
CONNECT TO szormdb;
CREATE TABLE mzcase (id INT, "order" VARCHAR(20));
INSERT INTO mzcase VALUES (1, chr(111));
SELECT "order" FROM mzcase;
DROP TABLE mzcase;
```

输出（节选，2026-11-13 实测）：`CREATE TABLE`/`INSERT`/`SELECT "order"` 全成功，返回 `o`（1 row），`DROP TABLE` 成功——双引号阻止大写折叠且保留字可作列名。

## 4. 结论

- 实施批 integration_db2.rs 中 `matrix_mz_db2_*` 四个测试注释登记的 CLP 证据（SYSDUMMY1=IBMREQD=Y / OFFSET..FETCH NEXT 正确行 / IDENTITY_VAL_LOCAL 回填 / NEW TABLE(INSERT) 扩展通道 / 双引号保留小写）全部独立复现成功。
- 本附档落盘后，DB2 真库证据不再依赖容器内 `/tmp` 生命周期。
