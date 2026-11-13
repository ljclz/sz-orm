# SQLite 方言矩阵真连验证（SQ1~SQ7）——2026-11-13

> 六环闭环批次。模式沿用 MySQL（MY）/Oracle（OR）方言矩阵；被测引擎为 **rusqlite/sqlx 捆绑 libsqlite3-sys 0.30.1 = SQLite 3.46.0**（实测 `rusqlite::version()` = 3.46.0；Cargo.lock `libsqlite3-sys 0.30.1`）。CLI 对拍引擎为 python 3.32.3 系统库（无 STRICT/RETURNING/IS NOT DISTINCT FROM/`->>`，仅作跨引擎对照，D-3 双引擎纪律）。
>
> **本批特征**：内存库免 env、无条件必跑（与 szrsql matrix 的 env 跳过模式不同）；每载荷 = 方言层生成（`packages/sz-orm-core/src/dialect.rs` SQLiteDialect）+ 真库执行 + 行为断言三步。

## 1. 交付物

| 项 | 内容 | 证据 |
|---|------|------|
| 测试 | `packages/sz-orm-core/tests/integration_sqlite_matrix.rs`（SQ0 引擎版本契约 + SQ1~SQ7 + DuckDB 同型回归，共 10 测试） | `cargo test -p sz-orm-core --test integration_sqlite_matrix` → **10 passed; 0 failed**（2026-11-13 实测） |
| 方言缺口修复 | **0 处**（见 §3 三分类——方言生成面经 12 方言 FTS quote 批 + ALTER INDEX/FK quote 批二后无裸拼残留；G37 门禁 0 命中佐证） | `python scripts/check-dialect-ddl-quoting.py` → 0 命中；`packages/sz-orm-core/src/dialect.rs`（SQLiteDialect 实现段） |
| 登记 | 引擎编译配置事实 1 项（FK 默认 ON，见 SQ5） | `D:/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/libsqlite3-sys-0.30.1/build.rs`（`-DSQLITE_DEFAULT_FOREIGN_KEYS=1` 旗标行） |

## 2. 矩阵结果逐载荷

### SQ0 引擎版本契约
`sqlite_version()` = **3.46.0**（bundled）。矩阵断言基准成立。

### SQ1 引擎版本契约（STRICT / RETURNING / IS NOT DISTINCT FROM）
- 方言能力声明：`supports_returning()` = true（`packages/sz-orm-core/src/dialect.rs:1149`），与 3.35+ 引擎事实一致——方言**有意暴露** RETURNING；`last_insert_id_sql()` = `Some("last_insert_rowid()")`（`packages/sz-orm-core/src/dialect.rs:1220`）作备选通道，双通道并存。
- STRICT 表（3.37+）：方言 `build_create_table` 产物（quote+类型）+ `) STRICT` 后缀真库可执行；STRICT 拒绝 INTEGER 列存 TEXT（行为级）。
- RETURNING 回填：`INSERT..DEFAULT VALUES RETURNING id` 真库回填成功。**rusqlite 调用方契约**（登记）：RETURNING 语句必须 `prepare`+`query_row`，`execute` 报 `ExecuteReturnedResults`。
- IS NOT DISTINCT FROM（3.39+）：`WHERE id IS NOT DISTINCT FROM 1` 真库命中 1 行。
- python 3.32 CLI 对照：STRICT/RETURNING/INDF 全部 `syntax error`（版本面偏旧佐证，非方言缺口）。

### SQ2 标识符三种 quoting
- 方言形态：双引号 `"id"`，内嵌引号 doubling（`packages/sz-orm-core/src/dialect.rs:1124-1139` quote/quote_into）。
- 真库行为：SQLite 三种 quoting（双引号/方括号/反引号）都收（**登记事实**，非方言缺口）。
- 保留字表/列名（`order`/`select`）+ Unicode 列名经方言 quote 往返可执行。
- G2 同型回归：`AddIndex` 索引名+列名均 quote（批二已修，`packages/sz-orm-core/src/dialect.rs:1286-1300`）——真库 `CREATE INDEX` 成功。

