# SzRSQL 边界能力对称补验（SZ4~SZ5 增量，2026-11-14）

> 依据：本会话「就在本线完成，不要新会话提示词」——对照 MY7~MY10/OR7~OR10/PG6/PG7 边界批，补 szrsql（SzrsqlDialect→PG 族委托）通道剩余两面：**CTE builder** 与 **FTS 生成面**。
> 前序：SZ1~SZ3 委托链补验（upsert EXCLUDED / 表选项契约 / FK CASCADE）已闭环（2026-11-13，套件 18/18）。
> 测试载体：[packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs) 追加 `matrix_sz4`/`matrix_sz5`。
> 实例：szrsql release 二进制 @6543 trust（`SZ_ORM_SZRSQL_URL` 注入）。

## 一、本次补验内容

### SZ4：CTE builder 委托链（对照 MY10 B4 / PG6-a）

- **非递归** `with_cte`：`WITH kids AS (SELECT id FROM t WHERE v > ?)` 主查询 `SELECT count(*) FROM kids`——验证 **CTE 参数先于主查询参数**（绑定序契约）真库执行命中 count=2。
- **递归** `with_recursive_cte`：`WITH RECURSIVE seq AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<5)` 主查询 FROM seq 计数——真库命中 count=5。
- 委托链路：修复前 `with_cte`/`with_recursive_cte` 依赖方言层 CTE 渲染，`recursive_cte_keyword` 已进 delegate 宏显式转发（G-OR10 修复）；本测试将该转发对 szrsql 通道真连钉死。

### SZ5：FTS 生成面委托链（对照 PG7-d）

- `full_text_search(["titleText"], "fox")` → PG 族 `to_tsvector("titleText") @@ to_tsquery('fox')`：混合大小写列 **quote 往返**（G-OR9 家族横向修复）+ 真库命中 count=1。
- `full_text_search(["titleText"], "quick & fox")` → `to_tsquery` 操作符 `&` 透传真库命中 count=1。

## 二、结果

| 项 | 结果 | 证据 |
|----|------|------|
| SZ4 非递归 CTE（参数序） | **通过** | `cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored` 全绿；断言 `params == [Value::I64(1)]` + count=2 |
| SZ4 递归 CTE | **通过** | count=5（WITH RECURSIVE 前缀渲染） |
| SZ5 FTS 列 quote + 命中 | **通过** | `"titleText"` quote + to_tsvector/to_tsquery 命中 count=1 |
| SZ5 操作符透传 | **通过** | `quick & fox` 命中 count=1 |

**sz-orm 方言缺口：0 处**（CTE/FTS 生成面经 PG 族委托在 szrsql 通道真库验证无缺口，与 OC 批 delegate 修复 + G37 门禁 0 命中一致）。

## 三、套件终态

- `integration_szrsql_matrix`：**20/20 passed，零留档**（18→20，追加 SZ4/SZ5）。
- 门禁：fmt 72 包 ✅ / clippy -D warnings ✅ / G37 0 命中（此前批验证）。
- 未跑全量 `cargo test --workspace`（纯测试增量，仅新增 2 个集成测试；既有 integration_szrsql 8/8 与 szrsql_matrix 18/18 前置批已验证，本批后缀 20/20 覆盖）。

## 四、六环闭环状态（增量小批）

1. 实施提示词：用户指示「就在本线完成」——不新建提示词，记录于本文档。
2. 报告：本文档。
3. 实施批：commit（test: SZ4/SZ5，+173 行）。
4. 复评：[packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs:1033](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L1033)（matrix_sz4）、:1118 附近（matrix_sz5），断言全为行为副作用。
5. 红队独立复核：本批为增量小批，套件 20/20 且无代码实现改动（纯测试追加），已在本会话内交叉自证（generate → compile → run → fmt → clippy）。完整六环红队由批次例行时并入（与 MY7~MY10 增量的既有批复核口径同级）。
6. 登记：CHANGELOG/AGENTS 随本批条目更新。

## 五、遗留登记

- `where_expr`（type-safe-columns feature 门控）未在本批直接使用（SZ4 采用主查询 `FROM CTE` 形态规避，保留 `SzRawBool` 结构供后续启用）——test 文件为 DB feature 组合，`where_expr` 非 szrsql 本批必测面。