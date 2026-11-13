# SQLite 方言矩阵第 5 环红队独立复核——2026-11-13

> 复核对象：commit `cc29f8a6`（SQ1~SQ7 六环闭环批次）+ 报告 `docs/assessment/2026-11-13-sqlite-dialect-matrix.md`。
> 复核者：红队独立 Agent（非实施 session）。探针 crate：`D:\tmp\redteam_sq_probe`（rusqlite 0.32 bundled，独立于被测测试套件）。

## 裁决：**条件 PASS**

阻断缺陷 0，P2 缺陷 3（均不否定矩阵测试与方言缺口 0 结论本身，但门禁声称/登记归属须修正）；P3 建议 2。

---

## 1. 机械复跑（自取 EXIT 码）

| 项 | 命令 | 结果 | EXIT |
|---|------|------|------|
| 矩阵 | `cargo test -p sz-orm-core --test integration_sqlite_matrix` | **10 passed; 0 failed** | 0 |
| 回归 | `cargo test -p sz-orm-core --test integration_sqlite` | **25 passed; 0 failed** | 0 |
| 回归 | `cargo test -p sz-orm-sqlx --test integration_sq_differential -- --include-ignored` | **8 passed; 0 failed** | 0 |
| 回归 | `cargo test -p sz-orm-core --test integration_duckdb` | **7 passed; 0 failed** | 0 |
| 回归 | `cargo test -p sz-orm-cabi --lib` | **88 passed; 0 failed** | 0 |
| G37 | `python scripts/check-dialect-ddl-quoting.py` | 0 命中；`--self-test` 10/10 | 0 |
| G9 | `python scripts/check-sql-injection-auto.py --strict` | 46/46 Safe；自测 15/15 | 0 |
| G3 局部 | `cargo clippy -p sz-orm-cabi --all-targets -- -D warnings` | Finished，0 error | 0 |
| audit-verify | `bash scripts/audit-verify.sh docs/assessment/2026-11-13-sqlite-dialect-matrix.md` | 通过 1 / 失败 0 / **警告 13**（见 P2-1） | 0 |

报告声称的测试数（10/25/8/7/88）全部复现一致。

## 2. 报告断言 vs 独立探针（不信测试自身，独立 crate 复现）

探针输出（`D:\tmp\redteam_sq_probe\src\main.rs`，`cargo run` EXIT=0，`rusqlite::version()` = **3.46.0**）：

| 断言 | 探针结果 | 与报告一致 |
|---|---|---|
| bundled 默认 foreign_keys=ON | 孤儿 FK 插入 `Err(ConstraintViolation, 787 "FOREIGN KEY constraint failed")` | ✅ |
| WITHOUT ROWID `last_insert_rowid()` 不推进 | base=1 after=1 unchanged=true；RETURNING 在 WITHOUT ROWID 表返回 v=2 | ✅ |
| `OR REPLACE + ON CONFLICT(k) DO NOTHING` 混用被接受 | `Ok(0)`（无报错），行保持 (a,1)——DO NOTHING 生效 | ✅ |
| 三种 quoting（双引号/方括号/反引号）都收 | 三种均返回 'x' | ✅ |
| STRICT 拒绝 TEXT 入 INTEGER | `Err(3091 "cannot store TEXT value in INTEGER column")` | ✅ |
| FTS5 + MATCH 表级 | match count=1 | ✅ |
| 引擎版本 | 3.46.0；Cargo.lock `libsqlite3-sys 0.30.1`（Cargo.lock:3903） | ✅ |
| 编译旗标 | `D:/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/libsqlite3-sys-0.30.1/build.rs:123` = `-DSQLITE_DEFAULT_FOREIGN_KEYS=1`、`:129` = `-DSQLITE_ENABLE_FTS5`（亲自 grep registry 源码；registry 路径在仓库外，audit-verify 无法机检，探针运行时行为为行为级佐证） | ✅ |

「混用禁区修正」如实记录：报告 §SQ3 明确写「原假设『被拒绝』不成立」，未粉饰；探针证实混用形态无报错且 DO NOTHING 语义生效。**报告可信度此项成立。**

## 3. 「方言缺口 0 处」审查

逐函数审 `packages/sz-orm-core/src/dialect.rs` SqliteDialect（1108~1397 行）：

