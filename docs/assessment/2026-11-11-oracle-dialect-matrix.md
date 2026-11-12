# sz-orm Oracle 方言矩阵真连验证报告（2026-11-11）

> **评测对象**：sz-orm 自己的 OracleDialect 生成面（`DbType::Oracle` 载荷 → 真库执行 → 行为断言）
> **语义基准**：Oracle 26ai Free（23ai 家族，Release 23.26.3.0.0）官方语义 + 真库实测；未拿 PG18/MySQL 当仲裁
> **版本**：sz-orm v10.13.4（workspace.package.version）
> **提示词**：[2026-11-11-oracle-dialect-matrix-prompt.md](2026-11-11-oracle-dialect-matrix-prompt.md)（第 1 环已闭环）
> **六环状态**：第 1 环提示词 ✅ / 第 2 环本报告 ✅（含红队更正）/ 第 3 环实施批 ✅ / 第 4 环复评 ✅ / 第 5 环红队 ✅（FAIL→回执修复→条件 PASS）/ 第 6 环登记 ✅（CHANGELOG [Unreleased] + AGENTS.md 已发布行，与同期 MySQL 批同模式；crates.io 发布随下版走标准流程）

---

## 1. 环境与通道

| 项 | 值 |
|----|----|
| 数据库 | Oracle AI Database 26ai Free Release 23.26.3.0.0 @ `127.0.0.1:1521/freepdb1`（sqlplus `v$version` 实测） |
| 用户 | `sz_orm_test`（DBA；密码经 `SZ_ORM_ORACLE_PASSWORD` 环境变量注入，不入库） |
| 凭据治理 | 本机 `D:\tools\gate.env` 已被清理丢失 → 经本地 `sqlplus / AS SYSDBA`（OS 认证）重置 sz_orm_test 密码并重建 gate.env；全程凭据零入库 |
| 连接通道 | `oracle` crate 0.6.3（rust-oracle）直连；测试靶场 [packages/sz-orm-core/tests/integration_oracle_matrix.rs](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs) |
| 既有回归 | `integration_oracle.rs` 既有真连 10/10 通过（本批前后各跑一次，零回归） |

## 2. 矩阵结果总览（OR1~OR6，19 项真连断言全绿）

```
cargo test -p sz-orm-core --test integration_oracle_matrix -- --ignored
test result: ok. 19 passed; 0 failed
```

