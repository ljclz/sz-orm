# 新会话提示词：sz-orm FTS 列名 quote 横向修复批（v1，2026-11-12）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：G-OR1（Oracle AddIndex，a37937a8）→ MY 批 G2（MySQL AddIndex，206f5623）→ G-OR9（Oracle CONTAINS，0f7bf985）三次真连矩阵均抓出**同型缺口：标识符裸拼**。Oracle 边界批报告（`2026-11-12-oracle-dialect-boundary.md` §7 遗留项 1）登记了 4 方言 FTS 同型残留。本批做横向收尾：**全工作区 `full_text_search` 实现的列名 quote 一致化**。
> **六环闭环**：本批按评测交付闭环链条执行。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：横向修复 `dialect.rs` 全部 `full_text_search` 实现的列名裸拼（quote 一致化），每方言补生成面单测，真连面按本机可用服务尽力验证（MySQL/PG/Oracle 就绪，SQLite 走 lib 内存；其余方言登记无环境）。

**目标仓库**：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4，main）

## 缺口全景（勘察已确认，逐项修复）

| 行号（勘察时点，自行复核） | 方言 | 现状生成 | 修复后形态 |
|---------------------------|------|---------|-----------|
| ~529 | MySqlDialect | `` MATCH({cols raw join}) AGAINST(...) `` | 列逐个 `` `c` `` 后 join |
| ~876 | PostgreSqlDialect | `{c}::text` 裸拼 + `\|\| ' ' \|\|` | `"c"::text`；**注意**：`to_tsquery('kw')` 整串单引号形态维持（PG tsquery 语法面，不拆词） |
| ~1107 | SqliteDialect | `{c} LIKE '%kw%'` 裸拼 | `"c" LIKE '%kw%'`（SQLite 双引号标识符） |
| ~1736 | SqlServerDialect | `CONTAINS({cols raw join}, 'kw')` | 列逐个 `[c]` 后 join |
| ~2172 | SnowflakeDialect | `{c} ILIKE '%kw%'` | `"c" ILIKE ...` |
| ~2451 | ClickHouseDialect | `position({c}, 'kw')` | `` position(`c`, 'kw') ``（ClickHouse 反引号） |
| ~2706 | DuckDBDialect | `{c} LIKE '%kw%'` | `"c" LIKE ...`（DuckDB 双引号） |
| ~2920 | Db2Dialect | `CONTAINS({c}, 'kw') > 0` | `"c"` 双引号 |
| ~3256 | InformixDialect | `CONTAINS({c}, 'kw') > 0` | 双引号（Informix ANSI 形态） |
| ~3482 | SapHanaDialect | `CONTAINS({c}, 'kw') > 0` | 双引号 |
| ~3720 | FirebirdDialect | `CONTAINING({c}, 'kw')` | 双引号 |
| ~1471 | OracleDialect | **已修复**（G-OR9，0f7bf985） | 仅核对不重改 |

**既有测试同步**（`tests/dialect_advanced_test.rs` `test_full_text_search!` 宏 7 处调用断言裸拼形态片段，如 `"CONTAINS(title, 'keyword') > 0"`）——修复后同步为 quote 形态；宏的 `$fragment` 参数逐方言更新。MySQL（MATCH/AGAINST）与 Oracle（CONTAINS x,y,1）不在宏内，单独处理。

## 硬性要求

- **零破坏口径**：只改列名渲染（quote 包裹），不改函数结构/关键字/转义；每方言一个最小 diff
- **单测**：每方言 `full_text_search(&["userName"], "kw")` 断言 quote 形态（与 G-OR9 的 `test_oracle_full_text_search_quoting` 同模式）；多列 OR/空格分隔形态逐方言核对（PG `\|\| ' ' \|\|`、MySQL/SQLServer 逗号 join、其余 OR）
- **真连面**（本机就绪服务尽力，其余登记）：
  - MySQL（宝塔 8802，`SZ_ORM_MYSQL_URL`）：FULLTEXT 索引 + MATCH/AGAINST 真连命中（quote 列名）
  - PG（5432）：`to_tsvector("c"::text) @@ to_tsquery` 真连
  - Oracle（1521/freepdb1，`SZ_ORM_ORACLE_PASSWORD` + OCI PATH）：CONTEXT 索引真连（复用 OR9 模式，核对不回归即可）
  - SQLite：lib 内存测试（`t_sqlite_full_text_search` 既有，核对不回归）
  - SQLServer/DB2/Informix/SapHana/ClickHouse/DuckDB/Snowflake/Firebird：无本机环境，登记「生成面单测覆盖，真连留 CI」
- **门禁**：G9（--strict + 自测 + secrets 如实披露，szrsql 线 2 处先在命中）/G12/G15/G16/G30/G33/G35/G36
- **提交纪律**：每方言（或每组合并）conventional commit；与并行会话 hunk 级隔离；Mimosa 误报时用 python 脚本落盘（`python -X utf8 -I <<EOF`）

## 验收清单（逐项留输出）

1. `cargo check -p sz-orm-core --all-targets` / clippy / fmt 本批文件全绿
2. `cargo test -p sz-orm-core --lib`：全过（含新单测）；`dialect_advanced_test` 7 处宏断言同步后全过
3. 真连：MySQL/PG/Oracle FTS 各 1 项新增或核对测试全过；`integration_mysql_matrix`/`integration_oracle_matrix` 零回归
4. 报告 `docs/assessment/2026-11-12-fts-quote-sweep.md`（修复清单 + 三分类 + 真连覆盖矩阵），audit-verify 0 失败
5. 红队独立复核
6. 六环登记（CHANGELOG [Unreleased] + AGENTS.md 已发布行 + commit）

## 环境坑位速查（沿用前批）

- git 在 `D:\Git`；`cargo fmt -p sz-orm-core -- --check` 逐文件核对（勿按预期归属过滤——OR1 批红队教训）
- 管道后 `$?` 是尾命令的——secrets 退出码用 `> file 2>&1; echo $?` 捕获（OR1 批红队教训）
- python 门禁用 `python -X utf8 -I`；audit-verify 引用格式：`` `path:line` [must-contain: X] ``
- Mimosa 假阳性：测试内长 SQL format!/占位符重写循环会被拦——python 脚本落盘绕过
- 并行会话可能活跃：`git diff --stat` 先查在途改动，提交 hunk 级隔离
