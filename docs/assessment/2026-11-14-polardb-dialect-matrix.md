# PolarDB 方言矩阵真连验证（PL 批，2026-11-14）

> 六环闭环：环 1 提示词 `2026-11-14-polardb-dialect-matrix-prompt.md`（commit c062c9f0）→ 本报告（环 2）→ 实施批（环 3，commit fe3161fa）→ 复评（环 4）→ 红队独立复核（环 5）→ 登记（环 6）。

## 0. 批次背景与先决裁定

方言矩阵二波补批候选 **PolarDB**（`DbType::PolarDB`，`PolarDbDialect`）。供给探查结论改变了载荷形态：

**供给事实（第 5 环红队重点复核对象）**：
1. `PolarDbDialect` 委托 `PostgreSqlDialect`（[packages/sz-orm-core/src/dialect.rs:2193](../../packages/sz-orm-core/src/dialect.rs#L2193)），面向**阿里云 PolarDB for PostgreSQL**。
2. 社区容器生态**无 PolarDB for PostgreSQL 镜像**（docker hub + mirror 通道均无；此前 `docker search` 假阴性教训再次确认）——PG 语义通道本机不可真连。
3. 可自动供给的是 **PolarDB-X（MySQL 协议族）一键镜像** `polardbx/polardb-x:latest`（内含 GMS/DN/CN/CDC 全组件）。真连通道经 **`DbType::MySQL`**（MySqlDialect）直连其 MySQL 协议端口。

**裁定**：本批拆为两件事，不混写——
- **A 件（真连）**：PolarDB-X 作为 MySQL 兼容目标库的兼容性矩阵（`DbType::MySQL` 生成面）。
- **B 件（边界登记）**：`PolarDbDialect`（PG 委托）适用边界静态登记 + 文档精确化。

这与二波 GBase 批「GBaseDialect→SqlServerDialect 委托仅 TDS 网关企业实例可执行，纯 Informix 社区版不可执行」的边界登记同型。

## 1. 实例供给（A 件前置）

| 项 | 值 |
|---|---|
| 镜像 | `polardbx/polardb-x:latest`（一键镜像，mirror 通道拉取，约 3.6GB） |
| 容器 | `pdbx-1stop`（hostname 同名，单容器 play 模式） |
| 端口 | 宿主 `18506` → 容器 `3306`（CN MySQL 协议；容器内经 `mysql -h127.1 -P8527` 访问） |
| 凭据 | `polardbx_root` / 无密码（引擎自报 `5.6.29-PXC-5.4.19-SNAPSHOT`） |
| 引擎事实 | 单表模式 `PARTITION BY` 被拒（PXC-4500）；`ON DUPLICATE KEY UPDATE` 生效；`SHOW CREATE TABLE` 落 InnoDB/utf8mb4_general_ci |
| 环境坑 | 容器 init 脚本含 stop-all 分支会随机退出（Exit 255 非 OOM）——每次跑套件前必须守门探活循环；Rust 测试侧 env 门控双保险 |

## 2. A 件：PolarDB-X 真连矩阵（PLX1~PLX6）

新增 `packages/sz-orm-core/tests/integration_polardbx.rs`（env 门控 `SZ_ORM_POLARDBX_URL`，未配置自动跳过不假红）。

| # | 载荷 | 结果 | 证据 |
|---|------|------|------|
| PLX1 | 引擎身份：`SELECT VERSION()` 含 `PXC`；方言建表产物 `SHOW CREATE TABLE` 落 InnoDB | ✅ | packages/sz-orm-core/tests/integration_polardbx.rs:158（`async fn matrix_plx1_engine_identity`） |
| PLX2 | 反引号 quoting + IF NOT EXISTS 幂等 + 保留字列名（`order`）读写往返 | ✅ | packages/sz-orm-core/tests/integration_polardbx.rs:209（`async fn matrix_plx2_ddl_quoting_idempotent`） |
| PLX3 | upsert `ON DUPLICATE KEY UPDATE` 行为级：首插分支 + 命中更新分支读回验证 | ✅ | packages/sz-orm-core/tests/integration_polardbx.rs:273（`async fn matrix_plx3_upsert_behavior`） |
| PLX4 | QueryBuilder ORDER BY + LIMIT/OFFSET 行为级（多键排序语义锚定） | ✅ | packages/sz-orm-core/tests/integration_polardbx.rs:343（`async fn matrix_plx4_pagination_order`） |
| PLX5 | 注入惰性真库行为级：SQL 元字符 payload 精确命中 1 行、表不被破坏 | ✅ | packages/sz-orm-core/tests/integration_polardbx.rs:417（`async fn matrix_plx5_injection_inert`） |
| PLX6 | 单表模式 `PARTITION BY` 被拒探针（PXC-4500）——引擎边界登记，非方言缺陷（sz-orm MySQL 方言不生成 PARTITION BY） | ✅ | packages/sz-orm-core/tests/integration_polardbx.rs:498（`async fn matrix_plx6_partition_by_rejected`） |

**套件输出**：`test result: ok. 6 passed; 0 failed`（cargo test -p sz-orm-core --test integration_polardbx -- --ignored，2026-11-14）。
**门控验证**：env 未配置时 `0 passed; 6 ignored; 0 failed`（不假红）。

**sz-orm 方言缺口：0 处**——MySqlDialect 生成面（quote/upsert/分页/参数化）在 PolarDB-X 上全部真库验证通过，与 MY 矩阵（MySQL 8.0.29）结论一致。

## 3. B 件：PolarDbDialect 边界登记（静态）

| 项 | 内容 | 证据 |
|---|------|------|
| dialect.rs 注释 | 适用边界：仅 PolarDB for PostgreSQL；PolarDB-X 用户选 `DbType::MySQL` | packages/sz-orm-core/src/dialect.rs:2190-2192（`// 适用边界（2026-11-14 PolarDB-X 矩阵批登记）`注释块） |
| 使用指南 | PolarDB 行从「PG/MySQL 兼容，$1/?，双引号/反引号」精确化为「PG 兼容（PolarDB for PostgreSQL）；PolarDB-X（MySQL 族）请选 DbType::MySQL，$1，双引号」 | docs/sz-orm使用指南.md:2173（表格行 `| PolarDB | PG 兼容…`） |
| 委托正确性 | 既有单测覆盖（quote 继承/RECURSIVE 关键字/db_type）——未改动，零回归 | dialect.rs 测试模块（lib 3294 全过佐证） |

## 4. 门禁记录

| 门禁 | 结果 |
|---|---|
| G1 fmt | ✅（cargo fmt -p sz-orm-core 过） |
| G3 clippy -D warnings | ✅（顺带抓出 integration_dameng.rs:401（packages/sz-orm-core/tests/integration_dameng.rs:401，`let r1 = run_dm_sql(...)`）useless_format 历史遗留——越批修复，hunk 级隔离） |
| G9 SQL 注入扫描 | ✅ 46/46 Safe |
| G37 dialect quoting | ✅ 0 命中 |
| G12 文档一致性 | ✅ PASS |
| G14 文档同步 | ✅ SYNCED |
| lib 全测 | ✅ 3294 passed |
| 矩阵真连 | ✅ 6 passed |

## 5. 「建议」与「缺陷」分列

**缺陷（sz-orm 侧）**：0 项。

**引擎/环境事实（登记，非缺陷）**：
1. PolarDB-X 单表模式拒绝 `PARTITION BY`（PXC-4500）——分布式特性需按官方语法建分区表；sz-orm 不生成该子句，无影响。
2. 一键镜像容器 init 脚本含 stop-all 退出分支——CI 编排须 restart 策略 + 探活守门。
3. 容器内 MySQL 协议端口为 8527（`mysql -h127.1 -P8527`），对外映射经 3306→宿主 18506；测试经宿主端口访问。

**建议（择机项）**：
1. PolarDB for PostgreSQL（PG 版）真连验证：需云实例或官方镜像出现后补批（登记在案）。
2. 若后续支持 `szorm://` 之外的新 scheme（如 `polardb://`），须同步 `DbType::from_str`/default_port 语义——当前无需求。

## 6. 与既有批次的关系

- 依赖 MY 矩阵载荷形态（MY1~MY6 模式复用），PLX 载荷为精简版（6 vs 11 项，聚焦兼容性锚定而非方言能力补齐——PolarDB-X 不是新方言族）。
- 边界登记模式与 GBase 批同型；env 门控与 szrsql 惯例一致。

## 7. audit-verify 自证

```
$ bash scripts/audit-verify.sh docs/assessment/2026-11-14-polardb-dialect-matrix.md
==========================================
  验证结果
==========================================
  ✅ 通过: 8
  ❌ 失败: 0
  ⚠️  警告: 0（仅弱语义提示级别提示，非引用缺失）
==========================================
✅ 审计证据验证通过
```

（初跑 8 条「仓库根未找到」警告系引用采用 `../../` markdown 相对链接形态、audit-verify v4 re_any 捕获组截到纯文件名所致——既有脚本不解析相对前缀（resolve_file 无 ../ 规则）。引用改写为仓库相对裸路径 `packages/...:NN` 形态后复跑 0 失败。）
