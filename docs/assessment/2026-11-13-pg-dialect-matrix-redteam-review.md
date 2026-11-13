# 红队独立复核报告：PostgreSQL 方言矩阵真连验证批（PG1~PG7）——2026-11-13

> 评测交付闭环链条第 5 环。复核者与实施者不同 session（红队独立复核，质量官 sz-orm-qa 语义拥有否决权）。
> 复核对象：commit `a2e5762`、`packages/sz-orm-core/tests/integration_pg_matrix.rs`（1265 行，20 测试）、报告 `docs/assessment/2026-11-13-pg-dialect-matrix.md`、CHANGELOG [Unreleased] + AGENTS.md 登记。
> 凭据口径：DSN 一律引用 `SZ_ORM_PG_URL` 环境变量名。

---

## 1. 独立机械验证（第 4 环要求）

### 1.1 矩阵测试套件（红队亲自运行）

```
export SZ_ORM_PG_URL="postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"
cargo test -p sz-orm-core --features type-safe-columns --test integration_pg_matrix -- --ignored --test-threads=1
→ test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.68s   EXIT=0
```

**与报告 §1 / CHANGELOG / AGENTS.md 声称的 20/20 一致**（20 测试名逐一对上：pg1×3 / pg2×1 / pg3×2 / pg4×2 / pg5×3 / pg6×3 / pg7×6）。

### 1.2 audit-verify 复跑

```
bash scripts/audit-verify.sh docs/assessment/2026-11-13-pg-dialect-matrix.md
→ ✅ 通过: 24   ❌ 失败: 0   ⚠️ 警告: 12   EXIT=0
```

与登记声称「audit-verify 24 通过 0 失败」一致（12 警告均为行号偏移类提示，非编造证据）。

### 1.3 回归套件

```
cargo test -p sz-orm-core --test integration_pg -- --ignored
→ test result: ok. 22 passed; 0 failed   EXIT=0
```

与声称「integration_pg 22 零回归」一致。**szrsql 委托族回归（15+8）未复跑**：本机 6543 端口实测无监听（`netstat -ano | grep 6543` 零命中），szrsql 实例已停——按复核指令该项跳过并注明；报告 §4 声称的 szrsql 数字属实施时快照，无法独立复核，不作为放行依据但也不构成反证（该批零方言改动，委托族回归风险低）。

### 1.4 门禁抽验

| 门禁 | 命令 | 结果 |
|---|---|---|
| G9 SQL 注入扫描 --strict | `python -X utf8 -I scripts/check-sql-injection-auto.py --strict` | Total 46 / Safe 46 / Unsafe 0，PASSED，EXIT=0 |
| G9 自测 | `python -X utf8 -I scripts/test_check_sql_injection_auto.py` | 15 passed / 0 failed |
| G37 方言 quote 门禁 | `python -X utf8 -I scripts/check-dialect-ddl-quoting.py` | 0 命中；--self-test 10/10 |
| G1 fmt | `bash scripts/check-fmt-all.sh` | **FMT_FAIL: packages/sz-orm-core（1 文件漂移，见 §4 缺陷 D-2）** |
| G3 clippy（矩阵 feature 组合） | `cargo clippy -p sz-orm-core --features type-safe-columns --all-targets -- -D warnings` | **2 errors（见 §4 缺陷 D-1）** |

---

## 2. 测试质量反审查（门禁 30 / 硬约束 12 语义）

### 2.1 幻影测试形态扫描

逐测试核对 `packages/sz-orm-core/tests/integration_pg_matrix.rs` 全部 20 测试：
- **零断言测试：0 个**（最弱的 `pg5_index_if_not_exists_idempotent` 亦含 `supports_if_not_exists` 断言 + 双次执行幂等探针，`:789`、`:799`）。
- **双字面量断言：0 个**（无 `assert_eq!(x, x)` 形态）。
- **永真断言：0 个**（无 `>= 0.0` 类恒真比较；`pg2 :528` 的 `assert!(id > 0)` 依赖 BIGSERIAL 回填副作用，非恒真）。
- **结果丢弃：0 个**（`let _ = ` 仅出现在刻意探错场景，均紧跟 `assert!(err.is_err())`，如 `:328-334`、`:490-493`）。

### 2.2 行为断言真库副作用抽查

- **PG3-a EXCLUDED 行为级**（`:579-633`）：两批 upsert 后断言 3 行 + 冲突行 `name` 保持 `"a"`（不在 update_columns）+ `value` 更新为 111 + 新行 `"c"` 插入——四个断言分别排除「全量覆盖写」「upsert 退化纯 insert」「DO NOTHING 误用」三种假通过路径，非 SQL 字符串形态断言。
- **PG1-a 大小写对照**（`:326-334`）：裸混合大小写表名真库报错作为 quote 必要性对照臂——负向对照设计正确。
- **PG5-b CONCURRENTLY 探针**（`:821-833`）：事务内执行断言 `err.is_err()`——真库拒绝为唯一可能来源（同形态事务外 `:817` 先成功），对照成立。

### 2.3 SKIP LOCKED 假通过证伪实验（红队自行设计对照）

