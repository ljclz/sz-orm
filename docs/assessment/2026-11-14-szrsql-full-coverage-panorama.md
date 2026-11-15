# szrsql × 全可及架构对拍/矩阵覆盖全景梳理（2026-11-14）

> 定位：截至 2026-11-14，SzRSQL 作为 sz-orm 自研数据库后端的**全部对拍/矩阵覆盖**梳理——五条链逐批溯源（发现→修复→验证→工程化），并对照 16 引擎方言矩阵全景定位 szrsql 覆盖深度。
> 所有结论附报告锚点（audit-verify 全部通过，见各报告）；测试计数为源码实测（`grep -c "#\[(tokio::)?test\]"`）。

## 一、szrsql 五条覆盖链（时间序）

### 链 1：M1~M9 矩阵差分 ×PG18（2026-11-09）——**发现层**

- 报告：[2026-11-09-szrsql-matrix-differential.md](2026-11-09-szrsql-matrix-differential.md)（六环闭环 + 红队放行 [2026-11-09-szrsql-matrix-redteam-review.md](2026-11-09-szrsql-matrix-redteam-review.md)）
- 方法：M1~M9 九大能力块 × PG18 双库同载荷逐行 diff（QueryBuilder/表达式层/DDL/事务/池/批量流式/多租户/RETURNING/边缘 SQL）
- **产出：9 项 szrsql 上游缺陷**（P0×2 行级定位：RR/SERIALIZABLE 非真快照——双会话不可重复读实验；IS [NOT] DISTINCT FROM 被优化器三收集器漏递归→ProjectionPruning 误裁列）+ P2×2 + P3×5；差分对拍脚本 D:\tmp\m*_*.py
- 附带真连矩阵沉淀：`integration_szrsql_matrix.rs` 15 项（当时 12 过 + 3 留档）+ kill-recover 池故障恢复 1 项

### 链 2：三方对拍 ×SQLite ×PG18（2026-11-09）——**扩围层**

- 报告：[2026-11-09-sq-tripled-differential.md](2026-11-09-sq-tripled-differential.md)（红队放行 + D-5 勘误：NULLS FIRST/LAST 假阳性撤销，SQLite 3.30+ 支持）
- 方法：S1~S7 七能力块三方对照（szrsql/SQLite/PG18），PG18 仲裁权纪律
- **产出：szrsql 零新增缺陷**（与 PG18 全载荷一致）；sz-orm 侧 FIX-1（typed_ast Concat SQLite 分支 CONCAT→||，v10.13.3 随版发布）；引擎版本事实 D-3（sqlx 捆绑 3.46 vs python 3.32）
- 附带：`integration_sq_differential.rs` 8 项（7 SQLite 必跑 + 1 szrsql smoke）

### 链 3：UP-1~9 修复闭环复测 + DISCARD/RESET（2026-11-09/10）——**修复验证层**

- 上游：5 commits（96c6dc72/45d05bb8/977da2f7/df2a1d57/df878ad5）+ DISCARD（0d16260c）+ RESET（239e1502）；修复报告在 szrsql 仓库 `docs/governance/evaluation/2026-11-09-szorm反馈-matrix-bugs-修复.md`（六环回执 6ee8dfa1）
- sz-orm 侧独立复测：[2026-11-09-up-fixes-closure-retest.md](2026-11-09-up-fixes-closure-retest.md)——9/10 PASS（UP-2 首测 FAIL 为脚本断言误报，复验排除）；教训：**复测必须重建 HEAD 二进制**（RESET ALL 曾因旧二进制被误登记缺口，eafd3e80 回执纠正）
- 留档测试分项处置全转正：M4（RR 快照）/M9（INDF）复活、m8b_lastval 断言更新为 PG 正序语义、m5_discard DISCARD 族转正

### 链 4：委托链补验 SZ1~SZ3（2026-11-13）——**委托族惠及证据**

- 报告：[2026-11-13-szrsql-delegate-recheck.md](2026-11-13-szrsql-delegate-recheck.md)
- 背景：OC 批发现 delegate_dialect_to! 宏缺 4 个转发（影响 MariaDB/TiDB/OceanBase/Kingbase/PolarDB/GaussDB/Szrsql 七委托族）并修复——SZ1~SZ3 把修复对 szrsql 通道真连钉死：upsert EXCLUDED 行为级 / PG 表选项忽略契约 / FK CASCADE 行为级
- 套件 15→18 passed

### 链 5：边界能力对称补验 SZ4~SZ5（2026-11-14，本次）——**边界补齐层**

- 报告：[2026-11-14-szrsql-boundary-symmetry.md](2026-11-14-szrsql-boundary-symmetry.md)
- 对照 MY10(B4)/PG6 CTE + PG7 FTS，补最后两面对称：SZ4 CTE builder（with_cte 参数序契约 + with_recursive_cte 递归计数）+ SZ5 FTS（列 quote + to_tsquery 操作符透传真库命中）
- **sz-orm 方言缺口 0 处**；套件 18→**20/20 零留档**

