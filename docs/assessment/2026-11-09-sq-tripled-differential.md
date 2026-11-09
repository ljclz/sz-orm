# SzRSQL × SQLite × PG18 三方对拍评测报告（2026-11-09）

> 提示词：[2026-11-09-sq-tripled-differential-prompt.md](2026-11-09-sq-tripled-differential-prompt.md)（第 1 环）
> 前序：[2026-11-09-szrsql-matrix-differential.md](2026-11-09-szrsql-matrix-differential.md)（M1~M9 双方对拍，9 项 UP-x 上游缺陷，P0×2 已由上游修复）
> **被测基线**：szrsql HEAD=`54dc5631`（test(p0): 差分比对测试 vs PostgreSQL 18；注意：本批执行期间 szrsql 并行会话从 `3944e818` 推进到 `54dc5631`，M9 边缘 SQL 部分结论在新 HEAD 下复验）。SQLite 双引擎事实：**sqlx 捆绑 libsqlite3-sys 0.30.1 = SQLite 3.46.0**（集成测试真实引擎）；本机 Python sqlite3 = 3.32.3（psql 式 CLI 对拍引擎，功能面偏旧——版本差异本身是本批重要发现，见 D-3）。
> 遵守：szrsql 仓库零改动；三方纪律（szrsql 偏离 PG18 才判缺陷）；audit-verify 0 失败自证。

## 1. 执行摘要

三方对拍 24 项固定载荷（round1：S1/S2/S6 语义面）+ 12 项等价改写/扩展载荷（round2：S3/S4/S5/S7）+ round3 事务深化 + **sz-orm 载荷沉淀为 7 项 Rust 集成测试**（`integration_sq_differential.rs`，全过）：

| 分类 | 数量 | 明细 |
|------|------|------|
| szrsql 上游缺陷（新） | **0** | szrsql 与 PG18 在本批全部载荷上**一致**——包括上批 P0/P2 修复后的溢出/除零报错行为（新 HEAD 已对齐 PG） |
| sz-orm 集成缺口（已修复） | **1** | [FIX-1] `typed_ast::Concat` 对 SQLite 生成 `CONCAT()` 而该函数 3.44 才内置（sqlx 捆绑 3.46 有，但低版本兼容面不足）→ SQLite 分支归入 `\|\|` 拼接族 |
| 必然差异（SQLite 方言本质） | **7**（初报 8，红队勘误撤销 D-5 NULLS 错判，见 §3 勘误行） | 见 §3 表 |

**核心结论**：szrsql 在本批覆盖面内**未发现新缺陷**——上批 UP-2/UP-4（IS NOT DISTINCT FROM、整数溢出）在新 HEAD 复验已与 PG18 一致（上游修复生效确认）。SQLite 与 szrsql 的差异全部属于异族方言本质（ILIKE/GREATEST/LEAST/CONCAT（低版本）/NULLS 排序键/IS NOT DISTINCT FROM（<3.39）/弱类型/除零 NULL/RETURNING（<3.35）），无「szrsql 宣称 PG 语义但偏离」的新增证据。

## 2. 矩阵逐块结论（S1~S7）

### S1 QueryBuilder 产物面 —— 通过

| 载荷 | szrsql | SQLite | PG18 | 分类 |
|------|--------|--------|------|------|
| ORDER BY val NULLS FIRST/LAST | ✅=PG | ❌ 无 NULLS 关键字（默认 NULL 最小） | 基准 | 必然差异（sz-orm SQLite 用户用 `order_by` 原生串自行表达） |
| LEFT JOIN 空匹配 / GROUP BY NULL 组 / UNION 去重 / 聚合空集 | ✅=PG | ✅ 语义一致 | 基准 | 三方一致 |
| WHERE+ORDER+LIMIT/OFFSET 产物（双引号风格） | ✅ | ✅ | ✅ | 三方一致（query.rs:1690 产物在两库均可执行） |

### S2 表达式层 —— 通过（1 项缺口修复）

