# sz-orm MySQL 方言矩阵真连验证报告（2026-11-11）

> **批次**：MySQL 方言能力面真连矩阵（MY1~MY6），按评测交付闭环链条执行。
> **提示词**：`docs/assessment/2026-11-11-mysql-dialect-matrix-prompt.md`（commit 74c9bc5b）
> **实施批**：commit 206f562（矩阵套件 + G1/G2/G4 修复）
> **被测**：sz-orm v10.13.4 方言层（`DbType::MySQL`）；宝塔 MySQL 8.0.29 @ 127.0.0.1:8802（本机常驻，`SELECT VERSION()` 实测 8.0.29）
> **凭据**：全部经 `SZ_ORM_MYSQL_URL` 环境变量注入，报告不含任何密码（审计合规铁律）
> **语义基准**：MySQL 8.0 官方语义 + 真库实测行为；未拿 PG18 当仲裁（MySQL 是独立方言族）

## 1. 评测定位与方法

评测对象是 **sz-orm 自己的方言层**，非数据库缺陷：同一功能载荷按 `DbType::MySQL` 方言生成 SQL，在真 MySQL 上执行，验证「生成被接受且结果符合 MySQL 自身语义」。每载荷三步：方言层生成 → 真库执行 → 行为断言。

与既有 `integration_mysql.rs` 25 项功能集成测试（G7 战役通过，本批复跑无回归）的关系：矩阵聚焦**方言生成面**（dialect 生成 SQL 的真库接受性与语义锚定），不重复功能覆盖。

测试产物：`packages/sz-orm-core/tests/integration_mysql_matrix.rs`（7 测试：MY1/MY2/MY2b/MY3/MY4/MY5/MY6），凭据未配置自动跳过。

## 2. 三分类清单

### A 类：sz-orm 方言缺口（已修复，真库验证）

| # | 缺口 | 缺陷机理 | 修复 |
|---|------|---------|------|
| G1 | `where_eq(col, Value::Null)` 生成 `= ?`/`= NULL` | SQL 三值逻辑下 `col = NULL` 恒为 UNKNOWN → **静默空结果集**（用户预期 IS NULL 行为）。预修复真连实证：生成 SQL 为 `WHERE \`val\` = ?`（本报告 §4 MY6） | `Value::is_null()`（value.rs:422）判空；`QueryBuilder::render_eq_fragment`（query.rs:1732）统一渲染 Eq/Ne（含 Or 变体）：Null → `IS [NOT] NULL` 且不产生绑定参数。覆盖全部 4 条渲染路径（query.rs:2262/2269 内联参数化、query.rs:2377/2384 conditions 变体、query.rs:2426/2433 Or 变体、query.rs:2831/2838 变体）+ 内联路径（query.rs:1809/1817/1850/1858） |
| G2 | `build_alter_table` AddIndex/DropIndex 列名与索引名未 quote | 保留字列 `order` 生成 `ADD INDEX idx_order (order)` → MySQL 语法错误拒绝（预修复真连实证，本报告 §4 MY2b）。外键约束名 `fk_{table}_{cols}` 亦未 quote | dialect.rs:494 AddIndex 索引名+列名 quote；DropIndex quote；AddForeignKey 约束名 quote（dialect.rs:542 前 build_alter_table 内）。ClickHouseDialect 同类缺陷顺手修复（dialect.rs:2339） |
| G4 | `quote_checked` 硬编码 63 字符上限（PG 限制） | MySQL 标识符上限 64 字符；64 字符合法 MySQL 标识符被 sz-orm 侧错误拒绝（fail-closed 方向，无错误结果，属能力缺口）。真库实证：64 字符表名建表成功、65 拒绝（§4 MY2） | `Dialect::max_identifier_len()` 方言感知上限（dialect.rs:66 trait 默认 63；MySqlDialect 覆写 64，dialect.rs:312）；`quote_checked`（dialect.rs:51）改走 `validate_identifier_with_max`（sql_safety.rs:23）。迁移/权限模块直调 `validate_identifier` 的 63 上限维持不变（见 §4 建议项 S2） |

**G2-CH（矩阵外顺手修复）**：ClickHouseDialect 的 AddIndex/DropIndex 存在与 G2 完全相同的未 quote 缺陷（dialect.rs:2339），同批修复并登记。ClickHouse 未在本矩阵范围内，该项不参与 MY 计数。

### B 类：MySQL 环境特有（真库自身行为，登记不修）