- `quote`/`quote_into`（dialect.rs:1145/1150）：双引号 + doubling，正确。
- `escape_string`（1159 区）：仅 `''` 翻倍——SQLite 标准做法，参数化主通道下仅为字面量兜底。
- `json_extract`（~1173）：path 经 `escape_string` 注入单引号字面量；**column 参数裸拼**（与 MySQL/PG/Oracle 等全部方言同型，调用方约定传列名/表达式，见 §6 建议-2，非本批新缺口）。
- `full_text_search`（~1195）：列名逐个 `self.quote(c.trim())`（FTS 横向批产物，dialect.rs:1183 附近），keyword 经 escape_string。
- `concat`（~1205）：COALESCE 包裹 + `||`，依赖除零/NULL 语义，探针与 SQ6 验证。
- `build_create_table`/`build_alter_table`：表/列/索引名全 quote；ModifyColumn 与 AddForeignKey 生成 `--` 注释提示串（quote-gate-safe 登记）。
- `build_upsert_on_conflict`：conflict/update 列全 quote，EXCLUDED 无 quote（SQLite 保留前缀，正确）；空 update 列退化 DO NOTHING。
- `build_pagination_clause`：LIMIT/OFFSET 数值型参数（usize）无注入面；仅 offset 时 `LIMIT -1 OFFSET n` 符合 SQLite 语义。

G37 门禁 0 命中 + 上述逐函数审读，**「方言缺口 0 处」结论成立**。

## 4. 发现的缺陷清单

### P2-1：报告 file:line 引用 13/14 被 audit-verify 跳过，验证覆盖仅 1/14

- 证据：audit-verify 输出「⚠️ dialect.rs:1146 — 仓库根未找到该文件（跳过）」×12（dialect.rs/build.rs 均为省略包前缀的短路径），仅 `packages/sz-orm-core/Cargo.toml:364` 一条真正校验通过。
- 报告头部声称「audit-verify 0 失败」：字面为真，但 14 条引用中 13 条**根本未被机器校验**。本复核已逐条人工核对这些短路径行号真实存在且内容匹配（如 dialect.rs:1146 = `fn supports_returning`、dialect.rs:1217 = `fn last_insert_id_sql`、build.rs:123 = FK 旗标、Cargo.toml:360~364 = `integration_pg_matrix` 注册），行号无编造——但后续批次引用应写全路径（`packages/sz-orm-core/src/dialect.rs:1146`）使 audit-verify 可机检。

### P2-2：G1 fmt「72 包通过」声称与提交内容矛盾——`integration_gaussdb.rs` 随本 commit 入库即带 fmt 漂移

- 证据：`cargo fmt -p sz-orm-core -- --check` 报 `Diff in packages/sz-orm-core/tests/integration_gaussdb.rs` 行 262 处；fmt 漂移语句为 [must-contain: let mode_raw: String =] packages/sz-orm-core/tests/integration_gaussdb.rs:265（`let mode_raw: String =` 跨行）；`git diff cc29f8a6 -- packages/sz-orm-core/tests/integration_gaussdb.rs` 为空（该文件自 cc29f8a6 起未再改动，漂移由本 commit 引入）；`bash scripts/check-fmt-all.sh` → `FMT_FAIL: packages/sz-orm-core`。报告 §4 门禁表 G1 行写「本批顺带修复 sz-orm-parallel/sz-orm-core 既有漂移」，但 gaussdb 漂移未修。
- 即：以 cc29f8a6 提交内容复跑 G1 **不可能** 72 包通过。报告 §4 门禁表的 G1 ✅ 不可在本 commit 内容上复现（可能是提交时窗口/工具差异，或提交前漂移检测漏跑该文件）。截至 HEAD（a2e5762b）仍未修复。

### P2-3：`bigdecimal 0.4` + sqlx `uuid/bigdecimal` feature 新增物理落在本 commit，但归属登记在 PG 矩阵线且本批 commit message 未提及

