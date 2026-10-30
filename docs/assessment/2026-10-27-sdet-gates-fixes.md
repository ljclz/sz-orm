# v10.13.0 SDET 评测门禁固化交付记录（2026-10-28，门禁 30/31 + G26-B/C）

> **来源**：`docs/assessment/2026-10-27-sdet-review.md`（SDET 评测）后续整改
> **目标**：把本会话发现的所有问题提炼为可持续门禁，验收标准 = 能命中历史样本、对当前代码零误报
> **范围**：新增 2 道硬门禁（幻影测试扫描 / CI 阻断语义审计）+ 2 项门禁体系加固（脚本自测完整性 / 优雅预检）+ 3 个新自测文件 + 接线

## 一、新增门禁

### 门禁 30：幻影测试扫描（`scripts/check-phantom-tests.py`）

7 条规则（P1/P2/P3/P5 硬规则，P2b/P4/P6 软提示）：

| 规则 | 检测 | 硬度 | 历史样本（评测发现） |
|------|------|------|---------------------|
| P1 | 同表达式互斥或（`x || !x` 永真断言） | 硬 | `dialect_advanced_test.rs:90-91` 原代码 |
| P2 | 双字面量断言（`assert_eq!("a", "b")`） | 硬 | 评测发现的字面量比较模式 |
| P2b | 变量 vs 字面量断言（软） | 软 | `assert_eq!(tenant, "", ...)` 空串识别 |
| P3 | 测试函数零断言（无 assert/panic） | 硬 | 原 `test_saphana_supports_if`（`let _ =` 丢弃） |
| P4 | contains 兜底或（`contains(x) \|\| is_empty()`） | 软 | 原 `dialect_advanced_test.rs:110` |
| P5 | 差分命名规范（differential/_diff_test/diff_fuzz 文件必须含有效对拍断言） | 硬 | `smallstring_differential.rs` 名义差分 |
| P6 | 集成测试未引用产品 crate（软） | 软 | `connection_tenant_security_test.rs` 测标准库 |

**关键设计**：
- `extract_test_fn_bodies` 用括号匹配提取测试函数体，P3 统计断言宏；**豁免编译期 trait 约束检查**（`fn _assert_clone<T: Clone>()` / `assert_send::<T>()`）。
- P3 存量豁免文件 `docs/assessment/2026-10-27-p3-legacy-exempt.txt`（227 条"只构造不验证"历史遗留测试登记，增量红牌）。
- P5 支持 `prop_assert_eq!` 与带参方法调用（`o.get(k)`），对 `plan_cache_differential`/`simd_differential`/`differential_fuzz`/`typed_ast_zst_diff_test` 零误报；`qb_migration_diff_test` 登记为 P5 存量豁免（待整改为对拍断言）。
- 软提示输出截断（最多 50 条）。

证据：规则定义见 `file:///D:/www/test/鲜视达/rust/sz-orm/scripts/check-phantom-tests.py#L35-L95`（P1~P6 正则）、`#L129-L160`（extract_test_fn_bodies）、`#L230-L250`（P5 存量豁免）、`#L281-L350`（main 与豁免加载）。

### 门禁 31：CI 阻断语义审计（`scripts/check-ci-blocking.py`）

- 解析 `.github/workflows/*.yml` 所有 job 级 `continue-on-error`（无条件 `true` + 条件式 `${{ ... }}`）。
- 任何非阻断 job 必须登记在 `EXEMPT_JOBS` 豁免表（含理由），否则红牌（fail-closed）。
- 当前仓库 5 个 job 级 continue-on-error 全部登记（feature-matrix 无条件；benchmark/fuzz/integration/mq-integration 为条件式——2026-10-27 并发会话已改造为 main 分支阻断、PR 分支不阻断）。

证据：`file:///D:/www/test/鲜视达/rust/sz-orm/scripts/check-ci-blocking.py#L31-L45`（EXEMPT_JOBS）、`#L56-L95`（audit_workflows 无条件/条件式识别）。

## 二、门禁体系加固

### G26-B：门禁脚本自测完整性元测试

`tests/test_gate_scripts.py` 新增 `TestGateScriptSelfTestCoverage`：
- 遍历 `scripts/check-*.py`，不在豁免登记表（22 个存量脚本）内的必须有自测覆盖，否则红牌；
- 新门禁脚本（check-phantom-tests / check-ci-blocking）不在豁免表且自带自测。

证据：`file:///D:/www/test/鲜视达/rust/sz-orm/tests/test_gate_scripts.py#L232-L280`（EXEMPT_SCRIPTS 与元测试）。

### G26-C：外部工具优雅预检（修复 `check-coverage.py`）

- 原 `python scripts/check-coverage.py` 在缺 cargo-llvm-cov 时抛 FileNotFoundError traceback（WinError 2）。
- 修复：`find_cargo_bin` 找不到返回 `None`，main 预检后打印"未安装 + 安装命令"并以 exit 1 优雅退出；新增 `require_tool()` 供复用。