| # | 事实 | 证据 |
|---|------|------|
| E1 | `GROUP_CONCAT` 默认按 `group_concat_max_len`（本机 1024）截断：200×8+199=1799 字符拼接实测输出恰 1024 字节、113 完整条目+1 截断尾部 | 集成测试 matrix_my5（integration_mysql_matrix.rs:787）真连断言通过；`SELECT @@group_concat_max_len` 实测 1024 |
| E2 | 运行时会话 sql_mode 与全局不同：全局 `STRICT_TRANS_TABLES,NO_ENGINE_SUBSTITUTION`，经 sqlx 连接后 `@@sql_mode` = `PIPES_AS_CONCAT,IGNORE_SPACE,STRICT_TRANS_TABLES,NO_ENGINE_SUBSTITUTION`（宝塔实例级配置）。方言层正确地不假设 sql_mode | matrix_my6 真连读取；CLI 全局探针与池内会话探针差分 |
| E3 | 除零：`SELECT 1/0` 在 SELECT 上下文恒 NULL（与 sql_mode 无关）；`INSERT 1/0` 在当前 sql_mode（无 `ERROR_FOR_DIVISION_BY_ZERO`）下存 NULL 不报错 | matrix_my6 双分支断言（按 `@@sql_mode` 条件分支），真连通过 |
| E4 | 弱类型：`'false' = 0` 为真（字符串隐式转型 0）；`'5' + 1 = 6`；空串 `''` 是一等值（有别于 Oracle：`WHERE txt = ''` 命中、`txt IS NULL` 不命中） | matrix_my6 真连断言通过 |
| E5 | `escape_string` 反斜杠转义依赖 `NO_BACKSLASH_ESCAPES` 关闭（本机默认关闭）。若环境开启该模式，反斜杠转义会产生字面反斜杠 | dialect.rs:326 实现注释 + integration_mysql.rs:118 既有说明；本机 sql_mode 实测未开启（E2），风险登记 |
| E6 | MySQL 协议类型面：`LAST_INSERT_ID()`/`ROW_NUMBER()`/`@@max_allowed_packet`/`@@group_concat_max_len` 均返回 `BIGINT UNSIGNED`，sqlx 严格类型须按 u64 解码（i64 解码报 ColumnDecode 错） | matrix_my3/my4/my5 修复过程实测（sqlx ColumnDecode 错误信息）；sz-orm 方言层生成无涉，属调用侧解码约定 |
| E7 | 批量 INSERT 参数协议极限 65535 占位符/语句：21845 行×3 列=65535 真库接受；21846 行=65538 被拒（ER_PS_MANY_PARAM） | matrix_my4 上限探针真连双向断言通过 |
| E8 | `max_allowed_packet` 本机 1 GiB（1073741824） | matrix_my4 探针实测（≥4MB 断言通过） |

### C 类：文档措辞（登记，本批不改）

| # | 项 | 说明 |
|---|-----|------|
| D1 | `packages/sz-orm-core/tests/integration_mysql.rs:3,8,21` 头注释与默认 URL 写 "MySQL 9.6.0 @ 3306" | 本机实为宝塔 8.0.29 @ 8802（AGENTS.md 2026-10-28 已勘误）；该文件属既有 G7 套件，改动会牵动其默认 URL 语义，留待该套件下次维护批次顺带更新（注释性，不影响执行——env 注入优先） |
| D2 | `VALUES()` 在 ODKU 中的 8.0.20+ 弃用 | `build_upsert_on_conflict`（dialect.rs:542）生成 `col=VALUES(col)`，MySQL 8.0.20+ 标记 deprecated 但**正常执行**（8.0.29 实测，matrix_my4 通过）。改用 `INSERT ... AS new` 别名语法会破坏 MySQL 5.7/MariaDB 兼容。裁定：维持 VALUES()，不修；本文档登记弃用事实即措辞闭环 |

### 建议项（与缺陷分开）

