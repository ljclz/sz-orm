# OceanBase 方言矩阵真连验证（OC1~OC6）——2026-11-13

> 六环闭环批次。容器化二波登记（`2026-11-13-container-dialect-matrix.md` §B9）首个可自动供给实例。模式沿用 PG/SQLite/MySQL 方言矩阵；**本批发现并修复 sz-orm 方言缺口 1 处（委托宏转发缺漏，影响 7 个委托方言族）**。
>
> 被测引擎：**OceanBase CE 4.4.2.1**（MySQL 模式，`ob_compatibility_mode=MYSQL` 实测），容器 `ob-final2`，宿主端口 12881，租户用户 `root@szorm`。矩阵断言以真库实测为准。

## 0. 供给边界（二波四实例评估结论，登记）

| 实例 | 供给形态 | 结论 |
|------|---------|------|
| OceanBase CE | Docker Hub 公开镜像（2.05GB） | ✅ 本批供给并验证 |
| KingbaseES | 官网 docker tar，需验证码表单 | ❌ 人工供给门槛，留待补批 |
| Dameng DM8 | eco 文档站无公开 docker 入口（仅 Win/Linux 安装包） | ❌ 人工供给门槛，留待补批 |
| GBase 8s | 官网开发版需登录下载 | ❌ 人工供给门槛，留待补批 |

## 1. 环境事实（重要坑位，登记）

- **daemon 模式容器死亡循环**：entrypoint `start.sh` 在 boot success 后约 2 分钟容器退出（exit 255，`OOM=false`、cgroup `memory.events` 全零、内核 dmesg 无 OOM——非资源问题，疑似 entrypoint 脚本退出语义与容器 daemon 模式交互缺陷）；`--restart=always` 兜底自动拉起，每次拉起后 observer 需约 2~4 分钟可连（`ERROR 8001 Server is initializing` → 可连）。**跑测试必须守门探活**（循环 `SELECT 1` 至 ALIVE 再触发套件）；活窗口约 2~4 分钟，单线程 14 项 5.4s 可全过。既有 3 项 `setup_pool` 无守门（30s acquire 超时）会撞死点报 PoolTimedOut——**该失败模式是环境产物，不是方言缺陷**（同套件守门后全过可证）。
- 连接 URL：`mysql://root%40szorm:<pw>@127.0.0.1:12881/sz_orm_test`（`root@szorm` 租户用户 `@` 须编码 `%40`）。
- 快照镜像 `ob-szorm-v2:latest`（`docker commit`，含租户密码与 sz_orm_test 库）——容器重建可从快照起。

## 2. 交付物

| 项 | 内容 | 证据 |
|---|------|------|
| 测试 | `packages/sz-orm-core/tests/integration_oceanbase.rs` 追加 `matrix_oc_*` 6 测试（OC1~OC6，MZ 批同型追加既有文件） | `cargo test -p sz-orm-core --test integration_oceanbase -- --include-ignored --test-threads=1` → **14 passed; 0 failed**（2026-11-13 实测，守门探活后） |
| **方言缺口修复** | `delegate_dialect_to!` 宏补 4 个转发：`build_upsert_on_conflict` / `build_create_table_with_options` / `render_table_options` / `build_add_foreign_key`——此前在 7 个委托方言（MariaDB/TiDB/OceanBase/Kingbase/PolarDB/GaussDB/Szrsql 等）上直接落到 trait 默认实现（upsert=None / 表选项被忽略 / FK 走 ANSI 默认），与委托目标真实能力不一致 | `packages/sz-orm-core/src/dialect.rs:2147-2172`；方言单测 261 全过；MY 矩阵 11 项零回归 |
| OC2 载荷修正 | TEXT 列作主键被 OB 拒（1167 can't index column，同 MySQL 引擎约束）——载荷改非主键保留字列 | `packages/sz-orm-core/tests/integration_oceanbase.rs`（OC2 注释） |

## 3. 矩阵结果逐载荷

### OC1 引擎身份契约
`@@version_comment` = `OceanBase_CE 4.4.2.1 (r101000022026050611-...)`（真库身份）；`ob_compatibility_mode=MYSQL`（实测）。方言链：`db_type()=OceanBase` + `is_mysql_family()=true`（`packages/sz-orm-core/src/dialect.rs` DbType 实现）。版本前缀 5.7.25 与 MySQL 8.0.29 差异对纯文本协议面无影响。

### OC2 DDL 生成面
- **MY7 表选项 API**（修复前 OB 上被静默忽略——trait 默认 `render_table_options` 空串；修复后）带 `ENGINE=InnoDB CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci` 建表真库接受，且 `information_schema.TABLES.TABLE_COLLATION` 实测 `utf8mb4_general_ci` 生效。
- quote 往返：保留字列名（`select`）经方言反引号 quote 后可执行。
- **OB 引擎特有（登记）**：TEXT 列不可作主键（错误 1167 "The used storage engine can't index column 'select'"）——与 MySQL 同型约束，载荷修正记录。

