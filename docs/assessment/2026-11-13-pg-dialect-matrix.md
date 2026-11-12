# PostgreSQL 方言矩阵真连验证（PG1~PG7）——2026-11-13

> 六环闭环批次。模式沿用 MySQL（MY）/Oracle（OR）/SQLite（SQ）方言矩阵；被测引擎为 **PostgreSQL 18.4 @ 127.0.0.1:5432/sz_orm_test**（本机宿主实例，`SELECT version()` 实测 "PostgreSQL 18.4 on x86_64-windows, compiled by msvc-19.44.35226, 64-bit"；Docker 内 pg-diff 容器占 25432，本批未混用）。DSN 经 `SZ_ORM_PG_URL` 环境变量注入，报告一律引用变量名。
>
> **背景**：PG18 此前在 SzRSQL 差分中担任仲裁基准（载荷间接执行过），但那批按 szrsql 探针设计——本批按 MY/OR/SQ 模板补齐 **sz-orm `PostgreSqlDialect` 自身生成面**的独立矩阵。PG 是 **Szrsql/Kingbase/PolarDB/GaussDB/CockroachDB/YugabyteDB/Redshift 七方言委托族**的委托基准（`packages/sz-orm-core/src/dialect.rs:2149-2180` `delegate_dialect_to!` 链），本批验证惠及全部下游委托方言。
>
> **本批特征**：每载荷 = 方言层生成（`packages/sz-orm-core/src/dialect.rs:814` 起 `impl Dialect for PostgreSqlDialect`）+ 真库执行 + 行为断言三步；20 测试全部 `#[ignore]` + env 注入跳过模式。

## 1. 交付物

| 项 | 内容 | 证据 |
|---|------|------|
| 测试 | `packages/sz-orm-core/tests/integration_pg_matrix.rs`（PG1~PG7 共 20 测试，1265 行） | `cargo test -p sz-orm-core --features type-safe-columns --test integration_pg_matrix -- --ignored --test-threads=1` → **20 passed; 0 failed**（2026-11-13 实测，EXIT=0） |
| 方言缺口修复 | **0 处**（见 §3 三分类——PG 生成面经 12 方言 FTS quote 批 + ALTER INDEX/FK quote 批二 + G1/G2/G4 同型修复后无裸拼残留；G37 门禁 0 命中佐证） | `python scripts/check-dialect-ddl-quoting.py`（G37）→ self-test 10/10 + 全仓 0 命中 |
| 测试基建 | `packages/sz-orm-core/Cargo.toml` dev-dependencies 新增 `bigdecimal = "0.4"` + sqlx `uuid`/`bigdecimal` feature（NUMERIC 38 位/UUID 绑定通道，与 sz-orm-sqlx `bind_pg_value` R2 口径一致）；`[[test]] integration_pg_matrix` 登记 + `required-features = ["type-safe-columns"]`（where_expr 通道需要） | `packages/sz-orm-core/Cargo.toml:324-326,362-364`；`cargo check --workspace --all-targets` EXIT=0 |
| 登记 | CHANGELOG.md `[Unreleased]` 新增条目 + AGENTS.md「已发布」行追加 | 本报告 §6 |

## 2. 矩阵结果逐载荷

### PG1 标识符（quoting / 大小写保留 / 63 字节上限 / 三臂全 quote）
- 方言形态：双引号 `"` + 内嵌引号 doubling（`packages/sz-orm-core/src/dialect.rs:823-832` quote/quote_into）。
- **大小写保留语义**（与 Oracle 折叠大写相反）：`build_create_table` 对混合大小写表名/列名全 quote（断言生成 SQL 含 `"titleText"`）；裸混合大小写表名真库报 `relation does not exist`（折叠为小写）——quote 保留语义对照成立，行为级证明。
- 63 字节上限：63 字节表名真库 `CREATE TABLE` 成功；64 字节被 `quote_checked`（`packages/sz-orm-core/src/dialect.rs:94-101`，`max_identifier_len` PG 默认 63 `dialect.rs:109`）拦截——MY 批 G4 方言感知上限的 PG 侧对称验证。
- G2 同型回归：`build_alter_table` 三臂（AddIndex 索引名+列名 / DropIndex / AddForeignKey 约束名 `fk_{table}_{cols}` 组装后 quote）混合大小写真库往返全部成功（`packages/sz-orm-core/src/dialect.rs:948-1049`）；FK 行为级：无父行插入被真库拒绝（23503）。
- `pg_indexes` 往返：AddIndex 建出索引 1 行命中，DropIndex 后 0 行。

