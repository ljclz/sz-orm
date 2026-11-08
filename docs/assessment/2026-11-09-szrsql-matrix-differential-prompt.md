# 新会话提示词：sz-orm 全功能矩阵 × SzRSQL 差分评测（v1，2026-11-08）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：sz-orm v10.13.2 已完成 SzRSQL（v1.0.0-rc.2+，pgwire v3.0）基础集成——`DbType::Szrsql` + PG 方言族委托 + `szrsql://` scheme + 8 项基础真连集成测试（`packages/sz-orm-sqlx/tests/integration_szrsql.rs`，CRUD/参数化/事务/类型/分页/元数据/池）。历史发现：D-5 COPY（sqlx 需 PgCopyIn 专用 API，非 szrsql 缺陷）、D-6 NOTIFY（已由 szrsql `3261c7dd` 修复，PgListener 可用）。
> **定位**：本批为「bug 猎场 + 支持面扩张验收」双重目标——把 sz-orm 全部方言特性打向 szrsql，跑不过的地方要么是 szrsql bug（反馈上游，只读不改），要么是 sz-orm 集成缺口（本批修复）。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师 + 差分评测工程师。任务：以 sz-orm 全功能矩阵为测试载荷，对 SzRSQL 做系统性差分评测（对照本机真实 PostgreSQL 18），产出 bug 清单（分「szrsql 上游缺陷」/「sz-orm 集成缺口」/「双方文档措辞」三类），并修复 sz-orm 侧可修复项。

**目标仓库**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.2，已发布 crates.io，69 包 + cli 同步）
- 差分基准：本机 PostgreSQL 18 `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- 被测：SzRSQL（`D:\www\test\鲜视达\rust\szrsql`，**只读，严禁修改**——ADR-0001 同样适用；HEAD 快速变动，动手前 `git log --format="%h" -1` 记录基线并重建二进制）
- 历史闭环参照：`docs/assessment/2026-11-08-szrsql-integration.md`（§7/§7.1）与 `szrsql/docs/governance/evaluation/2026-11-08-szorm反馈-copy-notify-修复.md`

## 被测环境启动（第一步，未启动自动跳过的测试需先起服务）

```bash
cd /d/www/test/鲜视达/rust/szrsql
CARGO_TARGET_DIR=D:/rust-target cargo build -p szrsql-bin
D:/rust-target/debug/szrsql.exe --port 6543 --auth-mode trust --connection-idle-timeout 0 \
  --data-dir D:/tmp/szrsql-matrix > D:/tmp/szrsql-matrix.log 2>&1 &
# 注意 --connection-idle-timeout 0：评测长测试会被默认 300s 空闲超时切断（实测教训）
# 差分对拍用真库：PG18 已在 5432
```

## 评测矩阵（按 sz-orm 方言能力面逐块过，每块 = szrsql 执行 + PG18 对照执行 + 结果 diff）

| # | 能力块 | 载荷（sz-orm API 面） | 重点 |
|---|--------|----------------------|------|
| M1 | QueryBuilder 生成 SQL 执行 | chained where/orderBy/group/having/join（inner/left）/subquery/union/聚合函数集 | 生成 SQL 在两库的**结果集逐行 diff**（含 NULL 排序语义、聚合遇 NULL/空集） |
| M2 | 表达式层（typed_ast） | ILIKE/DISTINCT ON/CAST/字符串函数（Lower/Upper/Trim/Concat/Substring）/数学函数/Extract(Year…) | PG 特化分支（Szrsql 已纳入 PG 家族 match）是否真被 szrsql 执行器接受 |
| M3 | 迁移与 schema_sync | `MigrationResolver` + `SchemaBuilder` 建表/索引/FK 全流程在 szrsql 端到端跑通；`migration.rs` 迁移锁（advisory lock 语义） | DDL 变体（IF NOT EXISTS/COMMENT ON/部分索引/表达式索引）；迁移锁在 szrsql 是否真生效（双进程并发迁移） |
| M4 | 事务深化 | 隔离级别 SET TRANSACTION（ReadCommitted/RepeatableRead/Serializable）/ 多事务 TransactionManager / savepoint 栈（sz-orm 侧 8 层嵌套）/ panic 安全回滚 | szrsql 隔离级别是真语义还是仅接受语句（RepeatableRead 下不可重复读复现实验） |
| M5 | 连接池行为 | 自研池 acquire/release/session_reset（DISCARD ALL）/ max_lifetime 连接轮换 / 故障连接剔除（中途 kill szrsql 二进制再观察池恢复） | session 复位 SQL 在 szrsql 的兼容性；池健康检查对 szrsql 断连的检出 |
| M6 | 批量与流式 | execute_batch / 批量 INSERT 1000+ 行 / `cursor_stream`（服务端游标流式读）/ prepared statement 缓存命中率路径 | 大结果集（>10 万行）流式读是否稳定；批量参数上限 |
| M7 | 多租户与 N+1 防护 | `connection_tenant` 租户上下文 / `N1QueryDetector` 接入 szrsql 连接 / `#[detect_n_plus_one]` 运行时检测 | 纯 sz-orm 侧，验证在 szrsql 后端下不回归即可 |
| M8 | RETURNING/写后读 | INSERT..RETURNING / UPDATE..RETURNING / last_insert_id（PG 族 lastval 语义）| sz-orm active_model save 后主键回填路径 |
| M9 | 边缘 SQL（bug 猎场主区） | 三值 NULL 边界（`= NULL`/`IS NOT DISTINCT FROM`/聚合遇 NULL）/ 隐式类型转换（int↔text 比较、bool↔int）/ 保留字表名列名（加引号）/ Unicode 标识符 / 超长标识符与参数 / 空串 vs NULL | **与 PG18 对照执行**，diff 即 bug 候选；每条附两库原始输出 |

