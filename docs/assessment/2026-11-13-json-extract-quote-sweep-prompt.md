# 新会话提示词：sz-orm json_extract 列名 quote 横向修复批（v1，2026-11-13）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：sz-orm v10.13.4。quote 类缺口已六批真连发现（G-OR1/MY G2/G-OR9/FTS 批/批二/PG 矩阵红队第六形态）。PG 批红队与 SQLite 批红队均裁定：**`json_extract` 的 column 参数全方言裸拼为既有共同约定**，登记为下批候选——本批即该下批。
> **定位**：`Dialect::json_extract`（trait 方法 `packages/sz-orm-core/src/dialect.rs:138`）12 个直接实现中仅 `PostgreSqlDialect`（`:863`，PG 矩阵批 624974c9 已修）走 `self.quote(column)`；其余 11 个实现（MySQL `:516`/SQLite `:1162`/Oracle `:1535`/SqlServer `:1800`/Snowflake `:2266`/ClickHouse `:2555`/DuckDB `:2814`/DB2 `:3024`/Informix `:3379`/SapHana `:3617`/Firebird `:3867`）全部裸拼 column——混合大小写列在折叠语义方言（Oracle/MSSQL/DB2/Informix/Firebird/Snowflake 等）真库错位，在 PG 族引用列报 column does not exist。同时 **G37 Q2 敏感模板清单未含 JSON 函数**（`JSON_EXTRACT(`/`json_extract(`/`JSON_VALUE(`/`JSONExtract(`/`#>>`/`->`），属静态门禁盲区（PG 批红队 §3.3 实证预期内漏检）。
> **六环闭环**：按评测交付闭环链条执行。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：对全部方言的 `Dialect::json_extract` 做列名 quote 横向一致化修复（G-OR1 家族第七次同型收尾），并把 JSON 函数面纳入 G37 静态门禁，产出报告并完成六环登记。

**目标仓库**：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4）

## 现状基线（实施前已核实，直接采用）

- 12 个 `json_extract` 实现行号（`packages/sz-orm-core/src/dialect.rs`）：MySQL 516 / PG 863（已修）/ SQLite 1162 / Oracle 1535 / SqlServer 1800 / delegate 宏 2064（转发，随目标方言之修复自动惠及）/ Snowflake 2266 / ClickHouse 2555 / DuckDB 2814 / DB2 3024 / Informix 3379 / SapHana 3617 / Firebird 3867
- 生产消费点：`typed_ast.rs` JsonGet/JsonGetText/JsonPathGet/JsonPathGetText（`typed-dsl` feature，**已各自 quote**——`col = dialect.quote(C::NAME)`，不在本批范围）；`json_query.rs` JsonQuery（think-orm 风格，**自带手写 quote/裸拼混合**——本批范围仅 `Dialect::json_extract` trait 面，json_query.rs 另行评估，如同步修复须单独 hunk 并在报告注明）；外部无直接 `.json_extract(` 生产调用（any_driver.rs 仅测试触碰）
- G37：`scripts/check-dialect-ddl-quoting.py` `SENSITIVE_PATTERNS`（`:41-55`）12 关键词，无 JSON 函数面

## 评测矩阵（每方言 = 形态确认 + 真库/单测断言两步）

| # | 方言 | json_extract 生成形态 | 修复形态 | 真连可行性 |
|---|------|----------------------|---------|-----------|
| J1 | MySQL | `JSON_EXTRACT(col, '$.p')` | `JSON_EXTRACT(`col`, ...)` | ✅ 8802 宝塔 MySQL |
| J2 | SQLite | `json_extract(col, '$.p')` | `json_extract("col", ...)` | ✅ 内存库免 env |
| J3 | Oracle | `JSON_VALUE(col, '$.p')` | `JSON_VALUE("col", ...)` | ✅ 1521/freepdb1（可选） |
| J4 | SqlServer | `JSON_VALUE(col, '$.p')` | `JSON_VALUE([col]?, ...)`——**先查该方言 quote() 形态（方括号）** | ❌ 本机无实例，单测形态断言 |
| J5 | Snowflake | `col:path` | `"col":path` | ❌ 无实例，单测 |
| J6 | ClickHouse | `JSONExtractString(col, 'p')` | `JSONExtractString("col", ...)`——**注意 CH 反引号 quote()，验证生成串可解析** | ❌ 无实例（122 隧道可选） |
| J7 | DuckDB | `col -> 'p'` | `"col" -> 'p'` | ❌ 无实例，单测（duckdb crate bundled 可选真连） |
| J8 | DB2 | `JSON_VALUE(col, '$.p')` | `JSON_VALUE("col", ...)` | ❌ 容器可选，单测 |
| J9 | Informix/SapHana/Firebird | `JSON_VALUE(col, ...)` / `JSON_GET(col, ...)` | 同型 quote | ❌ 无实例，单测 |
| J10 | 委托族 | delegate 宏转发 | 零改动，随目标方言 | — |

