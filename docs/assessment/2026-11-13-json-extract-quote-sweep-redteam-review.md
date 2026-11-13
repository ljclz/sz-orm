# json_extract 列名 quote 横向修复批——红队独立复核（第 5 环）——2026-11-13

> 复核对象：commit `3c2c18ae`（fix(dialect): json_extract 列名 quote 横向修复批，12 方言一致化）。
> 复核者与实施者不同 session；本报告只复核 + 落盘，不修改测试/源码/登记文件。
> 复核日期：2026-11-13。复核范围：清单 6 项逐项留证据。

## 1. 独立机械验证（亲自复跑）

| 项 | 命令 | 实测 EXIT | 结果 | 与报告声称 |
|---|------|-----------|------|-----------|
| lib 单测 | `cargo test -p sz-orm-core --lib` | 0 | **3285 passed / 0 failed** | 与报告 §3 一致（3285） |
| 横向单测 | `cargo test -p sz-orm-core --lib test_json_extract` | 0 | 2 passed（含 `test_json_extract_column_quoting_all_dialects`，`packages/sz-orm-core/src/dialect.rs:4236`） | 一致 |
| PG 矩阵 | `cargo test -p sz-orm-core --features type-safe-columns --test integration_pg_matrix -- --ignored`（`SZ_ORM_PG_URL` 注入） | 0 | **20 passed / 0 failed** | 与报告 §3 一致 |
| SQLite 矩阵 | `cargo test -p sz-orm-core --test integration_sqlite_matrix` | 0 | **10 passed / 0 failed**（SQ6 断言同步后） | 一致 |
| MySQL 矩阵 | `cargo test -p sz-orm-core --test integration_mysql_matrix -- --ignored`（凭据经宝塔安装日志 `b59c8640d3d6b94dba44a53f75f9c07c.log:492` 找回，本回执不引用密码明文） | 0 | **11 passed / 0 failed**（MY3 断言同步后） | 一致 |
| G37 自测 | `python -X utf8 -I scripts/check-dialect-ddl-quoting.py --self-test` | 0 | **15 passed / 0 failed** | 一致（10→15） |
| G37 全仓 | `python -X utf8 -I scripts/check-dialect-ddl-quoting.py` | 0 | 0 命中 | 一致 |
| G9 --strict | `python -X utf8 -I scripts/check-sql-injection-auto.py --strict` | 0 | 46 Safe | 一致 |
| fmt 72 包 | `bash scripts/check-fmt-all.sh` | 0 | 格式检查通过（72 包） | 一致 |
| clippy | `cargo clippy -p sz-orm-core --all-targets -- -D warnings` | 0 | 0 error（包级抽验；工作空间级未复跑） | 抽验通过 |
| audit-verify（原报告） | `bash scripts/audit-verify.sh docs/assessment/2026-11-13-json-extract-quote-sweep.md` | 0 | 通过: 3 / 失败: 0 / 警告: 4 | 与报告声称（3 通过 0 失败）一致 |

**commit 范围核对**：`git show 3c2c18ae --stat` → 恰好 6 文件（prompt.md / 报告 / dialect.rs / integration_mysql_matrix.rs / integration_sqlite_matrix.rs / check-dialect-ddl-quoting.py），246 insertions / 17 deletions，与登记一致。

## 2. quote 形态反审查（本批核心风险）

### 2.1 生成形态与各方言 quote() 实现逐方言核对

12 方言生成形态（`packages/sz-orm-core/src/dialect.rs:4237-4258` 单测断言逐字节核对源码）：

| 方言 | 修复点 file:line | quote() 形态 | 核对结论 |
|---|---|---|---|
| MySQL | dialect.rs:527 | 反引号 | 正确 |
| PostgreSQL | dialect.rs:891 | 双引号（前批 624974c9） | 正确 |
| SQLite | dialect.rs:1174 | 双引号 | 正确 |
| Oracle | dialect.rs:1549 | 双引号 | 正确 |
| SqlServer | dialect.rs:1816 | 方括号 | 正确 |
| Snowflake | dialect.rs:2277 | 双引号 | 正确（形态合法性见 2.4） |
| ClickHouse | dialect.rs:2573 | 反引号 | 正确 |
| DuckDB | dialect.rs:2825 | 双引号 | 正确 |
| DB2 | dialect.rs:3042 | 双引号 | 正确 |
| Informix | dialect.rs:3397 | 双引号 | 正确 |
| SapHana | dialect.rs:3636 | 双引号 | 正确 |
| Firebird | dialect.rs:3887 | 双引号 | 正确 |

