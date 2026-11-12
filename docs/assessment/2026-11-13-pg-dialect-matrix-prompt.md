# 新会话提示词：sz-orm PostgreSQL 方言矩阵真连验证（v1，2026-11-13）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：sz-orm v10.13.4。方言矩阵模式已在 MySQL（MY1~MY6+MY7~MY10）、Oracle（OR1~OR6+OR7~OR10）落地并六环闭环。PG18 此前在 SzRSQL 差分中担任**仲裁基准**（每条载荷间接执行过），但那批载荷按 szrsql 探针设计——本批按 MY/OR 模板补齐 **sz-orm PGDialect 自身生成面**的独立矩阵。
> **定位**：查 sz-orm 方言缺口（生成 SQL 真库拒绝/结果错位）。MySQL/Oracle 批各自独立发现过 G1（where_eq(Null) 渲染）/G2（约束名 quote）/G4（quote 上限）同型缺口——PG 生成面未矩阵化的盲区同样需系统过一遍。
> **六环闭环**：按评测交付闭环链条执行。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：以 PG 方言能力面为载荷做真连矩阵验证，产出三分类清单（**sz-orm 方言缺口**[修复]/ **PG 环境特有**[登记]/ **文档措辞**），并修复 sz-orm 侧可修复项。

**目标仓库与环境**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4）
- 被测：PostgreSQL 18.4 @ `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`（本机常驻，psql 在 `D:\PostgreSQL\18\bin`）
- 方言层：`packages/sz-orm-core/src/dialect.rs`（PostgreSqlDialect——Szrsql/Kingbase/GaussDB 委托基准，**本批修复惠及下游委托族**）
- 通道：`packages/sz-orm-sqlx`（SqlxPg 通道）+ `sz-orm-core` 直连；既有 `integration_pg.rs` 22 项功能测试不重复

## 环境探活（第一步）

```bash
export PATH="/d/PostgreSQL/18/bin:$PATH"
PGPASSWORD=test123 psql -h 127.0.0.1 -p 5432 -U postgres -d sz_orm_test -tA -c "SELECT version();"
# 另注意 Docker 内 pg-diff 容器（25432 端口）——本批用 5432 宿主实例，勿混淆
```

## 评测矩阵（PG 方言特有面；每载荷 = 方言层生成 + 真库执行 + 行为断言三步）

| # | 能力块 | 载荷（方言生成面） | 重点 |
|---|--------|-------------------|------|
| PG1 | 标识符 | 双引号 quoting（方言 `quote()`）；大小写**保留**语义（PG 未加引号折叠小写——与 Oracle 相反）；保留字表列名；63 字节标识符上限 | MY 批 G2/G4 同型：约束名/索引名/FK 臂是否全 quote；63 上限探针 |
| PG2 | 主键回填与 RETURNING | BIGSERIAL/IDENTITY + `INSERT..RETURNING id` 全家（INSERT/UPDATE/DELETE）；`build_batch_upsert` 生成面 | RETURNING 通道在 sqlx Pg 通道的列映射；批量 upsert 的 `ON CONFLICT` 生成 |
| PG3 | Upsert | `ON CONFLICT (col) DO NOTHING / DO UPDATE SET`（方言生成）；冲突目标缺失形态（依赖约束推断）| 生成 SQL 真库执行 + 冲突行为断言；DO UPDATE 的 `excluded.` 引用 |
| PG4 | 类型与 JSONB | `::` 双冒号 cast；JSONB `->> / @> / ?` 操作符生成面；数组类型（INT[]/TEXT[]）参数往返；NUMERIC 38 位精度 | sqlx Pg 通道数组参数绑定（历史 D5 占位符同源面）；JSONB 往返不丢键序语义 |
| PG5 | 索引与约束 | 部分索引（WHERE）/表达式索引（lower(col)）/EXCLUDE 约束/GIN（JSONB）；`CREATE INDEX IF NOT EXISTS`/CONCURRENTLY（方言是否生成——CONCURRENTLY 不能在事务内！） | G2 同型扫描 + **CONCURRENTLY 事务冲突**探针（migration/schema_sync 场景） |
| PG6 | 查询面 | DISTINCT ON / ILIKE / NULLS FIRST/LAST / `FETCH FIRST n ROWS ONLY` / CTE（含 RECURSIVE）/ 窗口函数 | 方言透传完整性；MY 批 B4 CTE builder 的 PG 对称验证 |
| PG7 | 边缘与并发 | 三值 NULL；除零报错；空串≠NULL；`FOR UPDATE SKIP LOCKED`；LISTEN/NOTIFY（szrsql 曾缺——PG 真库必须过，sz-orm PgListener 通道）；事务内错误 aborted 语义 | LISTEN/NOTIFY 行为级；SKIP LOCKED 生成面（sz-pay 队列场景前置） |

**不纳入**（登记理由）：流复制/逻辑订阅（运维面）、FDW、表继承（ Legacy 特性，sz-orm 不生成）、性能基准（sz-orm-bench 已覆盖）。

## 硬性要求

- **语义基准**：PG 18 官方语义 + 真库实测（PG 即自身基准，无仲裁库）
- **sz-orm 方言缺口**：修复 `dialect.rs` PostgreSqlDialect + 单测 + 真连断言；修复须评估对 **Szrsql/Kingbase/GaussDB 委托族**的影响（delegate 宏转发链），委托族回归随批跑
- **参数化铁律**（门禁 9）；零 unsafe、零 todo!
- **测试产物**：新增 `packages/sz-orm-core/tests/integration_pg_matrix.rs`（env 注入跳过模式）
- **提交纪律**（硬约束 13）；串行 cargo；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / clippy -D warnings / fmt 72 包全绿
2. `cargo test -p sz-orm-core --test integration_pg_matrix -- --ignored`：PG1~PG7 真连全过
3. `cargo test -p sz-orm-core --test integration_pg -- --ignored`：既有 22 项不回归；`integration_szrsql_matrix` 15 项不回归（委托族）
4. 三分类清单：每条附证据（生成 SQL + 真库输出 + `dialect.rs` file:line）
5. 门禁：G9（--strict + 自测 15/15）/G12/G15/G16/G37/G33/G35/G36 全过
6. 报告 `docs/assessment/2026-11-13-pg-dialect-matrix.md`，audit-verify 0 失败；六环登记（CHANGELOG/AGENTS/commit）

## 环境坑位速查

- git 在 `D:\Git`；`cargo fmt --all` 用 `bash scripts/check-fmt-all.sh`
- 宿主 PG18 在 5432；Docker pg-diff 容器占 25432——别跑错实例
- CONCURRENTLY 类载荷须在**事务外**执行（真库限制），测试内注意连接事务状态
- 凭据不入库（本批 PG 密码已在 AGENTS.md 明文属历史遗留，报告引用一律写环境变量/DSN 变量名）
- 临时脚本放 `D:\tmp`；`python -I` 运行