质疑点：`pg7_for_update_skip_locked`（`:1185`）断言 `skipped.len() == 0`，若 WHERE 条件本身不匹配，0 行与「锁跳过」不可区分。红队在 PG18.4（psql 直连，`SZ_ORM_PG_URL` 同实例）设计三臂对照：

```
臂 A（同 WHERE 锁定行，无 SKIP LOCKED，lock_timeout 2s）→ ERROR: canceling statement due to lock timeout
臂 B（同 WHERE 锁定行 + SKIP LOCKED）→ 0 行
臂 C（未锁定行 value=2 + SKIP LOCKED）→ 返回 'b'
```

臂 A 证明同条件不加 SKIP LOCKED 会阻塞报错（排除条件不匹配），臂 C 证明 SKIP LOCKED 不误跳未锁行——测试 `:1169-1185` 的 0 行断言**确为锁跳过语义**，不可能假通过。测试本身的 tx1/tx2 双事务结构（`:1167-1187`）与红队三臂结论一致。

### 2.4 其余抽查

- PG7-a 三值 NULL（`:1063-1106`）：IS NULL 命中 2 行 / IS NOT NULL 命中 2 行 / 空串精确匹配 1 行，数据构造含 NULL 两行 + 非空两行 + 空串一行，断言值与数据集互斥可区分。
- PG6-a CTE 参数顺序（`:924`）：`assert_eq!(params, vec![Value::I64(1)])` 为参数序契约的真断言（非恒真）。

**结论：20 测试无幻影形态，行为断言均验证真库副作用。**

---

## 3. 「方言缺口 0 处」声称复核

### 3.1 G37 门禁

`check-dialect-ddl-quoting.py` 全仓 0 命中、self-test 10/10——报告声称一致。

### 3.2 人工抽查 PostgreSqlDialect 生成面（`packages/sz-orm-core/src/dialect.rs:814-1100`）

对 PG impl 全文 grep 裸拼形态，逐处核对：
- quote/quote_into（`:823-832`）：双引号 + doubling，正确；
- build_create_table / build_alter_table 三臂（含 FK 约束名组装后 quote，`:1010` 附近）/ build_upsert_on_conflict：均经 `self.quote`；
- full_text_search（`:889-898`）：列名逐个 quote + keyword 经 `escape_string`——FTS 批修复保持；
- 分页/CONCAT/bool_to_int：无标识符插值。

### 3.3 红队新发现一处同型 quote 缺口（G-OR1 家族第六形态）——`json_extract` 列名裸拼

`packages/sz-orm-core/src/dialect.rs:863-887` `PostgreSqlDialect::json_extract`：

```rust
format!("{}#>>'{{{}}}'", column, path_lit_escaped)   // column 未经 self.quote
```

真库证伪实验（psql 直连本批同实例）：

```
CREATE TABLE "rtJ" (id BIGSERIAL PRIMARY KEY, "metaCol" JSONB);
SELECT "metaCol"#>>'{x}' FROM "rtJ";   → 1（quote 列正常）
SELECT metaCol#>>'{x}'   FROM "rtJ";   → ERROR: column "metacol" does not exist
```

即 `json_extract("MetaCol", ...)` 混合大小写列在 PG 真库必炸（裸标识符折叠小写），与本批 PG1-a 证明的 quote 保留语义直接冲突。矩阵 `pg4_cast_and_numeric_precision`（`:721-727`）恰用小写列名 `meta` 探测，**未覆盖混合大小写形态**——「quote 类五批横向修复已系统性覆盖 PG 生成面」「缺口 0 处」的声称在该方法上不成立（G37 Q2 敏感模板清单未含 `#>>`，静态门禁漏检属预期内盲区）。

**定性**：非本批引入（历史存量），生产影响面有限（`typed_ast` 的 JsonGet/JsonPathGet 路径经 `dialect.quote(C::NAME)` 已 quote，`typed_ast.rs:2158/2185/2204/2225`；裸用 `dialect.json_extract` 的生产调用点未发现，现有使用均为测试/文档示例），但同型家族（G-OR1→G2→G-OR9→FTS 批→批二）第六形态应当登记并按既有惯例修复（列名走 `self.quote`）。

### 3.4 「缺口 0 处」裁定

主生成面（DDL/FK/Upsert/FTS/NULL/上限）复核**确认无缺口**；但 §3.3 的 `json_extract` 列名裸拼构成对「0 处」声称的**局部推翻**——按红队纪律不构成对本批测试/登记的阻断（纯验证批、零方言改动、缺口为存量），列为回执修复项。

---

## 4. 缺陷（红队发现）

### D-1（回执项，非阻断）：矩阵 feature 组合下 clippy -D warnings 红牌 2 处

```
cargo clippy -p sz-orm-core --features type-safe-columns --all-targets -- -D warnings
→ error: useless use of `vec!`  ×2
  packages\sz-orm-core\tests\integration_pg_matrix.rs:395:13
  packages\sz-orm-core\tests\integration_pg_matrix.rs:403:13
  （build_alter_table 两臂的 &vec![col(...)] 应直接用 &[col(...)]）
```

