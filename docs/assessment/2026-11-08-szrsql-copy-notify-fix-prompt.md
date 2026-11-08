# SzRSQL 上游反馈新会话提示词（v1，2026-11-08）

> **用法**：将下方「---」之间的内容整体复制到新会话（在 szrsql 仓库工作），作为首个提示词发送。
> **来源**：sz-orm v10.13.2 SzRSQL 集成评测报告 [docs/assessment/2026-11-08-szrsql-integration.md](2026-11-08-szrsql-integration.md) §7 遗留项复测（D-5/D-6）。
> **注意**：本提示词的目标仓库是 **szrsql 本体**（`D:\www\test\鲜视达\rust\szrsql`，可修改）；sz-orm 侧为只读参照。

---

**角色**：你是 SzRSQL 的 pgwire 协议工程师。sz-orm（下游 ORM，Rust，基于 sqlx 0.9 PostgreSQL 驱动）已完成对 SzRSQL（v1.0.0-rc.2，HEAD=2a0285db 时点）的真连集成评测，6 项扩展场景中 4 项全通（SAVEPOINT 嵌套/游标/ALTER TABLE 全变体/SCRAM-SHA-256 认证），**2 项发现 pgwire 子协议与 sqlx 驱动的帧语义不兼容**。你的任务是复现、定位并修复这两项（P1-1 COPY、P1-2 NOTIFY）。

## 缺陷 1（P1-1）：COPY 子协议与 sqlx 客户端不兼容

**复现**（sz-orm 侧 sqlx PG 驱动连接 `127.0.0.1:6543`，`--auth-mode trust`）：

```sql
CREATE TABLE t_copy (id INT PRIMARY KEY, name TEXT);
COPY t_copy (id, name) FROM stdin WITH (FORMAT text);
```

sqlx 报错：`Connection error: execute: unexpected message: CopyInResponse`。
反向 `COPY t_copy (id, name) TO stdout` 报：`IO error: expected to read 5 bytes, got 0 bytes at EOF`。

**初步定位线索**：
- szrsql 侧 COPY 实现位于 `crates/szrsql-protocol/src/pgwire/session.rs`（grep `COPY_INSERT_CHUNK`、`copy_in`、`CopyInResponse`；session.rs:3413 附近注释提到 copy_in() 消费 BindComplete/CopyInResponse）。
- sqlx 侧（0.9）：`sqlx-postgres` 的 `copy_in`/`copy_out` 走标准 pgwire COPY 子协议（CopyInResponse → CopyData* → CopyDone/CopyFail；CopyOut 侧持续读 CopyData 直到 CopyDone）。**注意 sqlx 在普通 `execute()` 路径收到 CopyInResponse 也会报 unexpected message——先确认 szrsql 是否在「非显式 COPY 客户端」发起了协议内 COPY 语义**（例如 SQL 文本含 COPY 时直接回 CopyInResponse，而标准 PG 同样如此，但 sqlx 期望用户使用专用 copy_in API；此点需对照真实 PostgreSQL 18 行为做差分，本机 5432 可用）。
- 关键差分点：真实 PG 18 对普通 execute("COPY ... FROM stdin") 同样回 CopyInResponse 且 sqlx 同样报错——**若差分证明行为与 PG 一致，则缺陷重定性为"sqlx 使用方式问题"，验证 `sqlx::postgres::copy_in` API 对 szrsql 是否可用即为验收线**；若 szrsql 在帧分帧/CopyDone 语义上与 PG 不同（如提前 EOF、缺 CopyDone），则是真实协议缺陷。

**验收**：
1. 与 PG 18 的 COPY 行为差分结论（附对拍输出）；
2. `sqlx::postgres::copy_in`/`copy_out` 对 szrsql 真连可用（或明确报告不可用原因 + szrsql 侧修复 diff）；
3. 补充 e2e 测试（`szrsql-pgcompat` 或 `szrsql-protocol/tests`）覆盖 COPY IN/OUT 与真实驱动。

## 缺陷 2（P1-2）：NOTIFY 异步 NotificationResponse 使 sqlx 普通 execute 报错

**复现**：

```sql
LISTEN ch_probe;    -- ok
NOTIFY ch_probe, 'hello';   -- sqlx 报 Connection error: execute: unexpected message: NotificationResponse
UNLISTEN ch_probe;  -- ok
```

**初步定位线索**：
- szrsql 侧 LISTEN/UNLISTEN/NOTIFY 预处理在 `crates/szrsql-sql/src/parser.rs:1240-1274`（Phase 4.6，sqlparser 0.53.0 不支持故自实现）。
- sqlx 侧：`NotificationResponse` 是**异步服务器推送帧**，sqlx 在连接读循环中应有后台任务消费它（真实 PG 下 sqlx `execute("NOTIFY ...")` 不会报错——PG 对 NOTIFY 的响应是 CommandComplete，NotificationResponse 只推给**其他**监听连接，不回显给发起者）。
- **重点排查**：szrsql 是否把 NotificationResponse **回显给了 NOTIFY 发起连接自身**？真实 PG 语义：发起者只收 CommandComplete，监听者（其他 session）才收 NotificationResponse。若 szrsql 回显给自己，即协议语义缺陷，修复方向是改为仅广播给其他持有 LISTEN 的 session。
- 次要排查：sqlx PgListener 专用路径（`sqlx::postgres::PgListener`）对 szrsql 是否可用（新建专用连接 LISTEN 后另一连接 NOTIFY，验证能收到通知）。

**验收**：
1. 与 PG 18 NOTIFY 语义差分结论（谁该收到 NotificationResponse）；
2. 修复后：sqlx 普通 execute("NOTIFY ...") 不再报错；`PgListener` 跨连接收发真连验证通过；
3. 补充 e2e 测试覆盖 LISTEN/NOTIFY/UNLISTEN + 跨 session 通知。

## 硬性要求

- 遵守 szrsql AGENTS.md 门禁清单（接线审计/防幻影交付/e2e 从入口触达/文档同步/自动提交）；
- 修复涉及 `session.rs`/`parser.rs` 高风险文件，动手前 `git pull` + 确认工作区干净（当前有并行会话 WIP：`pgwire/session.rs`、`lsm_table_adapter.rs`）；
- 与 PG 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）做差分对拍，禁止凭记忆断言协议语义；
- 两缺陷独立修复、独立提交（conventional commit），每项附复现→定位→修复→验证四段证据（file:line + 测试输出）。

## 输出要求

报告落盘 `docs/governance/evaluation/2026-11-08-szorm反馈-copy-notify-修复.md`，含：差分对拍表、根因分析（file:line）、修复 diff 摘要、验收输出（sqlx 真连 + e2e 测试）；完成后在 sz-orm 侧报告 `docs/assessment/2026-11-08-szrsql-integration.md` §7 的 D-5/D-6 状态变更需反馈回 sz-orm 会话登记。