## 硬性要求

- **逐方言确认 `quote()` 形态后再套**：PG/SQLite/DuckDB/DB2 双引号、MySQL/ClickHouse 反引号、SqlServer 方括号、Oracle/Snowflake 双引号——修复必须用 `self.quote(column)` 而非硬编码引号（方言感知）
- **单测**：新增横向单测 `test_json_extract_column_quoting_all_dialects`（12 方言混合大小写列 `MetaCol` 逐方言断言 quote 形态；与既有 `test_fts_column_quoting_all_dialects`、`test_alter_index_fk_quoting_all_dialects` 同型同落点——`packages/sz-orm-core/src/dialect.rs` tests 模块或 dialect 横向测试文件，找既有两测试落点跟随）
- **G37 增补**：`SENSITIVE_PATTERNS` 追加 JSON 函数面关键词（`JSON_EXTRACT\s*\(`、`json_extract\s*\(`、`JSON_VALUE\s*\(`、`JSONExtract`、`JSON_GET\s*\(`）+ 自测用例（修复后形态必不命中 + 历史裸拼形态必命中）；**增补后全仓扫描可能命中 json_query.rs 与 typed_ast 之外的既有调用——逐条处置（quote 或 quote-gate-safe 豁免登记），不得静默豁免**
- **真连回归**（本机可用面）：PG 18@5432（`SZ_ORM_PG_URL`）跑 integration_pg_matrix 20 项 + integration_pg 22 项；SQLite 内存库跑 integration_sqlite_matrix + integration_sqlite；MySQL 8802（`SZ_ORM_MYSQL_URL`，凭据经 `D:\tools\gate.env`）跑 integration_mysql_matrix + integration_mysql；Oracle/其它无实例方言以单测形态断言覆盖
- 参数化铁律（门禁 9）；零 unsafe、零 todo!；`cargo fmt` 用 `bash scripts/check-fmt-all.sh`（**注意：不要跑 `cargo fmt -p sz-orm-core`——会格式化并行会话未提交文件，只跑 check-fmt-all.sh 全仓检查，修复仅限本批文件**）
- Python 门禁脚本一律 `python -X utf8 -I`
- git 在 `D:\Git`；串行 cargo；提交纪律（硬约束 13）——本批文件单独提交，不打包并行会话改动

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / `cargo clippy --workspace --all-targets -- -D warnings` / fmt 72 包全绿
2. `cargo test -p sz-orm-core --lib`：全过（含新增横向单测）
3. 真连回归：PG matrix 20 + integration_pg 22 + SQLite matrix + sqlite 25 + MySQL matrix + mysql 25 全绿（env 注入模式）
4. G37：`--self-test` 增补后全过 + 全仓扫描 0 命中（豁免登记除外）
5. 报告 `docs/assessment/2026-11-13-json-extract-quote-sweep.md`，audit-verify 0 失败
6. 六环登记（CHANGELOG [Unreleased] / AGENTS.md 已发布行 / commit）

## 环境坑位速查

- 本机 DB 凭据 `source /d/tools/gate.env`；PG 密码在 AGENTS.md 属历史遗留，报告引用变量名
- szrsql 6543 如需回归：`"D:/cargo-target/release/szrsql.exe" --port 6543 --auth-mode trust --data-dir <dir>` 后台起（源码仓 HEAD 编译坏，勿 `cargo run`）
- Oracle 真连：`export PATH="/d/oracle/product/26ai/dbhomeFree/bin:$PATH"` + `SZ_ORM_ORACLE_PASSWORD`（gate.env）——可选，非本批硬验收
- 临时脚本放 `D:\tmp`；`python -I` 运行