实施者声称「clippy --workspace --all-targets -D warnings EXIT=0」在默认 feature 下为真（`required-features` 使该文件默认不编译，`packages/sz-orm-core/Cargo.toml:362-365`），但**矩阵测试自身要求的 feature 组合下 clippy 失败**——报告 §4 门禁清单未披露该口径限制。仅测试代码、不影响生产，列为回执修复（`&vec![...]` → `&[...]` 两处）。

### D-2（并行批次归属，非本批缺陷）：G1 fmt 漂移 1 文件

```
bash scripts/check-fmt-all.sh → FMT_FAIL: packages/sz-orm-core
cargo fmt -- --check（sz-orm-core）→ Diff in tests/integration_gaussdb.rs:262（唯一漂移）
```

漂移内容系 `cc29f8a6`（容器化矩阵批）入库的 `integration_gaussdb.rs` 新增代码块，**非本批文件**（`git show a2e5762 --name-only` 不含 gaussdb）。本批报告声称「fmt 72 包通过」与当前 HEAD 实态不符（漂移在先批已存在）；归属并行线处置，不阻断本批。

### D-3（登记精度问题，非阻断）：Cargo.toml 测试基建变更的提交归属

报告 §1 将「dev-deps 新增 bigdecimal + sqlx uuid/bigdecimal feature + [[test]] 注册」列为本批交付物，但 commit `a2e5762` 仅含 `Cargo.lock`（1 行）；Cargo.toml 实际落点为前一批 commit `cc29f8a6`（`git show cc29f8a6 -- packages/sz-orm-core/Cargo.toml` 可证，含 `[[test]] integration_pg_matrix` 块）。CHANGELOG 第 31 行已有「hunk 级隔离、归属并行线」披露，属并行会话交叠下的有意收纳而非隐瞒，但报告 §1 交付物表未注明该归属，读者易误认为基建随 a2e5762 入库。

---

## 5. 建议（非缺陷）

1. `json_extract` 列名 quote 修复（§3.3）：`PostgreSqlDialect::json_extract` 及 G37 Q2 敏感模板可考虑增补 `#>>`/`#>` 形态检测，防第七形态复发；修复后补混合大小写列真连探针（可并入下批矩阵）。
2. 报告模板增加「clippy 以矩阵 required-features 组合复跑」一条，避免 required-features 屏蔽效应造成的门禁盲区。
3. szrsql 委托族回归依赖常驻 6543 实例，建议 CI 侧供给（既有 A3 integration.yml 已有 service，本地复验时注明即可）。

---

## 6. 复核结论汇总

| 项 | 结果 |
|---|---|
| 矩阵 20/20 | ✅ 独立复现，EXIT=0 |
| audit-verify 24/0 | ✅ 复跑一致 |
| integration_pg 22 回归 | ✅ 独立复现 |
| szrsql 15+8 回归 | ⚠️ 6543 未运行，跳过并注明（不作为放行依据） |
| 幻影测试 | ✅ 0 形态 |
| SKIP LOCKED 证伪 | ✅ 三臂对照成立，非假通过 |
| 缺口 0 处声称 | ⚠️ 主生成面成立；json_extract 列名裸拼 1 处同型存量缺口（§3.3） |
| 提交 a2e5762 文件范围 | ✅ 仅 5 文件（AGENTS/CHANGELOG/Cargo.lock/报告/测试），未误吞并行会话文件 |
| Cargo.toml 变更最小性 | ✅ bigdecimal+uuid/bigdecimal feature 最小必要；但归属在 cc29f8a6（D-3 登记精度） |
| G9/G37/G9 自测 | ✅ 全过 |
| G1 fmt / G3 clippy | ❌ 各 1 项红牌（D-2 归属先批 / D-1 矩阵 feature 组合） |

**回执项（由实施会话处置，非阻断）**：
1. `integration_pg_matrix.rs:395,403` `&vec![...]` → `&[...]`（D-1，恢复矩阵 feature 组合 clippy 全绿）；
2. `PostgreSqlDialect::json_extract` 列名 quote + 混合大小写真连探针（§3.3，G-OR1 家族第六形态）；
3. 报告 §1 交付物表补注 Cargo.toml 变更归属 cc29f8a6（D-3）；
4. `integration_gaussdb.rs:262` fmt 漂移归属容器矩阵线处置（D-2，与本批无关但阻塞 G1 当前绿）。

红队裁决: 条件 PASS（回执项 4 条——D-1 clippy 两处、json_extract quote 存量缺口、D-3 登记补注、D-2 并行线 fmt 漂移；上述均不推翻本批 20/20 真连结果与登记主体，处置后无需重跑全矩阵，仅需补跑对应验证）

---

## 附：audit-verify 自证输出

```
bash scripts/audit-verify.sh docs/assessment/2026-11-13-pg-dialect-matrix-redteam-review.md
  ✅ 通过: 2   ❌ 失败: 0   ⚠️ 警告: 4
✅ 审计证据验证通过
```

（4 条警告均为省略路径的短引用被脚本跳过，正文均带完整文件路径可人工核对。）
