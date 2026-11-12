# sz-orm MySQL 方言边界能力补齐报告（2026-11-12）

> **批次**：承接 2026-11-11 MySQL 方言矩阵（`docs/assessment/2026-11-11-mysql-dialect-matrix.md`）报告 §2 的 4 项「边界」升级为「能力」，全部真连验证（MY7~MY10）。
> **定位澄清**：本批 4 项均为 **sz-orm 自己方言层未暴露的能力**，与 szrsql 无关（szrsql 是 PG 协议族自研库，`DbType::MySQL` 路径不经过它；szrsql 能力面已于 2026-11-09/10 独立矩阵验证）。
> **提示词**：`docs/assessment/2026-11-12-mysql-dialect-boundary-prompt.md`
> **实施批**：commit 5161cdcf（B1/B2/B3）+ 27be39db（B4 + 矩阵 MY7~MY10）
> **被测**：sz-orm v10.13.4 方言层；宝塔 MySQL 8.0.29 @ 127.0.0.1:8802；凭据经 `SZ_ORM_MYSQL_URL` 注入，报告不含密码。

## 1. 补齐项与真连验证（MY7~MY10，11/11 全绿）

| # | 能力 | API（纯新增，零破坏） | 真连验证 |
|---|------|----------------------|---------|
| B1 | 建表表选项 | `TableOption`（Charset/Collate/Engine）（`packages/sz-orm-core/src/dialect.rs:15`）+ `build_create_table_with_options`（`packages/sz-orm-core/src/dialect.rs:172`，默认实现空选项等价既有行为）+ `MySqlDialect::render_table_options`（`packages/sz-orm-core/src/dialect.rs:529`，选项值 `validate_identifier` 白名单 fail-closed） | **MY7**（`packages/sz-orm-core/tests/integration_mysql_matrix.rs:1146`）：DDL 被真库接受；`information_schema.TABLES` 实测 `ENGINE=InnoDB`、`TABLE_COLLATION=utf8mb4_unicode_ci` 真实生效；既有 `build_create_table` 产物零变化 |
| B2 | 外键动作 | `ForeignKeyDef`（on_delete/on_update）（`packages/sz-orm-core/src/dialect.rs:39`）+ `Dialect::build_add_foreign_key`（`packages/sz-orm-core/src/dialect.rs:213`，默认实现 ANSI 语法 + `validate_fk_action` 白名单，非法动作 `DbError::InvalidInput`） | **MY8**（`packages/sz-orm-core/tests/integration_mysql_matrix.rs:1200`）：`ON DELETE CASCADE` DDL 被真库接受；行为级证明——插入父行+2 子行，删父行后子行数=0（级联真实生效）；非法动作生成侧拒绝不出笼 |
| B3 | 全文检索三模式 | `FullTextMode`（`packages/sz-orm-core/src/dialect.rs:26`）+ `Dialect::full_text_search_mode`（`packages/sz-orm-core/src/dialect.rs:135`，默认委托旧行为）+ `MySqlDialect` 覆写（`packages/sz-orm-core/src/dialect.rs:514`） | **MY9**（`packages/sz-orm-core/tests/integration_mysql_matrix.rs:1315`）：NL 模式命中 2 行；BOOLEAN `+database -mysql` 操作符过滤仅 1 行；QE 模式可执行不缩圈。载荷为英文 token（InnoDB 默认分词器不切中文，中文全文检索需 ngram FULLTEXT，登记见 §3） |
| B4 | QueryBuilder CTE | `CteClause`（`packages/sz-orm-core/src/query.rs:107`）+ `with_cte`/`with_recursive_cte`（`packages/sz-orm-core/src/query.rs:367`/`packages/sz-orm-core/src/query.rs:381`，名称过方言感知标识符校验）+ 渲染进 `build_select_with_params` 与 `sql()`（`WITH [RECURSIVE]` 前缀；CTE 参数先于主查询参数绑定） | **MY10**（`packages/sz-orm-core/tests/integration_mysql_matrix.rs:1377`）：`WITH RECURSIVE` 组织树 4 节点深度 [1,2,3,2]（行为级递归验证）；参数顺序行为证明——CTE 参数绑 parent_id=1、主查询 id>2 → 结果仅 id=4；混合 CTE 统一 RECURSIVE 前缀 |
| S4 | where_in Null 裁定 | **不改码**：`IN (?)` 绑 NULL 恒不匹配是 ANSI SQL 正确语义（三值逻辑），不是缺陷；`where_null` 才是 NULL 查询通道 | 裁定依据见 §3 |

## 2. 三分类清单

### A 类：sz-orm 方言缺口（本批补齐 4 项）

即 §1 的 B1~B4，全部纯新增 API、既有调用方零改动（`cargo check --workspace --all-targets` 0 error）、真连验证通过。