- 证据：`git log -S bigdecimal -- packages/sz-orm-core/Cargo.toml` 唯一命中 cc29f8a6（diff +9/-1，sqlx features 行 + bigdecimal = "0.4" + PG 注释行）；而 PG 矩阵 commit a2e5762b 的 CHANGELOG 声称「测试基建：sz-orm-core dev-deps 新增 bigdecimal 0.4 + sqlx uuid/bigdecimal feature」——登记位置与物理 hunk 所在 commit 不一致；cc29f8a6 commit message 与其 CHANGELOG 条目（只列 required-features/clippy/fmt 三类）均未提及此依赖新增。
- 内容本身正确（PG4 NUMERIC 通道需要）且已登记，但按硬约束 13「提交必须连带依赖文件 + 不打包并行会话半成品」及归属可追溯性要求，物理 hunk 与登记线应对齐（后续发布面审计按 commit 追溯时会错位）。

### P3-1：本 commit 收纳 MZ-* 容器矩阵 6 文件 1141 行测试，commit message 零提及

- 证据：`git show cc29f8a6 --stat` 中 integration_mssql/mariadb/tidb/clickhouse/db2/gaussdb 共 +1141 行；MZ docs commit 05a2380a 注明「测试落点 cc29f8a6 hunk 级隔离收纳」——有意为之且在后续 commit 有说明，但 cc29f8a6 自身仅声称 mssql/tidb/mariadb 三处 clippy 修复，透明度不足（三线并行会话的产物混入单 commit）。

### P3-2（建议，非缺陷）：`json_extract` 的 column 参数全方言裸拼（含 SQLite，dialect.rs ~1173）

- 与本批 FTS quote 横向修复同型的潜在引用面，但属既有全方言共同约定（调用方传列名/表达式），非本批引入、非本批登记范围。建议下批横向扫描时与「其余方言 AddIndex cols.join 裸拼」一并评估。

## 5. 并行线修复归属与 cabi 专项审查

- **cabi lib.rs**：diff 为 doc-comment 复位（`sz_orm_pool_new` 的 Safety 文档块从 `normalize_szrsql_dsn` 头部错误挂靠处移回正确位置，`#[no_mangle]` 随之归位）——无逻辑改动。`normalize_szrsql_dsn`（packages/sz-orm-cabi/src/lib.rs:259）函数体完整、`sz_orm_pool_new`（packages/sz-orm-cabi/src/lib.rs:277）无重复/残缺体；`cargo test -p sz-orm-cabi --lib` 88/88、clippy -D warnings 0 error。**无缺陷。**
- **pg_matrix required-features**（packages/sz-orm-core/Cargo.toml 的 `[[test]] integration_pg_matrix` 注册块，第 360~366 行附近）：最小修复，与报告/commit message 声称一致。
- **mssql/tidb/mariadb clippy 形态**：`.err().expect()` → `match`+panic，语义等价（expect 在 Ok 时 panic，match 臂保持同语义），无越权改动。
- **fmt 漂移文件**（clonecheap_parity/vectorized_parity/prepared_cache_parity/semantic_invariants* 等）：抽验 diff 均为纯 rustfmt 重排，零语义变化。

## 6. 登记一致性

- CHANGELOG `[Unreleased]` Added 段（SQLite 条目）：10 测试、25+8+7 回归、缺口 0、引擎事实 4 类——与本复核实测一致。**但**「G1 72 包全绿」声称见 P2-2；dev-deps 新增遗漏见 P2-3。
- AGENTS.md 已发布行 2026-11-13 SQLite 条目：与报告/实测一致。
- 报告一处小瑕疵：§4 回归行写「integration_sq_differential ✅ 8 passed」未注明该套件在 sz-orm-sqlx 包（不在 sz-orm-core），复跑时需 `-p sz-orm-sqlx --include-ignored`（P3 级措辞问题，数 8 正确）。

## 7. 处置要求（放行条件）

1. P2-2：立即修复 `packages/sz-orm-core/tests/integration_gaussdb.rs:265` fmt 漂移（`cargo fmt -p sz-orm-core`）并登记勘误——G1 声称必须在仓库内容上可复现。
2. P2-1：报告引用路径补全为仓库根相对全路径后重跑 audit-verify，使 14/14 可机检（或在下批报告起强制全路径）。
3. P2-3/P3-1：登记勘误（可在下批 CHANGELOG 条目补注 dev-deps 新增物理落点 cc29f8a6），不要求改历史 commit。

质量官 sz-orm-qa 拥有最终否决权。