| # | 能力块 | 测试（file:line） | 结果 | 关键锚定 |
|---|--------|------------------|------|---------|
| OR1 | 空串与 NULL | `or1_empty_string_insert_then_is_null` [tests:169](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L275) | ✅ | '' 与 NULL 同满足 IS NULL；`col = ''` 三值逻辑 0 行 |
| OR1 | 空串字面量语义 | `or1_empty_string_literal_semantics_on_dual` [tests:269](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L298) | ✅ | `'' IS NULL` 真；`'' = ''` UNKNOWN；NVL 空串按 NULL |
| OR1 | \|\| 拼接传播 | `or1_concat_null_propagates_as_empty` [tests:292](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L343) | ✅ | 方言 `concat` 生成 `\|\|` 连接；NULL/'' 参与拼接均视为空串 |
| OR2 | 未引号折叠 | `or2_unquoted_identifier_folds_to_uppercase` [tests:334](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L387) | ✅ | 真库折叠大写；引号小写名 ORA-00942 |
| OR2 | 引号往返 + 元数据 | `or2_quoted_lowercase_roundtrip_and_metadata_probe` [tests:375](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L474) | ✅ | user_tables 逐字节原样；ALL_TAB_COLUMNS 探测姿势=表名原样+列名 UPPER |
| OR2 | 128 字节上限 | `or2_identifier_length_limit_128` [tests:462](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L508) | ✅ | 128 字节接受；129 字节 ORA-00972 |
| OR2 | 索引生成面 | `or2_add_index_mixed_case_and_reserved_columns` [tests:496](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L641) | ✅（修复后） | 见第 3 节缺口 G-OR1 |
| OR3 | BY DEFAULT + RETURNING | `or3_identity_by_default_and_returning_into` [tests:626](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L731) | ✅ | RETURNING INTO 回填成功；连续插入 id 恰好 +1；显式 id 合法 |
| OR3 | ALWAYS 对照 | `or3_identity_always_rejects_explicit_id` [tests:715](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L768) | ✅ | ORA-32795 拒绝显式 id（sz-orm 不生成 ALWAYS，语义对照面） |
| OR3 | 序列+触发器旧路径 | `or3_sequence_trigger_legacy_path` [tests:755](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L851) | ✅ | 触发器回填 id=1；PL/SQL 坑位登记（见 4.2） |
| OR4 | 仅 offset 分页 | `or4_pagination_clause_offset_only_executes` [tests:834](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L911) | ✅ | `build_pagination_clause` 仅 offset 省 FETCH；窗口 [2,3,4] 断言 |
| OR4 | NULLS 序 | `or4_nulls_first_last_and_default_null_ordering` [tests:894](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1017) | ✅ | ASC→NULLS LAST、DESC→NULLS FIRST（Oracle NULL 恒视作最大）；NULLS FIRST/LAST 透传真库行序正确 |
| OR5 | NUMBER 精度 | `or5_number_precision_roundtrip` [tests:1000](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1114) | ✅ | 30 位整数（超 i64）TO_NUMBER 往返无损；i64::MAX 参数化无损；NUMBER(10,2) 舍入 123.46 |
| OR5 | VARCHAR2 边界 | `or5_varchar2_4000_boundary` [tests:1096](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1173) | ✅ | 4000 字符完整往返；4001 ORA-12899 |
| OR5 | CLOB/RAW/TIMESTAMP | `or5_clob_raw_timestamp_roundtrip` [tests:1155](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1290) | ✅ | CLOB 万字符逐字节；RAW(16) Vec\<u8\> 往返；TIMESTAMP(6) 微秒 |
| OR6 | 三值 NULL + 除零 | `or6_three_valued_null_and_division_by_zero` [tests:1272](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1342) | ✅ | `= NULL` 恒空；NOT IN 含 NULL 全空；除零 ORA-01476（无宽容开关） |
| OR6 | 隐式转换 | `or6_implicit_conversion_varchar2_number` [tests:1324](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1390) | ✅ | '123' 与 NUMBER 命中；混入 'abc' 后 ORA-01722 |
| OR6 | DUAL / FROM-less | `or6_dual_and_fromless_select_semantics` [tests:1368](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1416) | ✅ | **提示词假设过时修正**：见第 5 节 D-OR1 |
| OR6 | DDL 隐式提交 | `or6_ddl_implicit_commit_probe` [tests:1394](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1394) | ✅ | 第二连接交叉验证：DDL 提交先前 DML，ROLLBACK 无效 |

## 3. 三分类清单

### 3.1 sz-orm 方言缺口（已修复）

#### G-OR1：`OracleDialect::build_alter_table` 索引/约束生成面裸拼标识符（P1，已修复）