- **S1**：`build_batch_upsert_with_params`（query.rs:3155 附近）在 HashMap 列序下依赖 `rows[0].keys()` 的迭代序——列序在语句内一致但不跨语句稳定，纯观测建议，无缺陷。
- **S2**：`quote_checked` 已方言感知（G4），但迁移/权限模块直调 `validate_identifier`（63 上限）路径未随之方言化——MySQL 64 字符标识符在这些路径仍会被拒。fail-closed 方向无错误结果，登记为已知边界，建议后续按调用方方言逐点接入 `validate_identifier_with_max`。
- **S3**：`select()` 对列名走 `validate_identifier`（63+ASCII 白名单），表达式列须走 `select_expr`（逃生口，query.rs:800 附近 doc）。矩阵实测 `COUNT(*) AS c` 走 select_expr 正常。属 API 设计现状，登记。
- **S4**（红队复核采纳项）：`where_in` 含 `Value::Null` 时仍原样绑定 `IN (?)`——三值逻辑下恒不匹配，与 G1 同族的静默空结果边界。G1 声明范围仅 Eq/Ne，本批未声称覆盖 In；留后续批次评估语义化处理。

## 3. 矩阵逐块结果（修复后真连 7/7 全绿）

| 块 | 载荷 | 结果 | 关键真库锚定 |
|----|------|------|-------------|
| MY1 | ORDER BY ASC/DESC 含 NULL；多键排序 | ✅ | MySQL ASC NULL 最前（ids 2,3,1）/DESC 最后（1,3,2）；方言未注入 NULLS FIRST/LAST（MySQL 8.0 不支持该语法） |
| MY2 | 反引号 quoting；保留字表+列 `order`；Unicode 标识符；64 长度 | ✅ | 保留字端到端（建表/INSERT/SELECT 真库接受）；Unicode 标识符真库接受而 quote_checked 按安全白名单拒绝（fail-closed，S 类边界）；64 放行/65 拒绝双向 |
| MY2b | ALTER ADD INDEX/DROP INDEX/FK quote | ✅ | 保留字列 `order` 上建索引/删索引/加外键真库接受 |
| MY3 | AUTO_INCREMENT 回填；TEXT/BLOB/JSON/DECIMAL 往返；utf8mb4 | ✅ | 同会话 LAST_INSERT_ID()=1 与行 id 一致；`鲜视达🚀` 中文 emoji 往返相等；DECIMAL(20,4) 精度无损；JSON_EXTRACT 方言生成面真库接受 |
| MY4 | ODKU upsert；批量 1000 行；65535 极限探针 | ✅ | 冲突路径 name 更新（b→b2）且总行数 4（不新增）；65535 接受/65538 拒绝 |
| MY5 | LIMIT/OFFSET；offset-only u64::MAX 占位；CTE；窗口；GROUP_CONCAT | ✅ | 第 2 页 id 11~20；offset-only 语法被真库接受（25 行）；CTE/ROW_NUMBER 透传；GROUP_CONCAT 截断锚定（E1） |
| MY6 | 三值 NULL；除零探针；隐式转换；空串 vs NULL；弱类型 | ✅ | where_eq(Null) 生成 IS NULL 命中 1 行（G1 修复后）；`= ?` 绑 NULL 恒空（MySQL 语义锚定）；E3/E4 语义锚定 |

运行输出（修复后终态）：

```text
test matrix_my1_order_by_null_semantics ... ok
test matrix_my2_identifier_quoting ... ok
test matrix_my2b_alter_index_reserved_word ... ok
test matrix_my3_autoinc_and_type_roundtrip ... ok
test matrix_my4_upsert_and_batch_limits ... ok
test matrix_my5_pagination_cte_window_groupconcat ... ok
test matrix_my6_edge_semantics ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
```

预修复基线（缺口实证）：`test result: FAILED. 1 passed; 6 failed`——失败点即 G1（MY6 生成 `WHERE \`val\` = ?`）、G2（MY2b 生成 `ADD INDEX idx_order (order)`）、G4（MY2 quote_checked 拒 64 字符）+ 测试侧 E6 解码修正（u64），详见 commit 206f562 提交说明。

## 4. 修复证据（file:line）