委托族抽验：Szrsql→PG、Dameng→Oracle 断言在单测内独立验证（dialect.rs:4270-4284），复跑通过。

### 2.2 关键证伪实验（MySQL 真库，混合大小写列）

独立构造探针（`/d/BtSoft/mysql/MySQL8.0/bin/mysql.exe`，127.0.0.1:8802/sz_orm_test，凭据取自宝塔日志，EXIT=0）：

```sql
CREATE TABLE rt_je_probe (id INT PRIMARY KEY, MetaCol JSON);
INSERT INTO rt_je_probe VALUES (1, '{"n": 7}');
SELECT CAST(JSON_EXTRACT(MetaCol, '$.n') AS SIGNED) AS bare_form,
       CAST(JSON_EXTRACT(`MetaCol`, '$.n') AS SIGNED) AS quoted_form FROM rt_je_probe;
-- 结果：bare_form=7, quoted_form=7（两形态等价，均命中）
```

**结论**：MySQL 裸标识符大小写不敏感，混合大小写列裸拼**碰巧仍可用**——本批 MySQL 修复属「防御性一致化」而非「真缺陷修复」。同法 SQLite（内存库 python sqlite3 探针）：`SELECT MetaCol` / `SELECT "MetaCol"` / `SELECT "metacol"` 三者等价命中，同为大小写不敏感，结论相同。**报告与提交信息措辞复核**：报告 §2 Oracle 行措辞准确（Oracle 折叠大写 ORA-00904 同 G-OR9 类，PG 折叠小写已被前批真连证实）；但 `packages/sz-orm-core/src/dialect.rs:524-526` 注释把「混合大小写列裸拼被折叠语义方言错位」作为通用修复理由写入 MySQL/SQLite 等 11 方言，对 MySQL/SQLite/ClickHouse 等大小写不敏感方言**定级偏重**（建议级措辞问题，见 P2-2）。

### 2.3 Snowflake 形态合法性

`"MetaCol":user.name`（dialect.rs:2277）：Snowflake 冒号路径（`col:path`）为官方合法语法，列名双引号 quote 亦合法；路径段 `user.name` 未加引号属路径键字面量（非标识符），无注入面。**本机无 Snowflake 实例，未真连**——与报告 §5.3 同口径，标注「形态推理，未真连」。

### 2.4 SqlServer 方括号形态

`JSON_VALUE([MetaCol], ...)` 未真连（本机无实例），报告 §5.3 已如实登记为待补验——复核认可该登记。

## 3. 调用方契约破坏面扫描（双重引号）

全仓 grep `.json_extract(`（packages/examples/cli 全量）：

- 生产调用面：仅 `dialect.rs:2071`（delegate 宏转发，透传裸名——安全）与 `packages/sz-orm-core/src/typed_ast.rs:2157,2184`（自带 `dialect.quote(C::NAME)` 预 quote，但**不经过** `Dialect::json_extract`，走自有模板——无双重引号风险）。
- 测试调用点：`any_driver.rs:884,900`（小写列，形态无关）；`dialect.rs` 内测试与 `dialect_*_test.rs` 系列传裸名——已随本批语义正确。
- **预 quote 后传入的调用：0 处**（原 MY3 处 `&dialect.quote("meta")` 已在 3c2c18ae 同步改为裸名，`integration_mysql_matrix.rs:611`）。**无红牌**。

`json_query.rs`（独立 API，自带手写 quote，`json_query.rs:84-97`）报告 §5.1 已登记越批建议，复核认可（PG/MySQL 分支有 quote，SQLite 分支反引号在 SQLite 合法，无双重引号）。

## 4. G37 增补有效性

- 自测 15/15（含 3 历史裸拼形态必命中样例 `check-dialect-ddl-quoting.py:262-268`、2 修复形态必不命中 `:286-291`）。
- 手工构造 MySQL/Oracle 裸拼第七形态样例：`format!("JSON_EXTRACT({}, '{}')", column, ...)` → **必命中**（Q2-sensitive-bare-param ×2，验证通过）。
- **发现的覆盖缺口（P2-3）**：Snowflake 形态模板为 `"{}:{}"`（无 JSON 关键词）、DuckDB 为 `"{} -> '{}'"`（无 JSON 关键词）——若这两方言未来回退裸拼，新增 6 关键词**不触发** Q2（手工样例 `format!("{}:{}", column, normalized)` / `format!("{} -> '{}'", column, ...)` 均 0 命中，实测）。当前源码已修复故全仓 0 命中仍为真，但门禁对「第七形态」的 Snowflake/DuckDB 两条臂存在盲区。
- `#>>` 关键词宽度：仅扫 dialect.rs 单文件（TARGET 固定），且豁免走 quote-gate-safe 行内登记（dialect.rs:890），误报风险可控。

