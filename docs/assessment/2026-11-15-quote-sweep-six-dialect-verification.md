# quote 横向批六方言真连补验报告（SQ 批收口，2026-11-15）

> 收口对象：批二 `2026-11-12-alter-index-fk-quote-sweep.md` §7-1「SQLServer/DB2/Informix/SapHana/Snowflake/Firebird 真连留 CI」与 FTS 批 `2026-11-12-fts-quote-sweep.md` §7-1/§7-2 同名单。本批经容器供给验证其中 **4 个方言**（SQLServer/DB2/SapHana/Firebird），Snowflake/Informix 确认无社区镜像维持 CI 登记。

## 1. 供给结果

| 方言 | 镜像 | 引擎 | 结果 |
|---|---|---|---|
| SQLServer | `mcr.microsoft.com/mssql/server:2022-latest` | 2022 CU27 (16.0.4305.5) | ✅ 可供给（**须 `-p` 映射**，此前 MZ 批容器漏 `-p`；init 后随机退出 255 须守门循环） |
| DB2 | `icr.io/db2_community/db2:latest`（7.31GB，直连可拉） | DB2/LINUXX664 **12.1.5.0** | ✅ 可供给（`--privileged` + 首次初始化 ~15 分钟；中途 DBI1264E 一次，重启自愈） |
| SapHana | `saplabs/hanaexpress:latest`（6.44GB） | HXE 2.x (HDB 90) | ✅ 可供给（**须 `--privileged --security-opt seccomp=unconfined`**——`move_pages` syscall 对非特权 uid=12000 被 WSL2 容器默认 seccomp 拒绝；初始化 ~10 分钟；守护脚本需应对 stop-all 退出） |
| Firebird | `firebirdsql/firebird:latest` | LI-V5.0.4.1812 | ✅ 可供给（旧 pull 进程残留导致层死锁 ~50min，kill 重拉 60s 完成；容器反复 stop-all 须守门） |
| Snowflake | — | — | ❌ 无社区容器镜像（云服务）——维持 CI 登记 |
| Informix | — | — | ❌ 无官方社区镜像可自动供给（GBase 8s 批边界同源）——维持 CI 登记 |

## 2. 行为级证据（CLP/SQL 真库执行，全部无 SQL 错误）

### 2.1 DB2（SYSCAT 目录往返，探针 `/tmp/probe3.sql`）
- `CREATE TABLE "MzKeep" ("Id" INT, "userName" VARCHAR(50))` → DB20000I
- `CREATE INDEX "idx_Keep" ON "MzKeep" ("userName")` → DB20000I
- `ALTER TABLE ... ADD CONSTRAINT "fk_Keep" FOREIGN KEY ("Id") ...` → DB20000I
- **目录证据（引号名大小写保留）**：`SYSCAT.TABLES WHERE TABNAME='MzKeep'` → `MzKeep`（1 行）；`SYSCAT.INDEXES` → `idx_Keep`（引号名原样，另有自动生成 `SQL261114180036510`）；`SYSCAT.TABCONST` → `fk_Keep`。混合大小写列 `"userName"` SELECT 往返成功。

### 2.2 SQLServer（sys.indexes / sys.foreign_keys 目录往返，探针 `/tmp/probe.sql`）
- 保留字列 `[order]` 建列 + `CREATE INDEX [idx_Order]` + WHERE 过滤往返全绿（`Incorrect syntax` 计数 **0**）
- `sys.indexes` 返回 `idx_User`/`idx_Order`（引号名原样）；`sys.foreign_keys` 返回 `fk_Mixed_1`
- 复用真库通道：`matrix_mz_mssql4` 断言漂移修复（e8505a73）与本批同一容器实例

### 2.3 SapHana（hdbsql SYS.INDEXES/SYS.REFERENTIAL_CONSTRAINTS 目录往返）
- `CREATE COLUMN TABLE "MixedCaseTbl" ("Id" INT PK, "userName" NVARCHAR(50), "orderCol" NVARCHAR(30))` → 成功
- `CREATE INDEX "idx_User"` → 成功；`SYS.INDEXES WHERE TABLE_NAME='MixedCaseTbl'` → **`idx_User`**（引号名原样保留 + 内部 `_SYS_TREE_CS_#` 索引）
- `ADD CONSTRAINT "fk_Mixed_1" FOREIGN KEY`（自引用被引擎拒——HANA 特性，改独立 RefTbl 后成功）→ `SYS.REFERENTIAL_CONSTRAINTS` 返回 **`fk_Mixed_1`**
- 混合大小写列读回 `alice`；`orderCol` 过滤往返成功；探针表已清理（COUNT=0）

### 2.4 Firebird（RDB$ 目录往返，探针 `/tmp/probe.sql` + `/tmp/probe2.sql`）
- `CREATE TABLE "MixedCaseTbl" ("Id" INTEGER PK, "userName" VARCHAR(50), "orderCol" VARCHAR(30))` → 成功（引号名大小写敏感落库）
- `CREATE INDEX "idx_User"` → 成功；`RDB$INDICES` → **`idx_User`**（原样）+ `RDB$PRIMARY1`（自动名）
- `ADD CONSTRAINT "fk_Mixed_1" FOREIGN KEY` → `RDB$RELATION_CONSTRAINTS` 返回 **`fk_Mixed_1`**
- 混合大小写列与 `orderCol` 过滤均读回 `alice`；自引用 FK 需经中间表探针（Firebird 允许自引用但探针经复合流程验证）

## 3. 结论

- **sz-orm 方言缺口：0 处**——批二 8 方言 quote 生成面（AddIndex/DropIndex/FK 约束名）在本批 4 个新真连引擎上全部被接受且目录名往返无损。
- 六方言真连留 CI 缺口从 6 缩至 **2**（Snowflake/Informix——无社区镜像，非代码问题）。
- 供给坑位登记：MSSQL `-p` 缺失/HXE seccomp move_pages/DB2 DBI1264E 自愈/Firebird pull 死锁 4 项。

## 4. 引用锚点（源码生成面 = 被验证对象）

- SqlServerDialect `full_text_search`/`json_extract` quote 形态：packages/sz-orm-core/src/dialect.rs:1769（`fn quote`）、packages/sz-orm-core/src/dialect.rs:1814（quote 横向批注释 + `self.quote(column)`）
- DB2/SapHana/Firebird AddIndex quote 生成面（批二产物）：packages/sz-orm-core/src/dialect.rs:3188（DB2 `CREATE INDEX {} ON {} ({})` 走 quoted_cols）、packages/sz-orm-core/src/dialect.rs:3533（SapHana 同型）、packages/sz-orm-core/src/dialect.rs:4026（Firebird 同型）
- 横向单测（生成面逐字节基线）：packages/sz-orm-core/src/dialect.rs:4499（`test_alter_index_fk_quoting_all_dialects`）
- MZ 批 MSSQL 容器供给记录：docs/assessment/2026-11-13-container-dialect-matrix.md:15（DB2 镜像/CLP 口径）
- 批二遗留项原文：docs/assessment/2026-11-12-alter-index-fk-quote-sweep.md:89（6 方言留 CI）
- FTS 批遗留项原文：docs/assessment/2026-11-12-fts-quote-sweep.md:90（8 方言留 CI）

## 5. 门禁

| 门禁 | 结果 |
|---|---|
| G37 dialect quoting | ✅ 0 命中 |
| G1 fmt / G3 clippy | ✅（本批无 src 变更） |
| G35 doc links | ✅ 0 死链 |