### PG2 主键回填与 RETURNING
- `supports_returning()` = true（`packages/sz-orm-core/src/dialect.rs:848-850`）。
- `build_insert_with_params`（`packages/sz-orm-core/src/query.rs:3139`，单条 insert `?` 占位符）+ 尾接 `RETURNING id` → BIGSERIAL 主键回填成功（id > 0）。
- `build_update_with_params`（`query.rs:3311`，SET 走 `?`）+ `RETURNING "value"` → 新值 20 回填。
- `build_force_delete_with_params`（`query.rs:3384`）+ `RETURNING id` → 被删主键回填。
- RETURNING 全家（INSERT/UPDATE/DELETE）在 sqlx Pg 通道列映射（`Row::try_get(0)`）全过。

### PG3 Upsert（ON CONFLICT 生成面）
- `build_batch_upsert_with_params`（`packages/sz-orm-core/src/query.rs:3271`）→ `build_upsert_on_conflict`（`packages/sz-orm-core/src/dialect.rs:1051-1096`）：批量 INSERT 部分对 PG 直接生成 `$n` 原生占位符（`query.rs:3233-3237` is_pg 分支）+ `ON CONFLICT ("id") DO UPDATE SET "value"=EXCLUDED."value"`。
- 行为级：两批 upsert 后 3 行；冲突行 name（不在 update_columns）保持旧值 `"a"`、value 更新为 `EXCLUDED` 新值 111；新行正常插入——`excluded.` 引用语义真库证明。
- 冲突目标缺失形态：`insert_or_ignore()`（`query.rs:678`）→ 尾接 `ON CONFLICT DO NOTHING`（`packages/sz-orm-core/src/dialect.rs:1099-1101` `build_insert_or_ignore_suffix`），依赖约束推断，真库冲突静默忽略、原值保留。
- 对照既有 22 项 integration_pg 的 upsert 套件（单行/批量/混合/指定列/NULL/Unicode 六场景）不回归。

### PG4 类型与 JSONB
- `::` cast：`json_extract`（`packages/sz-orm-core/src/dialect.rs:863-887`）生成 `"meta"#>>'{a,b}'`，真库 `#>>` 路径提取命中 `"ok"`；`::jsonb`/`::text`/`::numeric` cast 全部真库可用。
- **NUMERIC 38 位无损**：`Value::Decimal("12345678901234567890123456789012345678")` 经 BigDecimal 绑定（R2 口径）参数化插入 NUMERIC(38,0) 列后 `amt::text` 读回逐字符相等——**行为级发现**：测试初版以 `Value::Decimal` 按 TEXT 兜底绑定被真库拒绝（`column "amt" is of type numeric but expression is of type text`），证实 `bind_pg_value`（`packages/sz-orm-sqlx/src/any.rs:2050-2053`）的 BigDecimal 通道是**必要的**而非仅优化；本测试基建同口径复刻。
- JSONB：`Value::Json` 经 serde_json 绑定 jsonb 列往返成功（复刻 `packages/sz-orm-sqlx/src/any.rs:2073-2075` Json 变体通道）；`@>` 包含操作符真库命中 1 行。
- 数组：TEXT[] 经 sqlx Pg 通道 `Vec<String>` 绑定往返无损（`[x, y]`）。**登记**：sz-orm `Value::Array` 变体在 `bind_pg_value` 走 `other => to_string()` TEXT 兜底（`any.rs:2076-2077`），未走 PG 原生数组绑定——`Value::Array` 参数到 PG `INT[]`/`TEXT[]` 列需调用方自行展开或用原生 sqlx；登记为已知边界（sz-orm Value 类型系统未含数组类型语义）。

### PG5 索引与约束
- `supports_if_not_exists()` = true（`packages/sz-orm-core/src/dialect.rs:977-979`）；`CREATE INDEX IF NOT EXISTS` 两次执行幂等（第二次不报错）。
- **CONCURRENTLY 事务冲突探针**：事务外 `CREATE INDEX CONCURRENTLY` 成功；事务内同形态被真库拒绝（25P03 类，`CREATE INDEX CONCURRENTLY cannot run inside a transaction block`）——迁移/schema_sync 场景登记依据：**方言不生成 CONCURRENTLY（契约成立），调用方需在事务外自行执行**；sz-orm `build_alter_table` AddIndex 产物为普通 CREATE INDEX，事务内安全。
- 部分索引（WHERE）/表达式索引（lower(col)）/GIN(JSONB) 三类透传 SQL 真库全部建成（pg_indexes 3 行命中）。**登记**：sz-orm 方言未封装此三类索引（`TableChange::AddIndex` 仅普通列索引），透传通道可行。