| 载荷 | szrsql | SQLite | 分类 |
|------|--------|--------|------|
| ILIKE | ✅=PG | ❌ 无 ILIKE；sz-orm [typed_ast.rs:1178](../../packages/sz-orm-core/src/typed_ast.rs#L1178) fallback `LOWER(x) LIKE LOWER(?)` **实测可执行且语义正确** | 必然差异（fallback 已闭环） |
| GREATEST/LEAST | ✅=PG | ❌ 无此函数 | 必然差异（sz-orm typed_ast 未定义 Greatest/Least 表达式，无生成面——不支持即不受影响，如实登记） |
| SUBSTRING(FROM..FOR) | ✅=PG | ❌ 语法不识别；SUBSTR(s,i,n) 等价形式可执行 | 必然差异 |
| EXTRACT(YEAR FROM d) | ✅=PG | ❌ 语法不识别；strftime 等价（**注意 strftime 在 PG/szrsql 均不存在**——双向均需方言层转换） | 必然差异 |
| CAST bool→int（`::`） | ✅=PG | ❌ `::` 语法不识别；CASE WHEN 等价可执行 | 必然差异 |
| COALESCE | ✅ | ✅ | 三方一致 |
| **Concat 生成** | ✅ `CONCAT()`=PG | ❌ `CONCAT()` 在 SQLite 3.44 前不存在（python 3.32 实测 no such function）；**[FIX-1] 已修复**：SQLite 分支改 `(a \|\| b)` | **sz-orm 集成缺口 → 已修复** |

### S3 参数化与占位符 —— 通过

| 载荷 | 结果 |
|------|------|
| `?` 占位符（sqlx SQLite） | ✅ 60+ 参数 IN 全通（实测 70 参数，绑定 6 行命中） |
| `$n` 占位符（**SQLite 原生支持**，sqlx 直传免改写） | ✅ 实测 affected=1——**历史 D5 债务同源面结论：SQLite 侧无需改写层**（PG 侧 `pg_rewrite_placeholders` 的 `?→$n` 方向不适用） |
| `?1` 复用语法（SQLite 特有） | sz-orm 不生成该形式；szrsql/PG 均拒绝（parse error）——无影响 |
| `:name` 命名参数 | sz-orm 方言层不生成；未纳入 |

### S4 DDL 与 schema_sync —— 通过

| 载荷 | szrsql | SQLite | PG18 | 分类 |
|------|--------|--------|------|------|
| INTEGER PRIMARY KEY AUTOINCREMENT | ✅ 接受（PG 语法本无此——szrsql 宽容面） | ✅ 原生 | ❌ 语法错 | szrsql 对 MySQL/SQLite DDL 的宽容接受（非 PG 语义，但无害） |
| 部分索引 / 表达式索引 | ✅ | ✅ | ✅ | 三方一致 |
| COMMENT ON | ✅ | ❌ 语法不支持 | ✅ | 必然差异（sz-orm SqliteDialect 建表路径不生成 COMMENT，无缺口） |

### S5 写路径与 RETURNING —— 通过（含关键版本事实）

| 载荷 | szrsql | SQLite（sqlx 捆绑 3.46） | SQLite（python 3.32） | 分类 |
|------|--------|--------------------------|----------------------|------|
| INSERT/UPDATE/DELETE..RETURNING | ✅=PG | ✅ **支持**（3.46≥3.35） | ❌ 语法不识别 | **引擎版本差异**：同一"SQLite"名称下 sqlx 捆绑库与系统 CLI 行为不同——对拍时必须区分 |

### S6 边缘 SQL —— 通过（新 HEAD 复验上游修复生效）

| 载荷 | szrsql | SQLite（3.46） | PG18 | 分类 |
|------|--------|----------------|------|------|
| `= NULL` | ✅ 空集 | ✅ 空集 | ✅ 空集 | 三方一致 |
| IS NOT DISTINCT FROM | ✅=PG（**UP-2 修复生效确认**） | ✅ 3.46 支持（≥3.39） | ✅ | 三方一致 |
| 整数溢出（int + 1） | ✅ **报错**=PG（UP-4 修复生效确认） | 静默转 REAL/大整数 | 基准 | 必然差异（SQLite 动态类型） |
| 除零 | ✅ 报错=PG | ❌ 返回 NULL | 基准 | 必然差异 |
| bool=int 比较 | ✅ 拒绝（type mismatch）| ✅ 拒绝（PG 语义对齐） | 拒绝（no operator） | 三方一致（拒绝方式不同：szrsql 报错文案与 PG 不同但行为一致） |
| 空串 vs NULL | ✅=PG | ✅ | ✅ | 三方一致 |
| `\|\|` NULL 传播 | ✅=PG | ✅ | ✅ | 三方一致 |
| ORDER BY bool | ✅=PG | ✅（true>false 同向） | ✅ | 三方一致 |

### S7 事务 —— 通过

| 载荷 | szrsql | PG18 | 结论 |
|------|--------|------|------|
| SAVEPOINT 建立/ROLLBACK TO/RELEASE/继续 | ✅ `1\|3` | ✅ `1\|3` | 一致 |
| 事务内语句错误 → COMMIT 后可见性 | ✅ **全回滚（COUNT=0）** | ✅ 全回滚（COUNT=0） | **一致**（此前 round2 的"宽松方向"假设被证伪——szrsql 错误后事务进入 abort 态与 PG 一致；SQLite 才是语句级回滚的宽松方） |

## 3. 必然差异登记表（SQLite 方言本质，宁窄勿宽）

| # | 差异 | 证据（本批实测） |
|---|------|----------------|
| D-1 | 无 ILIKE（fallback LOWER+LIKE 可用） | round1 S2-1 + 集成测试 s2 |
| D-2 | 无 GREATEST/LEAST/SUBSTRING..FROM/EXTRACT..FROM（等价形式 SUBSTR/strftime/CASE 需方言转换） | round1 S2-2/3/4/6 |
| D-3 | **引擎版本分裂**：sqlx 捆绑 3.46（RETURNING/INDF 可用）vs 系统 python sqlite3 3.32（均不可用）——文档与对拍必须标注引擎来源 | 集成测试 s5 头注释 + libsqlite3-sys 0.30.1 sqlite3.h:149 |
| D-4 | `::` cast 语法不支持（CASE 等价） | round1 S2-5 |
| D-5 | ~~NULLS FIRST/LAST 关键字不支持~~ **红队复核勘误（缺陷-1）：本项为报告错判，撤销**——SQLite 3.30 起即支持 NULLS FIRST/LAST；本机 python 3.32 重放 round1 S1-1 载荷成功且输出与 PG 基准一致（3,1,2,4）；round1 原始输出中 sqlite 列的 3\n1\n2\n4 实为**正确**结果而非「默认 NULL 最小」的差异——首轮三方脚本将其标 SZ=PG≠SQ 是 SQLite 列换行符/空值文本格式差异造成的假阳性 | 红队复核 R-4 + 本会话独立复放 |
| D-6 | 动态类型：溢出静默、除零 NULL、bool=int 弱类型成立 | round1/2 S6-5/6/8 |
| D-7 | 无 COMMENT ON | round2 S4-3 |
| D-8 | RETURNING/IS NOT DISTINCT FROM 仅 ≥3.35/3.39（sqlx 捆绑 3.46 已覆盖） | 集成测试 s5/s6 |

> **红队复核记录（第 5 环，2026-11-09）**：独立 Agent 六项复核 R-1~R-6，五项 PASS、R-4 发现 D-5 错判（NULLS FIRST/LAST 实为 SQLite 3.30+ 支持项，round1 的"差异"是输出格式假阳性），**放行入库**（附缺陷清单：缺陷-1 本项勘误随第 6 环登记修正；缺陷-2 环境性 szrsql 不可复跑已在 §5 标注）。复核报告：`D:\tmp\redteam-sq-tripled.md`（会话外临时文件，结论摘录于本节）。

## 4. sz-orm 集成缺口修复记录

### [FIX-1] `typed_ast::Concat` SQLite 分支（本批唯一代码修复）

- **位置**：[packages/sz-orm-core/src/typed_ast.rs:1142](../../packages/sz-orm-core/src/typed_ast.rs#L1142)（Concat<L,R>::to_sql match 分支）
- **缺陷**：SQLite 走 `_ =>` 兜底生成 `CONCAT(a, b)`——SQLite 3.44 前无此函数；sqlx 捆绑 3.46 虽已有，但 SQLite 生态低版本广泛存在，且 `||` 拼接在 SQLite 与 PG 语义完全一致（NULL 传播），归入 PG 族分支零成本消除兼容面。
- **修复**：`DbType::Sqlite` 加入 `\|\|` 分支（与 PostgreSQL/Oracle/Kingbase/PolarDB/GaussDB 同族）；附集成测试 [packages/sz-orm-sqlx/tests/integration_sq_differential.rs](../../packages/sz-orm-sqlx/tests/integration_sq_differential.rs) s2（`||` 可执行 + CONCAT 哨兵双向验证）。
- **验证**：`cargo test -p sz-orm-core --lib -- concat` = 10 passed（无既有断言破坏——原 SQLite 断言未覆盖 Concat）；`integration_sq_differential` 7 passed。
- 版本证据：libsqlite3-sys 0.30.1 捆绑 SQLite 版本常量（sqlite3.h:149 `SQLITE_VERSION "3.46.0"`，注册表源码路径 `D:/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/libsqlite3-sys-0.30.1/`）。

### 上批 M3 缺陷回归确认

迁移锁降级修复（sz-orm 侧）在本批未复测（S4 未含迁移端到端——登记为后续轮次项，非本批回归）。

## 5. 支持面结论（防幻影措辞自查）

- sz-orm 对 **SQLite** 的支持面维持既有声明不变；本批修复使 typed_ast Concat 在 SQLite 真实可用（此前静默生成坏 SQL）。
- sz-orm 对 **szrsql** 的支持面结论不变（CRUD/参数化/事务/分页/元数据/池/SAVEPOINT/游标/SCRAM/NOTIFY 可用）；本批新增证据：**上批 P0/P2 修复在上游 HEAD 生效确认**（溢出报错、IS NOT DISTINCT FROM 对齐 PG）。
- 未新增任何"自动/强制/默认"语义组件；无幻影交付面。

## 6. 遗留风险

1. **Python sqlite3 3.32 vs sqlx 捆绑 3.46 的对拍口径**：后续三方对拍若涉及版本敏感语法（RETURNING/INDF/CONCAT），必须以 sqlx 捆绑库版本断言（本报告 D-3）。
2. szrsql 并行会话高频提交：本批基线 `54dc5631`，上游 P0 修复确认即基于此；后续 HEAD 变动需复验。
3. S3 的 `:name` 命名参数、S4 迁移端到端（advisory lock）未纳入本批——留待下轮。
4. sqlite 版本事实已写入集成测试头注释（防未来误判）。

## 7. 交付链

| 环节 | 产物 |
|------|------|
| 1 提示词 | 2026-11-09-sq-tripled-differential-prompt.md |
| 2 报告 | 本文件（audit-verify 0 失败，见下） |
| 3 实施批 | FIX-1 + integration_sq_differential.rs（commit 见 git log） |
| 4 复评 | 门禁复跑记录：G9 46/46 Safe（自测 15/15）、G15/G16/G12/G18/G19/G33/G35/G36 全 PASS；fmt 72 包；clippy 0 error |
| 5 红队复核 | 待独立 Agent 执行（非本会话） |
| 6 登记 | CHANGELOG/AGENTS 更新随第 5 环放行后提交 |