### OC3 Upsert 家族
- **修复前缺口行为**：`build_upsert_on_conflict` 在 OceanBaseDialect 上返回 `None`（宏未转发，落 trait 默认）——`build_batch_upsert_with_params` 对 OB/MariaDB/TiDB 等族不可用；**修复后**渲染 `ON DUPLICATE KEY UPDATE \`value\`=VALUES(\`value\`)`，真库冲突更新语义验证（v 10→99）。
- `INSERT IGNORE`（`build_insert_or_ignore_prefix`，宏原有转发）冲突保留原值验证。

### OC4 类型与值面
- `BIGINT UNSIGNED` 顶格（u64::MAX）往返无损；`DECIMAL(38,6)` 20 位整数大数无损（CAST CHAR 读回逐位断言）。
- **UNIQUE 冲突错误码 1062**（"Duplicate entry"）——MySqlDialect 错误映射假设在 OB 上成立（真库实测）。

### OC5 分布式特有
- `SHOW VARIABLES LIKE 'ob_%'` 大量子集可读（分布式内核面存在性）。
- **HASH 分区表 DDL**（`PARTITION BY HASH(id) PARTITIONS 4`，方言生成面之外的 MySQL 兼容语法）真库执行 + 插入 + 计数——**登记为引擎能力**（sz-orm 方言不生成分区子句，调用方以 AssertSqlSafe 形态自管）。

### OC6 SQL 注入防护
- 参数化路径：注入向量（`x' OR '1'='1' --`）作为绑定参数返回空集（惰性，真连）。
- 方言分页产物 `build_pagination`（LIMIT 2 OFFSET 2）真库执行命中 2 行。

## 4. 三分类清单

| # | 分类 | 内容 | 处置 |
|---|------|------|------|
| 1 | **sz-orm 方言缺口（已修复）** | `delegate_dialect_to!` 宏缺 4 个转发（upsert/表选项/表选项渲染/FK 动作）——影响全部 7+ 委托方言族；MariaDB/TiDB 同族既有批未覆盖此面（MY 批矩阵以 MySqlDialect 直测） | `packages/sz-orm-core/src/dialect.rs:2147-2172` 补转发 + MY 矩阵 11 项 + 方言单测 261 零回归；登记 [Unreleased] Fixed |
| 2 | OceanBase 引擎特有 | daemon 容器 ~2min 退出循环（restart=always 兜底）；TEXT 列不可作 PK（1167）；HASH 分区表语法可用但非方言生成面 | 登记 |
| 3 | OceanBase 引擎特有 | `ob_compatibility_mode=MYSQL`；UNIQUE 冲突 1062 与 MySQL 映射一致；`SHOW VARIABLES ob_%` 子集 | 登记 |
| 4 | 文档措辞 | integration_oceanbase.rs 头注释「用 MySQL 9.6 验证兼容性」——本批实际用 OceanBase CE 4.4.2.1 真连，注释应更新 | 本批顺带修正（见 §2 测试文件头） |

## 5. 门禁记录（2026-11-13 实测）

| 门禁 | 结果 |
|------|------|
| G1 fmt（`bash scripts/check-fmt-all.sh`） | ✅ 72 包通过 |
| G2 `cargo check --workspace --all-targets` | ✅ 0 error |
| G3 `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 error（本批 try_pool `.ok()` 形态修正） |
| 矩阵 | ✅ **14/14**（OC 6 + 既有 8，`--include-ignored --test-threads=1`，守门探活后） |
| 回归 MY 矩阵 `integration_mysql_matrix -- --include-ignored` | ✅ 11/11（委托宏改动影响面） |
| 回归方言单测 `--lib dialect` | ✅ 261/261 |
| G9 `check-sql-injection-auto.py --strict` | ✅ 46/46 Safe；自测 15/15 |
| G37 `check-dialect-ddl-quoting.py` | ✅ 0 命中；自测 19/19 |
| G12 doc-consistency / G15 phantom-delivery / G16 semantic-patterns / G33 doc-version / G35 doc-links / G36 changelog-consistency | ✅ 全过 |

> 注：守门探活前旧 3 项（setup_pool 无守门）在死亡窗口会报 PoolTimedOut——环境产物非方言缺陷（§1 登记，同套件守门后 14/14 可证）。

## 6. 结论

- **sz-orm 方言缺口 1 处修复**（委托宏转发缺漏，影响面大于 OB 单族——7 个委托方言族的 upsert/表选项/FK 能力全部修正）；OB 真连行为级验证通过。
- 矩阵 14/14 全绿；MY 基线零回归。
- 二波剩余（Kingbase/Dameng/GBase）供给门槛为人工注册/验证码，登记留待补批（§0）。
