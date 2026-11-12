# 新会话提示词：sz-orm MySQL 方言边界能力补齐（v1，2026-11-12）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：承接 2026-11-11 MySQL 方言矩阵（`docs/assessment/2026-11-11-mysql-dialect-matrix.md`，六环闭环）。该批报告将 4 项 MySQL 能力从「边界」升级为「能力」的需求明确化。**注意：这些是 sz-orm 自己方言层未暴露的能力，与 szrsql 无关**（szrsql 是 PG 协议族，MySQL 方言路径不经过它）。
> **六环闭环**：本批按评测交付闭环链条执行。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：补齐 MySQL 方言层 4 项能力边界，全部经真 MySQL 真连验证（MY7~MY10），并单独裁定 S4（where_in Null）语义。

**目标仓库与环境**：同 2026-11-11 批次（仓库 v10.13.4；宝塔 MySQL 8.0.29 @ 127.0.0.1:8802；凭据经 `SZ_ORM_MYSQL_URL` 注入，密码不入库；密码见宝塔安装日志 `D:/BtSoft/panel/logs/install/`）。

## 补齐项（B1~B4 + S4 裁定）

| # | 项 | 现状 | 目标 |
|---|-----|------|------|
| B1 | 建表表选项 | `build_create_table` 不生成 CHARSET/COLLATE/ENGINE（依赖库级配置） | 新增 `TableOption`（Charset/Collate/Engine）+ `build_create_table_with_options`，trait 默认实现忽略/或追加方言无选项；**不得改变既有 `build_create_table` 行为**（零破坏） |
| B2 | 外键动作 | `TableChange::AddForeignKey` 不生成 ON DELETE/ON UPDATE；`validate_fk_action`（sql_safety.rs:47）已有但未接线 | AddForeignKey 增加动作字段（评估扩字段 vs 新增变体，选调用点改动最小者），生成前必须过 `validate_fk_action`；None 时保持现状 |
| B3 | 全文检索模式 | `full_text_search` 固定 NATURAL LANGUAGE MODE | 新增 `FullTextMode`（NaturalLanguage/Boolean/QueryExpansion）+ trait 新方法 `full_text_search_mode`（默认委托旧行为，非破坏）；MySqlDialect 生成三模式 |
| B4 | QueryBuilder CTE | QueryBuilder 无 CTE builder（typed_ast 有 WITH RECURSIVE） | 新增 `with_cte(name, sql, params)` / `with_recursive_cte(...)`：名称过 `validate_identifier_with_max`；CTE SQL 为调用方信任的逃生口（文档明示，同 select_expr 口径）；**CTE 参数须先于主查询参数绑定**（MySQL 占位符从左到右）；渲染进 build_select 系路径 |
| S4 | where_in Null 裁定 | `IN (?)` 绑 NULL 恒不匹配 | 裁定：这是 ANSI SQL 正确语义（IN 含 NULL 对 NULL 行结果 UNKNOWN），**不改码**，报告登记语义说明即可 |

## 硬性要求

- **API 兼容**：B1/B3 走纯新增（trait 默认方法委托），B2 若扩字段必须同步全工作区所有构造/匹配点并跑 `cargo check --workspace --all-targets`
- **语义基准**：MySQL 8.0 官方语义 + 真库实测；FK 动作/BOOLEAN MODE/递归 CTE 均以真库执行为准
- **参数化铁律**（门禁 9）：CTE 主查询参数化；CTE 内部 SQL 为信任边界内拼接（文档声明）
- **测试产物**：`packages/sz-orm-core/tests/integration_mysql_matrix.rs` 追加 MY7~MY10（沿用既有模式）；单测随方言改动同步
- **提交纪律**：每能力块 conventional commit；串行 cargo；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / clippy -D warnings / fmt 本批文件全绿
2. `cargo test -p sz-orm-core --test integration_mysql_matrix -- --ignored`：MY1~MY10 全绿（MY7~MY10 逐块留输出）
3. `cargo test -p sz-orm-core --lib`：全过（含 B1~B4 新单测）
4. 门禁：G9（--strict）/G12/G15/G16/G33/G35/G36 全过
5. 报告 `docs/assessment/2026-11-12-mysql-dialect-boundary.md`（三分类 + S4 裁定），audit-verify 0 失败
6. 六环登记（CHANGELOG/AGENTS/commit）

## 环境坑位速查（沿用上批实测）

- sqlx 0.9：动态 SQL 须 `AssertSqlSafe`；`LAST_INSERT_ID()` 等返回 BIGINT UNSIGNED 须 u64 解码；`Value` 为 `#[non_exhaustive]`——集成测试 match 须留 wildcard，macro_rules 内 `#[cfg]` 臂不可靠
- MySQL 协议 65535 占位符/语句极限；会话 sql_mode 与全局不同（PIPES_AS_CONCAT,IGNORE_SPACE）
- 递归 CTE 真连载荷建议：组织架构树（id/parent_id/name，WITH RECURSIVE 向下 3 层）——同时覆盖 B4 与 MySQL 8.0 递归语义
- git 在 `D:\Git`；临时脚本放 `D:\tmp`；报告引用凭据一律写环境变量名
