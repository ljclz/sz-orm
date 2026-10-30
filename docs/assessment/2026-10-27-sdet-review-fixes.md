# v10.13.0 SDET 评测整改交付记录（2026-10-27 评测 ROI-1 ~ ROI-6）

> **来源**：`docs/assessment/2026-10-27-sdet-review.md`（SDET 评测，2026-10-27）
> **范围**：ROI-1 幻影断言修复 / ROI-2 metrics-real 版本校验 / ROI-3 门禁脚本自测 + CI / ROI-4 mutation 跨包测试包 / ROI-5 CI 阻断语义（2026-10-28 决策落地）/ ROI-6 OWASP 真实 DB 行为断言（2026-10-28 闭环）
> **状态更新（2026-10-28）**：ROI-5 与 ROI-6 均已闭环，详见「五、遗留项闭环记录」。

## 一、整改总表

| # | 问题（评测发现） | 修复 | 证据 | 验证 |
|---|----------------|------|------|------|
| F1 | `dialect_advanced_test.rs:90-91` 永真断言、`:180-181` 丢弃结果 | FTS 宏按方言参数化精确断言；`test_saphana_supports_if` 改为真实布尔断言 | `packages/sz-orm-core/tests/dialect_advanced_test.rs:92-150`、`:230-234` | `cargo test --test dialect_advanced_test` → **38 passed** |
| F2 | `multi_tenant_security_test.rs:9,15` 字面量比较（幻影） | 重写为 `MultiTenantPoolManager` 配额隔离/自动注册/限流副作用测试 | `packages/sz-orm-core/tests/multi_tenant_security_test.rs:25-68` | `cargo test --test multi_tenant_security_test` → **3 passed** |
| F3 | `connection_tenant_security_test.rs:8-14` 测标准库 `str::replace` | 重写为 `ConnectionTenantBinder` 真实行为：SET SQL 转义（CWE-89）/空租户校验拒绝/绑定解绑生命周期 | `packages/sz-orm-core/tests/connection_tenant_security_test.rs:28-64` | `cargo test --features connection-level-tenant --test connection_tenant_security_test` → **25 passed**（含 common 22） |
| F4 | `pool_security_test.rs:8,17` 配置值回显（弱幻影） | 重写为 `PoolConfigBuilder` 校验拒绝非法配置的真实错误路径 | `packages/sz-orm-core/tests/pool_security_test.rs:8-24` | `cargo test --test pool_security_test` → **3 passed** |
| F5 | `smallstring_differential.rs` 名义差分（声称 feature 对拍实为子串包含） | 更正头注释为 SQL 构造行为验证、`diff_*` 更名 `build_*`；文件名保留（CQO 工作流按名调用） | `packages/sz-orm-core/tests/smallstring_differential.rs:1-8` | `cargo test --test smallstring_differential` → **16 passed** |
| F6 | `check-metrics-real.py` 不校验 README 版本号（版本漂移不可拦截） | 新增 `read_workspace_version()` + 4 条"当前版本"声称模式 + `_claimed_value` 字符串度量比较 | `file:///D:/www/test/鲜视达/rust/sz-orm/scripts/check-metrics-real.py#L81-L95`、`#L136-L144`、`#L148-L151` | 本机实测：版本声称（v10.13.0）零报错；合成用例 `test_stale_version_claims_are_flagged` 通过（见 F9） |
| F7 | 门禁脚本绝大多数无自测；唯一自测未进 CI | 新增 `tests/test_gate_scripts.py`（19 个用例，覆盖 5 个门禁脚本）+ ci.yml 新增 `checker-self-tests` job | `file:///D:/www/test/鲜视达/rust/sz-orm/tests/test_gate_scripts.py#L1-L30`、`file:///D:/www/test/鲜视达/rust/sz-orm/.github/workflows/ci.yml#L75-L118` | `pytest tests/test_gate_scripts.py` → **19 passed** |
| F8 | CI mutation job 未传 `--test-package sz-orm-sqlx`（跨包变异盲区默认未启用） | 调用处显式传入 `--test-package sz-orm-sqlx` | `file:///D:/www/test/鲜视达/rust/sz-orm/.github/workflows/ci.yml#L1021-L1024` | YAML 解析通过（`yaml.safe_load` OK）；语义为脚本既有参数（`file:///D:/www/test/鲜视达/rust/sz-orm/scripts/collect-mutation-diff.sh#L43`、`#L98`） |

