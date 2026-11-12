# 新会话提示词：sz-orm Oracle 方言边界能力补齐（v1，2026-11-12）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：承接 2026-11-11 Oracle 方言矩阵（`docs/assessment/2026-11-11-oracle-dialect-matrix.md`，六环闭环，G-OR1 已修复）。该矩阵报告第 7 节遗留项 1 明确"构建绑定面真连另批执行"。同期 MySQL 侧已完成 MY7~MY10 边界补齐（`docs/assessment/2026-11-12-mysql-dialect-boundary.md`：B1 TableOption / B2 ForeignKeyDef / B3 FullTextMode / B4 QueryBuilder CTE builder，均为 sz-orm 自己方言层的新能力面）。本批做 **Oracle 侧对称验证**：这些新 API 在 OracleDialect 上的语义面（默认实现委托/忽略 + Oracle 真库接受度），重点锚定 Oracle 与 MySQL 的语法差异。
> **六环闭环**：本批按评测交付闭环链条执行。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：对 MY7~MY10 新增的 4 项能力 API 做 Oracle 侧对称真连验证（OR7~OR10），产出三分类清单（**sz-orm 方言缺口**[修复] / **Oracle 环境特有**[登记] / **文档措辞**），修复 sz-orm 侧可修复项。

**目标仓库与环境**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4，main 含 MY7~MY10 + OR1~OR6 全部改动）
- 被测：Oracle 26ai Free（Release 23.26.3.0.0）@ `127.0.0.1:1521/freepdb1`，用户 `sz_orm_test`（已授 DBA）
- 凭据：`source /d/tools/gate.env`（2026-11-12 重建版）或 `export SZ_ORM_ORACLE_PASSWORD=<密码>`；OCI 库 `export PATH="/d/oracle/product/26ai/dbhomeFree/bin:$PATH"`
- 测试产物：`packages/sz-orm-core/tests/integration_oracle_matrix.rs` **追加** OR7~OR10（沿用既有 env 注入 + assert 拒跑 + `assert_ora_code` 模式）

## 验证矩阵（OR7~OR10；每载荷 = 方言层生成 + 真库执行 + 行为断言三步）

| # | 能力块 | 现状（勘察已确认） | Oracle 语义锚点（真库实测为准） |
|---|--------|-------------------|------------------------------|
| OR7 | 表选项（对 MY7-B1） | `TableOption`（Charset/Collate/Engine）MySQL 专属；`render_table_options` 默认返回空串 → `build_create_table_with_options` 等价于 `build_create_table`（文档化契约：默认实现忽略选项不 panic） | 断言生成形态等价 + 真库执行接受；登记：MySQL 专属表选项在 Oracle 无对应面（Oracle 无库表级 ENGINE/CHARSET，字符集为库级 AL32UTF8），**不改码裁定** |
| OR8 | 外键动作（对 MY8-B2） | `ForeignKeyDef` + `build_add_foreign_key`（trait 默认 ANSI 语法，`validate_fk_action` 白名单 5 动作 × ON DELETE/ON UPDATE） | **Oracle 只支持 `ON DELETE {CASCADE \| SET NULL}`**：真库行为级证明（删父行→子行级联删除/子键置 NULL）；`ON UPDATE <任意动作>` Oracle 全部拒绝；`ON DELETE {RESTRICT \| SET DEFAULT \| NO ACTION}` 实测报错码登记；评估是否方言级 fail-fast（倾向登记不改码——白名单是 ANSI 通用面，真库拒绝即最终防线） |
| OR9 | 全文检索三模式（对 MY9-B3） | `full_text_search_mode` 默认委托 `full_text_search` → Oracle 恒生成 `CONTAINS(col, 'kw', 1) > 0`（三模式同形） | CONTEXT 索引真连（`INDEXTYPE IS CTXSYS.CONTEXT` + `PARAMETERS ('SYNC (ON COMMIT)')` 保证提交即可检索；sz_orm_test 有 DBA 应可建）：NL=默认 CONTAINS 命中；Boolean=CONTAINS 查询串操作符 `&`(AND)/`\|`(OR)/`-`(MINUS) 行为级验证；QE=Oracle 无查询扩展对应（登记差异，默认委托维持） |
| OR10 | CTE builder（对 MY10-B4） | `with_cte`/`with_recursive_cte` → `WITH [RECURSIVE] name AS (sql)`；CTE 参数先于主查询绑定 | **非递归**：`WITH name AS (...) SELECT` Oracle 接受（subquery factoring），真连行为级（含 CTE 参数先绑顺序契约经 rust-oracle `:n` 对齐验证）；**递归**：`WITH RECURSIVE` 关键字 Oracle 不支持（11.2+ 递归子查询因子化**不带 RECURSIVE 关键字**）→ 真库拒绝 = 疑似方言缺口 → 修复路径：trait 新增 `recursive_cte_keyword()`（默认 `"RECURSIVE "`，OracleDialect 覆写 `""`，Dameng 经 delegate 自动继承），`render_cte_prefix` 接线 + 单测 + 真连转正 |

