# 新会话提示词：sz-orm 支持 szrsql 数据库（v1，2026-11-08）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。分两阶段：先理解 szrsql（自研 Rust 数据库，D:\www\test\鲜视达\rust\szrsql），再让 sz-orm 新增 szrsql 后端支持。
>
> **背景**：sz-orm 当前 28 个 DbType 映射到 5 个方言族（MySQL/PostgreSQL/Oracle/MSSQL/SQLite），未包含 szrsql。szrsql 是同一作者的自研 Rust 数据库（v1.0.0-rc.2，51 crate），对标 PostgreSQL 18，**默认走 pgwire v3.0 协议**（`--port 6543`，PG14 兼容声明；可同时开 `--mysql-port`/`--tds-port`/`--oracle-port` 多协议前端，全部经同一套 PG 语义执行器）。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师，任务分两阶段：**阶段一** 深入理解 szrsql（只读探索 + 本机启动实测）；**阶段二** 设计并实施 sz-orm 对 szrsql 的完整后端支持（新 DbType + 方言族 + 连接池 handle + 真连集成测试 + 文档/门禁同步）。

**目标仓库**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.1，72 包，36 道门禁）
- 适配目标：`D:\www\test\鲜视达\rust\szrsql`（v1.0.0-rc.2，**只读，严禁修改**——它是上游独立仓库，ADR-0001 同样适用）

## 阶段一：理解 szrsql（必做，附证据）

1. 通读 `szrsql/README.md`、`szrsql/AGENTS.md`（架构速览 + 三态标注 + 防幻影交付清单）、`szrsql/docs/feature_matrix.toml`。
2. **关键结论先行验证**：szrsql 对外是标准 PostgreSQL 线协议（pgwire v3.0 + SCRAM-SHA-256 + 扩展协议 Prepared/Portal）。默认端口 **6543**（避开 PG 的 5432）。在 szrsql 仓库 grep 确认：`szrsql-protocol` crate 的 pgwire session、SCRAM 认证、prepared statement 支持到什么程度（`--auth-mode trust` 是否可用于本地测试）。
3. **本机启动实测**（L2 证据）：
   ```bash
   cd /d/www/test/鲜视达/rust/szrsql
   cargo build -p szrsql-bin --release   # 或 debug，视时间预算
   cargo run -p szrsql-bin -- --port 6543 --auth-mode trust --data-dir /tmp/szrsql-test
   ```
   另开终端用 `psql` 或 sz-orm 的 PG 连接直连 `127.0.0.1:6543` 验证：建表、INSERT 参数绑定、SELECT、事务 BEGIN/COMMIT、RETURNING、LIMIT/OFFSET 分页。
4. **预判兼容性风险并逐项记录**（每条附 szrsql `file:line` 或实测输出）：占位符 `$n` 是否完整支持、类型系统映射（BIGINT/TEXT/TIMESTAMP/BOOL/BLOB/UUID/JSON）、`information_schema` 支持度（sz-orm schema 探测依赖它）、`SHOW`/`EXPLAIN` 差异、DDL（ALTER/IF EXISTS）、RETURNING、事务隔离语义。

## 阶段二：sz-orm 集成设计与实施

**推荐架构（先验证再动手，若实测推翻须说明证据）**：新增 `DbType::Szrsql`，走 **PostgreSQL 方言族的委托实现**（复用 pg 方言 + `$n` 占位符），理由：szrsql 语义层即 PG 语义（README 明示六前端经同一 ExecutorService 执行 PG 语义 SQL，`AGENTS.md` 架构速览）。连接层复用 `sz-orm-sqlx` 的 `PgPoolHandle`，仅加 URL scheme/端口识别（如 `szrsql://` 或复用 `postgres://` 指向 6543 端口）。**禁止**为 szrsql 新写 wire 协议实现（szrsql 自带 pgwire）。

