# sz-orm ALTER INDEX/FK quote 横向修复批二报告（2026-11-12）

> **评测对象**：8 方言 `build_alter_table` 的 AddIndex/DropIndex 列名与索引名、AddForeignKey 约束名渲染一致化（G-OR1 同型缺口第五次发现横向收尾）
> **语义基准**：各方言官方标识符引用语义 + 本机就绪服务真库实测
> **版本**：sz-orm v10.13.4（workspace.package.version）
> **提示词**：[2026-11-12-alter-index-fk-quote-sweep-prompt.md](2026-11-12-alter-index-fk-quote-sweep-prompt.md)（第 1 环已闭环）
> **六环状态**：第 1 环提示词 ✅ / 第 2 环本报告 ✅（含红队回执处置）/ 第 3 环实施批 ✅ / 第 4 环复评 ✅ / 第 5 环红队 ✅ / 第 6 环登记 ✅

---

## 1. 缺口全景与修复清单（8 方言）

**缺口类**：`build_alter_table` 三臂标识符裸拼（FTS 批红队反向验证实证的遗留项）——AddIndex 索引名+列名、DropIndex 索引名、AddForeignKey 约束名（`fk_{table}_{cols}` 组装后未 quote）。混合大小写列裸拼被真库折叠与引号列不匹配；保留字列裸拼被解析器吞掉（G-OR1 实测 ORA-00904/00936；PG 同型 42601/42703）。

| 方言 | 修复形态（quote 后） | commit |
|------|---------------------|--------|
| PostgreSQL | `CREATE INDEX "idx" ON "t" ("col")`；`DROP INDEX "idx"`；`CONSTRAINT "fk_t_c"` | `0554331` |
| SQLite | 双引号同上 | `0554331` |
| SQL Server | `CREATE INDEX [idx] ON [t] ([col])`；`DROP INDEX [idx] ON [t]`（表名结构保持） | `0554331` |
| Snowflake | 双引号 | `0554331` |
| DB2 | 双引号 | `0554331` |
| Informix | 双引号 | `0554331` |
| SapHana | 双引号 | `0554331` |
| Firebird | 双引号 | `0554331` |
| MySQL/Oracle/ClickHouse | **已修**（206f5623/a37937a8），本批核对不重改 | — |

- **实现位置**：[packages/sz-orm-core/src/dialect.rs](../../packages/sz-orm-core/src/dialect.rs) 各 `impl Dialect` 的 `build_alter_table`；统一 `self.quote`（括号机制随方言自动）。
- **零破坏口径**：只改标识符渲染；SQLServer `DROP INDEX ... ON 表名` 方言结构保持。

## 2. 测试同步与新增

- **新增横向单测**：`test_alter_index_fk_quoting_all_dialects` `packages/sz-orm-core/src/dialect.rs:4484` [must-contain: fn test_alter_index_fk_quoting_all_dialects]——8 方言 AddIndex/DropIndex 逐字节断言（1 passed）。
- **既有断言同步**：`test_sqlserver_alter_table` DropIndex 断言更新为 `[idx_name]` quote 形态（lib 3284 全过实证无其他漏网）。

## 3. 真连覆盖矩阵

| 服务 | 状态 | 证据 |
|------|------|------|
| PostgreSQL 18 @5432 | ✅ 行为级真连 | `test_pg_alter_index_fk_quoted_realconn` `packages/sz-orm-core/tests/integration_pg.rs:1275` [must-contain: fn test_pg_alter_index_fk_quoted_realconn]——混合大小写列建索引（pg_indexes 1→0 往返）+ 保留字列 `order` 建索引真库接受 + FK CASCADE 约束名 quote 真库接受；PG 套件 22/22 |
| MySQL 8.0.29 @8802 | ✅ 真连回归 | `matrix_my2b_alter_index_reserved_word` `packages/sz-orm-core/tests/integration_mysql_matrix.rs:390` [must-contain: fn matrix_my2b_alter_index_reserved_word] 等 11/11 全过 |
| Oracle 26ai @1521 | ✅ 真连回归 | `or2_add_index_mixed_case_and_reserved_columns` `packages/sz-orm-core/tests/integration_oracle_matrix.rs:508` [must-contain: fn or2_add_index_mixed] 等 24/24 + integration_oracle 10/10 |
| SQLite | ✅ lib 内存 | lib 3284（含 SQLite 方言生成面断言） |
| SQLServer/Snowflake/DB2/Informix/SapHana/Firebird | ⚠️ 无本机环境，登记 | 生成面单测逐字节覆盖；真连留 CI（G7 既有口径） |

## 4. 门禁结果（EXIT 实测）

| 门禁 | 结果 | 证据 |
|------|------|------|
| G1 fmt | ✅ | 本批 dialect.rs/integration_pg.rs rustfmt 单文件清零后 `--check` 0 漂移 |
| G2/G3 | ✅ | `cargo check --all-targets` 0 error；clippy lib 0 警告 |
| G4 lib | ✅ | `cargo test -p sz-orm-core --lib` **3284 passed**（含 8 方言横向单测） |
| G9 | ✅/⚠️ | 注入 --strict 46/46 + 自测 15/15；secrets EXIT=1 **先在披露**（szrsql 线 2 处占位串，前批回执 R-2 同源转交） |
| G12/G15/G16/G33/G35/G36 | ✅ 全过 | EXIT 全 0 |
| G30 幻影测试（仓库级，先在） | ⚠️ 非本批 | EXIT=1 先在红牌（szrsql/cabi 线）；本批测试无零断言/永真/discard 反模式 |
| 真连回归 | ✅ | PG 22/22（含新增 1）+ MySQL matrix 11/11 + Oracle matrix 24/24 + integration_oracle 10/10 |
| 既有报告 audit-verify | ✅ | 本批 dialect.rs 位移致前三批报告 5 处引用漂移——全部修正复绿（commit `5fd3e204`：matrix 0/2、boundary 0/0、fts 0/0） |

## 5. 提交清单（第 3 环实施批）

| commit | 内容 |
|--------|------|
| （提示词）| 第 1 环 |
| `0554331` | 实施批：8 方言 AddIndex/DropIndex/FK 约束名 quote + 横向单测 + 断言同步 |
| `c544901` | PG 行为级真连验证（新增测试） |
| `5fd3e204` | 三份前批报告行号漂移修正 |

## 6. 复评（第 4 环）

- 修复闭环：8 方言生成面逐字节（横向单测）+ PG 行为级四断言（登记/保留字/往返/FK）+ MySQL/Oracle 回归；
- 零破坏实证：lib 3284 + advanced 38 + 四套真连套件全绿；
- audit-verify 四份活跃报告全 0 失败（本报告 + 前三批）。

## 7. 遗留与建议（非缺陷）

1. **6 方言真连环境**：SQLServer/DB2/Informix/SapHana/Snowflake/Firebird 真连留 CI（生成面已逐字节覆盖；G7 既有口径）。
2. **quote 类缺口预防机制**（建议）：quote 类缺口已在 5 批独立发现（G-OR1/MY G2/G-OR9/FTS 批/本批）——建议在 `check-gate-scripts.py` 或新增门禁中加静态规则：`dialect.rs` 内 `format!` 生成的 DDL 片段含裸标识符插值（非 `self.quote`）时警告，将「横向扫描」自动化。
3. **szrsql 线先在项**（转交）：G9 secrets 2 处 + G30 cabi 零断言（前批同源，归属 szrsql 维护方）。