### SQ3 Upsert 家族
- `build_upsert_on_conflict` 空 update_columns 路径 = `ON CONFLICT ("k") DO UPDATE SET "v"=EXCLUDED."v"`（**非 DO NOTHING**——更新全部非冲突列，真库行为断言 v 42 生效）；冲突列覆盖全部列时退化 `DO NOTHING`。
- **边界证据**：`ON CONFLICT (k, v)` 不匹配单列 PK → 真库报 "ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint"（复合唯一约束表验证通过）。
- `build_insert_or_ignore_prefix` = `INSERT OR IGNORE INTO "sq3"`（`packages/sz-orm-core/src/dialect.rs:1366`）；冲突保留原值。
- OR REPLACE「删后插」探针：AUTOINCREMENT id 前后对比 `first_id → first_id+1`，rowid 变化（副作用证据成立）。
- **混用禁区修正**：`INSERT OR REPLACE ... ON CONFLICT (k) DO NOTHING` 混用形态 **3.46 接受**（python 3.32 亦接受，跨引擎一致）——原假设「被拒绝」不成立，登记为引擎事实，调用方不应依赖该形态。

### SQ4 主键回填与 rowid
- `auto_increment=true` → `PRIMARY KEY AUTOINCREMENT`（`packages/sz-orm-core/src/dialect.rs:1236`）；删最大行后新 id 严格大于已删 max（防复用，行为级）。
- **WITHOUT ROWID 失效探针成立**：`WITHOUT ROWID` 表插入后 `last_insert_rowid()` 不推进（= 基线）；RETURNING 在 WITHOUT ROWID 表仍可用 → **主键回填必须走 RETURNING**。
- **登记缺口（文档措辞级）**：`last_insert_id_sql()` 恒返回 `Some("last_insert_rowid()")`，对 WITHOUT ROWID 表语义为「上一条 rowid 表插入」——调用方在 WITHOUT ROWID 表场景必须改用 RETURNING（方言层不感知表形态，属引擎约束，不改码）。

### SQ5 PRAGMA 面
- **重要引擎编译配置事实（登记）**：libsqlite3-sys 0.30.1 bundled 编译带 `-DSQLITE_DEFAULT_FOREIGN_KEYS=1`（`D:/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/libsqlite3-sys-0.30.1/build.rs` 旗标行（原 123 行）），**bundled 引擎默认 foreign_keys=ON**，与 SQLite 官方上游默认 OFF 不同。行为级证据：孤儿外键插入默认被拒；显式 `PRAGMA foreign_keys=OFF` 后被放过；重新 ON 后再拒。
- **风险探针结论**：sz-pay 类业务的「FK 静默丢约束」风险在 **bundled 构建天然规避**；但**非 bundled 链接系统库**（python 3.32 CLI 探针默认 OFF、部分发行版 so）仍默认 OFF——换非 bundled 构建会静默丢 FK 约束，需显式 `PRAGMA foreign_keys=ON`。sz-orm 连接层未统一开启（**登记裁决项**，保持现状：bundled 默认 ON 已覆盖，强制开启反而会在非 bundled 场景改变语义，交由调用方按构建形态决策）。
- rusqlite 契约（登记）：`PRAGMA busy_timeout=5000` 等返回行的语句用 `execute_batch` 设置 / `query_row` 读取；`PRAGMA foreign_keys` 无值查询返回值不可靠，判定走行为级。
- 内存库 `journal_mode` 恒 MEMORY（WAL 仅文件库有效——SQ7 文件库探针断言 WAL 成功）。
- `pragma_table_info` 反射方言建表产物：2 列一致。

### SQ6 类型亲和性
- INTEGER 列存 TEXT 真库不报错（弱类型存储，SQLite 特有）；Value 往返按声明类型读回。
- `json_extract` 生成面（`packages/sz-orm-core/src/dialect.rs:1162-1174`）= `json_extract(j, '$.k')`，真库执行命中 1；`->>`（3.38+）真库可用。
- REAL f64 往返无损（0.1）；除零返回 NULL 不报错（方言 `concat` 的 COALESCE 链依赖此语义，`packages/sz-orm-core/src/dialect.rs:1195-1206`）。

### SQ7 边缘与并发
- 空串 ≠ NULL（`s=''` 命中 1 行、`IS NULL` 命中 1 行）。
- FTS 列名 quote（G-OR9 同型第 12+1 方言扫描位）：方言 `full_text_search` LIKE 降级路径列名已 quote（FTS 横向批已修，`packages/sz-orm-core/src/dialect.rs:1176`）——混合大小写列 `titleText` 真库命中 1 行。
- FTS5 虚拟表（bundled 3.46 带 SQLITE_ENABLE_FTS5，`build.rs SQLITE_ENABLE_FTS5 旗标`）：`sq7_f5 MATCH 'rust'` 命中——方言降级理由（MATCH 作用于表本身而非单列）行为级佐证。
- **并发写 SQLITE_BUSY**：WAL 文件库双连接，w1 `BEGIN IMMEDIATE` 持锁 + w2 `busy_timeout=50ms` 写 → `ErrorCode::DatabaseBusy`（错误码映射到 rusqlite 的行为断言）；`busy_timeout=10s` + w1 提交释放 → w2 等待路径成功（WAL 单写者串行化）。