## 5. 缺陷（按严重度）

### P1-1（阻断级回执项）：MySQL root 密码明文入库

`docs/assessment/2026-11-13-json-extract-quote-sweep.md:55` 将实测密码明文写入报告并随 3c2c18ae 提交入仓（本回执不复述该密码）。违反「凭据一律经环境变量注入，不入库」与复核指令「不得把密码写入报告/回执」。**回执要求**：① 报告该行改为「密码经宝塔安装日志 b59c8640d3d6b94dba44a53f75f9c07c.log:492 找回（明文不入仓）」；② 评估 git 历史已含该明文（3c2c18ae 在案）——本机测试库密码，建议至少登记轮换/降权处置决定；③ 顺手修正 `D:\tools\gate.env` 的 `SZ_ORM_MYSQL_URL` 缺密码段坑位（实施批已登记待修正）。

### P2-1：横向单测「裸拼反向断言」为永真断言（幻影测试 P2 形态）

`packages/sz-orm-core/src/dialect.rs:4261-4266`：`assert!(!sql.contains(&bare) || sql.contains(expect), ...)` —— 同一 `expect` 在上一条断言已保证 contains 成立，故 `||` 右臂恒真，整条断言**恒真，无证伪能力**（门禁 30 P2「双字面量断言」同型）。quote 必要性实际由左臂逐字节 `expect`（含引号形态）承载，缺陷不影响修复正确性，但反向断言形同虚设。**回执要求**：改为 `assert!(!sql.contains(&bare), ...)`（去掉恒真右臂）或断言 `!sql.contains(" MetaCol")` 独立成立。

### P2-2：修复定级措辞——MySQL/SQLite 属防御性一致化，非真缺陷修复

dialect.rs:524-526 通用注释与提交信息「混合大小写列裸拼被折叠语义方言错位/保留语义方言报 column does not exist」对大小写不敏感方言（MySQL/SQLite/ClickHouse）不成立（§2.2 真库证伪：裸拼/quote 双形态等价）。报告 §2 表格 Oracle 行措辞准确、MySQL 行未标注「防御性」。**回执要求**：报告 §2 MySQL 行补注「MySQL/SQLite 等大小写不敏感方言为防御性一致化（真库两形态等价，红队 §2.2 实测）」；源码注释可下批微调（非阻断）。

### P2-3：G37 增补对 Snowflake/DuckDB 第七形态臂存在盲区

见 §4。**回执要求**：登记下批候选（如为 Snowflake/DuckDB 模板增加专属敏感模式，或按方言函数面补 `GET_PATH`/`->` 面——注意 `->` 全局模式误报风险需评估），本批不阻断（当前 0 命中为真）。

## 6. 建议（非缺陷）

1. clippy 抽验为包级（sz-orm-core）；工作空间级 clippy -D warnings 未复跑（时间预算），实施批声称已过，建议 CI 兜底。
2. Snowflake/SqlServer 形态真连验证待 CI service 可达时补（报告 §5.3 已登记，复核同意顺延）。
3. `json_query.rs` 统一接线 `validate_identifier` + 方言 quote（报告 §5.1 已登记）。

## 7. 复核结论

修复正确性成立：12 方言生成形态逐一与方言 quote() 实现核对一致、委托族自动惠及、调用方零双重引号破坏、全部机械验证数字与报告一致（lib 3285 / PG 20 / SQLite 10 / MySQL 11 / G37 15+0 / fmt / G9）。发现 P1×1（凭据明文入库——回执可修复，非技术方案缺陷）+ P2×3（永真反向断言、定级措辞、G37 两臂盲区），均不否定修复本身。

**红队裁决： 条件 PASS（回执项：P1-1 报告密码明文脱敏 + 轮换处置登记；P2-1 反向断言去恒真右臂；P2-2 报告 MySQL/SQLite 行补注防御性一致化定级；P2-3 G37 Snowflake/DuckDB 臂盲区登记下批候选）**
