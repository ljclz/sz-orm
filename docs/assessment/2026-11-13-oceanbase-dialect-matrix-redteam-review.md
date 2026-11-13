# OceanBase 方言矩阵二波补批（OC1~OC6）——第 5 环红队独立复核回执

> 复核对象：实施 commit `8230561d`（2026-11-13 OceanBase 方言矩阵二波补批），报告 `docs/assessment/2026-11-13-oceanbase-dialect-matrix.md`。
> 复核者：独立 Agent（非实施 session）。日期：2026-11-13。
> **裁决：条件 PASS**——P0/P1 = 0；P2×2（已逐项登记，均有独立复现证据）；复核期间红队自行追加了 3 个行为级回归单测（见 §5，属复核动作不属缺陷）。

## 裁决摘要

| 维度 | 结论 |
|------|------|
| 机械门禁复跑（G1/G2抽验/G9/G37/audit-verify） | 全过（G37 需 PYTHONIOENCODING=utf-8——见 P2-2） |
| 核心修复（委托宏 4 转发）正确性 | 逐参一致、无遗漏其他非默认 trait 方法、行为级验证通过 |
| 真连复跑 OceanBase | **matrix_oc_* 6/6 全过**；旧 3 项在本机死亡循环窗口复现 PoolTimedOut（环境死点，与报告 §1 登记一致）；报告"14/14"单轮全过在本机未复现（见 P2-1） |
| 报告可信度（供给边界/OC2 注释/引擎事实） | 抽验金仓下载门（验证码+联系表单）属实；OC2 注释与报告一致；daemon 死亡循环独立复现（53 次 boot success） |
| 登记一致性 | **CHANGELOG.md [Unreleased] 与 AGENTS.md 均未登记本批**（见 P2-1） |

## 1. 机械复跑（不信任报告声称，自取 EXIT 码）

| 项 | 命令 | 结果 | EXIT |
|----|------|------|------|
| 方言单测 | `cargo test -p sz-orm-core --lib dialect` | **261 passed; 0 failed**（filter 后 3030 filtered） | 0 |
| 方言单测（红队加 3 测后） | 同上 | **264 passed; 0 failed** | 0 |
| lib 全量回归 | `cargo test -p sz-orm-core --lib` | **3294 passed; 0 failed** | 0 |
| dialect_advanced_test | `cargo test -p sz-orm-core --test dialect_advanced_test` | **38 passed; 0 failed** | 0 |
| MY 矩阵（委托宏影响面） | `source /d/tools/gate.env && cargo test -p sz-orm-core --test integration_mysql_matrix -- --include-ignored` | **11 passed; 0 failed** | 0 |
| G37 | `python -I scripts/check-dialect-ddl-quoting.py` + `--self-test` | **0 命中 + self-test 19/19**（需 `PYTHONIOENCODING=utf-8`，见 P2-2） | 0 |
| G9 | `python -I scripts/check-sql-injection-auto.py --strict` + 自测 | **46/46 Safe + 自测 15/15** | 0 |
| G1 fmt | `bash scripts/check-fmt-all.sh` | **72 包通过** | 0 |
| audit-verify | `bash scripts/audit-verify.sh docs/assessment/2026-11-13-oceanbase-dialect-matrix.md` | **通过: 2 / 失败: 0 / 警告: 1**（弱语义警告：dialect.rs:2147 为纯路径引用，非编造） | 0 |

## 2. 真连复跑（OceanBase ob-final2，两轮守门探活）

容器状态：`docker ps` 显示 daemon 死亡循环——每轮检查时 Status 多为 `Up 1 second`；`docker logs ob-final2 | grep -c "boot success"` = **53**（独立证实报告 §1 死亡循环登记；`RestartCount=0` 因 restart 由宿主 docker daemon 层处理）。

- 第 1 轮：`sleep 250` + obclient 循环探活 → **ALIVE** → 立即跑全套件（14 项，`--test-threads=1`）→ **11 passed; 3 failed**（失败全部为既有 `test_oceanbase_*` 3 项，`PoolTimedOut` @ integration_oceanbase.rs:42 `setup_pool` 无守门 30s acquire——与报告 §1 "环境产物非方言缺陷"登记一致）；但 matrix_oc_* 6 项因先跑旧项耗尽活窗口未及执行。
- 第 2 轮：`sleep 300` + 探活 → **ALIVE** → 拆分执行：
  - `matrix_oc` 过滤 → **6 passed; 0 failed**（60.05s）✅ 矩阵载荷全绿
  - 旧 3 项（--skip matrix_oc）→ **5 passed; 3 failed**（同一 150s 窗口内仍撞死点）

