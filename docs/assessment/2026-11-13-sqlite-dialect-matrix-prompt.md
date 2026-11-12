# 新会话提示词：sz-orm SQLite 方言矩阵真连验证（v1，2026-11-13）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：sz-orm v10.13.4。方言矩阵模式已在 MySQL/Oracle 落地。SQLite 此前在三方对拍（S1~S7）中作为**被测引擎**覆盖了部分语义面，且 FIX-1（Concat→||）已修——本批按 MY/OR 模板补齐 **sz-orm SQLiteDialect 自身生成面**的独立矩阵。
> **版本事实（D-3 延续）**：sqlx 捆绑 libsqlite3-sys 0.30.1 = **SQLite 3.46**（集成测试真实引擎）；系统 python sqlite3 = 3.32（CLI 对拍引擎，功能面偏旧）。矩阵断言以 **3.46** 为准，CLI 探针须标注引擎版本。
> **六环闭环**：按评测交付闭环链条执行。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：以 SQLite 方言能力面为载荷做真连矩阵验证，产出三分类清单（**sz-orm 方言缺口**[修复]/ **SQLite 引擎版本特有**[登记]/ **文档措辞**），并修复 sz-orm 侧可修复项。

**目标仓库与环境**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4）
- 被测：SQLite 3.46（sqlx 捆绑 bundled，`sqlite::memory:` / 文件库，**免服务免配置**——矩阵测试无条件必跑）
- 方言层：`packages/sz-orm-core/src/dialect.rs`（SQLiteDialect）
- 通道：`packages/sz-orm-sqlx`（Sqlite 通道）；既有 `integration_sqlite.rs` 25 项 + `any_sqlite_test.rs` 不重复
- 参考：`packages/sz-orm-core/tests/integration_duckdb.rs`（同嵌入式模式先例）

## 环境探活（第一步）

```bash
cargo test -p sz-orm-sqlx --test any_sqlite_test 2>&1 | tail -2   # 通道探活
python -c "import sqlite3; print('python 引擎', sqlite3.sqlite_version)"  # 3.32，仅 CLI 探针用
```

## 评测矩阵（SQLite 方言特有面；每载荷 = 方言层生成 + 真库执行 + 行为断言三步）

| # | 能力块 | 载荷（方言生成面） | 重点 |
|---|--------|-------------------|------|
| SQ1 | 引擎版本契约 | bundled 3.46 特性面：STRICT 表（3.37+）/RETURNING（3.35+）/`IS NOT DISTINCT FROM`（3.39+）在**方言生成下的断言** | 三方批已验语义——本批验证**方言层是否有意暴露/规避**这些特性（如 RETURNING 回填路径是否启用）；低于 3.46 的兼容策略登记 |
| SQ2 | 标识符三种 quoting | 双引号/反引号/方括号（SQLite 三种都收）——方言 `quote()` 用哪种；保留字表列名；Unicode 标识符 | G2 同型：约束名/索引名/FK 臂 quote 一致性 |
| SQ3 | Upsert 家族 | `ON CONFLICT (col) DO NOTHING/DO UPDATE`（3.24+）；`INSERT OR IGNORE / OR REPLACE`（方言 `build_insert_or_ignore_prefix`——DuckDB 同型先例）；**OR REPLACE 与 ON CONFLICT 混用禁区** | 生成 SQL 真库执行 + 冲突行为断言；OR REPLACE 是「删后插」语义（触发器/外键副作用）探针 |
| SQ4 | 主键回填与 rowid | `RETURNING`（3.35+）回填通道；INTEGER PRIMARY KEY = rowid 别名；`AUTOINCREMENT`（严格单调，防复用 rowid）；**WITHOUT ROWID 表**（方言生成面） | 主键回填走 RETURNING 还是 last_insert_rowid()；WITHOUT ROWID 下 last_insert_rowid 失效探针 |
| SQ5 | PRAGMA 面 | `PRAGMA foreign_keys`（**默认 OFF**！——sz-orm 外键行为探针：方言/连接层是否主动开）；`journal_mode=WAL`；`busy_timeout`；`PRAGMA table_info` 元数据面 | 外键默认关 = **sz-pay 类业务静默丢 FK 约束**的风险探针——连接层是否应统一 `PRAGMA foreign_keys=ON`（登记裁决）；WAL 与并发读写 |
| SQ6 | 类型亲和性 | INTEGER 列存 TEXT（弱类型存储）→ sz-orm Value 映射往返不串型；JSON1 函数（`json_extract`/`->>`3.38+）生成面；REAL 精度 | 弱类型往返是 SQLite 特有面——Value 枚举序列化链路系统性过 |
| SQ7 | 边缘与并发 | 三值 NULL；除零返回 NULL（不报错）；空串≠NULL；FTS5 全文（方言 `full_text_search` 生成面——G-OR9 同型扫描：列名 quote）；**并发写 SQLITE_BUSY**（WAL 下双连接写锁探针 + busy_timeout 行为） | FTS5 列名 quote（横向批第 12+1 方言补漏位）；BUSY 错误码到 sz-orm DbError 的映射 |

**不纳入**（登记理由）：`PRAGMA optimize/vacuum` 运维面、自定义 SQLite 扩展编译、See-JE（加密版）。

## 硬性要求

- **语义基准**：SQLite 官方文档（以 bundled 3.46 为准）+ 真库实测；禁止拿 PG18/MySQL 当仲裁
- **sz-orm 方言缺口**：修复 `dialect.rs` SQLiteDialect + 单测 + 真连断言；DuckDBDialect 同型面（`build_insert_or_ignore_prefix` 共享）顺带回归
- **参数化铁律**（门禁 9）；零 unsafe、零 todo!
- **测试产物**：新增 `packages/sz-orm-core/tests/integration_sqlite_matrix.rs`（**内存库免 env，无条件必跑**——与 integration_szrsql_matrix 的 env 跳过模式不同）
- **提交纪律**（硬约束 13）；串行 cargo；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / clippy -D warnings / fmt 72 包全绿
2. `cargo test -p sz-orm-core --test integration_sqlite_matrix`（无 --ignored）：SQ1~SQ7 全过
3. `cargo test -p sz-orm-core --test integration_sqlite`：既有 25 项不回归；`integration_sq_differential` 8 项不回归
4. 三分类清单：每条附证据（生成 SQL + 真库输出 + `dialect.rs` file:line）
5. 门禁：G9（--strict + 自测 15/15）/G12/G15/G16/G37/G33/G35/G36 全过
6. 报告 `docs/assessment/2026-11-13-sqlite-dialect-matrix.md`，audit-verify 0 失败；六环登记（CHANGELOG/AGENTS/commit）

## 环境坑位速查

- git 在 `D:\Git`；`cargo fmt --all` 用 `bash scripts/check-fmt-all.sh`
- **双引擎纪律**（D-3）：psql 式 CLI 探针走 python 3.32（部分特性无）；Rust 测试走 sqlx 捆绑 3.46——报告引用行为必须标注引擎与版本
- 内存库每连接独立（`sqlite::memory:`）——跨连接探针（WAL/BUSY/文件库）须开文件库并清理
- G37 门禁（dialect DDL/FTS 裸标识符检测）已上线——修复 quote 缺口时跑 `python scripts/check-dialect-ddl-quoting.py --self-test`
- 临时脚本放 `D:\tmp`；`python -I` 运行
