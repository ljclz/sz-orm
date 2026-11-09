# 新会话提示词：sz-orm 全功能矩阵 × SzRSQL × SQLite 三方对拍评测（v1，2026-11-09）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：2026-11-09 已完成 SzRSQL × PG18 差分评测（`docs/assessment/2026-11-09-szrsql-matrix-differential.md`，六环闭环 + 红队放行；9 项 szrsql 上游缺陷，其中 P0×2 上游已修复 `96c6dc72`/`45d05bb8`）。本批为**增量评测**：差分基准扩展到 SQLite，回答「同一 sz-orm 载荷在 szrsql 与 SQLite 两个非 PG 后端的兼容面」。
> **定位**：SQLite 是**参照系而非语义目标**——szrsql 走 PG 方言族，与 SQLite 本就异族（类型系统/DDL/事务语法不同），差分不判「谁对谁错」，产出三分类：**必然差异**（方言本质不同，登记即可）/ **szrsql 缺陷**（szrsql 宣称 PG 语义但偏离 PG 的点，对照 PG18 仲裁）/ **sz-orm 集成缺口**（sz-orm SQLite 方言或 sqlx 适配层问题）。
> **PG18 仲裁权**：凡 szrsql 行为存疑处，以 PG18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）为语义基准三方对照。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师 + 差分评测工程师。任务：以 sz-orm 全功能矩阵为测试载荷，做 **SzRSQL × SQLite（× PG18 仲裁）三方对拍**，产出三分类 bug 清单，并修复 sz-orm 侧可修复项。

**目标仓库**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.2）
- 被测 A：SzRSQL 6543（trust，`szrsql://postgres:trust@127.0.0.1:6543/postgres`；HEAD 快速变动，动手前 `git -C D:/www/test/鲜视达/rust/szrsql log --format="%h" -1` 记录基线；**只读严禁修改**——ADR-0001）
- 被测 B：SQLite（本机 Python 3.32.3 内置；sqlx 后端 `sqlite::memory:` 或 `sqlite://D:/tmp/szorm_sq_differential.db?mode=rwc`，无需额外安装服务）
- 仲裁基准：PG18 5432（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）

## 被测环境启动（第一步）

```bash
# szrsql（若 6543 未监听）
D:/rust-target/debug/szrsql.exe --port 6543 --auth-mode trust --connection-idle-timeout 0 \
  --data-dir D:/tmp/szrsql-matrix >> D:/tmp/szrsql-matrix.log 2>&1 &
# 注意 --connection-idle-timeout 0（长测试会被默认 300s 空闲超时切断）
# SQLite：sqlx 直连文件/内存库，无需启动；对拍脚本经 python -c "import sqlite3" 原生执行
```

## 评测矩阵（M1~M9 中筛选三方对拍有信息量的块；每载荷 = szrsql + SQLite + PG18 三方执行，结果三向 diff）

| # | 能力块 | 载荷 | 三方对拍重点 |
|---|--------|------|------------|
| S1 | QueryBuilder 生成 SQL | chained where/orderBy/group/having/join/union/聚合函数集（复用上批 M1 载荷） | sz-orm SQLite 方言生成的 SQL 在真 SQLite 上的执行正确性；NULL 排序（SQLite 无 NULLS FIRST/LAST 关键字——sz-orm 方言层是否做了兼容改写）、聚合遇 NULL/空集 |
| S2 | 表达式层 | ILIKE（SQLite 无 ILIKE，方言层改写 `LIKE` + lower？）/CAST/字符串函数（substr vs substring）/数学函数/Extract/COALESCE/greatest/least（SQLite 无 GREATEST——方言层如何处理） | sz-orm SQLite 方言的 PG→SQLite 表达式降级是否完整；szrsql 侧同为 PG 语义面（应与 PG18 一致） |
| S3 | 参数化与占位符 | $n / ? / :name 三种占位符经 sz-orm 方言层落到 SQLite 的改写；60+ 参数上限；相同参数复用（SQLite `?1` 复用语法） | 占位符改写正确性（历史 D5 债务的同源面）；SQL 注入面复查（门禁 9 参数化铁律） |
| S4 | DDL 与 schema_sync | CREATE TABLE（AUTOINCREMENT vs BIGSERIAL）/索引（部分索引 SQLite 支持/表达式索引）/COMMENT（SQLite 无 COMMENT ON——方言层跳过或降级？）/ALTER 变体 | sz-orm SchemaBuilder 对 SQLite 的 DDL 生成 vs 真 SQLite 接受度；szrsql 侧同上批 M3（已闭环，快速回归即可） |
| S5 | 写路径与 RETURNING | INSERT/UPDATE/DELETE .. RETURNING（SQLite 3.35+ 支持）/批量 INSERT/last_insert_rowid（sz-orm SQLite 主键回填路径） | RETURNING 在 SQLite 的列名元数据（szrsql RETURNING 已验证 OK）；sqlx SQLite 后端返回列映射 |
| S6 | 边缘 SQL（bug 猎场） | 三值 NULL（= NULL/IS NOT DISTINCT FROM——**SQLite 无 IS NOT DISTINCT FROM**，三方对照）/隐式类型转换（SQLite 亲和性 int↔text 弱类型 vs PG 强类型——必然差异区）/保留字引号/Unicode 标识符/空串 vs NULL/int 溢出（SQLite 静默转 REAL vs PG 报错——必然差异）/除零（SQLite 返回 NULL vs PG 报错——必然差异） | 区分「必然差异」与「真缺陷」的纪律：szrsql 侧任何偏离必须回对照 PG18 仲裁 |
| S7 | 事务与连接 | begin/commit/rollback/savepoint（SQLite 支持标准 SAVEPOINT）/事务内错误后行为（SQLite 语句失败不 abort 事务——与 szrsql 同为宽松方向，对照 PG18 严格 abort） | 三方事务语义对照表；sz-orm TransactionManager 在 SQLite 后端的既有行为不回归 |