**红队复跑结论**：矩阵载荷 OC1~OC6 独立复现全过；报告 §2 "14 passed; 0 failed" 的**单轮**全过在本机两轮尝试均未复现（旧 3 项在 150s 窗口内撞死点概率高）。这不构成矩阵缺陷（矩阵 6/6 是本批交付核心），但"14/14 单轮"声明在本机环境下不可稳定复现，登记为 P2-1。

### P2-1（报告可信度/登记，不阻断矩阵结论）
- 报告 §2 表格声称 "14 passed; 0 failed（守门探活后）"——本机两次 ALIVE 窗口复跑均为 11+3 fail（旧 3 项 PoolTimedOut）。守门探活只保证**起跑时** ALIVE，不保证 150s 全程存活（死亡循环周期 ~2min）。建议：旧 3 项 `setup_pool` 同型改造为 `try_pool()` 守门（matrix_oc_* 已如此），或报告措辞降为"矩阵 6/6 + 旧 8 项含环境死点 3 项（守门后可达 14/14）"。
- **CHANGELOG.md [Unreleased] 无本批条目；AGENTS.md「已发布」行无本批登记**（`git show 8230561d --name-only` 仅 4 文件：prompt/报告/dialect.rs/integration_oceanbase.rs）。六环第 6 环"CHANGELOG 版本条目 + AGENTS.md 更新 + git commit 三件齐备"未闭合——commit message 声称"登记 [Unreleased]"（矩阵批先例 cc29f8a6 均入 CHANGELOG）。本回执落盘后由实施侧补登记。

## 3. 核心修复对抗审查（宏转发，本批重心）

### 3.1 签名逐参一致性（亲读 `packages/sz-orm-core/src/dialect.rs`）

宏体新增 4 转发（`dialect.rs:2147-2172`）对照 trait 定义：

| 转发 | trait 定义行 | 签名一致性 | 备注 |
|------|-------------|-----------|------|
| `build_upsert_on_conflict` | `dialect.rs:295-303`（trait 默认 None） | ✅ `(&[&str], &[&str], &[String]) -> Option<String>` 逐参一致 | |
| `build_create_table_with_options` | `dialect.rs:191-210`（trait 默认忽略选项） | ✅ `(&str, &[ColumnDef], &[TableOption]) -> String` 一致 | |
| `render_table_options` | `dialect.rs:222-225`（trait 默认空串） | ✅ `(&[TableOption]) -> String` 一致 | |
| `build_add_foreign_key` | `dialect.rs:238-267`（trait 默认 ANSI） | ✅ `(&str, &ForeignKeyDef) -> Result<String, DbError>` 一致 | 返回路径 `crate::DbError` 正确——trait 侧即 `Result<String, DbError>`（`dialect.rs:238`），crate 根有 `pub use` DbError，宏内 `crate::DbError` 编译通过（264 lib 全绿佐证） |

### 3.2 转发遗漏扫描（对照 trait 全部非默认/默认方法逐项 grep 宏体）

trait 66~388 行全部方法 vs 宏体（2041-2175）转发清单比对：**无其他遗漏**。宏已转发：quote/escape_string/supports_returning/build_pagination/json_type/json_extract/full_text_search/full_text_search_mode/recursive_cte_keyword/bool_to_int/supports_if_exists/supports_if_not_exists/auto_increment_keyword/last_insert_id_sql/build_create_table/build_alter_table/build_drop_table/build_lock_clause/supports_lock_for_update/supports_lock_shared/build_insert_or_ignore_prefix/build_insert_or_ignore_suffix/supports_insert_or_ignore/build_pagination_clause/requires_order_by_for_pagination + 本批 4 个 = 29 个。
**未转发但有意落默认的**：`quote_into`（MySqlDialect 有优化覆写 @469）、`max_identifier_len`（MySQL=64 @465）、`quote_checked`（依赖 max_identifier_len）。**发现（P3-1，建议级）**：MySQL 族的委托 wrapper（MariaDB/TiDB/OceanBase）`quote_checked` 长度上限因此仍为 63 而非 MySQL 语义的 64——与 trait 文档（`dialect.rs:106-108`"如需方言感知由调用方自行选择"）契约相符，且 MY2 批已知、非本批引入，登记为后续可选转发，不改码。

### 3.3 行为级影响面验证（红队新增 3 单测，已入工作区未提交——属复核产物）