- **缺口**：[packages/sz-orm-core/src/dialect.rs:1669](../../packages/sz-orm-core/src/dialect.rs#L1669)[must-contain: OR2]（修复前为 `cols.join(", ")` + `name` 裸拼）——AddIndex 列名/索引名、DropIndex 索引名、AddForeignKey 约束名均未经 `quote()`。
- **真库证据（修复前，commit `00760ab6` 留档）**：
  - 混合大小写列 `userName`（quote 建列，区分大小写存储）裸拼建索引 → 折叠为 USERNAME 与引号列不匹配 → **ORA-00904**；
  - 保留字列 `order` 裸拼建索引 → 被解析器吞为关键字 → **ORA-00936**（missing expression）；
  - 对照组：quote 形态 DDL 真库执行成功并按原样登记 user_indexes。
- **修复**：commit `a37937a8`——OracleDialect `build_alter_table` 三臂（AddIndex/DropIndex/AddForeignKey，锚行 `packages/sz-orm-core/src/dialect.rs:1669` [must-contain: OR2]）：AddIndex 索引名+列名全 quote、DropIndex 与 AddIndex 形态一致、AddForeignKey 约束名 quote。
- **单测**：`test_oracle_alter_table_index_fk_quoting` `packages/sz-orm-core/src/dialect.rs:4668` [must-contain: OR2]——3 项生成形态逐字节断言，通过。
- **真连断言（修复后）**：[tests:496](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L496)——混合大小写列/保留字列 AddIndex 真库成功 + user_indexes 登记 + DropIndex 往返后不存在。
- **备注**：MySQL/ClickHouse 方言的同型缺口由并行会话（MySQL 方言矩阵批）同期修复，本报告只认领 Oracle 面。

### 3.2 Oracle 环境特有（登记，不修 sz-orm）

#### E-OR1：空串绑定插入 NOT NULL 列被 ORA-01400 拒绝

- 绑定 `""` 到 NOT NULL VARCHAR2 列在绑定层已转换 NULL → ORA-01400（[tests:169](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L169) 第 2 部分断言）。MySQL/PG 中 `""` 是合法非 NULL 值。**对 sz-orm 的意义**：方言层生成不假设空串语义的佐证；调用方以空串填充 NOT NULL 列时 MySQL/PG 与 Oracle 行为分叉，属应用侧适配责任。

#### E-OR2：DDL 隐式提交（事务管理器影响探针，本批核心登记项）

- 事务中执行 CREATE TABLE 会**隐式提交**此前未提交的 DML，随后 ROLLBACK 无法撤销——经第二连接交叉验证（[tests:1394](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1394)）。与 MySQL/PG 可回滚 DDL 语义相反。
- **对 sz-orm 事务管理器的影响（登记）**：migration/schema_sync 场景在事务中混合 DDL+DML 时，Oracle 上 DML 部分已提交不可回滚。调用方须将 DDL 与 DML 隔离在不同事务；OracleDialect 无 DDL 包裹/延缓机制（真库行为，方言层无法也不应屏蔽）。

#### E-OR3：PL/SQL 触发器体两坑位（or3 旧路径验证中实测）

- `:NEW.` 引用的列名必须与建表引号形态一致：未引号 `id` 折叠为大写 ID 与引号小写列 `"id"` 不匹配 → ORA-04098 触发器重验证失败；
- PL/SQL 匿名块末尾必须 `END;`（带分号）：缺分号 → PLS-00103 end-of-file（经 `user_errors` 视图定位）。
- 证据：[tests:755](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L755)（触发器体改用 `dialect.quote("id")` 后通过）。sz-orm 不生成触发器（登记：旧路径由调用方/DBA 自建）。

#### E-OR4：Oracle 默认 NULL 排序（环境特有）与定点舍入（SQL 标准语义）

- ORDER BY 默认：ASC → NULLS LAST、DESC → NULLS FIRST（NULL 恒视作最大）——**Oracle 环境特有**（MySQL ASC 默认 NULL 最前），[tests:894](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L894) 四组行序断言；
- NUMBER(10,2) 存储舍入：123.456 → 123.46——**SQL 标准语义**（MySQL DECIMAL/PG NUMERIC 同样舍入；红队 R-4 标签精度更正），[tests:1000](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1000)。

#### E-OR5：三值逻辑陷阱（SQL 标准语义）与除零不可配置（Oracle 环境特有）

- `NOT IN (含 NULL)` 恒空集、`= NULL` 恒空（[tests:1272](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1272)）；除零 ORA-01476 无 MySQL sql_mode 类宽容开关；VARCHAR2 数字比较混入不可转换行 ORA-01722（[tests:1324](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1324)）。

### 3.3 文档措辞 / 提示词假设修正

#### D-OR1：提示词 OR6「SELECT 无 FROM 必须补 DUAL」假设在 23ai+ 过时

- 提示词载荷假设 FROM-less SELECT 报 ORA-00923、须落 DUAL。**26ai（23.26.3.0.0）实测 `SELECT 1` / `SELECT :1` 直接返回**——Oracle Database 23ai 新特性（FROM-less SELECT）；19c/21c 才是 ORA-00923。
- 测试改锚定新语义：[tests:1368](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L1368)（DUAL 形态保持可用 + FROM-less 原生支持双断言）。
- **对 sz-orm 的意义**：OracleDialect 无「自动补 DUAL」生成面（经查 `QueryBuilder` 生成的 SELECT 必带 FROM），该假设过时后此缺口不存在；调用方手写标量查询在 23ai+ 无需 FROM DUAL，低版本仍需。

#### D-OR2：`last_insert_id_sql` 返回 None 的注释与实测一致

- [packages/sz-orm-core/src/dialect.rs](../../packages/sz-orm-core/src/dialect.rs) OracleDialect `last_insert_id_sql()` 注释声称 `RETURNING {pk} INTO :bind` 必须附加在 INSERT 之后、不能独立执行——OR3 RETURNING INTO 通道实测（[tests:626](../../packages/sz-orm-core/tests/integration_oracle_matrix.rs#L626)）与该文档注释完全一致，无措辞修正需求。

## 4. 门禁结果（提交前必过清单，本批相关项）

| 门禁 | 结果 | 证据 |
|------|------|------|
| G1 fmt | ✅（红队回执 R-1 更正后） | 初判失实：本批自有 integration_oracle_matrix.rs 含 16 处漂移（红队抓出）→ commit `4d30e1c` rustfmt 单文件清零（仅格式不改语义，rustfmt 解析即证语法有效）；复验 `cargo fmt -p sz-orm-core -- --check` 本文件 0 漂移。剩余漂移（dialect.rs MySQL 段并行会话在途 hunk、query.rs、sz-orm-sqlx、sz-orm-parallel、clonecheap_parity.rs）均归并行会话在途文件，不在本批处置范围 |
| G2/G3 check/clippy | 我的文件 0 警告 | `cargo check -p sz-orm-core --all-targets` 0 warning；`cargo clippy -p sz-orm-core --test integration_oracle_matrix -- -D warnings` 通过（剩余 2 警告位于并行会话 integration_mysql_matrix.rs，归属清晰） |
| G9 SQL 注入 + secrets | ✅（红队回执 R-2 更正后） | `check-sql-injection-auto.py --strict` 46/46 Safe + 自测 15/15 全过。**secrets 更正**：初报「exit 0」系管道取尾命令退出码所致误报，实际 `check-secrets.py` EXIT=1（2 处先在命中：[.github/workflows/integration.yml:97](../../.github/workflows/integration.yml#L97)、[packages/sz-orm-sqlx/tests/integration_szrsql_kill_recover.rs:74](../../packages/sz-orm-sqlx/tests/integration_szrsql_kill_recover.rs#L74)，均 `postgres://postgres:trust@` 本地信任模式占位串；两文件与扫描器末次改动先于本批，归属 szrsql 发布线 fff51ad9，非本批引入）——处置转交 szrsql 维护方（改写连接串形态或登记 trust 占位豁免） |
| G30 幻影测试（仓库级，先在） | ⚠️ 非本批 | 红队回执 R-3：`check-phantom-tests.py` 仓库级 EXIT=1（P3 硬规则 1 处：`packages/sz-orm-cabi/src/lib.rs:2635` 零断言测试，fff51ad9 先在）；本批 integration_oracle_matrix.rs 对该扫描器 0 命中 |
| G12 文档一致性 | ✅（修复先在漂移） | `docs/sz-orm-engineering-practices.md:4` [must-contain: v10.13.4] v10.13.3→v10.13.4（commit `d6465aa4`，漂移先于本批存在）；复跑 PASS |
| G15 幻影交付 | ✅ | `check-phantom-delivery.py` PHANTOM-2 未登记 0 |
| G16 语义反模式 | ✅ | 硬规则 0 / 软规则 7（提示级） |
| G33 文档版本声明 | ✅ | `check-doc-version-consistency.py` 通过 |
| G35 文档链接 | ✅ | `check-doc-links.py` 248 文件 0 死链 |
| G36 CHANGELOG | ✅ | 79 版本头，workspace=10.13.4，通过 |
| 真连回归 | ✅ | `integration_oracle` 既有 10/10（本批前后各一次） |

**并行会话声明**：本批执行期间存在并行会话（MySQL 方言矩阵批）在途修改 `query.rs`/`sql_safety.rs`/`integration_mysql_matrix.rs` 及 `dialect.rs` 的 MySQL/ClickHouse 段。本批全部提交采用 hunk 级隔离（`git apply --cached` 仅staging Oracle hunks），未打包对方半成品改动。

## 5. 提交清单（第 3 环实施批）

| commit | 内容 |
|--------|------|
| `8a83ed3a` | OR1 空串与 NULL 矩阵（含 ORA-01400 NOT NULL 拒绝发现） |
| `00760ab6` | OR2 大小写折叠矩阵 + AddIndex 裸拼缺口真库证据 |
| `9eb4f707` | OR3 主键回填矩阵（RETURNING INTO 通道 + ORA-32795 对照） |
| `384a2caa` | OR4 分页与排序矩阵 |
| `e3074efc` | OR5 类型往返矩阵 |
| `c5ed06a5` | OR6 边缘语义矩阵（DDL 隐式提交探针 + 23ai FROM-less SELECT） |
| `a37937a8` | G-OR1 修复：OracleDialect AddIndex/DropIndex/AddForeignKey 全 quote |
| `a39df388`/`513d7ea6`/`32853a9f` | clippy/fmt 清零（仅 Oracle hunks） |
| `d6465aa4` | G12 先在漂移修复（engineering-practices 版本行） |

## 6. 复评（第 4 环，独立机械验证）

- 每条矩阵结论按维度 grep 复核：测试函数行号逐一核实存在（`grep -n "^fn or[1-6]_"` 19 项全中）；
- 测试套件退出码：`integration_oracle_matrix` 19 passed / 0 failed（真连）；`integration_oracle` 10 passed / 0 failed（真连回归）；`test_oracle_alter_table_index_fk_quoting` 1 passed（单测）；
- 缺口修复闭环：修复前 ORA 证据（commit `00760ab6` 内 expect_err 断言）→ 修复后同载荷转成功断言（commit `a37937a8`）——TDD 式证据链完整。

## 6.5 第 5 环红队独立复核回执

**复核方式**：独立 Agent（非报告作者 session）只读复核，逐项机械验证 + 真连复跑。

**初裁决：FAIL**（2 阻断项）——技术交付本体经机械验证全部为真（19+10 真连全绿、G-OR1 修复正确且 hunk 隔离干净、19 项测试零幻影命中、audit-verify 0 失败、三分类无错分漏报），阻断项均为门禁证据完整性问题：

| # | 级别 | 内容 | 处置 |
|---|------|------|------|
| R-1 | P1 阻断 | 本批自有 integration_oracle_matrix.rs 含 16 处 rustfmt 漂移（跨 OR1~OR6 提交引入；报告 G1「我的段落全清」失实——初判只 grep 了 dialect.rs 漏了测试文件） | ✅ commit `4d30e1c`：rustfmt 单文件清零（仅格式不改语义）+ 报告 G1 行更正 |
| R-2 | P2 阻断 | 报告 G9 secrets「exit 0」不可复现（初判 `echo $?` 取了管道尾命令退出码）——实际 EXIT=1，2 处先在命中（integration.yml:97 + szrsql_kill_recover.rs:74，`postgres://postgres:trust@` 本地信任占位串，归属 szrsql 发布线 fff51ad9，先于本批） | ✅ 报告 G9 行更正为如实披露；修复归属 szrsql 维护方（改写连接串或登记 trust 占位豁免） |
| R-3 | P2 | 门禁 30 幻影测试仓库级 EXIT=1（P3 零断言测试 packages/sz-orm-cabi/src/lib.rs:2636 [must-contain: test_pool_new_szrsql_dsn]，fff51ad9 先在；本批测试文件 0 命中） | ✅ 报告门禁表登记先在状态 |
| R-4 | P2 | E-OR4 舍入/E-OR5 三值逻辑子项实为跨引擎 SQL 标准语义，「Oracle 环境特有」标签不精确 | ✅ 标签更正（处置「登记不修」不变） |
| R-5 | P2 建议 | or6 探针可加对照组排除驱动 autocommit 泄漏替代解释 | ✅ commit `4d30e1c`：新增对照组（无 DDL 时 ROLLBACK → 第二连接 0 行断言） |
| R-6 | P2 登记 | G-OR1 修复仅在 main，10.13.4 已发布 crates.io——须开新版本线承载修复 | ✅ 第 6 环按 CHANGELOG [Unreleased] 登记（与同期 MySQL 批同模式：修复随下版发布，crates.io 走标准流程） |
| R-7 | 备注 | 测试注释中 dialect.rs 行号引用因并行提交移位漂移 | ✅ commit `4d30e1c`：5 处脆弱行号改函数名定位 |

**裁决演进**：红队明示「阻断项均为机械可修，修复后本复核结论可升级为 PASS，无需重跑真连套件」；R-1/R-5/R-7 修复后真连套件仍 19/19（`4d30e1c` 提交前实测）。

## 7. 遗留与建议（非缺陷）

1. **构建绑定面真连**（建议，未纳入本批）：`sz-orm-sqlx` 统一池的 Oracle 通道（`OracleConnectionFactory`/`OraclePoolHandle`）真连矩阵建议另批执行（提示词环境面已含 `unified_pool.rs` 通道说明）；本批聚焦 sz-orm-core 方言生成面。
2. **窗口函数/分析函数生成面**：OracleDialect 尚无 OVER 子句生成面，矩阵未覆盖（sz-orm 无该 API，非缺口）。
3. **23ai 布尔类型**：Oracle 23ai 新增原生 BOOLEAN 类型，`map_to_oracle_type` 将 BOOLEAN 映射 NUMBER(1)（`packages/sz-orm-core/src/dialect.rs:1419` [must-contain: BOOLEAN]）仍为正确选择（26ai Free PDB 默认兼容级未启用 23.0.0 兼容行为时 BOOLEAN 可用性依赖 compat 设置），保持现状。
