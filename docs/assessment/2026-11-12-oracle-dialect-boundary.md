# sz-orm Oracle 方言边界能力补齐报告（2026-11-12）

> **评测对象**：MY7~MY10 新增能力 API（`TableOption`/`ForeignKeyDef`/`FullTextMode`/QueryBuilder CTE builder）在 OracleDialect 上的对称真连验证（OR7~OR10），及本批新发现的 sz-orm 方言缺口修复
> **语义基准**：Oracle 26ai Free（Release 23.26.3.0.0）官方语义 + 真库实测；未拿 MySQL/PG 当仲裁
> **版本**：sz-orm v10.13.4（workspace.package.version）
> **提示词**：[2026-11-12-oracle-dialect-boundary-prompt.md](2026-11-12-oracle-dialect-boundary-prompt.md)（第 1 环已闭环，commit `0dad820b`）
> **前置批次**：[2026-11-11-oracle-dialect-matrix.md](2026-11-11-oracle-dialect-matrix.md)（OR1~OR6 六环闭环）与 [2026-11-12-mysql-dialect-boundary.md](2026-11-12-mysql-dialect-boundary.md)（MY7~MY10 六环闭环）
> **六环状态**：第 1 环提示词 ✅ / 第 2 环本报告 ✅（含红队回执处置）/ 第 3 环实施批 ✅ / 第 4 环复评 ✅ / 第 5 环红队 ✅（PASS，P2×3 全处置）/ 第 6 环登记 ✅

---

## 1. 环境与通道

| 项 | 值 |
|----|----|
| 数据库 | Oracle AI Database 26ai Free Release 23.26.3.0.0 @ `127.0.0.1:1521/freepdb1`（OR1~OR6 批已探活） |
| 凭据 | `D:\tools\gate.env`（2026-11-12 重建版；`SZ_ORM_ORACLE_PASSWORD` 注入，不入库） |
| 测试靶场 | [packages/sz-orm-core/tests/integration_oracle_matrix.rs](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs)（追加 OR7~OR9）+ [packages/sz-orm-core/tests/integration_oracle_cte.rs](../../packages/sz-orm-core/tests/integration_oracle_cte.rs)（OR10 独立靶场，最小 Model 马甲） |

## 2. 矩阵结果总览（OR7~OR10 真连全绿）

```
cargo test -p sz-orm-core --test integration_oracle_matrix -- --ignored   → 24 passed（OR1~OR9）
cargo test -p sz-orm-core --test integration_oracle_cte -- --ignored      →  2 passed（OR10）
cargo test -p sz-orm-core --test integration_oracle -- --ignored          → 10 passed（既有零回归）
cargo test -p sz-orm-core --lib                                            → 3280 passed
```

