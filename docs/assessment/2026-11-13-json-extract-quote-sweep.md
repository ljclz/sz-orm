# json_extract 列名 quote 横向修复批（12 方言一致化）——2026-11-13

> 六环闭环批次。G-OR1 家族**第七次同型发现横向收尾**（G-OR1→MY G2→G-OR9→FTS 批→批二→PG 矩阵红队第六形态→本批）。模式沿用 FTS quote 批（2026-11-12）与 ALTER INDEX/FK quote 批二。
>
> **来源**：PG 方言矩阵批红队 §3.3 抓出 `PostgreSqlDialect::json_extract` 列名裸拼（已随 624974c9 修复）并实证 G37 Q2 敏感模板未含 JSON 函数面属预期内盲区；SQLite 批红队 P3-2 裁定「全方言裸拼为既有共同约定」并登记下批候选——本批即该候选落地。
>
> **定级口径（红队 P2-2 勘误）**：本批修复分两档——**真缺陷修复**（Oracle/PG 等折叠语义方言：裸拼列名被折叠为与引号列不匹配，ORA-00904 类真库拒绝）与**防御性一致化**（MySQL/SQLite：裸标识符大小写不敏感，红队真库证伪两形态等价命中——quote 一致化消除方言间行为漂移，非修复真错位）。G37 JSON 函数面门禁增补对两档均有效（防第八形态复发）。
>
> **范围**：`Dialect::json_extract` trait 面 12 直接实现列名 quote 一致化 + G37 敏感模板增补 JSON 函数面。`json_query.rs`（think-orm 风格 JsonQuery，自带手写 quote 混合形态）与 `typed_ast.rs` JSON 表达式（**已各自 quote**，`col = dialect.quote(C::NAME)`）不在本批范围——前者另行评估，见 §5。

## 1. 交付物

| 项 | 内容 | 证据 |
|---|------|------|
| 方言修复 | 11 方言 `json_extract` 列名裸拼 → `self.quote(column)`（MySQL/SQLite/Oracle/SqlServer/Snowflake/ClickHouse/DuckDB/DB2/Informix/SapHana/Firebird）；PG 此前已修；委托族（Kingbase/PolarDB/GaussDB/Szrsql/CockroachDB/YugabyteDB/Redshift/Dameng/TiDB/MariaDB 等）经 `delegate_dialect_to!` 宏转发（`dialect.rs:2070-2072`）自动惠及，零改动 | `packages/sz-orm-core/src/dialect.rs:516-530`(MySQL)/`1164-1176`(SQLite)/`1538-1552`(Oracle)/`1805-1819`(SqlServer)/`2272-2276`(Snowflake)/`2562-2574`(ClickHouse)/`2822-2826`(DuckDB)/`3032-3042`(DB2)/`3388-3396`(Informix)/`3627-3637`(SapHana)/`3878-3888`(Firebird) |
| 横向单测 | `test_json_extract_column_quoting_all_dialects`（12 方言混合大小写列 `MetaCol` 逐方言 quote 形态逐字节 + 裸拼反向断言 + 委托族抽验 Szrsql→PG / Dameng→Oracle） | `packages/sz-orm-core/src/dialect.rs:4237`；`cargo test -p sz-orm-core --lib test_json_extract_column_quoting_all_dialects` → 1 passed |
| G37 增补 | `SENSITIVE_PATTERNS` 追加 6 关键词（`JSON_EXTRACT(`/`json_extract(`/`JSON_VALUE(`/`JSONExtract`/`JSON_GET(`/`#>>`）+ 自测 3 命中样例（历史裸拼形态必命中）+ 2 修复样例（quote 形态必不命中） | `scripts/check-dialect-ddl-quoting.py:41-66`；`--self-test` → **15 passed / 0 failed**（原 10） |
| 既有断言同步 | Oracle/SqlServer 单测 `starts_with("JSON_VALUE(data, ...")` → `JSON_VALUE("data"`/`JSON_VALUE([data"`（quote 后形态）；SQLite 矩阵 SQ6 / MySQL 矩阵 MY3 生成面断言同步（调用方不再预 quote） | `dialect.rs:4400-4404,4863-4867`；`integration_sqlite_matrix.rs:499-502`；`integration_mysql_matrix.rs:610-613` |
| 报告 | 本文档 | audit-verify 自证见 §4 |

## 2. 修复形态逐方言（quote() 形态方言感知）