## 硬性要求

- **API 兼容**：OR10 若确认缺口，修复必须纯新增 trait 默认方法（非破坏）；不改动 CteClause 结构（无列名字段，Oracle 列名可省略由锚分支推断——真库实测确认）
- **语义基准**：Oracle 26ai 官方语义 + 真库实测；禁止拿 MySQL/PG 当仲裁
- **参数化铁律**（门禁 9）：主查询参数化；CTE SQL 为调用方信任逃生口（文档明示口径）；rust-oracle 通道 `?` 非 Oracle 语法——矩阵测试手工以 `:1..:n` 对齐绑定序，登记 QueryBuilder 占位符为适配层重写约定
- **测试产物**：`integration_oracle_matrix.rs` 追加 OR7~OR10；单测随方言改动同步（dialect.rs tests + query.rs tests 若接线）
- **提交纪律**（硬约束 13）：每能力块 conventional commit；串行 cargo；与并行会话改动 hunk 级隔离（沿用 `git diff > patch + python 提取 + git apply --cached` 手法）

## 验收清单（逐项留输出）

1. `cargo check -p sz-orm-core --all-targets` / clippy -D warnings / fmt 本批文件全绿
2. `cargo test -p sz-orm-core --test integration_oracle_matrix -- --ignored`：OR1~OR10 全绿（OR7~OR10 逐块留输出）
3. `cargo test -p sz-orm-core --lib`：全过（含新单测）；`integration_oracle` 既有 10 项零回归
4. 门禁：G9（--strict + 自测）/G12/G15/G16/G33/G35/G36 全过
5. 报告 `docs/assessment/2026-11-12-oracle-dialect-boundary.md`（三分类），audit-verify 0 失败
6. 六环登记（CHANGELOG [Unreleased] + AGENTS.md 已发布行 + commit，同 MySQL 批模式）

## 环境坑位速查（沿用 OR1~OR6 批实测）

- rust-oracle `execute()` 拒绝 SELECT（InvalidOperation）——SELECT 错误断言走 `query_row_as`
- Oracle 空串即 NULL：空串绑定到 NOT NULL 列 ORA-01400
- CONTEXT 索引异步：必须 `SYNC (ON COMMIT)` 参数，否则 DML 后需 CTX_DDL.SYNC 才可检索
- 递归 CTE 真连载荷建议：组织架构树（id/parent_id/name，锚分支 + UNION ALL 递归分支 3 层）——同时覆盖非递归与递归路径
- python 门禁用 `python -X utf8 -I`（`-I` 吞 PYTHONUTF8）；git 在 `D:\Git`；临时脚本放 `D:\tmp`；报告引用凭据一律写环境变量名
- audit-verify 弱语义引用格式：纯反引号 `` `path:line` [must-contain: X] ``（must-contain 紧跟 ref，勿用 markdown 链接包裹）