### 方言缺口回归（DuckDB 同型）
`build_insert_or_ignore_prefix("t")` = `INSERT OR IGNORE INTO "t"`（共享语义先例回归）+ `integration_duckdb` 7/7 无回归。

## 3. 三分类清单

| # | 分类 | 内容 | 处置 |
|---|------|------|------|
| 1 | SQLite 引擎编译配置特有 | bundled 默认 `foreign_keys=ON`（`-DSQLITE_DEFAULT_FOREIGN_KEYS=1`，libsqlite3-sys-0.30.1/build.rs 旗标），上游默认 OFF | 登记；非 bundled 构建风险探针结论见 SQ5 |
| 2 | SQLite 引擎特有 | 三种标识符 quoting 都收；`OR REPLACE + ON CONFLICT` 混用被接受（跨引擎一致） | 登记；调用方不依赖混用形态 |
| 3 | SQLite 引擎特有 | WITHOUT ROWID 表 `last_insert_rowid()` 不推进；RETURNING 仍可用 | 登记；主键回填走 RETURNING（SQ4） |
| 4 | SQLite 引擎特有 | `ON CONFLICT (k,v)` 不匹配单列 PK 报错；除零→NULL；空串≠NULL；内存库 journal_mode 恒 MEMORY | 登记 |
| 5 | 文档措辞 | `last_insert_id_sql()` 对 WITHOUT ROWID 表语义为上一条 rowid 表插入——trait 文档应注明「仅 rowid 表有效」 | 登记（trait 契约已在 `packages/sz-orm-core/src/dialect.rs:167-176` 注明回填优先 RETURNING，补充语义由本报告承载） |
| 6 | sz-orm 方言缺口 | **0 处**——FTS LIKE 降级列名 quote（横向批）、AddIndex/DropIndex quote（批二）、concat COALESCE、ON CONFLICT/EXCLUDED 生成面均经真库验证无缺口 | 无需改码；G37 0 命中佐证 |

## 4. 门禁记录（2026-11-13 实测）

| 门禁 | 结果 |
|------|------|
| G1 fmt（`bash scripts/check-fmt-all.sh`） | ✅ 72 包通过（本批顺带修复 sz-orm-parallel/sz-orm-core 既有漂移） |
| G2 `cargo check --workspace --all-targets` | ✅ Finished（并行会话 `integration_pg_matrix.rs` 半成品缺 `required-features = ["type-safe-columns"]`，本批补齐 `packages/sz-orm-core/Cargo.toml:364`） |
| G3 `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 error（并行会话 tidb/mssql/mariadb 三处 `.err().expect()` clippy 违例，本批改为 match+panic 语义等价修复） |
| 矩阵 `packages/sz-orm-core/tests/integration_sqlite_matrix.rs` | ✅ 10 passed |
| 回归 `integration_sqlite` | ✅ 25 passed |
| 回归 `integration_sq_differential` | ✅ 8 passed（--include-ignored） |
| 回归 `integration_duckdb` | ✅ 7 passed |
| 回归 cabi `--lib` | ✅ 88 passed |
| G9 `check-sql-injection-auto.py --strict` | ✅ 46/46 Safe；自测 15/15 |
| G37 `check-dialect-ddl-quoting.py` | ✅ 0 命中；自测 10/10 |
| G12 doc-consistency / G14 doc-sync / G15 phantom-delivery / G16 semantic-patterns / G24 examples-readme / G33 doc-version / G35 doc-links / G36 changelog-consistency | ✅ 全过（G14 触发 Rule 10 由本批 AGENTS.md/CHANGELOG 登记满足） |

> 注：工作区含并行会话（PG 矩阵 PG1~PG7、容器化 MZ-*）未提交半成品，本批仅提交自身产物 + 为使 G2/G3 转绿所必需的最小并行修复（`required-features` 补齐 + 三处 clippy 形态修复 + fmt 漂移），hunk 级隔离见 commit 拆分。

## 5. 结论

SQLite 方言生成面在真库（bundled 3.46）下 **零方言缺口**；矩阵 10/10 全绿，既有 25+8+7 项零回归。登记引擎事实 4 类（§3 #1~#4），文档措辞 1 项（#5）。FIX 类登记入 CHANGELOG [Unreleased]：并行会话半成品的门禁转绿修复（required-features + clippy 形态）与本批 fmt 漂移修复。
