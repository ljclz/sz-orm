# PolarDB 方言矩阵真连验证——实施提示词（环 1）

> 落盘日期：2026-11-14。复制本文给新会话即可独立执行。六环闭环流程见 AGENTS.md「评测交付闭环链条」。

## 背景

- 仓库：`D:\www\test\鲜视达\rust\sz-orm`（main，勿动上游——ADR-0001 不适用本仓库内部，但勿改 szrsql）。
- 本批是方言矩阵二波补批的延续（Kingbase/GBase/Dameng 已闭环，OceanBase/PG/SQLite/MySQL/Oracle/MZ 容器批均已闭环）。
- 候选方言：**PolarDB**（`DbType::PolarDB`，`PolarDbDialect`，`delegate_dialect_to!(PolarDbDialect, PostgreSqlDialect, DbType::PolarDB)`，见 `packages/sz-orm-core/src/dialect.rs:2189-2190`）。

## 关键供给/方言匹配事实（本批先决裁定，红队重点复核对象）

1. **可用容器只有 PolarDB-X（MySQL 族）**：`polardbx/polardb-x:latest` 一键镜像（内含 GMS/DN/CN/CDC 全组件，entrypoint 内置 metaDB），非 PolarDB for PostgreSQL。
   - 实例：容器名 `pdbx-1stop`，宿主端口映射 `18506 -> 3306`，用户 `polardbx_root`（无密码），引擎自报 `5.6.29-PXC-5.4.19-SNAPSHOT`。
   - 容器易退出（init 脚本 stop-all 分支），跑测试前必须守门探活（循环 `mysql -h127.1 -P8527` 直到 READY），测试内用 env 门控跳过。
   - 引擎事实：单表模式 `PARTITION BY` 被拒（PXC-4500 Do not support partition by）；`ON DUPLICATE KEY UPDATE` 生效；`SHOW CREATE TABLE` 返回 InnoDB/utf8mb4_general_ci。
2. **`PolarDbDialect` 委托 PG 方言族，面向的是阿里云 PolarDB for PostgreSQL**——社区无 PG 版可用容器镜像（docker hub/mirror 均无）。
   - 因此本批对 PG 语义通道**无法真连**，只能静态验证（单测断言委托正确性，已有覆盖）。
   - **能真连的是 MySQL 协议通道**：sz-orm 侧用 `DbType::MySQL`（`MySqlDialect`）直连 PolarDB-X 的 MySQL 协议端口，验证「PolarDB-X 作为 MySQL 兼容目标库」的真实兼容性——这与 GBase 批「GBaseDialect→SqlServerDialect 仅 TDS 网关可执行」的边界登记同型。
3. **载荷定级**：PolarDB-X 真连测试产出的兼容性证据登记为「PolarDB-X（MySQL 族）矩阵」；`PolarDbDialect`（PG 委托）的适用边界登记为「需 PolarDB for PostgreSQL 实例，本机不可供给，留 CI/云实例补批」。两件事不得混写。

## 环境

- WSL docker：`wsl bash -c "docker ..."`（防 Windows 路径转换）。
- 守门探活命令模板：
  ```bash
  for i in $(seq 1 20); do
    docker exec pdbx-1stop mysql -h127.1 -P8527 -upolardbx_root -e 'SELECT 1' >/dev/null 2>&1 && { echo READY; break; }
    sleep 15
  done
  ```
- Rust 侧连接串：`mysql://polardbx_root@127.0.0.1:18506/szorm_plx`（无密码；经 `SZ_ORM_POLARDBX_URL` env 注入，未配置自动跳过——与 integration_szrsql 惯例一致）。
- 密码/凭据一律 env 注入不入库（P0-3 铁律）。本实例无密码，无泄露风险，但 DSN 仍走 env。

## 任务载荷（PL1~PL5）

- **PL1** 新增 `packages/sz-orm-core/tests/integration_polardbx.rs`：`matrix_plx_*` 系列测试（≥6 项），载荷对齐 MY 矩阵（引擎身份 `SELECT VERSION()` 断言含 `PXC`；DDL 建表幂等 IF NOT EXISTS；upsert `ON DUPLICATE KEY UPDATE` 行为级；分页 LIMIT/OFFSET；注入惰性（参数化输入恒作数据）；`SHOW CREATE TABLE` 引擎/字符集断言）。全部 `#[ignore]` + env 门控（`SZ_ORM_POLARDBX_URL`），套件内守门探活，未配置自动跳过不假红。
- **PL2** `PolarDbDialect`（PG 委托）静态验证补强：单测断言委托关键产物（quote/upsert EXCLUDED/RETURNING/占位符 $n），并在 doc 注释/报告中登记适用边界（PolarDB for PostgreSQL 专用；PolarDB-X 用户应选 `DbType::MySQL`）。
- **PL3** README/使用指南中 PolarDB 相关示例核对：如示例将 PolarDB-X 场景指向 `DbType::PolarDB` 则修正为 `DbType::MySQL`；G12 文档一致性、G18 示例清单、G33 版本声明过门禁。
- **PL4** G37 扫描 0 命中 + G3 clippy + G1 fmt + lib 全测过；`audit-verify.sh` 对报告全绿。
- **PL5** CHANGELOG.md `[Unreleased]` 节登记（PolarDB-X 矩阵批 + PolarDbDialect 边界登记），AGENTS.md 已发布行追加一句。

## 六环要求

1. 本提示词即环 1。
2. 报告落盘 `docs/assessment/2026-11-14-polardb-dialect-matrix.md`，每条结论附 file:line + 测试输出；跑 `bash scripts/audit-verify.sh` 附输出。
3. 实施批附 commit hash + 套件输出 + 门禁退出码。
4. 复评：按维度 grep 证据 + 跑套件拿 EXIT 码。
5. 红队：`background-task` 启独立 session 复核（复核者≠实施者），重点攻击：供给边界措辞是否混淆两族、env 门控是否假红、upsert 行为级证据真伪。
6. 登记：CHANGELOG + AGENTS.md + commit（conventional，附验证输出）。

## 已知坑位（继承）

- 容器随机退出 → 每次跑套件前守门探活；`docker start pdbx-1stop` 后等 ≥90s。
- `mysql -h127.1`（容器内）非 127.0.0.1。
- WSL 内 docker 命令必须 `wsl bash -c` 包裹。
- bash heredoc 双引号内反引号/`!` 破坏 commit message → 复杂 message 写临时文件 `git commit -F`。
- Windows 侧 `netstat` 确认 18506 LISTENING 后再跑 Rust 测试。
- MySQL DSN 无密码段是合法的（`mysql://user@host/db`），sqlx 接受。