### B 类：环境特有（登记）

| # | 事实 |
|---|------|
| E9 | InnoDB 默认分词器不切中文——中文全文检索需 `FULLTEXT KEY ... WITH PARSER ngram`（5.7.6+ 内置）；本批 MY9 载荷用英文 token 验证模式语义。ngram 中文 FT 的端到端载荷可作后续批次 |
| E10 | FK 动作约束要求 InnoDB（本机默认）+ 引用列须有索引（父表主键满足）；`INSERT INTO t () VALUES ()` 空列表语法 MySQL 合法（MY8 载荷使用） |

### C 类：文档措辞（登记）

| # | 项 |
|---|-----|
| D3 | `TableChange::AddForeignKey`（无动作）与 `build_add_foreign_key`（带动作）并存——后续批次可评估将 enum 变体迁移到 ForeignKeyDef 统一 API（涉及 ~25 处调用点，非本批范围） |
| D4 | `full_text_search` 旧方法的 doc 注释可补充指向 `full_text_search_mode`（本批未动，避免注释面回归风险） |

## 3. S4 裁定（where_in Null）

**不改码**。`WHERE col IN (?, NULL)` 中对 NULL 行的比较结果为 UNKNOWN → 不命中——这是 ANSI SQL 三值逻辑的正确语义（PostgreSQL/SQLite 同样如此），改写会造成跨方言语义分裂。`where_null`/`where_not_null` 才是 NULL 查询通道。2026-11-11 批次红队建议「评估语义化处理」，本批裁定为**按 SQL 标准语义维持**，登记于此闭环该项。

## 4. 设计决策（供红队对抗审查）

1. **B1 选项值 fail-closed**：`render_table_options` 对 Charset/Collate/Engine 值跑 `validate_identifier`（panic 为文档化契约——非 `Result` 因辅助渲染内联于无返回值方法；非法值在单测中经 `catch_unwind` 断言拒绝）
2. **B2 不动 enum**：给 `TableChange::AddForeignKey` 加字段需改 ~25 处调用点（含并行会话在途文件风险），采用并行 API `build_add_foreign_key` 零破坏达成能力；enum 迁移登记为 D3
3. **B4 逃生口口径**：CTE SQL 与 `select_expr` 同级信任边界（调用方信任来源拼接），名称与参数仍受控；UPDATE/DELETE 的 CTE 语法各方言差异大，明确不渲染（doc 注明），避免半吊子生成
4. **B4 RECURSIVE 语义**：MySQL 与 PG 均要求「任一递归→整条子句 RECURSIVE」，渲染器按任一 recursive 标志统一加前缀

## 5. 验收清单（逐项输出）

1. **check/clippy/fmt**：`cargo check --workspace --all-targets` 0 error；`cargo clippy -p sz-orm-core --lib --test integration_mysql_matrix -- -D warnings` 通过；本批 3 文件 rustfmt 后零漂移
2. **矩阵真连**：`cargo test -p sz-orm-core --test integration_mysql_matrix -- --ignored` → **11 passed**（MY1~MY10，含新增 MY7~MY10）
3. **lib 单测**：`cargo test -p sz-orm-core --lib` → **3279 passed; 0 failed**（含新增 12 个：MY7~MY9 8 个 + MY10 4 个）
4. **门禁**：G9 --strict 46/46 Safe；G15 PASS；G16 PASS；G35 PASS（0 死链）；G36 PASS；G33 PASS（登记提交后复跑）

## 5. 六环闭环状态

| 环 | 状态 |
|----|------|
| 1 提示词 | ✅ `2026-11-12-mysql-dialect-boundary-prompt.md` |
| 2 报告 | ✅ 本文件；audit-verify 输出见 §6 |
| 3 实施批 | ✅ commit 5161cdcf + 27be39db |
| 4 复评 | ✅ 矩阵 11/11 真连重跑 + lib 3279 + 门禁逐项退出码 |
| 5 红队复核 | ⏳ 独立 Agent 复核（回执 `2026-11-12-mysql-dialect-boundary-redteam-review.md`） |
| 5→6 | 登记于复核放行后落地 |

## 6. audit-verify 自证

报告落盘后执行 `bash scripts/audit-verify.sh docs/assessment/2026-11-12-mysql-dialect-boundary.md`，终态输出：

```text
==========================================
  验证结果
==========================================
  ✅ 通过: 15
  ❌ 失败: 0
  ⚠️  警告: 6
==========================================
✅ 审计证据验证通过
```

（警告为弱语义提示级；首次运行因报告引用使用裸文件名（dialect.rs:15）被判 0 引用 fail-closed，改为全路径引用后归零——该行为与 2026-11-11 批次口径一致。）