**不纳入**（理由登记，宁窄勿宽）：M4 隔离级别（SQLite 无多级隔离）、M5 连接池 kill-recover（单文件库无服务进程）、M7 多租户/N+1（sz-orm 侧组件，上批已验证不回归）、M6 流式游标（SQLite 无服务端游标概念）。

## 硬性要求

- **只读 szrsql**：发现缺陷 → 记录（复现 SQL + 三方输出 + 定位线索）→ 绝不改其文件；修复建议写入报告「上游反馈」节
- **三方纪律**：每条结论必须有 szrsql/SQLite/PG18 三方对照输出（或明确说明该载荷为何只做两方）；**szrsql 偏离 PG18 的才判 szrsql 缺陷**，szrsql 与 PG18 一致而 SQLite 不同的登记为「必然差异」；sz-orm 生成的 SQL 在 SQLite 上失败的判「sz-orm 集成缺口」并修复
- **sz-orm 侧修复**：SQLite 方言分支缺口（`packages/sz-orm-core/src/dialect.rs` SQLiteDialect）修复并补单测；历史 D5（PG 占位符）同源问题在 SQLite 侧一并排查（`pg_rewrite_placeholders` 对应面）
- **测试产物**：新增 `packages/sz-orm-sqlx/tests/integration_sq_differential.rs`（三方载荷沉淀为 `#[ignore]` 集成测试；SQLite 内存库免配置必跑，szrsql 未启动自动跳过；PG18 未启动自动跳过——沿用 integration_szrsql.rs 模式）；对拍脚本放 `D:\tmp`（不入仓库）
- **参数化铁律**（门禁 9）：所有测试 SQL 参数化，外部输入一律参数绑定，禁止 format/f-string 拼接值；新代码零 unsafe、零 todo!
- **提交纪律**（硬约束 13）：每完成一个能力块立即 conventional commit，提交可编译
- 串行 cargo；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / `clippy --workspace --all-targets -- -D warnings` / fmt 72 包全绿
2. `cargo test -p sz-orm-sqlx --test integration_sq_differential -- --ignored`：三方载荷真跑全过（SQLite 内存库项无条件必过；szrsql/PG18 未启动项自动跳过并留输出）
3. `cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored` 上批 15 项不回归（12 过 + 3 留档）
4. `cargo test --workspace`（PYTHONHOME 注入）全绿
5. 门禁：G9（--strict + 自测 13/13）/G12/G15/G16/G33/G35/G36 全过
6. 三分类清单：每条附证据（复现步骤 + 三方输出 + sz-orm 侧 file:line）
7. 报告完成后跑 `bash scripts/audit-verify.sh <报告>`（0 失败为验收线）

## 输出要求

1. 报告 `docs/assessment/2026-11-09-sq-tripled-differential.md`：矩阵逐块三方结论 → 三分类清单（必然差异 / szrsql 缺陷 [仅记，对照上批编号体系 UP-x 续编] / sz-orm 集成缺口修复记录）→ 支持面结论（防幻影措辞自查）→ 遗留风险。报告完成后跑 audit-verify 自证
2. sz-orm 集成缺口修复随批提交（conventional commit，附测试输出）
3. 评测交付六环闭环（本提示词为第 1 环 → 报告 → 实施批 → 复评 → 红队复核 → 登记 CHANGELOG/AGENTS/commit）
4. 诚实报告：SQLite 异族差异如实登记「必然差异」，禁止把方言本质不同包装成 szrsql 缺陷或 sz-orm 缺陷凑数；支持面结论宁窄勿宽

## 环境坑位速查

- git 在 `D:\Git`（不在 PATH）：`D:\Git\cmd\git.exe`；bash 在 `D:\git\bin\bash.exe`；`cargo fmt --all` 用 `bash scripts/check-fmt-all.sh`
- PG18 psql 在 `D:\PostgreSQL\18\bin`；szrsql 6543；szrsql 与 sz-orm 是两个独立 workspace，共享 `CARGO_TARGET_DIR=D:\rust-target` 但**不要同时并行构建**
- 差分对拍含中文时：bash heredoc 会产生 GBK 假编码错误（PG 报 0xb1），必须经 Python subprocess stdin UTF-8 注入；写临时脚本统一放 `D:\tmp`（勿入仓库），运行用 `python -I`
- 双会话/多语句并发实验：psql stdin 流式必须分线程 drain 子进程输出，否则死锁
- szrsql 进程可能静默退出（netstat :6543 检查重启）；audit-verify 的 src/ 简写分支已修复可正常跑；报告引用 szrsql 文件写完整 `crates/...` 路径，行号须以记录的基线 commit 为准并注明
- SQLite 类型亲和性：INTEGER 列可存 TEXT（弱类型），三方对照时结果集类型标注须显式（避免假差异）
