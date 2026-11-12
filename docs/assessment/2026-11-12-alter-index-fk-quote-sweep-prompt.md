# 新会话提示词：sz-orm ALTER INDEX/FK quote 横向修复批二（v1，2026-11-12）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：quote 类缺口第五次同型发现处置——G-OR1（Oracle AddIndex，a37937a8）→ MY G2（MySQL+ClickHouse，206f5623）→ G-OR9（Oracle CONTAINS，0f7bf985）→ FTS 横向批（12 方言，2564a43）后，FTS 批红队反向验证确认：**其余 8 方言的 `build_alter_table` AddIndex/DropIndex 列名与索引名仍裸拼**（`2026-11-12-fts-quote-sweep.md` §7 遗留项 2，红队 grep 实证）。本批横向收尾。
> **六环闭环**：本批按评测交付闭环链条执行。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：修复 8 方言 `build_alter_table` 的 AddIndex/DropIndex/AddForeignKey quote 缺口，每方言生成面单测，真连面按本机服务尽力（PG 全行为级 + SQLite lib + MySQL/Oracle 回归），其余登记。

**目标仓库**：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4，main）

## 缺口全景（勘察已确认 8 臂 + FK 约束名同源）

| 方言（AddIndex 臂行号为勘察时点，自行复核） | quote 形态 | 缺口 |
|-------------------------------------------|-----------|------|
| PostgreSqlDialect（~973） | 双引号 | AddIndex 名+列裸拼 / DropIndex 名裸拼 / FK 约束名裸拼 |
| SqliteDialect（~1228） | 双引号 | 同上 |
| SqlServerDialect（~1864；DROP INDEX 带表名） | 方括号 | 同上 |
| SnowflakeDialect（~2286） | 双引号 | 同上 |
| Db2Dialect（~3044） | 双引号 | 同上 |
| InformixDialect（~3370） | 双引号 | 同上 |
| SapHanaDialect（~3603） | 双引号 | 同上 |
| FirebirdDialect（~3837） | 双引号 | 同上 |
| MySQL/Oracle/ClickHouse | **已修**（689/1605/2565） | 仅核对不重改 |

**修复模式**（与 G-OR1 Oracle 修复形态逐字节同构，统一走 `self.quote` 自动随方言括号机制）：
- AddIndex：`CREATE INDEX {quote(name)} ON {quote(table)} ({cols.map(quote).join(", ")})`
- DropIndex：`DROP INDEX {quote(name)}`（SQLServer 维持 `DROP INDEX {quote(name)} ON {quote(table)}` 结构，只补 quote）
- AddForeignKey：约束名 `fk_{table}_{cols}` 组装后 quote（MySQL 批 G2 口径：`let fk_name = format!(...); self.quote(&fk_name)`）；列名 quote 保持

**既有测试同步**：`dialect.rs` tests + `tests/dialect_advanced_test.rs` 中 8 方言 alter_table/index 断言若有裸拼形态需同步（先 `cargo test -p sz-orm-core --lib` 全量跑一遍再改，翻出全部旧断言）。

## 真连面（本机就绪服务）

- **PG（5432）**：新增行为级测试——混合大小写列 + 保留字列建索引（裸拼将折叠/语法错）、DropIndex 往返、FK 约束名 quote 后真库接受；`integration_pg.rs` 追加
- **SQLite**：lib 内存测试（quote 后建索引/删索引）
- **MySQL/Oracle**：matrix 套件零回归（G2/G-OR1 修复面）
- **SQLServer/Snowflake/DB2/Informix/SapHana/Firebird**：无本机环境，登记真连留 CI

## 硬性要求

- **零破坏口径**：只改列名/索引名/约束名渲染（quote 包裹），关键字/结构不变
- **既有 API 不动**：`TableChange` 结构不变；`DROP INDEX` 方言差异（SQLServer 带表名）保持
- **门禁**：G9（--strict + 自测 + secrets 先在披露）/G12/G15/G16/G30/G33/G35/G36
- **提交纪律**：每方言或每组合并 conventional commit；与并行会话 hunk 级隔离；Mimosa 误报时 python 脚本落盘
- **fmt 纪律（前三批红队教训）**：本批触过的每个文件逐一 `cargo fmt -p sz-orm-core -- --check`，不靠清单记忆

## 验收清单（逐项留输出）

1. `cargo check -p sz-orm-core --all-targets` / clippy / fmt 本批文件全绿
2. `cargo test -p sz-orm-core --lib` 全过（含 8 方言新单测）；`dialect_advanced_test` 不回归
3. 真连：PG 新增测试全过 + `integration_pg` 套件零回归 + `integration_mysql_matrix`/`integration_oracle_matrix`/`integration_oracle` 零回归
4. 报告 `docs/assessment/2026-11-12-alter-index-fk-quote-sweep.md`（8 方言修复清单 + 真连覆盖矩阵 + 三分类），audit-verify 0 失败
5. 红队独立复核
6. 六环登记（CHANGELOG [Unreleased] + AGENTS.md 已发布行 + commit）

## 环境坑位速查（沿用前批实测）

- fmt 检查对本批全部触过文件逐一执行（OR1 批 16 处/边界批 8 处/FTS 批 1 处连续三批红队抓 fmt 漏检）
- 管道后 `$?` 是尾命令的——退出码用 `> file 2>&1; echo $?` 捕获
- 改生成面后先跑全量 `cargo test -p sz-orm-core --lib` 翻旧断言再提交
- python heredoc 中文首行加 `# -*- coding: utf-8 -*-`；Mimosa 假阳性用 python 落盘绕过
- audit-verify 引用格式：`` `path:line` [must-contain: X] ``；改 dialect.rs 后前批报告行号会漂移——提交前复跑前三批报告的 audit-verify（漂移即修）
- 并行会话可能活跃：`git diff --stat` 先查在途改动