## 硬性要求

- **只读 szrsql**：发现 bug → 记录（复现 SQL + szrsql 输出 + PG18 输出 + 定位 file:line）→ 绝不改其文件。修复建议写入报告「上游反馈」节，可另生成提示词。
- **差分纪律**：每条 M1~M9 结论必须有**两库对照输出**（同一载荷、同一数据集）；禁止凭单库行为断言"兼容/不兼容"。历史教训（COPY 自听 NOTIFY 偏见）——先对拍真库再下定性。
- **sz-orm 侧修复**：判为 sz-orm 集成缺口的（如方言分支缺 Szrsql、参数改写 bug），修复并补单测；判为 szrsql 缺陷的只记录。
- **测试产物**：新增 `packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs`（矩阵载荷沉淀为 `#[ignore]` 集成测试，`SZ_ORM_SZRSQL_URL` 凭据注入，未启动自动跳过——沿用 integration_szrsql.rs 模式）；对拍脚本可放 `scripts/`（Python，GBK 控制台 UTF-8 reconfigure）。
- 参数化铁律（门禁 9）：所有测试 SQL 参数化；新代码零 unsafe、零 todo!。
- 提交纪律（硬约束 13）：每完成一个能力块矩阵（M1~M9 为独立自洽单元）立即 conventional commit，提交可编译。
- 串行 cargo，禁止并行进程争锁；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`。

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / `clippy -D warnings` / `cargo test --workspace`（含 PYTHONHOME）全绿
2. `cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored`：矩阵测试真连全过（或：已知 szrsql 缺陷项标注 `#[ignore = "szrsql 上游 <bug-id>"]` 留档）
3. `cargo test -p sz-orm-sqlx --test integration_szrsql -- --ignored` 原有 8 项不回归
4. 差分对拍：M1~M9 每块附 szrsql vs PG18 对照输出（或明确说明为何该块无法对拍）
5. 门禁：G9（--strict）/G12/G15/G16/G18/G19/G33/G35/G36 + pytest ≥160 passed
6. bug 清单三分类，每条附证据（复现步骤 + 两库输出 + file:line）

## 输出要求

1. 报告 `docs/assessment/2026-11-09-szrsql-matrix-differential.md`：矩阵逐块结论（通过/缺陷/不支持）→ bug 三分类清单（szrsql 上游缺陷 P0~P3 分级 + sz-orm 集成缺口修复记录 + 文档措辞问题）→ 支持面扩张结论（Szrsql 文档宣称面是否需更新，防幻影交付措辞自查）→ 遗留风险。报告完成后跑 `bash scripts/audit-verify.sh <报告>`（0 失败为验收线）。
2. 若 szrsql 上游缺陷达到可反馈级别（协议正确性/数据正确性/崩溃类），生成反馈提示词落盘 `docs/assessment/2026-11-09-szrsql-matrix-bugs-feedback-prompt.md`（沿用 2026-11-08-szrsql-copy-notify-fix-prompt.md 结构：复现→定位线索→差分指引→验收线）。
3. 评测交付六环闭环（实施提示词→报告→实施批→复评→红队复核→登记 CHANGELOG/AGENTS.md/commit）。
4. 诚实报告原则：某块若 szrsql 表现优于预期（如 M4 隔离级别是真语义），如实记录"优于预期"；支持面结论宁窄勿宽——禁止为体现工作量夸大 sz-orm 对 szrsql 的支持声明。

## 环境坑位速查

- git 在 `D:\Git`（不在 PATH）；bash 在 `D:\git\bin\bash.exe`；`cargo fmt --all` 用 `bash scripts/check-fmt-all.sh`
- 本机 PG18 占 5432；szrsql 6543；szrsql 与 sz-orm 是两个独立 workspace，共享 `CARGO_TARGET_DIR=D:\rust-target` 但**不要同时并行构建**
- szrsql HEAD 快速变动（并行 AI 会话），动手前记录基线 commit；szrsql 默认 auth=scram，trust 仅本地测试
- audit-verify 的 src/ 简写分支已修复（可正常跑）；报告引用 szrsql 文件时写完整 `crates/...` 路径不加行号（脚本会按 sz-orm 仓库解析而告警）
- Python 门禁脚本已内置 UTF-8 reconfigure；写临时脚本统一放 `D:\tmp`（勿入仓库）
