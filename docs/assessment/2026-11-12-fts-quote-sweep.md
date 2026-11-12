# sz-orm FTS 列名 quote 横向修复批报告（2026-11-12）

> **评测对象**：`dialect.rs` 全部 `full_text_search`/`full_text_search_mode` 实现的列名渲染一致化（G-OR9 同型缺口横向收尾）
> **语义基准**：各方言官方标识符引用语义 + 本机就绪服务真库实测
> **版本**：sz-orm v10.13.4（workspace.package.version）
> **提示词**：[2026-11-12-fts-quote-sweep-prompt.md](2026-11-12-fts-quote-sweep-prompt.md)（第 1 环已闭环，commit `58a71b7c`）
> **六环状态**：第 1 环提示词 ✅ / 第 2 环本报告 ✅（含红队回执处置）/ 第 3 环实施批 ✅ / 第 4 环复评 ✅ / 第 5 环红队 ✅（PASS，P2×1 处置）/ 第 6 环登记 ✅

---

## 1. 缺口全景与修复清单（12 方言）

**缺口类**：FTS 生成 SQL 中列名裸拼——混合大小写列（经 quote 创建、区分大小写存储）被真库折叠为大写与引号列不匹配（G-OR9 Oracle 实测 ORA-00904；各真库同型：PG 42703、MySQL 未知列、SQLServer 无效列名）。此为 G-OR1（AddIndex，a37937a8）→ MY G2（206f5623）→ G-OR9（0f7bf985）后第四次同型发现，本批一次性横向收尾。

| 方言 | 修复前生成（列名裸拼） | 修复后生成（quote 形态） | commit |
|------|----------------------|------------------------|--------|
| MySQL | ``MATCH(title, body) AGAINST`` | ``MATCH(`title`, `body`) AGAINST`` | `2564a43` |
| PostgreSQL | `to_tsvector(title::text)` | `to_tsvector("title"::text)` | `2564a43` |
| SQLite | `title LIKE '%kw%'` | `"title" LIKE '%kw%'` | `2564a43` |
| SQL Server | `CONTAINS(title, content, 'kw')` | `CONTAINS([title], [content], 'kw')` | `2564a43` |
| Snowflake | `title ILIKE '%kw%'` | `"title" ILIKE '%kw%'` | `2564a43` |
| ClickHouse | `position(title, 'kw') > 0` | ``position(`title`, 'kw') > 0`` | `2564a43` |
| DuckDB | `title LIKE '%kw%'` | `"title" LIKE '%kw%'` | `2564a43` |
| DB2 | `CONTAINS(title, 'kw') > 0` | `CONTAINS("title", 'kw') > 0` | `2564a43` |
| Informix | `CONTAINS(title, 'kw') > 0` | `CONTAINS("title", 'kw') > 0` | `2564a43` |
| SapHana | `CONTAINS(title, 'kw') > 0` | `CONTAINS("title", 'kw') > 0` | `2564a43` |
| Firebird | `CONTAINING(title, 'kw')` | `CONTAINING("title", 'kw')` | `2564a43` |
| Oracle | （G-OR9 已修复 `0f7bf985`） | `CONTAINS("title", 'kw', 1) > 0` | 核对不重改 |

- **实现位置**：[packages/sz-orm-core/src/dialect.rs](../../packages/sz-orm-core/src/dialect.rs) 各 `impl Dialect for XxxDialect` 的 `full_text_search`（MySQL 的 `full_text_search_mode` 与 `full_text_search` 共用列 join，一并 quote——NL 委托一致性依赖两方法同形）。
- **零破坏口径**：只改列名渲染（quote 包裹），关键字/转义/结构不变；每方言最小 diff。

## 2. 测试同步与新增

- **既有断言同步**（quote 形态）：[dialect_advanced_test.rs](../../packages/sz-orm-core/tests/dialect_advanced_test.rs) `test_full_text_search!` 宏 7 处 `$fragment` + [dialect.rs](../../packages/sz-orm-core/src/dialect.rs) tests 内 `test_sqlite_full_text_search`/`test_sqlserver_full_text_search`/`test_my9_full_text_search_modes` 三处（Lib 3283 全过实证无漏网断言）。
- **新增横向单测**：`test_fts_column_quoting_all_dialects` `packages/sz-orm-core/src/dialect.rs:4028` [must-contain: fn test_fts_column_quoting_all_dialects]——12 方言 × 混合大小写列（`userName`）逐字节断言 quote 形态（1 passed）。

## 3. 真连覆盖矩阵