实施范围：
1. `packages/sz-orm-core/src/dialect.rs`：`DbType::Szrsql` 变体 + `szrsql()` 方言（委托 PostgresDialect，允许少量差异点覆写——以阶段一实测差异为准）；现有 `all_dialects()`/单测循环按 PG 家族既有模式补齐断言。
2. `packages/sz-orm-sqlx/src/any.rs`：`SzrsqlPoolHandle`（或 PgPoolHandle 别名 + URL scheme `szrsql://` 解析），实现 `connect_with_max`，接入 MetricsConn 包装。
3. 迁移/建表：确认 szrsql 下 DDL（AUTO_INCREMENT vs PG SERIAL、类型映射）真实可执行，`schema_sync` 兼容性给出结论。
4. 新增集成测试 `packages/sz-orm-sqlx/tests/integration_szrsql.rs`：CRUD、参数化查询、事务回滚、类型往返（i64/String/DateTime/bool/Option）、分页、N+1 场景；测试需 szrsql 服务在 `127.0.0.1:6543`，未启动自动跳过（URL 经 `SZ_ORM_SZRSQL_URL` 环境变量注入，**凭据不入库**，遵循现有 integration_mysql.rs 的 `mysql_url()` 模式）。
5. 文档同步：README/README.zh 支持矩阵、AGENTS.md「本机数据库」节、CHANGELOG.md 新版本条目、门禁表如涉及变更同步。
6. 版本与发布：workspace 版本按 CHANGELOG 惯例 bump；G19/G33/G36 门禁数字同步。

## 硬性要求

- **只读 szrsql 仓库**：发现 szrsql 缺陷 → 记录到报告中，绝不改其文件。
- **所有 SQL 参数化**（门禁 9 自动扫描）；新代码零 `unsafe`、零 `todo!`。
- 凭据仅环境变量注入；`--auth-mode trust` 仅限本地测试命令行，不入库文档正文以外代码。
- 提交纪律（AGENTS 硬约束 13）：自洽任务单元立即 conventional commit，提交可编译。
- 禁止幻影交付：宣称"支持 szrsql"必须附真连测试通过输出；feature 门控无启用点用"提供 X 组件（需手动接入）"措辞。

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / `clippy -D warnings` / `cargo test --workspace`（串行 cargo，禁并行）全绿
2. `cargo test -p sz-orm-sqlx --test integration_szrsql -- --ignored`：真连 szrsql 全过
3. 门禁 8/9/12/15/16/19/33/35/36 复跑通过（重点：G12 文档一致性、G19 版本声明、G33 门禁数、G36 CHANGELOG）
4. `python -m pytest tests/` 全过
5. 阶段一 szrsql 兼容性差异记录表（每条附证据）

## 输出要求

1. 报告 `docs/assessment/2026-11-08-szrsql-integration.md`：阶段一探索结论（szrsql 协议面/兼容性差异表，附 file:line）→ 阶段二设计决策与依据 → 实施记录（commit hash + 测试输出）→ 遗留风险。
2. 报告完成跑 `bash scripts/audit-verify.sh docs/assessment/2026-11-08-szrsql-integration.md` 自证（0 失败为验收线）。
3. 若评估后结论为"复用 PG 适配零代码/极小改动即可支持"，如实报告该结论并给出最小 diff——不要为体现工作量而过度抽象。

## 环境坑位速查

- git 在 `D:\Git`（不在 PATH）；bash 在 `D:\git\bin\bash.exe`
- `cargo fmt --all` Windows 报 os error 206 → 用 `bash scripts/check-fmt-all.sh`
- 本机 PG 18 占用 5432；szrsql 默认 6543 不冲突；**串行 cargo，禁止并行进程争锁**
- `CARGO_TARGET_DIR=D:\rust-target` 共享缓存；szrsql 与 sz-orm 是两个独立 workspace，不要混用同一 target 时并行构建
- Python 门禁脚本 GBK 控制台已内置 UTF-8 reconfigure；pytest 需 ≥160 passed