## 二、整改详情

### F1 dialect_advanced_test.rs（ROI-1）

- **原状**：`test_full_text_search!` 宏断言 `assert!(non_empty.is_empty() || !non_empty.is_empty())`（恒真）；`test_saphana_supports_if` 用 `let _ =` 丢弃两个查询结果。
- **修复**：宏改为 `($name, $dialect, $empty, $fragment)` 四参——空列输入断言**精确降级值**（ClickHouse/DuckDB/Db2/Informix/SapHana/Firebird → `"0"`，Snowflake → `"FALSE"`），非空输出断言**方言专属片段**（`position(...)`/`CONTAINS(...)`/`CONTAINING(...)`/`LIKE`/`ILIKE`）+ 多列 `OR` 连接（`packages/sz-orm-core/tests/dialect_advanced_test.rs:92-107`）；7 个方言实例化 `:109-150`。`json_extract` 宏同时强化为"非空 + 引用目标列"（`:63-79`）；`bool_to_int` 宏强化为"非空 + 保留表达式"（`:152-161`）。
- **依据**：各方言实现见 `packages/sz-orm-core/src/dialect.rs`（Snowflake `full_text_search` 返回 `FALSE` 降级；Db2 `CONTAINS(...) > 0`；Firebird `CONTAINING(...)`；SapHana `supports_if_exists/supports_if_not_exists` 均为 `true`）。

### F6 check-metrics-real.py 版本校验（ROI-2）

- **实现**：
  - `read_workspace_version()`：解析 `Cargo.toml` `[workspace.package]` 的 `version`（`file:///D:/www/test/鲜视达/rust/sz-orm/scripts/check-metrics-real.py#L81-L95`）；
  - `real_metrics()` 增加 `"version"` 键（`#L108`）；
  - `CLAIM_PATTERNS` 新增 4 条仅匹配"当前版本"声称的锚点：顶部版本行（`^>\s*vX.Y.Z ·`）、版本徽章（`version-X.Y.Z`）、Overview 行（`vX.Y.Z includes`）、英文 Note 行（`latest features (vX.Y.Z)`）——历史版本日志（`### v5.1.0 ...`）不含锚点不误报（`#L136-L144`）；
  - `_claimed_value()`：字符串度量（version）原样比较、数值度量转 int（`#L148-L151`），`scan_readme`/`fix_readme` 均已接入（`#L165`、`#L185`）。
- **本机实测**：
  - 当前 README（v10.13.0）版本声称零报错；
  - 版本漂移拦截由 F7 的合成用例验证（不改动仓库 README）。

### F7 门禁脚本自测 + CI（ROI-3）

- `tests/test_gate_scripts.py`（19 用例）：
  - metrics-real：声称匹配/版本漂移拦截/历史版本不误报/`--fix` 修复与占位符残留检查/`read_workspace_version` semver 校验（6 例）；
  - semantic-patterns：R1~R4 规则命中（含 R3 `\b` 边界语义）+ `strip_tests` 剥离（5 例）；
  - coverage：`DEFAULT_MODULES` 文件存在性、`CRATE_FEATURE_MAP` 包与 feature 与 Cargo.toml 一致性、feature 解析排除 default（3 例）；
  - mutation-coverage：`DEFAULT_FILES` 存在性、`DEFAULT_FEATURES` 在 sz-orm-core 有效（2 例）；
  - phantom-delivery：接线断言登记表引用文件存在、符号登记表非空（2 例）。
  - 运行输出：`19 passed in 0.75s`（另有 `tests/test_check_doc_sync.py` 的 7 个 git 依赖用例在本机因 **git 不在 PATH** 失败——环境限制，CI（ubuntu-latest，自带 git）不受影响）。
- ci.yml 新增 `checker-self-tests` job（`file:///D:/www/test/鲜视达/rust/sz-orm/.github/workflows/ci.yml#L75-L118`）：安装 pytest+pyyaml → `pytest tests/test_check_doc_sync.py tests/test_gate_scripts.py -v` → 独立运行 `scripts/test_check_sql_injection_auto.py`，**阻断式**（无 continue-on-error）。