| 服务 | 状态 | 证据 |
|------|------|------|
| PostgreSQL 18 @5432 | ✅ 行为级真连 | `test_pg_full_text_search_quoted_column` `packages/sz-orm-core/tests/integration_pg.rs:1223` [must-contain: fn test_pg_full_text_search_quoted_column]——混合大小写列 `titleText`（裸拼将折叠为 TITLETEXT → undefined column）：生成断言 quote 形态 + `to_tsvector("titleText"::text) @@ 'rust'` 真连命中 2 行；PG 套件 21/21 |
| MySQL 8.0.29 @8802 | ✅ 真连回归 | 既有 `matrix_my9_fulltext_modes` `packages/sz-orm-core/tests/integration_mysql_matrix.rs:1315` [must-contain: fn matrix_my9_fulltext_modes]（FULLTEXT 索引 + MATCH/AGAINST quote 列名）11/11 全过 |
| Oracle 26ai @1521 | ✅ 真连回归 | OR9 `or9_full_text_modes_contains_semantics`（G-OR9 修复面）24/24 全过 + integration_oracle 10/10 |
| SQLite | ✅ lib 内存 | `test_sqlite_full_text_search`（quote 形态同步） |
| SQLServer/DB2/Informix/SapHana/ClickHouse/DuckDB/Snowflake/Firebird | ⚠️ 无本机环境，登记 | 生成面单测覆盖（横向单测 + 宏断言）；真连留 CI（G7 排除清单既有口径） |

## 4. 门禁结果（EXIT 实测）

| 门禁 | 结果 | 证据 |
|------|------|------|
| G1 fmt | ✅（红队回执 P2 处置后） | 红队抓出 integration_pg.rs（本批新增 `assert_eq!` 区段）1 处漂移（初判声明漏检）→ rustfmt 单文件清零复验：本批四文件（含 dialect_advanced_test.rs）0 漂移 |
| G2/G3 | ✅ | `cargo check --all-targets` 0 error；clippy lib 0 警告 |
| G4 lib | ✅ | `cargo test -p sz-orm-core --lib` **3283 passed**（含新横向单测 + 3 处同步断言） |
| G9 | ✅/⚠️ | 注入 --strict 46/46 + 自测 15/15；secrets EXIT=1 **先在披露**（szrsql 线 2 处占位串命中，file:line 详证见 Oracle 矩阵批报告 G9 行与红队回执 R-2——本批不重复登记，与 OR1 批红队回执 R-2 同源，转交 szrsql 维护方） |
| G12/G15/G16/G33/G35/G36 | ✅ 全过 | EXIT 全 0 |
| G30 幻影测试（仓库级，先在） | ⚠️ 非本批 | EXIT=1 先在红牌（szrsql/cabi 线，前批已披露）；本批新增测试无零断言/永真/discard 反模式 |
| 真连回归 | ✅ | PG 21/21（含新增 1）+ MySQL matrix 11/11 + Oracle matrix 24/24 + integration_oracle 10/10 |
| 既有报告 audit-verify | ✅ | 本批 dialect.rs 行号位移致 Oracle 矩阵报告 2 处引用漂移——已修正复绿（commit `0b85758a`，0 失败） |

## 5. 提交清单（第 3 环实施批）

| commit | 内容 |
|--------|------|
| `58a71b7c` | 第 1 环提示词（11 方言全景勘察） |
| `2564a43` | 实施批：11 方言 quote 一致化 + 单测/宏断言同步 |
| `5f80eb8` | PG 真连行为级验证（新增测试） |
| `0b85758` | 前批报告行号漂移修正 |

## 6. 复评（第 4 环）

- 修复闭环：12 方言生成面逐字节（横向单测）+ 3 服务真连行为级（PG 新增 / MySQL、Oracle 回归）；
- 零破坏实证：lib 3283 + advanced 38 + 四套真连套件全绿；
- audit-verify 三份活跃报告（本报告 + Oracle 矩阵 + Oracle 边界）全 0 失败。

## 6.5 第 5 环红队独立复核回执

**复核方式**：独立 Agent（非报告作者 session）只读复核；四套件真连独立重跑。

**总裁决：PASS**（0 阻断项，P2×1 处置）：

| # | 级别 | 内容 | 处置 |
|---|------|------|------|
| P2 | P2 | 报告 G1 fmt 声明失实：integration_pg.rs:1257 本批新增行 1 处漂移（初判单文件清单漏列该文件） | ✅ rustfmt 单文件清零 + 报告 G1 行更正 |

**红队正向确认（机械验证）**：12 方言无第 13 个实现遗漏（13 个委托方言经宏转发完好，quote 随委托目标生效）；2564a43 diff 无越界/无并行会话 hunks；PG FTS 测试非空转（行为级判别力实证）；遗留项 2（其他方言 AddIndex cols.join 裸拼）建议属实非误报。

## 7. 遗留与建议（非缺陷）

1. **8 方言真连环境**：SQLServer（LocalDB 不满足 TCP+SQL 认证）/DB2/Informix/SapHana/ClickHouse（可经 122 隧道）/DuckDB/Snowflake/Firebird 真连留 CI（G7 既有口径）；生成面已由横向单测逐字节覆盖。
2. **同型缺口预防**：`build_alter_table` 的 AddIndex 列 join 在 MySQL/Oracle/ClickHouse 已 quote（前批修复），其余方言（PG/SQLite/SQLServer/DuckDB/DB2 等）的 AddIndex/FK 列渲染建议下批用同一横向手法扫一遍（`git grep -n "cols.join" packages/sz-orm-core/src/dialect.rs`）。
3. **szrsql 线先在项**（转交）：G9 secrets 2 处 + G30 cabi 零断言测试（前批同源，归属 szrsql 维护方）。