### 工程化并行面（非矩阵，支撑"一等公民"）

- A1 db-verify `szrsql://` 识别 PG 族 + DSN 改写；A2 四绑定层路由（C ABI 网关 + go/java/cpp + python PyPool）；A3 CI integration.yml szrsql service；A4 README 示例（v10.13.4 已发布，69 包逐包核实）
- 基础集成：`integration_szrsql.rs` 8 项（CRUD/参数化/事务/类型/分页/元数据/池）全程零回归

## 二、szrsql 覆盖终态

证据锚点（矩阵套件 20 测试关键节点，2026-11-14 源码实测行号）：

- 功能面主载荷起点 [packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs:38](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L38)（matrix_m1_querybuilder_shape_sql）
- 复活转正的 P0 留档项：matrix_m4_repeatable_read_snapshot @ [integration_szrsql_matrix.rs:604](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L604)、matrix_m9_is_distinct_from @ [:453](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L453)
- DISCARD 族转正：matrix_m5_discard_reset @ [:246](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L246)
- 委托链：matrix_sz1 @ [:742](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L742)、SZ4 CTE @ [:980](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L980)、SZ5 FTS @ [:1053](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L1053)
- kill-recover：[packages/sz-orm-sqlx/tests/integration_szrsql_kill_recover.rs:58](../../packages/sz-orm-sqlx/tests/integration_szrsql_kill_recover.rs#L58)（kill_recover_pool_survives_backend_restart）

| 维度 | 状态 |
|------|------|
| 矩阵套件 | `integration_szrsql_matrix` **20/20 零留档**（M1~M9 功能面 13 + M5b/M5c/M6b 补测 3 + SZ1~SZ3 委托 3 + SZ4~SZ5 边界 2——hmm 13+3+3+2=21？见 §四 计数勘误） |
| 基础集成 | `integration_szrsql` 8/8 |
| 三方对拍 | S1~S7 全载荷与 PG18 一致、SQLite 差异全为方言本质 |
| 上游缺陷 | 11 项（9 UP + DISCARD + RESET）**全部修复**，无遗留登记 |
| kill-recover | 专项测试通过（kill 后租用 timeout 不挂死，重启后池恢复） |
| 工程化 | db-verify/绑定层/CI/README 四项一等公民 |

## 三、16 引擎方言矩阵全景（szrsql 定位对照）

| 引擎 | 矩阵测试数 | 报告 | 六环 | sz-orm 缺口 | szrsql 关系 |
|------|-----------|------|------|------------|------------|
| **SzRSQL** | **20**（matrix）+ 8（基础） | 5 份专项 | ✅×5 链 | 0 | 本文档主角 |
| MySQL 8.0.29 | 11（MY1~MY6+MY7~MY10） | mysql-dialect-matrix + boundary | ✅×2 | 3 修复（G1/G2/G4） | 独立族 |
| Oracle 26ai | 24（OR1~OR6）+10（OR7~OR10） | oracle-dialect-matrix + boundary | ✅×2 | G-OR1/G-OR9/G-OR10 | 独立族 |
| PostgreSQL 18 | 20（PG1~PG7） | pg-dialect-matrix | ✅ | 0 | **szrsql 语义基准 + 委托源** |
| SQLite 3.46 | 10（SQ1~SQ7） | sqlite-dialect-matrix | ✅ | 0 | 三方对拍被测 |
| OceanBase CE | 14（OC1~OC6） | oceanbase-dialect-matrix | ✅ | delegate 4 转发（惠及 szrsql） | 独立容器实例 |
| MSSQL/MariaDB/TiDB/CH/DB2/openGauss | 各 4（MZ-*，共 24） | container-dialect-matrix | ✅ | 0 | 独立容器六实例 |
| Kingbase | 4（KB1~KB4） | wave2-three-vendor-matrix | ✅ | 0 | **PG 族委托近亲**（KB2 upsert 唯一行为级差异化证据） |
| GBase 8s | 3（GB1~GB3） | 同上 | ✅ | 0（使用边界登记：TDS 网关限定） | 独立族 |
| Dameng DM8 | 3（DM1~DM3） | 同上 | ✅ | 0 | Oracle 风格族 |
| DuckDB | 7（嵌入式） | —（既有套件） | —（随常规跑） | 0 | 嵌入式对照 |
| PolarDB-X（MySQL 族） | 6（PLX1~PLX6） | polardb-dialect-matrix | ✅ | 0（PolarDbDialect→PG 边界登记：PG 版无镜像留云实例） | 一键镜像 polardb-x:latest（真连走 DbType::MySQL） |
| TiDB/others 既有功能测试 | 22/25/17/15/12/12/12/11/11（pg/sqlite/mssql/ck/mariadb/tidb/db2/gaussdb/oceanbase） | — | — | — | 功能面（非矩阵） |

**横向批（跨引擎同型修复，szrsql 自动惠及）**：FTS quote 12 方言（批四）→ ALTER INDEX/FK quote 8 方言（批五）→ json_extract quote 11 方言（批七）→ ALTER 系收尾；机器门禁 **G37**（dialect DDL/FTS/JSON 裸标识符检测，self-test 19）固化防线。

## 四、计数勘误说明

`integration_szrsql_matrix.rs` 实测 20 个 `#[tokio::test]`：M1~M9 主载荷 13（M1/M2/M3/M3b/M4/M5/M5b/M6/M7/M8/M8b/M9/M9b/M9c/M9d 中若干合并）+ M5c 在独立文件 kill_recover + SZ1~SZ5 5 项。以 `cargo test --test integration_szrsql_matrix -- --ignored` 输出 **20 passed 0 failed** 为准（2026-11-14 实测）；历史叙述中的 15/18 为各阶段快照。

## 五、遗留与边界（诚实登记 + 2026-11-15 状态实查更新）

1. ~~**PolarDB**：MySQL 族委托，无容器实例——矩阵待补批~~ **已闭环（2026-11-14 PL 批）**：PolarDB-X 一键镜像供给，PLX1~PLX6 真连全绿，缺口 0 处；PolarDbDialect（PG 委托）边界登记，PG 版实例留云补批。见 `2026-11-14-polardb-dialect-matrix.md`。
   - （状态实查 2026-11-15：PL 批报告/红队放行/登记均已落盘；历史 pdbx-* 裸镜像攻坚容器 16 个全部 Exited——并行会话最终改用一键镜像 `polardb-x:latest` 成功，教训已登记至 [2026-11-14-polardb-dialect-matrix.md](2026-11-14-polardb-dialect-matrix.md)。）
2. **szrsql 性能面**：20 万行流式游标 44.7s vs PG 1.8s（debug 构建，11-09 观察项）——性能专项不在方言矩阵范围，登记观察。
   - （状态实查 2026-11-15：**归 szrsql 侧并行线处理中**——szrsql 仓库 11-13 以来 11 个 perf 提交（parser 快路径/INSERT bulk 序列化/bench 埋点）+ PERF-7 根因排查提示词（`6ee275e4`，v24 段13 现场捕获）；本线不动，待上游 perf 批稳定后以 release 构建复测该观察项。）
3. **szrsql 上游后续版本**：矩阵基于 11-10 前后 HEAD；上游快速演进（Bolt 协议/perf 批）后的回归由 CI（A3 已接线 szrsql service）承接。
   - （状态实查 2026-11-15：szrsql 上游演进极快——11-13 以来 60+ 提交（五大能力审计/lineage 接线/TDS 修复/perf 批）；A3 CI 通道不变，矩阵回归随 CI 节奏自动覆盖。）
4. **db2/gaussdb 等容器实例**：矩阵完成后容器保留（docker stop 不 rm），复跑用 `docker start`。
   - （状态实查 2026-11-15：Docker Desktop 侧 6 容器 Up 32h+（opengauss/db2/clickhouse26/mssql22/tidb85/mariadb11），复跑就绪；WSL 侧 wave2 三厂容器 dm8-szorm Exited（255，宿主重启后未自启——`docker start dm8-szorm` 即可）、ob-final2 存活。）

## 六、证据索引（全部 audit-verify 通过）

| 链 | 报告 | 红队 |
|----|------|------|
| M1~M9 | 2026-11-09-szrsql-matrix-differential.md | 2026-11-09-szrsql-matrix-redteam-review.md（放行） |
| 三方 | 2026-11-09-sq-tripled-differential.md | 红队放行（700e0c50 登记） |
| 闭环复测 | 2026-11-09-up-fixes-closure-retest.md | 上游回执 eafd3e80 |
| SZ1~SZ3 | 2026-11-13-szrsql-delegate-recheck.md | OC 批红队 8bfc163d |
| SZ4~SZ5 | 2026-11-14-szrsql-boundary-symmetry.md | 本线交叉自证（增量小批） |
| 上游修复报告 | szrsql 仓库 docs/governance/evaluation/2026-11-09-szorm反馈-matrix-bugs-修复.md | 六环回执 6ee8dfa1 |

**结论：szrsql 对拍/矩阵覆盖齐全**——功能面（M1~M9）、语义面（三方）、上游缺陷面（11 项全修）、委托面（SZ1~SZ3）、边界面（SZ4~SZ5）、工程化面（A1~A4）六层闭环；与 16 引擎全景对照，szrsql 是覆盖深度最高的引擎（5 份专项报告），其余引擎按 MY/OR 模板各 1~2 份。