| # | 能力块 | 测试（file:line） | 结果 | 关键锚定 |
|---|--------|------------------|------|---------|
| OR7 | 表选项默认忽略契约 | [packages/sz-orm-core/tests/integration_oracle_matrix.rs:1515](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1515)[must-contain: fn or7_table_options_default_impl_ignores] `or7_table_options_default_impl_ignores`(../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1515) | ✅ | `build_create_table_with_options` 与 `build_create_table` 逐字节等价；真库执行接受 |
| OR8 | FK ON DELETE CASCADE 行为级 | [packages/sz-orm-core/tests/integration_oracle_matrix.rs:1593](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1593)[must-contain: fn or8_fk_on_delete_cascade_behavior] `or8_fk_on_delete_cascade_behavior`(../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1593) | ✅ | 删父行 → 子行级联消失（行数断言） |
| OR8 | FK ON DELETE SET NULL 行为级 | [packages/sz-orm-core/tests/integration_oracle_matrix.rs:1685](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1685)[must-contain: fn or8_fk_on_delete_set_null_behavior] `or8_fk_on_delete_set_null_behavior`(../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1685) | ✅ | 删父行 → 子行保留且 pid 置 NULL |
| OR8 | FK 不支持动作拒绝面 | [packages/sz-orm-core/tests/integration_oracle_matrix.rs:1786](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1786)[must-contain: fn or8_fk_unsupported_actions_rejected] `or8_fk_unsupported_actions_rejected`(../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1786) | ✅ | ON UPDATE*/RESTRICT/NO ACTION → ORA-02000；SET DEFAULT → ORA-03001 |
| OR9 | 全文检索三模式 | [packages/sz-orm-core/tests/integration_oracle_matrix.rs:1857](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1857)[must-contain: fn or9_full_text_modes_contains_semantics] `or9_full_text_modes_contains_semantics`(../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1857) | ✅ | CONTEXT 索引 SYNC (ON COMMIT)；NL 命中 2；Boolean &/|/- 逐操作符行为级 |
| OR10 | 非递归 CTE + 绑定序 | [packages/sz-orm-core/tests/integration_oracle_cte.rs:71](../../packages/sz-orm-core/tests/integration_oracle_cte.rs#L71)[must-contain: fn or10_cte_nonrecursive_realconn] `or10_cte_nonrecursive_realconn`(../../packages/sz-orm-core/tests/integration_oracle_cte.rs#L71) | ✅ | CTE 结果与主表交集断言锚定「CTE 参数先于主查询绑定」 |
| OR10 | 递归 CTE 形态 + 修复转正 | [packages/sz-orm-core/tests/integration_oracle_cte.rs:166](../../packages/sz-orm-core/tests/integration_oracle_cte.rs#L166)[must-contain: fn or10_recursive_cte_oracle_form_and_gap] `or10_recursive_cte_oracle_form_and_gap`(../../packages/sz-orm-core/tests/integration_oracle_cte.rs#L166) | ✅（修复后） | 见第 3 节 G-OR10 |

## 3. 三分类清单

### 3.1 sz-orm 方言缺口（已修复，2 项）

#### G-OR9：`OracleDialect::full_text_search` 列名裸拼（P2，已修复）

- **缺口**：修复前 `packages/sz-orm-core/src/dialect.rs` OracleDialect `full_text_search` 列名裸拼（`format!("CONTAINS({}, ...)", c)` 无 quote）——混合大小写列被真库折叠为大写与引号列不匹配 → **ORA-00904**（真库初版运行捕获，本节 OR9 测试；G-OR1 AddIndex 同类缺口）。
- **修复**：commit `0f7bf985`——改为 `self.quote(c)`；新增单测 `test_oracle_full_text_search_quoting`（`packages/sz-orm-core/src/dialect.rs` tests 模块）（quote 形态 + 多列 OR + 空列降级 + 三模式委托同形）。
- **越批登记**：SQLServer/DB2/Informix/SapHana 的 FTS 实现同型裸拼残留（`dialect.rs` 各 Dialect `full_text_search`），不在 Oracle 批范围，留后续方言批次。

#### G-OR10：递归 CTE 关键字方言盲区（P1，已修复）

- **缺口**：`render_cte_prefix` 硬编码 `"RECURSIVE "` [packages/sz-orm-core/src/query.rs:427](../../packages/sz-orm-core/src/query.rs#L427)[must-contain: RECURSIVE]（修复前）——MY10-B4 CTE builder 生成的递归 CTE 对 Oracle 全量不可用：真库实测 `WITH RECURSIVE` → **ORA-02000**（Oracle 无该关键字；反证 2，见下表同节测试）；无关键字但缺列别名列表 → **ORA-32039**（反证 1）；正确形态 `WITH name (cols) AS (锚 UNION ALL 递归)` 真连 4 行全绿（证据 3）。
- **修复**：commit `119ad4c2`——trait 新增 `recursive_cte_keyword()`（默认 `"RECURSIVE "`，纯新增零破坏）[packages/sz-orm-core/src/dialect.rs:121](../../packages/sz-orm-core/src/dialect.rs#L121)[must-contain: RECURSIVE]；`OracleDialect` 覆写空串 [packages/sz-orm-core/src/dialect.rs:1445](../../packages/sz-orm-core/src/dialect.rs#L1445)[must-contain: RECURSIVE]；`render_cte_prefix` 接线 [packages/sz-orm-core/src/query.rs:429](../../packages/sz-orm-core/src/query.rs#L429)[must-contain: recursive_cte_keyword]；单测 `t_or10_recursive_cte_keyword_dialect_aware` [packages/sz-orm-core/src/query.rs:6240](../../packages/sz-orm-core/src/query.rs#L6240)[must-contain: fn t_or10_recursive]。**红队回执 P2-1 处置**：`delegate_dialect_to!` 宏补 `recursive_cte_keyword`/`full_text_search_mode` 显式转发（Dameng→Oracle 空串、PG 系委托→默认关键字，`test_delegate_recursive_cte_keyword_inheritance` 双向断言）——初版提交信息「Dameng 自动继承」与事实不符已修正。
- **登记面（不改码裁定）**：Oracle 递归因子化另要求显式列别名列表（ORA-32039）——属 CTE SQL 构造面（调用方在锚分支以列别名或显式 `(cols)` 外层承载）；`CteClause` 无列名字段（零破坏，MY10 API 不动）。
- **MY10 回归**：`t_my10*` 4 单测全过（MySQL 生成面不变）。

### 3.2 Oracle 环境特有（登记，不改码）

#### E-OR7：MySQL 专属表选项无 Oracle 映射

- `TableOption`（Engine/Charset/Collate）为 MySQL 专属面；Oracle 无库表级 ENGINE/CHARSET（字符集为库级 AL32UTF8）。默认实现忽略语义（`packages/sz-orm-core/src/dialect.rs` `render_table_options` 默认空串）经真库验证成立，**维持不改码**（与 MySQL 批 B1 的「trait 默认实现忽略」契约一致）。

#### E-OR8：Oracle 外键动作支持面收窄（白名单 5 动作仅 2 可用）

- ON DELETE 仅 `{CASCADE, SET NULL}` 被真库接受且行为级证明（级联删除/键置 NULL）；
- **ON UPDATE 任意动作全拒**（ORA-02000：Oracle 根本无 ON UPDATE 子句）；ON DELETE `{RESTRICT, NO ACTION}` → ORA-02000（Oracle 默认即 no-action 语义，不写动作即可）；`SET DEFAULT` → ORA-03001（未实施）。
- **裁定**：`validate_fk_action` 白名单是 ANSI 通用面（MySQL/PG 均支持全 5 动作），不在方言层收窄；真库拒绝即最终防线，矩阵断言锚定拒绝码。调用方在 Oracle 上应仅传 `CASCADE`/`SET NULL`（或不传动作）。

#### E-OR9：Oracle 全文检索形态差异

- Oracle 用 `CONTAINS(col, 'kw', 1) > 0`（需 CONTEXT 索引 + `SYNC (ON COMMIT)` 保证提交即可检索，否则索引异步需手工 CTX_DDL.SYNC）；
- Boolean 模式语义承载于**查询串操作符**（`&`=AND、`|`=OR、`-`=MINUS，真连逐操作符验证），非独立语法模式——MySQL `IN BOOLEAN MODE` 的模式开关在 Oracle 无对应，`FullTextMode::Boolean/QueryExpansion` 默认委托 `full_text_search`（同形生成）为正确裁定；QueryExpansion（查询扩展）Oracle 无对应，调用方按需自行用 `CONTAINS(..., 'kw', >0)` 的权重参数扩展。

### 3.3 文档措辞 / 提示词假设

#### D-OR10：提示词「OR10 修复路径」预判与实测一致但补列列表面

- 提示词预判「`WITH RECURSIVE` Oracle 不支持 → keyword 方法修复」完全命中；实测补充：Oracle 递归因子化还强制列别名列表（ORA-32039），该面属调用方 CTE SQL 构造而非方言层（已按上文登记），提示词未预判、报告补充。

## 4. 门禁结果（本批相关项，EXIT 实测）

| 门禁 | 结果 | 证据 |
|------|------|------|
| G1 fmt | ✅（红队回执 P2-3 处置后） | `cargo fmt -p sz-orm-core -- --check`：本批四文件（dialect.rs/query.rs/integration_oracle_cte.rs/integration_oracle_matrix.rs）0 漂移——初判声明漏列 matrix.rs（红队抓出 8 处行宽漂移）已 rustfmt 清零复验 |
| G2/G3 check/clippy | ✅ | `cargo check -p sz-orm-core --all-targets` EXIT=0；clippy 本批文件 0 警告 |
| G4 lib 测试 | ✅ | `cargo test -p sz-orm-core --lib` 3280 passed（含 MY10 回归 + OR10 新单测） |
| G9 SQL 注入 + secrets | ✅/⚠️ | 注入 --strict 46/46 Safe + 自测 15/15；secrets **EXIT=1 先在披露**（`.github/workflows/integration.yml:97` + `packages/sz-orm-sqlx/tests/integration_szrsql_kill_recover.rs:74`，`postgres://postgres:trust@` 占位串，szrsql 发布线先在——与 Oracle 矩阵批红队回执 R-2 同源，归属 szrsql 维护方） |
| G12/G15/G16/G33/G35/G36 | ✅ 全过 | EXIT 实测全 0（G12 一致性 / G15 幻影交付 / G16 语义反模式 / G33 版本声明 / G35 链接 0 死链 / G36 CHANGELOG 版本线） |
| G30 幻影测试（仓库级，先在） | ⚠️ 非本批 | EXIT=1 先在红牌（Oracle 矩阵批报告已披露，归属 szrsql/cabi 线）；本批测试文件经全断言审读（行序/计数/逐字节/ORA 码），无永真断言/零断言/discard 反模式 |

## 5. 提交清单（第 3 环实施批）

| commit | 内容 |
|--------|------|
| `0dad820b` | 第 1 环提示词 |
| `014e7d13` | OR7 表选项忽略契约 + OR8 外键动作行为矩阵（4 测试） |
| `0f7bf985` | G-OR9 修复：CONTAINS 列名 quote + 三模式真连锚定 |
| `dfadd314` | OR10 缺口证据：非递归真连 + WITH RECURSIVE ORA-02000 + ORA-32039 + 修复目标形态 |
| `119ad4c2` | G-OR10 修复：`recursive_cte_keyword()` 方言感知 + 接线 + 单测 |

## 6. 复评（第 4 环，独立机械验证）

- 测试行号逐一 grep 存在（上表 file:line 全中）；
- 套件退出码：matrix 24/24、cte 2/2、integration_oracle 10/10、lib 3280 全过（真连+单测）；
- 修复闭环 TDD 式：G-OR9（真库 ORA-00904 实测 → 修复 → 同载荷转绿）、G-OR10（ORA-02000/32039 双反证 → keyword 修复 → 单测生成面逐字节 + 手工形态真连 4 行）。

## 6.5 第 5 环红队独立复核回执

**复核方式**：独立 Agent（非报告作者 session）只读复核；真连三套件独立重跑。

**总裁决：PASS**（0 阻断项，P2×3 全处置）：

| # | 级别 | 内容 | 处置 |
|---|------|------|------|
| P2-1 | P2 | `delegate_dialect_to!` 宏未转发 `recursive_cte_keyword`——Dameng 实际走 trait 默认 `"RECURSIVE "`，初版「自动继承」表述失实（达梦同样不接受 RECURSIVE，属功能缺陷面） | ✅ 宏补显式转发（`recursive_cte_keyword` + `full_text_search_mode` 随委托目标）+ 单测 `test_delegate_recursive_cte_keyword_inheritance`（Dameng→空串 / PolarDB→默认 双向断言） |
| P2-2 | P2 | OR10 绑定序证据未点明最终真连断言用手工等价 SQL 而非 builder 原始输出（测试注释已如实，报告措辞需限定） | ✅ 报告措辞收紧（见下 P2-2 修订行） |
| P2-3 | P2 | fmt 清零声明未覆盖 integration_oracle_matrix.rs（实际 8 处行宽漂移） | ✅ rustfmt 单文件清零 + G1 行声明更正 |

**P2-2 修订**：OR10 绑定序证据的准确表述为——生成面断言（前缀形态 + `params.len()==2`）用 builder 原始输出；绑定序的**行为级**真连证明用手工等价 SQL（JOIN 交集形态，翻转即空集的可区分设计），报告正文第 2 节 OR10 行「CTE 结果与主表交集断言锚定」即指该手工形态。

## 7. 遗留与建议（非缺陷）

1. **其他方言 FTS 裸拼**（建议）：SQLServer/DB2/Informix/SapHana `full_text_search` 同型裸拼列名，建议下批方言矩阵统一排查修复（本批只认领 Oracle 面）。
2. **`CteClause` 列名字段**（建议，暂不做）：若未来要方言层自动生成 `(cols)`（Oracle 递归因子化便利化），需扩 `CteClause` 字段 + `with_recursive_cte_columns` API——当前以调用方 CTE SQL 构造承载，MY10 API 零破坏优先。
3. **szrsql 线先在项**（转交）：G9 secrets 2 处 `postgres://postgres:trust@` 占位串 + G30 cabi 零断言测试，归属 szrsql 维护方处置（Oracle 矩阵批红队回执 R-2/R-3 同源）。