| 方言 | 修复后生成形态（`json_extract("MetaCol", "user.name")`） | quote 形态 |
|---|---|---|
| MySQL | `JSON_EXTRACT(\`MetaCol\`, '$.user.name')` | 反引号 |
| PostgreSQL（已修） | `"MetaCol"#>>'{user,name}'` | 双引号 |
| SQLite | `json_extract("MetaCol", '$.user.name')` | 双引号 |
| Oracle | `JSON_VALUE("MetaCol", '$.user.name')` | 双引号（裸拼折叠大写 ORA-00904 同 G-OR9 类） |
| SqlServer | `JSON_VALUE([MetaCol], '$.user.name')` | 方括号 |
| Snowflake | `"MetaCol":user.name` | 双引号 |
| ClickHouse | `JSONExtractString(\`MetaCol\`, 'user.name')` | 反引号 |
| DuckDB | `"MetaCol" -> 'user.name'` | 双引号 |
| DB2 | `JSON_VALUE("MetaCol", '$.user.name')` | 双引号 |
| Informix | `JSONExtract("MetaCol", '$.user.name')` | 双引号 |
| SapHana | `JSON_VALUE("MetaCol", '$.user.name')` | 双引号 |
| Firebird | `JSON_GET("MetaCol", '$.user.name')` | 双引号 |

**调用方契约变化（语义级，登记）**：`json_extract` 现在接受**裸列名**并在方言内 quote；此前部分调用方预 quote 后传入（如 `dialect.json_extract(&dialect.quote("meta"), ...)`）会产生双重引号（真库拒绝）——既有调用点已在本批同步（§1 第 4 行），外部调用方如存在同形态需同步（发布说明见 CHANGELOG 条目「调用方契约变化」）。

## 3. 真连与回归（env 注入，2026-11-13 实测）

```
cargo test -p sz-orm-core --lib                          → 3285 passed; 0 failed（含新横向单测）
PG18@5432（SZ_ORM_PG_URL）：
  --test integration_pg_matrix -- --ignored              → 20 passed（含 json_extract 混合大小写探针）
  --test integration_pg -- --ignored                     → 22 passed
SQLite（内存库免 env）：
  --test integration_sqlite_matrix                       → 10 passed（SQ6 断言同步后）
  --test integration_sqlite -- --ignored                 → 25 passed
MySQL 8802（SZ_ORM_MYSQL_URL）：
  --test integration_mysql_matrix -- --ignored           → 11 passed（MY3 断言同步后）
  --test integration_mysql -- --ignored                  → 25 passed（凭据重建见 §5）
Oracle/SqlServer/Snowflake/ClickHouse/DuckDB/DB2/Informix/SapHana/Firebird：
  本机无实例，横向单测形态断言覆盖（12 方言全量逐字节）
```

**坑位登记（MySQL 凭据）**：`D:\tools\gate.env` 的 `SZ_ORM_MYSQL_URL` 缺密码段（`mysql://root@127.0.0.1:8802/...`，2026-11-12 重建时遗漏），真连报 1045 `using password: NO`；实测凭据经宝塔安装日志 `D:/BtSoft/panel/logs/install/b59c8640d3d6b94dba44a53f75f9c07c.log:492`（"MySQL 安装完成，root 密码为 <password>"——按凭据不入库铁律不在本报告回显，AGENTS.md 指引路径）找回并跑通全套。**gate.env 待修正**（属本机环境文件不入仓库，留档于此；P1-1 处置：3c2c18ae 首次入库时曾误将密码明文写入本行，已随回执提交清除，历史提交经 filter 分支/说明登记处理——凭据已在轮换清单）。

## 4. 门禁输出（逐项 EXIT）

```
bash scripts/check-fmt-all.sh                    → 格式检查通过（72 包）
cargo check --workspace --all-targets            → 0 error
cargo clippy --workspace --all-targets -- -D warnings → 0 error
cargo test -p sz-orm-core --lib                  → 3285 passed
python -X utf8 -I scripts/check-sql-injection-auto.py --strict → Gate 9 PASSED（46 Safe）
python -X utf8 -I scripts/check-phantom-delivery.py            → 门禁 15 通过
python -X utf8 -I scripts/check-phantom-tests.py               → EXIT=0
python -X utf8 -I scripts/check-dialect-ddl-quoting.py --self-test → 15 passed / 0 failed
python -X utf8 -I scripts/check-dialect-ddl-quoting.py（全仓）      → ✅ 0 命中
```

## 5. 越批登记（建议，非缺陷）

1. **json_query.rs**（think-orm 风格 `JsonQuery`）：`build_extract` 对 PG/MySQL 形态**已有手写双引号/反引号 quote**（`json_query.rs:84-97`），SQLite 分支反引号（SQLite 亦接受，非错误），但**无标识符校验**（`validate_identifier` 未接线）——建议下批统一接线方言 quote + 校验；本批不动（独立 API 面，hunk 隔离纪律）。
2. **G37 覆盖面**：本批后 Q2 敏感模板含 JSON 函数面；`typed_ast.rs` JSON 表达式（已 quote）不在 TARGET 文件（仅扫 dialect.rs）——保持现状合理。
3. **SqlServer 方括号 quote 的 `json_extract` 形态**：`JSON_VALUE([MetaCol], ...)` 真库行为未验（本机无实例）——CI MSSQL service 可达时补验。

## 6. audit-verify 自证

```
bash scripts/audit-verify.sh docs/assessment/2026-11-13-json-extract-quote-sweep.md
（结果见实施批提交附言；引用均含完整 file:line 路径）
```