在 `packages/sz-orm-core/src/dialect.rs` tests 模块（4106 起）追加 3 测，**全部通过（lib 264/264）**：
1. `test_redteam_delegate_upsert_not_none`：OceanBase/MariaDB/TiDB wrapper `build_upsert_on_conflict` 返回 `Some("ON DUPLICATE KEY UPDATE ... VALUES(...)")`（修复前为 None）；Kingbase wrapper 返回 PG 形态 `ON CONFLICT ... DO UPDATE`；对照组 Dameng（委托 Oracle）保持 `None`——证明转发忠实于委托目标，不越权。
2. `test_redteam_delegate_table_options_rendered`：OB wrapper `build_create_table_with_options` 产物含 `ENGINE=InnoDB`/`CHARSET=utf8mb4` 且 ≠ 无选项产物；`render_table_options` 非空（修复前空串）；MariaDB 抽验同过。
3. `test_redteam_delegate_fk_action_forwarded`：OB wrapper `build_add_foreign_key` 产物含 `ON DELETE CASCADE`；非法动作（注入形态 `NO ACTION; DROP TABLE users`）被 `validate_fk_action` 白名单拒绝——fail-closed 语义随委托目标保持。

既有测试对旧 None/忽略行为的依赖排查：lib 3294 + dialect_advanced_test 38 全过，MY 矩阵 11/11——**零回归证实**。

## 4. 报告可信度抽验

- **供给边界（金仓）**：抽验 kingbase.com.cn/download.html——下载确有"验证码/下载验证"环节 + 联系表单弹窗 + 授权文件机制，与报告 §0 "验证码表单"结论一致 ✅（达梦/GBase 未逐一验证，按复核要求抽一即可）。
- **OC2 载荷修正**：测试内注释（`integration_oceanbase.rs` OC2 cols 构造处："OB 引擎事实（同 MySQL）：TEXT 列不可作主键（1167 can't index column），保留字列名覆盖改用非主键列验证"）与报告 §2/§3 一致 ✅；载荷确实 `primary_key: false` + `sql_type: "TEXT"`。
- **引擎事实（daemon 死亡循环）**：独立复现——53 次 boot success + 多轮 `Up 1 second`（§2）；**ob_compatibility_mode=MYSQL 由 OC1 真连断言独立证实**（matrix_oc1 passed，真库 `SHOW VARIABLES` 返回 MYSQL）。`obclient` 手工复现因恰逢死亡窗口未取得，以 OC1 通过测试为证据锚点。

## 5. 红队自身产物与提交边界

- 复核新增 3 单测位于 `packages/sz-orm-core/src/dialect.rs`（工作区未提交状态：M dialect.rs）——**本回执仅提交本报告文件**，3 单测留给实施方随 P2-1 补登记时一并收编或由用户决定；若不需要可直接 `git checkout -- packages/sz-orm-core/src/dialect.rs`（不影响实施批结论，lib 261 原态亦全绿）。
- 工作区无其他本批残留文件（`git status --porcelain` 仅上述 1 项）。

## 6. 发现汇总

| # | 级别 | 内容 | 处置建议 |
|---|------|------|---------|
| P2-1 | P2 | 报告"14/14 单轮"不可稳定复现（旧 3 项无守门 + 死亡循环周期 ~2min）；**CHANGELOG/AGENTS 第 6 环登记缺失**（commit 声称登记但文件未含） | 旧 3 项改 try_pool 守门；补 CHANGELOG [Unreleased] + AGENTS.md 登记后闭环 |
| P2-2 | P2 | G37 脚本在 Windows GBK 控制台直接运行 `UnicodeEncodeError`（`scripts/check-dialect-ddl-quoting.py:333` print ✅ emoji），须 `PYTHONIOENCODING=utf-8` 方能 EXIT=0——报告 §5 "0 命中"结论本身成立，但门禁在默认 env 下假红 | 脚本输出统一 ASCII 或强制 `sys.stdout.reconfigure(encoding='utf-8')` |
| P3-1 | P3 | MySQL 族 wrapper 未转发 `max_identifier_len`/`quote_into`（quote_checked 上限 63 而非 64）——trait 文档契约内、非本批引入 | 后续可选转发，登记即可 |

## 7. audit-verify 自证

`bash scripts/audit-verify.sh docs/assessment/2026-11-13-oceanbase-dialect-matrix.md` → 通过 2 / 失败 0 / 警告 1（弱语义：`dialect.rs:2147` 纯路径引用），**EXIT=0**。本回执自身引用行均为复核当日实读实跑（file:line 见上表）。

## 8. 结论

核心修复（委托宏 4 转发）**正确、签名一致、无遗漏、行为级证实、零回归**；矩阵载荷 OC1~OC6 独立真连复现 6/6 全绿。放行条件：P2-1 两项（14/14 措辞收敛或旧项守门化 + CHANGELOG/AGENTS 补登记）由实施侧闭环；P2-2/P3-1 登记下批处置。