证据：`file:///D:/www/test/鲜视达/rust/sz-orm/scripts/check-coverage.py#L111-L137`（find_cargo_bin 返回 None + require_tool）、`#L206-L211`（main 预检）。

## 三、新自测文件（3 个，61 用例全通过）

| 文件 | 覆盖 | 历史样本 fixture |
|------|------|-----------------|
| `tests/test_check_phantom_tests.py` | P1~P6 规则命中/不误报 + 当前文件零误报 | 永真断言/双字面量/零断言/弱或/测标准库/名义差分 |
| `tests/test_check_ci_blocking.py` | 豁免表通过、未登记报错、条件式识别、真实仓库审计 | 当前仓库 5 个 job 全部登记 |
| `tests/test_check_coverage.py` | find_cargo_bin 返回 None、require_tool 错误信息、main 优雅退出无 traceback | 缺工具场景 |

## 四、接线

- `scripts/run-gates-v10120-server.sh`：新增 Gate 30（check-phantom-tests.py）、Gate 31（check-ci-blocking.py）。
- `scripts/check-gate-scripts.py`：`REQUIRED_CHECKS` 追加两个新脚本，强制接入。
- `AGENTS.md`：门禁表追加 #30/#31 行（含命令、规则摘要、自测文件）。
- `.github/workflows/ci.yml`：`checker-self-tests` job 追加 3 个新自测文件 + Gate 30/31 对真实仓库的冒烟运行。

证据：`file:///D:/www/test/鲜视达/rust/sz-orm/scripts/run-gates-v10120-server.sh#L178-L188`（Gate 30/31）、`file:///D:/www/test/鲜视达/rust/sz-orm/scripts/check-gate-scripts.py#L26-L36`（REQUIRED_CHECKS）、`file:///D:/www/test/鲜视达/rust/sz-orm/.github/workflows/ci.yml#L89-L99`（checker-self-tests 扩展）。

## 五、验证记录（本会话真实输出）

| 检查 | 结果 |
|------|------|
| `pytest tests/test_gate_scripts.py tests/test_check_phantom_tests.py tests/test_check_ci_blocking.py tests/test_check_coverage.py -q` | ✅ **61 passed** |
| `python scripts/check-phantom-tests.py`（真实仓库） | ✅ 硬规则 **0 命中**（P3 存量豁免 227；P6 软提示 3285 条——观察期） |
| `python scripts/check-ci-blocking.py`（真实仓库） | ✅ 通过（5 个 job 全部登记豁免） |
| `python scripts/check-coverage.py`（缺 cargo-llvm-cov） | ✅ 优雅退出 exit 1（`cargo-llvm-cov 未安装（安装命令: cargo install cargo-llvm-cov --locked）`），无 traceback |
| `python scripts/check-gate-scripts.py` | ✅ PASS（关键脚本均已接入） |
| `python scripts/check-metrics-real.py --fix` | ✅ 通过（测试标注 21183，版本 10.13.0） |
| `yaml.safe_load('.github/workflows/ci.yml')` | ✅ YAML_OK |
| `cargo fmt -p sz-orm-core -- --check` | ✅ 通过 |

## 六、门禁落地过程中额外发现并修复

- **`dialect_exotic_test.rs:85-88`** `test_duckdb_supports_returning`：`assert!(d.supports_returning() || !d.supports_returning())` 永真断言——P1 规则真实命中，已修复为 `assert!(!d.supports_returning())`（DuckDB 不支持 RETURNING，与 `dialect.rs` 内部测试一致）。`cargo test -p sz-orm-core --test dialect_exotic_test` 已补跑确认（15/15）。

## 七、遗留项与后续

| 项 | 状态 | 处置 |
|----|------|------|
| P3 存量 227 条"只构造不验证"测试 | 已登记豁免（`docs/assessment/2026-10-27-p3-legacy-exempt.txt`，当前 190 条） | 逐条修复为行为断言后从豁免文件移除（示范修复见 §八） |
| P6 软提示 3285 条 | 已收紧（§八）：`3285 → 830` 条 | 继续观察，后续可再按"无任何间接产品引用"收紧 |
| `qb_migration_diff_test.rs` 名义差分 | **已整改（§八）**：归一化对拍断言，7/7 通过，已从 P5 豁免移除 | ✅ 完成 |
| ROI-5：fuzz/benchmark 阻断语义 | 已由并发会话改造为条件式（main 阻断） | 已纳入门禁 31 登记表 |
| `dialect_exotic_test.rs` 修复验证 | ✅ 已补跑 `cargo test -p sz-orm-core --test dialect_exotic_test` → 15/15 | ✅ 完成 |

## 八、遗留项处理进展（2026-10-28 追加）

### 1. P5 名义差分整改：`qb_migration_diff_test.rs` ✅