### F8 mutation `--test-package`（ROI-4）

- `.github/workflows/ci.yml#L1021-L1024`：`collect-mutation-diff.sh --diff ... --fail-under 70 --test-package sz-orm-sqlx`，消除"core 逻辑变异体、杀手测试在 sz-orm-sqlx"的跨包盲区（能力自 `file:///D:/www/test/鲜视达/rust/sz-orm/scripts/collect-mutation-diff.sh#L43`、`#L98` 已就位，此前 CI 未启用）。

## 三、验证记录（本会话真实输出）

| 检查 | 命令 | 结果 |
|------|------|------|
| 格式 | `cargo fmt -p sz-orm-core -- --check` | ✅ 通过（`cargo fmt --all` 在本机触发 os error 206，72 包命令行超长——环境已知问题，CI Linux 不受影响） |
| 静态 | `cargo clippy -p sz-orm-core --all-targets --features connection-level-tenant -- -D warnings` | ✅ `Finished` 0 warning |
| 测试 | `cargo test -p sz-orm-core --test dialect_advanced_test --test multi_tenant_security_test --test pool_security_test --test smallstring_differential` | ✅ **38 + 3 + 3 + 16 = 60 passed, 0 failed** |
| 测试（门控） | `cargo test -p sz-orm-core --features connection-level-tenant --test connection_tenant_security_test` | ✅ **25 passed**（3 新用例 + common 22） |
| 自测 | `python -m pytest tests/test_gate_scripts.py -q` | ✅ **19 passed** |
| 门禁 18 | `python scripts/check-metrics-real.py --fix` | ✅ 一致（实际统计 测试标注=21182 feature=508 版本=10.13.0；--fix 已同步 README 7 处并发漂移） |
| 门禁 9 自测 | `python scripts/test_check_sql_injection_auto.py` | ✅ 13 passed |
| 门禁 15/16 | `check-phantom-delivery.py` / `check-semantic-patterns.py` | ✅ PHANTOM-1 0、接线 4/4；硬规则 0 |
| YAML | `yaml.safe_load('.github/workflows/ci.yml')` | ✅ YAML_OK |

## 四、过程观察（并发会话）

整改期间检测到**并发会话持续修改仓库**：README 测试数在会话内经历 21161 → 21171 → 21178 → 21182 多次外部更新（含 `model_derive_test.rs` 新增用例）。本次整改按门禁标准流程以 `--fix` 将 README 与实际统计同步至 **21182**；若并发会话继续新增测试，提交前需再次执行 `python scripts/check-metrics-real.py --fix`。

## 五、遗留项闭环记录（2026-10-28 更新）

| 项 | 原理由 | 闭环状态 | 证据 |
|----|--------|----------|------|
| ROI-5：fuzz/benchmark/integration 从 `continue-on-error` 改为有条件阻断 | 改变合并阻断语义属团队决策（评测报告 §12 亦标注"对 main 分支阻断"需分支条件判断） | ✅ **已闭环**（2026-10-28）：用户决策「对 main 分支阻断」。`integration` / `mq-integration` / `benchmark` / `fuzz` 四个 job 的 `continue-on-error` 改为 `${{ github.ref != 'refs/heads/main' }}` | `.github/workflows/ci.yml:466-470`、`:598-602`、`:763-768`、`:1101-1105` |
| ROI-6：OWASP A01/A03 补真实 DB 行为断言 | 需真实 DB 集成套件改造 + 凭据环境，中长线 | ✅ **已闭环**（2026-10-28）：`integration_pg.rs` 新增 `owasp_a01_tenant_scoping_returns_empty_set_on_cross_tenant` / `owasp_a03_injection_payload_via_parameter_is_inert`；`integration_mysql.rs` 提供同构用例。PG 本机实测 **2 passed / 0 failed**；MySQL 部分本机 3306 未运行，由 CI integration job（main 阻断）覆盖 | `packages/sz-orm-core/tests/integration_pg.rs`（owasp_a01 / owasp_a03 测试）、`packages/sz-orm-core/tests/integration_mysql.rs`（同构用例） |

## 六、audit-verify 自证

见文末 `.\scripts\audit-verify.ps1 docs\assessment\2026-10-27-sdet-review-fixes.md` 输出。