### PG6 查询面
- **CTE builder（MY10 B4 的 PG 对称）**：`with_cte`（`packages/sz-orm-core/src/query.rs:371`）+ EXISTS 半连接（经 `where_expr` 原始布尔片段桥接）→ `WITH "kids" AS (SELECT ...) SELECT count(*) ...` 真库命中 2 行（value>1 过滤）；**参数顺序证明**：`params == [Value::I64(1)]` 仅 CTE 参数（`render_cte_prefix` 参数化路径 `query.rs:421-450`）。`with_recursive_cte`（`query.rs:385`）→ `WITH RECURSIVE "seq" AS (锚 UNION ALL 递归)` 计数 1..=5 = 5 行；`recursive_cte_keyword` PG 默认 `"RECURSIVE "`（`dialect.rs:121-123`）。
- DISTINCT ON / ILIKE / NULLS LAST / FETCH FIRST n ROWS ONLY / 窗口函数（ROW_NUMBER OVER PARTITION BY）：全部经 `select_expr` 透传通道真库可用——**方言透传完整性**成立（方言不注入不拦截，PG 特有语法经逃生口全通过）。
- 分页：`build_pagination_clause` 默认 `LIMIT n OFFSET m`（`packages/sz-orm-core/src/dialect.rs:367-384`），`LIMIT 2 OFFSET 1` 真库行为正确（第 2、3 行）。
- **方言无 DISTINCT ON 封装**（登记）：PG 特有 `DISTINCT ON` 需经 `select_expr` 透传，非缺口（MySQL 无对应物，方言共性接口不应含 PG 专有语法）。

### PG7 边缘与并发
- **三值 NULL**：`where_eq(col, Value::Null)` → `IS NULL`（MY 批 G1 同型，`packages/sz-orm-core/src/query.rs:1850-1873` `render_eq_fragment`）真库命中 2 行；`where_ne(col, Null)` → `IS NOT NULL` 命中 2 行；空串 `''` 可精确参数化匹配（命中 1 行）且与 NULL 区分——PG `'' ≠ NULL` 语义（对照 Oracle 空串即 NULL）行为级证明。
- 除零：`SELECT 1/0` 真库报错（22012 division_by_zero）；**aborted 事务语义**：事务内错误后不回滚直接再发语句 → 25P02（current transaction is aborted）——调用方须 ROLLBACK，sz-orm 事务 panic 安全回滚（v9.3.0）与该语义对齐。
- **FOR UPDATE**：`lock_for_update()`（`query.rs:616`）+ `build_lock_clause`（`packages/sz-orm-core/src/dialect.rs:1091-1094` 生成 `FOR UPDATE`）真库锁行成功；`FOR UPDATE SKIP LOCKED` 透传 SQL：tx1 锁行后 tx2 带 SKIP LOCKED 立即返回 0 行（行为级）。**登记**：sz-orm `LockType` 仅 ForUpdate/Shared 两枚举，未封装 SKIP LOCKED/NOWAIT 变体（sz-pay 队列场景如需可经透传 SQL 或后续扩展 LockType）。
- **LISTEN/NOTIFY 行为级**：`sqlx PgListener` LISTEN + `NOTIFY chan, 'payload'` → `recv()` 收到 channel/payload 全匹配（szrsql 曾缺该能力，PG 真库必须过——成立）。sz-orm 通道：`PgExtensions::listen/notify`（`packages/sz-orm-sqlx/src/any.rs:2699-2713`，接收侧文档注明需配合 `PgListener`，`any.rs:2627`）。
- **全文检索**：`full_text_search`（`packages/sz-orm-core/src/dialect.rs:889-898`）生成 `"titleText"::text || ' ' || ... 形态 + to_tsvector(...) @@ to_tsquery('fox')`，混合大小写列 quote 往返真库命中 1 行；`to_tsquery` 操作符透传（`'quick & fox'`）真库生效。`full_text_search_mode`（`dialect.rs:147-152` 默认委托）三模式同形（模式语义仅 MySQL）——单元断言 + `pg7_fts_mode_default_delegation` 真库通道双证。

## 3. 三分类清单

### A. sz-orm 方言缺口（修复）——0 处
PG 生成面在本矩阵 20 载荷下**零缺口**：
- quote 类（G-OR1/MY G2/G-OR9/FTS 批/批二五批横向修复覆盖）：`quote`/`quote_into`/`build_alter_table` 三臂/`full_text_search` 列名全部已 quote，真库往返通过；
- 三值 NULL 渲染（G1）：`render_eq_fragment` IS NULL/IS NOT NULL 正确；
- 标识符上限（G4）：`max_identifier_len` PG 63 正确拦截；
- G37 门禁（dialect DDL/FTS 裸标识符检测）全仓 0 命中佐证。
**结论**：PG 作为最严格基准方言（63 上限 + 大小写保留 + 三值 NULL + 空串语义），此前五批横向修复已系统性覆盖其生成面。