- G1 渲染统一：`packages/sz-orm-core/src/query.rs:1732` [must-contain: fn render_eq_fragment] + 8 处调用点（query.rs:2262/2269/2377/2384/2426/2433/2831/2838）+ 内联路径 query.rs:1809/1817/1850/1858
- G1 判空原语（既有）：`packages/sz-orm-core/src/value.rs:422` [must-contain: pub fn is_null]
- G2 MySQL AddIndex quote：`packages/sz-orm-core/src/dialect.rs:494` [must-contain: ADD INDEX]；ClickHouse 同类：`packages/sz-orm-core/src/dialect.rs:2339` [must-contain: ADD INDEX]
- G4 方言上限：`packages/sz-orm-core/src/dialect.rs:66` [must-contain: max_identifier_len]（trait 默认 63）、`packages/sz-orm-core/src/dialect.rs:312` [must-contain: max_identifier_len]（MySQL 64）、`packages/sz-orm-core/src/dialect.rs:51` [must-contain: quote_checked]（quote_checked 接线）、`packages/sz-orm-core/src/sql_safety.rs:23` [must-contain: validate_identifier_with_max]
- 单测同步：`packages/sz-orm-core/src/dialect.rs:4802` [must-contain: max_identifier_len]（63 边界 + 64 放行 + max_identifier_len=64 断言）、`packages/sz-orm-core/src/dialect.rs:5490` [must-contain: quote_checked]（65 拒绝）、AddIndex/DropIndex quote 断言（同文件 test 区块）
- 矩阵套件：`packages/sz-orm-core/tests/integration_mysql_matrix.rs:162` [must-contain: matrix_my1] / `:264` [must-contain: matrix_my2] / `:388` [must-contain: matrix_my2b] / `:483` [must-contain: matrix_my3] / `:633` [must-contain: matrix_my4] / `:787` [must-contain: matrix_my5] / `:939` [must-contain: matrix_my6]

## 5. 验收清单（逐项输出）

1. **fmt**：本批 4 文件 rustfmt 后 `cargo fmt -p sz-orm-core -- --check` 无本批文件漂移（clonecheap_parity.rs/integration_oracle_matrix.rs 等漂移系并行会话在途文件与既有漂移，未触碰）。
2. **check/clippy**：`cargo clippy -p sz-orm-core --lib --test integration_mysql_matrix -- -D warnings` 退出码 0。
3. **矩阵真连**：`cargo test -p sz-orm-core --test integration_mysql_matrix -- --ignored` → **7 passed**（§3 输出）。
4. **既有集成不回归**：`cargo test -p sz-orm-core --test integration_mysql -- --ignored` → **25 passed; 0 failed**（1098.10s）。
5. **核心单测**：`cargo test -p sz-orm-core --lib` → **3272 passed; 0 failed**（31.26s）。
6. **门禁**：G9 `--strict` 46/46 Safe；G12 PASS（首次运行撞上并行会话版本行写入竞态，复跑 PASS）；G15 PASS（PHANTOM-1 0 / 接线 4/4）；G16 PASS（`-X utf8` 运行，硬规则 0）；G33 PASS（18 处版本声明一致）；G35 PASS（248 文件 0 死链）；G36 PASS（79 版本头，workspace=10.13.4）。

## 6. 六环闭环状态

| 环 | 状态 |
|----|------|
| 1 提示词 | ✅ commit 74c9bc5b |
| 2 报告 | ✅ 本文件；audit-verify 输出见 §7 |
| 3 实施批 | ✅ commit 206f562（矩阵套件 + G1/G2/G4 + G2-CH），验证输出见 §3/§5 |
| 4 复评 | ✅ 独立机械验证：矩阵 7/7 真连重跑 + 门禁逐项退出码（§5），无自评分项 |
| 5 红队复核 | ✅ **放行**（阻断级 0 / 建议级 4 全采纳）：回执 `docs/assessment/2026-11-11-mysql-dialect-matrix-redteam-review.md`；S4（where_in Null）为采纳登记项 |
| 6 登记 | ✅ CHANGELOG.md [Unreleased] 条目 + AGENTS.md「已发布」行 + 登记 commit |

登记说明：commit 206f562 提交信息中「新增 8 测试」系笔误，实际矩阵测试为 7 个（文件与报告一致），按红队建议不改写历史提交，以本条为准。

## 7. audit-verify 自证

报告落盘后执行 `bash scripts/audit-verify.sh docs/assessment/2026-11-11-mysql-dialect-matrix.md`，终态输出：

```text
==========================================
  验证结果
==========================================
  ✅ 通过: 12
  ❌ 失败: 0
  ⚠️  警告: 24
==========================================
✅ 审计证据验证通过
```

（警告均为弱语义提示级：多锚点合并引用与测试文件行号引用未附 [must-contain]，核心 G1/G2/G4 锚点均已附 [must-contain] 断言。中途一次 1 失败系 dialect.rs 锚点行号未落在 must-contain 校验窗口内，修正后归零；终态 24 警告含本节行文提及历史行号产生的跳过提示，红队复跑口径一致。）