- 原状：`test_diff_*` 只分别断言 old/new SQL 包含关键子句（`contains("SELECT")` 等），无对拍。
- 整改：新增 `normalize_sql()`（小写/折叠空白/去分号）+ `assert_diff_eq()` 归一化对拍断言；INSERT/UPDATE 因旧库内联值、新库参数化（`?`）差异，对拍前将旧库内联值归一化为占位符（验证结构等价且值一致）。删除两个无价值的"needs_review"占位测试。
- 验证：`cargo test -p sz-orm-core --features qb-migration-tool --test qb_migration_diff` → **7 passed, 0 failed**。
- 门禁：已从 `P5_LEGACY_EXEMPT` 移除（`scripts/check-phantom-tests.py` `P5_LEGACY_EXEMPT = set()`）。

### 2. P6 规则收紧：`3285 → 830` 条软提示 ✅

- 原状：只检查 `use sz_orm_core`，误报大量通过 `use sz_orm_advisor`/`sz_orm_sqlx` 引用其他产品包的合法测试（如 `auto_tuning_wiring.rs` 引用 `sz_orm_advisor`）。
- 整改：`check-phantom-tests.py` P6 改为检查**任意 `sz_orm_` 产品包引用**（文件级或函数体）。
- 效果：真实仓库软提示从 **3285 条降至 830 条**（降幅 75%），门禁仍通过（硬规则 0）。

### 3. P3 存量示范修复：`test_token_store_new` ✅

- 原状：`packages/sz-orm-auth/tests/auth_test.rs` `test_token_store_new` 只构造 + 丢弃（`let _ = store.with_refresh_lifetime(3600);`）——零断言。
- 整改：改为 `test_token_store_new_with_refresh_lifetime`，断言 `with_refresh_lifetime(3600)` 的行为副作用——签发令牌的 `expires_at - created_at == 3600`。
- 验证：`cargo test -p sz-orm-auth --test auth_test` → **27 passed, 0 failed**。
- 豁免：已从 `2026-10-27-p3-legacy-exempt.txt` 移除该条目。

### 4. P3 豁免文件自动清理 ✅

- 写审计脚本（复用门禁自身的 `extract_test_fn_bodies`/`ASSERT_RE`/`COMPILE_TIME_ASSERT_RE`）扫描豁免清单，识别两类失效条目并移除：
  - **已修复条目**（函数体现已有断言）：`sz-orm-oracle/src/lib.rs:test_blocking_pool_new_and_drop`（并发会话已修复）。
  - **编译期断言类条目**（`fn _assert<T: Send+Sync>()` / `_check_send_sync` 等，门禁 G30 已自动豁免、无需登记）：共移除 8 条（`test_pool_into_state_returns_arc`、`test_paginator_trait_exists`、`test_paginator_builder_trait_exists`、`test_stream_trait_exists`、`test_async_trait_export`、`test_async_export`、`test_schema_macro_nullable`、`test_schema_macro_varchar_with_len`）。
  - 同时移除 4 条 `dialect_all_*` 冗余登记（并发会话已注明"函数体含循环内断言"）。
- 效果：豁免清单从 227 条降至 **182 条**（增量约 20%；其余为"不 panic 验证""类型签名验证""smoke 全链路"等有行为但无显式断言的合法测试）。

### 5. 剩余 182 条 P3 存量分类与整改指引

| 类别 | 占比（抽样） | 示例 | 建议 |
|------|------------|------|------|
| 编译期类型/签名验证 | 少量残留 | `test_is_available_returns_bool`（`is_available() -> bool` 环境相关） | 保留豁免；后续可让 G30 识别"函数体内定义 helper 函数"模式后自动豁免 |
| "不 panic"行为验证 | 较多 | `test_transaction_free_null_is_noop`、`test_redis_pubsub_disconnected_publish` | 保留豁免（验证不崩溃是有价值的）；或加注释说明 |
| smoke/E2E 全链路 | 较多 | `smoke_sqlite_full_chain`（调用完整流程） | 可保留豁免；严格模式应改为返回 Result 并 `assert!(...is_ok())` |
| 纯构造/丢弃空洞 | 少数 | `test_backup_manager_new`（`let _m = BackupManager::new(...)`） | **应修复**：断言构造后对象的方法副作用或状态 |

**修复模板**（纯构造空洞类）：
```rust
// 修复前：let _m = BackupManager::new(config);          // 零断言
// 修复后：
let m = BackupManager::new(config);
assert_eq!(m.some_state(), expected, "构造后状态应符合配置");
```
**验收**：修复一条即从 `docs/assessment/2026-10-27-p3-legacy-exempt.txt` 移除一行；新增零断言测试由 G30 直接红牌拦截。

### 验证（追加）

| 检查 | 结果 |
|------|------|
| `cargo test -p sz-orm-core --features qb-migration-tool --test qb_migration_diff` | ✅ **7 passed** |
| `cargo test -p sz-orm-auth --test auth_test` | ✅ **27 passed** |
| `python scripts/check-phantom-tests.py`（真实仓库） | ✅ 硬规则 **0 命中**（P3 豁免 182；P6 软提示 830） |
| `python scripts/check-ci-blocking.py`（真实仓库） | ✅ 通过 |
| `pytest tests/test_gate_scripts.py tests/test_check_phantom_tests.py tests/test_check_ci_blocking.py tests/test_check_coverage.py -q` | ✅ **72 passed** |

*交付记录完。*