### B. PG 环境特有（登记）——4 项
1. **CONCURRENTLY 不可在事务内**（PG5-b）：PG 真库限制；sz-orm 方言契约（不生成 CONCURRENTLY）成立，schema_sync/migration 调用方如需 CONCURRENTLY 须自行在事务外执行。
2. **Value::Array → PG 原生数组**（PG4-c）：`bind_pg_value` 对 `Value::Array` 走 TEXT 兜底（`packages/sz-orm-sqlx/src/any.rs:2076-2077`），PG 原生数组绑定需原生 sqlx `Vec<T>` 通道；sz-orm Value 类型系统无数组语义，登记为已知边界。
3. **SKIP LOCKED / NOWAIT 未封装**（PG7-c）：`LockType` 仅两枚举（`packages/sz-orm-core/src/dialect.rs:399-404`）；队列场景（sz-pay）可经透传 SQL，行为级已验证可行。
4. **DISTINCT ON 未封装**（PG6-b）：PG 专有语法，经 `select_expr` 透传；方言共性接口不应含单方言专有语法，非缺口。

### C. 文档措辞——1 项
1. `PgExtensions::listen/notify` 文档（`packages/sz-orm-sqlx/src/any.rs:2627`）已如实注明「接收通知需配合 `PgListener` 或轮询」——符合硬约束 11 措辞纪律（提供组件、非自动拦截），本批 LISTEN/NOTIFY 行为级测试补齐了该通道的真库证据链。无修正需求。

## 4. 验证输出（逐项留痕）

```
1. cargo check --workspace --all-targets          → EXIT=0
   cargo clippy --workspace --all-targets -- -D warnings → EXIT=0
   bash scripts/check-fmt-all.sh                  → 格式检查通过（72 包）
2. cargo test -p sz-orm-core --features type-safe-columns \
      --test integration_pg_matrix -- --ignored --test-threads=1
   → test result: ok. 20 passed; 0 failed（EXIT=0）
3. cargo test -p sz-orm-core --test integration_pg -- --ignored
   → test result: ok. 22 passed; 0 failed（零回归）
   cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored
   → test result: ok. 15 passed; 0 failed（委托族零回归，szrsql 6543 trust 实例）
   cargo test -p sz-orm-sqlx --test integration_szrsql -- --ignored
   → test result: ok. 8 passed; 0 failed
4. python scripts/check-sql-injection-auto.py --strict
   → Total: 46 | Safe: 46 | Unsafe: 0 → Gate 9 PASSED
   python scripts/test_check_sql_injection_auto.py → 15 passed（自测并入 G9）
5. python scripts/check-dialect-ddl-quoting.py --self-test → 10 passed / 0 failed
   python scripts/check-phantom-delivery.py       → 门禁 15 通过
   python -X utf8 scripts/check-semantic-patterns.py → 门禁 16 通过（软规则 1 提示，存量）
   python scripts/check-doc-consistency.py        → PASS
   python scripts/check-examples-readme.py        → 38 bin 全登记
   python scripts/check-doc-version-consistency.py → 通过
   python scripts/check-changelog-consistency.py  → 通过
   python scripts/check-doc-links.py              → 通过（0 死链）
   python scripts/check-security-doc-version.py   → PASS
```

## 5. 委托族影响

本批**零方言改动**（纯新增验证批），`delegate_dialect_to!` 委托链（Szrsql/Kingbase/PolarDB/GaussDB/CockroachDB/YugabyteDB/Redshift）无接口变化；`integration_szrsql_matrix` 15 项 + `integration_szrsql` 8 项真连回归全绿（szrsql 6543 trust 实例）作为委托族回归证据。

## 6. 环境与坑位

- PG 密码经 `SZ_ORM_PG_URL` 注入；报告中一律引用变量名（AGENTS.md 明文属历史遗留）。
- 宿主 PG18 在 5432；Docker pg-diff 容器在 25432——本批全程 5432。
- szrsql 6543 委托族回归用既有 release 二进制（`D:/cargo-target/release/szrsql.exe`）启动；szrsql 源码仓 HEAD 编译失败（`check_option`/`view_where` 字段缺失，上游进行中改动），与本批无关，未触碰（ADR-0001）。
- sqlx `query_as` 元组解构对绑定后查询不适用（PgRow），统一 `fetch_one` + `Row::try_get(0)`。
- 测试基建需 sqlx `uuid`/`bigdecimal` feature（`Value::Decimal`/`Value::Uuid` 绑定通道），已在 dev-dependencies 补齐。
