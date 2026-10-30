# sz-orm 测试体系 SDET 深度评测报告（2026-10-27）

> **评测人视角**：顶级自动化测试工程师（SDET），15 年经验（测试金字塔 / 幻影测试 / 变异与覆盖率 / 差分属性混沌 / CI 门禁 / flaky 治理 / 性能回归）
> **评测对象**：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.0，72 workspace 成员，21161+ 测试）
> **评测方式**：只读评测（未修改任何源码/文档/脚本）；L1 = 代码/文档 `file:line` 或命令输出；L2 = 本会话真实运行；L3 = 基于证据的专业推断
> **自证**：报告末尾附 `.\scripts\audit-verify.ps1` 输出

---

## 1. 执行摘要

**总评：测试体系规模与基础设施在同类项目中属于上游水平，但存在三类系统性问题：门禁脚本自身无测试（checker 未被 check）、~10 处确凿的幻影/空洞断言仍在测试套件中存活、CI 中 fuzz/benchmark/真实集成三个高价值门禁全部 `continue-on-error` 不阻断 PR。**

从 SDET 的三个核心问题回答：

1. **测试是否验证了行为（有效）？** 大部分是。属性测试（`property.rs` 16/16）、混沌池（`chaos_pool.rs` 14/14）、查询计划缓存差分（`plan_cache_differential.rs` 11/11）、安全攻击（`security_attacks.rs` 4/4、`sz-orm-auth` 5/5、`sz-orm-crypto` KAT 4/4）、Redis 集成（4/4）均为真实行为断言。**但**：`dialect_advanced_test.rs:90-91` 存在永真断言、`multi_tenant_security_test.rs:5-16` 存在字面量比较、`connection_tenant_security_test.rs:6-22` 在测标准库 `str::replace` 而非产品代码、`smallstring_differential.rs` 名义差分实为子串包含——这些是幻影测试的实锤。
2. **测试是否真的在跑（落地）？** 是。本机实测 7 组测试全部通过；`integration_redis` 本机 Redis 就绪 4/4；CI 的 `test`/`coverage`/`mutation`/`metrics-real`/`soak-smoke`/`secret-scanning` job 均为阻断式（fail-closed）。**但** `fuzz`/`benchmark`/`integration`/`mq-integration`/`feature-matrix`/`unused-deps` 六个 job 为 `continue-on-error: true`，失败不阻断合并。
3. **测试能否长期保持可靠（可维护）？** 中等。`common/` helper 组织优秀（`InMemoryDb`/`MockConnection`/`FaultyConnection`/`Rng`）；时序敏感测试已有降噪设计。**但** 24 道门禁的脚本只有 2 个有自测（`check-doc-sync`、`check-sql-injection-auto`），`check-coverage.py` 在本机缺 cargo-llvm-cov 时直接 traceback 而非优雅失败；`metrics-real` 只查数字度量不查 README 版本号，无法拦截"README 写 v10.12.0 而 Cargo.toml 是 10.13.0"这类版本漂移。

### 最严重的 5 个问题

| # | 严重度 | 问题 | 证据 |
|---|--------|------|------|
| S1 | 高 | 测试套件中存在确凿幻影断言（永真/字面量/测标准库），且 `check-semantic-patterns.py` 抓不到 | `dialect_advanced_test.rs:90-91`、`multi_tenant_security_test.rs:9,15`、`connection_tenant_security_test.rs:8-14` |
| S2 | 高 | 门禁脚本自身无测试：24 道门禁中仅 2 个脚本有自测，且唯一接入的自测（`test_check_doc_sync.py`）未接入 CI | `tests/test_check_doc_sync.py:1-14`、`.github/workflows/ci.yml`（无 pytest job） |
| S3 | 高 | CI 中 fuzz / benchmark / 真实服务集成三个高价值门禁全部 `continue-on-error: true`，失败不阻断 PR | `.github/workflows/ci.yml:410,700,1053` |
| S4 | 中 | `check-coverage.py` 在缺少 cargo-llvm-cov 时崩溃（traceback），无优雅降级 | `scripts/check-coverage.py:211`（L2 实测 FileNotFoundError） |
| S5 | 中 | `metrics-real` 不检查 README 版本号，无法拦截版本漂移；`check-publish-consistency.py` 检查版本但未接入 CI | `scripts/check-metrics-real.py:97-118`、`.github/workflows/ci.yml` |

### 最高 ROI 的 3 个改进

1. **删除/修复现存幻影断言**（`dialect_advanced_test.rs:90-91`、`multi_tenant_security_test.rs:5-16`、`connection_tenant_security_test.rs:6-22`、`pool_security_test.rs:6-18`、`smallstring_differential.rs` 更名或真差分）——1 天内可完成，直接消除"测试存在但无效"的安全感。
2. **给 `check-metrics-real.py` 增加 README 版本号校验**（或在 CI 接入 `check-publish-consistency.py`）——~20 行改动，堵住版本漂移盲区。
3. **CI mutation job 增加 `--test-package sz-orm-sqlx`**（一行），并给 `check-coverage.py`/`check-mutation-coverage.py` 补自测 + 把 `test_check_doc_sync.py` 接入 pytest job——治理 checker 盲区。

---

## 2. 测试资产盘点（独立统计）

### 2.1 独立统计数字（L2 实测）

本评测用独立脚本（`os.walk` + 正则，与 `metrics-real` 不同实现路径）统计 `packages/**/*.rs`：

| 口径 | 数量 | 说明 |
|------|------|------|
| `#[test]` / `#[tokio::test]` 标注总数 | **21,161** | 与 `scripts/check-metrics-real.py` 输出一致（L2） |
| 标注后紧跟 `fn` 的测试函数数 | **20,651** | 更严格口径（排除宏展开等） |
| 含测试的文件数 | **1,506** | 分布在全部包 |
| `packages/sz-orm-core/tests/` 下 .rs 文件 | **252** | `dir /b` 实测 |

### 2.2 与 README / 历史口径的关系

- README 当前声称 **21161+ tests**（`README.md:4`、`README.md:8` 徽章、`README.md:124` "21161 passed, 0 failed"），与我的独立统计一致（L1+L2）。
- 历史口径：`docs/assessment/2026-10-20-comprehensive-audit-report.md:18` 记录 21,132（`grep '#[test]\|#[tokio::test]' | wc -l`）。**口径漂移的原因**：① 仓库在持续增长（10-20 → 10-27 新增测试）；② 计数方法差异（标注数 vs 函数数）。
- **评测过程观察（重要）**：本会话开始时 README 显示 `v10.12.0 · 21153+ tests`（`README.md:4`），随后在 2026/10/27 17:49 被外部进程更新为 `v10.13.0 · 21161+ tests`（文件时间戳 2026/10/27 17:49，本会话首读与 `findstr` 复读结果不一致）。这说明**存在并发会话正在修改同一仓库**，评测证据具有时效性；同时印证 README 数字漂移确实发生过，且 `metrics-real` 门禁在 21153→21161 漂移期间应能拦截（claim 21153 ≠ actual 21161）。但**版本号漂移（v10.12.0 vs 10.13.0）无法被 `metrics-real` 拦截**（见 §8）。

### 2.3 测试类型分布（L3 基于文件清单）

`packages/sz-orm-core/tests/` 252 个文件覆盖：方言（~40 个 `dialect_*.rs`/`integration_*.rs`）、缓存（`l2_cache_*`/`plan_cache_*`）、连接池（`pool_*`/`chaos_pool`）、安全（`security_attacks.rs`/`owasp_*`/`whitehat`/`blackhat`）、事务（`transaction_*`）、迁移（`migration_test.rs`/`e2e_migration.rs`）、性能（`perf_*`）、专项（`property`/`fuzz`/`stress`/`jepsen`/`soak`/`tenant_concurrency`/`formal`）。

---

## 3. 测试策略评估

### 3.1 金字塔分布（L3）

- **单元/模块测试**：大量内联 `#[cfg(test)]` + 集成测试目录下的针对性测试。核心连接池逻辑有快速 mock 测试（`chaos_pool.rs` 14 个、`stress.rs`、`pool_*` 系列），且有 loom 模型检查（`loom-check/`）作为穷举交错补充——这是非常正确的策略。
- **集成测试**：252 个文件在 `sz-orm-core/tests/`，但多数为**内存/mock 级**（`common/mod.rs` 提供 `InMemoryDb`/`MockConnection`），非真实 DB。真实 DB 测试集中在 `integration_*.rs`（`#[ignore]`）与 `sz-orm-sqlx` 的 `real_*` 套件。
- **E2E**：`examples/` 下有 `real_db_*` 示例作为生产模式参考。

**结论**：金字塔形状大体合理——快速 mock 测试覆盖核心逻辑，真实 DB 测试用 `--ignored` 隔离、CI 用 service container 跑。**问题**：`tests/` 目录下大量"集成测试"实际是 mock 级，命名易误导（见 §6 Jepsen 过度包装）。

### 3.2 测试目标（L3）

大部分测试目标明确：行为副作用（池计数、SQL 参数化、缓存命中/未命中统计、密文一致性、KAT 向量）。但存在"为凑覆盖率而写"的测试：`multi_tenant_security_test.rs`、`pool_security_test.rs`、`connection_tenant_security_test.rs` 注释声称"覆盖安全分支"，实际断言的是字面量/标准库/配置回显（详见 §4）。

---

## 4. 测试质量评估（幻影测试抽查表）

AGENTS.md 门禁 12 对幻影测试的定义：*测试必须验证行为副作用，不能只验证初始值/配置值；禁止 `let _result = ...` 丢弃操作结果；禁止永真断言*。

### 4.1 确凿幻影断言（L1 代码证据）

| 测试文件:行 | 断言内容 | 类型 | 判定 |
|------------|---------|------|------|
| `packages/sz-orm-core/tests/dialect_advanced_test.rs:90` | `assert!(non_empty.is_empty() \|\| !non_empty.is_empty())` | **永真断言**（恒为 true） | ❌ 幻影 |
| `packages/sz-orm-core/tests/dialect_advanced_test.rs:91` | `assert!(empty.is_empty() \|\| !empty.is_empty())` | **永真断言** | ❌ 幻影 |
| `packages/sz-orm-core/tests/dialect_advanced_test.rs:180-181` | `let _ = d.supports_if_exists(); let _ = d.supports_if_not_exists();` | **丢弃操作结果**，零断言 | ❌ 空洞 |
| `packages/sz-orm-core/tests/multi_tenant_security_test.rs:9` | `assert_ne!(tenant_a, tenant_b, "不同租户应隔离")`（两个字符串常量） | **字面量比较**（恒真） | ❌ 幻影 |
| `packages/sz-orm-core/tests/multi_tenant_security_test.rs:15` | `assert_eq!(tenant, "", "空租户 ID 应正确识别")` | **字面量比较**（恒真） | ❌ 幻影 |
| `packages/sz-orm-core/tests/connection_tenant_security_test.rs:8-14` | 对字面量 `"tenant'; DROP TABLE--"` 调 `str::replace('\'', "''")` 并断言 | **测标准库**，未触及产品代码 | ❌ 幻影 |
| `packages/sz-orm-core/tests/pool_security_test.rs:8` | `assert!(config.max_size > 0)`（默认值自身属性） | **初始值/配置值回显** | ❌ 弱幻影 |
| `packages/sz-orm-core/tests/pool_security_test.rs:17` | `assert_eq!(config.max_size, 100)`（刚赋的值） | **配置值回显** | ❌ 弱幻影 |
| `packages/sz-orm-core/tests/dialect_advanced_test.rs:70` | `assert!(!with_dollar.is_empty() \|\| !without_dollar.is_empty())` | 弱断言（非恒真但极弱） | ⚠️ 弱 |
| `packages/sz-orm-core/tests/dialect_advanced_test.rs:110` | `assert!(sql.contains("active") \|\| sql.is_empty())` | 弱断言 | ⚠️ 弱 |
| `packages/sz-orm-core/tests/query_aggregate_test.rs:48-81` | 大量 `sql.contains("COUNT(*)")` 子串检查 | 浅层断言（真实输出但浅） | ⚠️ 弱 |

### 4.2 名义差分实为子串包含（幻影差分，L1）

`packages/sz-orm-core/tests/smallstring_differential.rs:1-4` 声称"验证 `perf-smallstring` feature 启用/禁用时 SQL 构造输出完全一致"，但代码（`:35-41` 等）从未切换 feature、从未做两侧对比，全部是 `sql.contains(...)` 子串断言。**这是命名与实现不符的幻影差分测试**。

### 4.3 OWASP 测试的行为副作用问题（L1 + L3）

- `owasp_a01_access_control.rs:41-65`：只断言表名前缀 `tenant_1_`/`tenant_2_`、参数化 SQL 文本——**没有"越权查询返回空集"的行为断言**。
- `owasp_a03_injection.rs:48-64` 等：只断言 SQL 文本不含注入、params 含注入串——**没有连接真实 DB 执行注入并验证失败**。
- `owasp_race_conditions.rs:21-282`：用真实线程 + Barrier 并发，但被测对象是**测试内手写的原子计数器**（模拟池/配额/优惠码），断言的是测试自造计数（如 `:118-123` 成功数=QUOTA）。竞态是真实的，但**被测代码不是产品代码**，属于"测试自己的测试逻辑"。

### 4.4 门禁脚本能否抓到？（L2 实测）

- `python scripts/check-semantic-patterns.py` → exit 0，硬规则命中 0、软规则 7（全是 `let _ = ...check/validate...` 类，**没有一条指向上述幻影断言**）。
- 原因：`check-semantic-patterns.py:32-61` 的规则库只有 R1~R4（无效递增/丢弃 check/无符号比较/释放传 0），**没有永真断言/字面量比较/配置回显规则**；`check-phantom-delivery.py` 只查符号的生产调用与接线（PHANTOM-1/2），不查测试断言质量。
- **结论**：现存幻影断言全部能通过门禁 16/15。`check-semantic-patterns.py` 的 `strip_tests`（`:80-113`）会剥掉 `#[cfg(test)]` 与 `mod tests`，但 `tests/` 集成测试文件不在其剥离范围，且规则本身也不覆盖这些模式。

---

## 5. 覆盖率与变异测试评估

### 5.1 覆盖率

- **脚本可运行性（L2）**：`python scripts/check-coverage.py` 在本机**无法运行**——因未安装 cargo-llvm-cov，脚本直接抛 `FileNotFoundError` traceback 并以 exit 1 结束（非优雅失败）。根因：`scripts/check-coverage.py:111-120` 的 `find_cargo_bin` 找不到工具时返回字面量名 `"cargo-llvm-cov"`，`:211` `subprocess.run` 触发 WinError 2。
- **CI 通道**：`collect-coverage-linux.sh` 在 Linux CI 安装 cargo-llvm-cov 后运行（`.github/workflows/ci.yml:902-906`），`--fail-under 60` 阻断。脚本本身有 fail-closed 设计（`LINES_TOTAL=0` 时 exit 1，`scripts/collect-coverage-linux.sh:61-64`）。
- **历史声称**：88.59%（v8.7.0 报告）→ 90.2%（v9.0.0 `docs/assessment/2026-09-26-gate-review-report.md:32`）→ 94.4%（README `README.md:96` 引述 2026-10-16 架构评审）。**本机无法复核**（无 cargo-llvm-cov），不采信亦不否定；CI coverage job 若真实运行会提供当前值。**注意**：`README.md:96` 的 94.4% 是架构评审引述，`metrics-real` 不校验覆盖率数字（CLAIM_PATTERNS 无覆盖率模式）。
- **盲区**：`check-coverage.py` 默认只测 ~70 个关键模块子集（`scripts/check-coverage.py:30-104`），不是全 workspace；行覆盖率低于 80% 才失败（`--threshold 0.8`），CI 的 `collect-coverage-linux.sh` 才是全量 60% 门槛。

### 5.2 变异测试

- **配置质量**：`.cargo-mutants.toml` 启用了 fn/expr/binop/unop/cmp 五类算子，排除了 drop/clone/default/tracing/debug 等无意义变异（`.cargo-mutants.toml:15-33`），配置合理。
- **增量 vs 全量**：`collect-mutation-diff.sh` 用 `--in-diff` 做日常增量（默认 fail-under 70），`collect-mutation-linux.sh` 做发布前全量（5 包，默认 70%）。两者都有 fail-closed 设计（无有效变异体且 mutants 失败时 exit 1；`collect-mutation-diff.sh:127-137`）。
- **跨包变异盲区（历史发现 #5）**：`collect-mutation-diff.sh:12,43,98` 已支持 `--test-package sz-orm-sqlx`（风险 3 修复），但**默认 `TEST_PACKAGE=""`（`:30-36`），CI 调用未传**（`.github/workflows/ci.yml:996` 只传 `--diff --fail-under`）。**部分缓解**：能力就位但默认/CI 未启用，跨包存活变异仍可能漏杀。
- **本机无法实测**（cargo-mutants 未安装），如实记录。

---

## 6. 专项测试评估（属性/差分/混沌/压力/模糊/并发模型）

| 专项 | 文件 | 实测/评估 | 结论 |
|------|------|-----------|------|
| 属性测试 | `packages/sz-orm-core/tests/property.rs` | **L2：16/16 通过（0.26s，2 遍）** | 真实 proptest，覆盖 Value 往返不变量 + SQL 注入防御引号成对（`:119-127`） |
| 混沌池 | `packages/sz-orm-core/tests/chaos_pool.rs` | **L2：14/14 通过（0.17s，2 遍）** | 真实故障注入（饥荒/泄漏/关闭后获取/超时/健康检查）；含 `chaos_pool_shutdown_race_no_new_connection`（`:819`） |
| 混沌 | `packages/sz-orm-core/tests/chaos.rs` | 只读评估 | 真实故障注入（网络分区/磁盘满/主从切换/Flaky 工厂），mock 层 |
| 差分（缓存） | `packages/sz-orm-core/tests/plan_cache_differential.rs` | **L2：11/11 通过（2 遍）** | 真实差分（缓存命中 vs 未缓存 Arc 指针等价、归一化、并发、失效） |
| 差分（SIMD） | `packages/sz-orm-core/tests/simd_differential.rs` | 只读评估 | 真实对拍（SIMD vs 标量参考实现，`:37`） |
| 差分（SmallString） | `packages/sz-orm-core/tests/smallstring_differential.rs` | 只读评估 | **幻影差分**：名不副实，只做子串包含 |
| 差分（Config） | `packages/sz-orm-config/tests/config_diff_test.rs:109-312` | 只读评估 | 对拍真实 Consul/Nacos，但全部 `#[ignore]` + feature 门控，默认不跑 |
| 压力 | `packages/sz-orm-core/tests/stress.rs` | 只读评估 | 真实 tokio 并发压力，断言 active≤max、无泄漏 |
| Jepsen（mock） | `packages/sz-orm-core/tests/jepsen.rs` | 只读评估 | **非真实 Jepsen**：mock 级事务状态机测试，无 `#[ignore]` 默认运行；README.md:969 将其列为 "Jepsen" 属过度包装 |
| Jepsen（真实 DB） | `packages/sz-orm-sqlx/tests/real_db_jepsen.rs:45,333` | 只读评估 | 真实 MySQL/PG 并发转账/保存点/池枯竭，`#[ignore]` |
| 租户并发 | `packages/sz-orm-core/tests/tenant_concurrency.rs:33-63` | 只读评估 | 真实 tokio 任务级交错，断言租户上下文不泄漏（`:60-62`） |
| 模糊（库内） | `packages/sz-orm-core/tests/fuzz.rs` | 只读评估 | 固定种子 Rng（`:37`），1000 轮随机输入验证 to_param 引号转义 |
| 模糊（libFuzzer） | `fuzz/fuzz_targets/`（6 个 target：query_builder/value_escape/pool_config/sql_validator/identifier_safety/firewall_bypass） | 只读评估 | 真实 cargo-fuzz 工程；CI 有 fuzz job 但 `continue-on-error` |
| 并发模型检查 | `loom-check/`（独立工作区） | 只读评估 | 真实 loom 模型，验证 shutdown 后不新建连接/计数守恒（`file:///D:/www/test/鲜视达/rust/sz-orm/loom-check/src/lib.rs#L77-L100`），README 记录"移除修复检查后测试 FAILED"——模型能抓到真实竞态 |

**压力/并发测试能否跑（L2）**：`jepsen.rs`/`tenant_concurrency.rs`/`stress.rs` 均无 `#[ignore]`，属于默认 `cargo test --workspace` 的一部分；本会话抽查的 chaos_pool/property/plan_cache_differential 均稳定通过 2 遍，未见 flaky。

---

## 7. 集成测试基础设施评估（G7）

### 7.1 本机就绪矩阵（L2 实测）

| 套件 | AGENTS.md 声称 | 本机实测 | 结论 |
|------|--------------|---------|------|
| MySQL 9.6 | 就绪 | 未实测（时间预算） | 与 AGENTS.md 一致（服务在跑） |
| PostgreSQL 18 | 就绪 | 未实测 | 一致 |
| Oracle 23ai | 就绪 | 未实测 | 一致 |
| **Redis** | 就绪 | **`integration_redis` 4/4 通过（2 遍，1.1s）** | ✅ 一致 |
| MSSQL | 排除（实例下线） | 未实测 | 一致（需自备实例） |
| GBase | 排除（需凭据） | 未实测 | 一致 |
| ClickHouse | 经 122 服务器隧道 | 未实测 | 一致（有隧道实测记录） |

### 7.2 `--ignored` 机制与 CI（L1+L3）

- `--ignored` 隔离真实服务测试是合理设计：默认 `cargo test --workspace` 不触碰外部依赖，CI 通过 service container 启动 MySQL/PG/RabbitMQ/Mosquitto/MinIO（`.github/workflows/integration.yml:31-89`）。
- `integration.yml` 每日 02:00 定时 + PR 打 `run-integration` 标签触发（`.github/workflows/integration.yml:8-29`）。
- **问题**：ci.yml 的 `integration` job（真实服务集成）是 `continue-on-error: true`（`.github/workflows/ci.yml:410`），真实 DB 集成失败不阻断 PR——与"集成测试是质量门禁"的定位矛盾。`integration-test` job（MySQL/PG 的 `integration_mysql`/`integration_pg`）无 `continue-on-error`，是阻塞的（`.github/workflows/ci.yml:337-401`），这是主要的真实 DB 门禁。
- **排除清单合理性**：MSSQL/GBase 以 `--skip` 跳过而非失败（`scripts/run-gates-v10120-server.sh:56`），并登记环境豁免（`docs/sz-orm-review-runbook.md:287-289`），处理合规。
- **数据隔离**：`common/cleanup.rs` 提供唯一表名（`unique_table`，见 `real_db_jepsen.rs:47`），避免测试间数据耦合；`--test-threads=1` 串行执行真实 DB 测试（`integration.yml:131`）。

---

## 8. CI/CD 测试门禁评估（audit the checkers）

### 8.1 fail-closed 验证（L1）

| job | 位置 | 是否阻断 | 说明 |
|-----|------|---------|------|
| `test`（workspace 测试） | `.github/workflows/ci.yml:262-287` | ✅ 阻断 | `cargo test --workspace` + SQLite 集成 |
| `coverage` | `.github/workflows/ci.yml:880-917` | ✅ 阻断 | `--fail-under 60` |
| `mutation` | `.github/workflows/ci.yml:925-1011` | ✅ 阻断 | 增量变异 `--fail-under 70` |
| `metrics-real` | `.github/workflows/ci.yml:53-65` | ✅ 阻断 | 数字漂移拦截 |
| `check-upstream`（ADR-0001） | `.github/workflows/ci.yml:73-94` | ✅ 阻断 | |
| `soak-smoke` | `.github/workflows/ci.yml:783-814` | ✅ 阻断 | 10s soak |
| `secret-scanning` | `.github/workflows/ci.yml:864-875` | ✅ 阻断 | |
| **`integration`（真实服务）** | `.github/workflows/ci.yml:406-410` | ❌ **不阻断** | `continue-on-error: true` |
| **`mq-integration`** | `.github/workflows/ci.yml:537-541` | ❌ **不阻断** | `continue-on-error: true` |
| **`benchmark`** | `.github/workflows/ci.yml:696-700` | ❌ **不阻断** | `continue-on-error: true`（ADR-005 性能波动不阻断） |
| **`fuzz`** | `.github/workflows/ci.yml:1048-1053` | ❌ **不阻断** | `continue-on-error: true` |
| `feature-matrix` / `unused-deps` | `.github/workflows/ci.yml:197,256` | ❌ 不阻断 | 分别为 `continue-on-error` / `|| true` |

**结论**：核心质量门禁（编译/测试/覆盖率/变异/度量/安全扫描）是 fail-closed 的；但 fuzz、benchmark、真实服务集成三类高价值门禁形同虚设（仅记录 artifact）。

### 8.2 metrics-real 能否拦截 README 版本漂移？（L2 实测 + L1）

- **L2 实测**：`python scripts/check-metrics-real.py` → exit 0，输出"实际统计: 包=70 成员=72 测试标注=21161 feature=508，✅ 一致"。当前 README 数字已同步（21161），脚本通过。
- **关键盲区**：`scripts/check-metrics-real.py:97-118` 的 `CLAIM_PATTERNS` 只匹配测试数/包数/方言数/feature 数——**没有任何版本号模式**；`real_metrics()`（`:81-91`）也不返回版本字段。因此 **"README 写 v10.12.0 而 Cargo.toml 是 10.13.0" 这类版本漂移，metrics-real 结构上不可能拦截**。
- 本会话开始时 README 确实处于 v10.12.0 漂移状态（随后被并发进程更新为 v10.13.0），`metrics-real` 不会因此失败——实测语义即"版本号不参与度量校验"。
- 版本一致性由 `scripts/check-publish-consistency.py` 检查（`scripts/check-publish-consistency.py:10-11`），但该脚本**未出现在任何 workflow 中**（`findstr` 全工作流仅命中 metrics-real）。

### 8.3 门禁脚本自测覆盖（audit the audit）

| 门禁脚本 | 自测 | 接入 CI |
|---------|------|--------|
| `check-doc-sync.py` | ✅ `tests/test_check_doc_sync.py`（覆盖规则加载/匹配/跳过/退出码） | ❌ **未接入 CI**（ci.yml 无 pytest job） |
| `check-sql-injection-auto.py` | ✅ `scripts/test_check_sql_injection_auto.py` | ❌ 未接入 CI |
| `check-coverage.py` | ❌ 无 | ❌ |
| `check-mutation-coverage.py` | ❌ 无 | ❌ |
| `check-metrics-real.py` | ❌ 无 | 脚本本身在 CI，自测不在 |
| `check-semantic-patterns.py` | ❌ 无 | 脚本本身在 CI，自测不在 |
| `check-phantom-delivery.py` | ❌ 无 | 脚本本身在 CI，自测不在 |

**结论**：24 道门禁的"检查器"绝大多数没有自测；唯一写好的自测（`test_check_doc_sync.py`）也没有跑进 CI。这是"谁来看守看守者"的结构性空白。

### 8.4 历史门禁缺陷是否根治（L1）

- `scripts/run-gates-v10120-server.sh:51`：Gate 6 已用 `&&`（`if [ $AUDIT_RC -eq 0 ] && [ $DENY_RC -eq 0 ]`）——历史 `||` bug 已修复。
- `scripts/run-gates-v10120-server.sh:80-82`：Gate 11 已真实执行 `git diff --name-only HEAD -- packages/ Cargo.toml Cargo.lock`——历史硬编码 PASS 已修复。
- `scripts/run-gates-v10120-server.sh:25-27`：Gate 1 fmt 已真实执行（M-4 整改注释），不再硬编码 PASS。

---

## 9. 测试可靠性评估

### 9.1 时序敏感测试（L1+L3）

- `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:34` `test_prepared_cache_hit_benefit`：已做降噪——miss 成本采样 6 次取最大值（`:44-69`）、命中侧用固定 200µs 延迟模拟 prepare 成本（`:65`），显著降低误报。仍属时序敏感，AGENTS.md 已将其列入高负载易误报清单。
- `packages/sz-orm-core/tests/soak.rs:54` `soak_pool_long_running_steady_state`：通过 `SOAK_DURATION` 环境变量控制时长、采样间隔自适应（≥1h 每 60s，否则每 5s），退化检测有明确阈值（`soak.rs:32-37`）。

### 9.2 随机种子可控性（L1）

- `fuzz.rs:37` `Rng::new(42)`、`:89` `Rng::new(123)` 固定种子；`common/mod.rs` 的 `Rng` 为 xorshift64（`common/mod.rs:452-507`）。**种子可控可复现**。
- proptest 默认随机种子，失败时可复现（proptest 会打印 seed）。

### 9.3 并行隔离（L1+L3）

- mock 级测试不共享真实资源（`common/mod.rs` 的 `InMemoryDb` 每测试独立）。
- 真实 DB 测试用 `unique_table` 唯一表名（`packages/sz-orm-sqlx/tests/real_db_jepsen.rs:47`）+ `--test-threads=1`。
- **本机注意**：并发运行多个 cargo 命令会因 package cache 锁竞争导致进程被杀（本会话实测两个并行 cargo test 均以 4294967295 退出）——这是环境/使用问题，不是测试代码问题，但 CI 矩阵 `fail-fast: false` 多 job 并发时需留意。

### 9.4 flaky 抽查（L2）

| 测试 | 第 1 遍 | 第 2 遍 | 判定 |
|------|--------|--------|------|
| `property` | 16/16 | 16/16 | 稳定 |
| `chaos_pool` | 14/14 | 14/14 | 稳定 |
| `plan_cache_differential` | 11/11 | 11/11 | 稳定 |
| `integration_redis` | 4/4 | 4/4 | 稳定 |
| `sz-orm-auth security_attacks` | 5/5 | — | 稳定 |
| `sz-orm-crypto kat` | 4/4 | — | 稳定 |

**结论**：抽查的 6 组测试两遍运行均稳定，未见 flaky 迹象；但本机为低负载环境，高负载误报风险仍存在于两个时序敏感测试（AGENTS.md 已登记）。

---

## 10. 性能回归测试评估

### 10.1 Criterion 与基准（L1+L3）

- `bench-comparison/` 使用 Criterion（`BENCHMARK_REPORT.md:5` 有 Criterion 参数记录），报告基于 median。
- **CI 接入情况**：`benchmark` job 存在但 `continue-on-error: true`（`.github/workflows/ci.yml:700`），注释明确"性能波动不阻断 PR（ADR-005）"。**结论：性能回归没有硬门禁**，基准结果仅作为 artifact 上传（`:764-772`）。
- `bench-comparison/bench-results*.txt` 是历史采集数据，可复现性依赖 `cargo bench` 环境一致性，未看到 CI 中的回归阈值检测 job（`regression_detector.rs` 等组件存在但需手动接入）。

### 10.2 Soak（L1）

- `soak.yml` 每周日 00:00 UTC 运行 6h soak（`.github/workflows/soak.yml:22-43`），有退化检测与失败通知（`:79-90`），是**真实运行的周期门禁**。
- `soak-self-hosted.yml` 存在（workflow 清单），支持 >6h 自托管。
- ci.yml 的 `soak-smoke` job 每次 push 跑 10s 冒烟（`.github/workflows/ci.yml:783-814`），阻断式。

---

## 11. 历史发现复核结果表

| # | 历史发现（来源） | 当前证据 | 状态 |
|---|----------------|---------|------|
| 1 | 门禁脚本 Gate 6 用错 `\|\|`、Gate 11 硬编码 PASS（2026-10-24 深度评测） | `scripts/run-gates-v10120-server.sh:51`（`&&`）、`:80-82`（真实 ADR 检查）、`:25-27`（Gate 1 真实 fmt） | ✅ **已修复**（但门禁脚本自身仍无测试） |
| 2 | 连接池 shutdown 竞态（2026-10-24 复现） | 回归测试 `chaos_pool_shutdown_race_no_new_connection`（`chaos_pool.rs:819`）存在且 L2 通过；loom 模型验证（`loom-check/src/lib.rs:77-100`） | ✅ **已修复**（回归测试 + 模型检查双保险） |
| 3 | 幻影测试风险（AGENTS.md 门禁 12） | `check-semantic-patterns.py` 实测抓不到；现存幻影断言 `dialect_advanced_test.rs:90-91`、`multi_tenant_security_test.rs:9,15`、`connection_tenant_security_test.rs:8-14` | ❌ **仍存在**（部分幻影测试存活） |
| 4 | 覆盖率声称漂移 88.59→90.2→94.4 | 本机无 cargo-llvm-cov，`check-coverage.py` traceback；CI coverage job 阻断式；`README.md:96` 仍引述 94.4% | ⚠️ **无法本机复核**（CI 提供当前值） |
| 5 | 变异测试跨包盲区（pool 回归在 sz-orm-sqlx） | `collect-mutation-diff.sh:12,43,98` 支持 `--test-package sz-orm-sqlx`，但默认 `TEST_PACKAGE=""`、CI 未传（`ci.yml:996`） | ⚠️ **部分缓解**（能力就位，默认/CI 未启用） |
| 6 | 测试数口径漂移 21,132/21,134/21,153 | 当前 21,161（标注数，`metrics-real` 与独立统计一致）；README 已同步；漂移源于仓库增长 + 计数口径 | ✅ **当前一致**（但 metrics-real 拦不住版本漂移） |
| 7 | G7 排除清单（MSSQL/GBase 排除、Redis 就绪、ClickHouse 隧道） | 本机 Redis `integration_redis` 4/4；MSSQL/GBase 以 `--skip` 跳过并登记豁免 | ✅ **与 AGENTS.md 一致** |
| 8 | fuzz 是否接入 CI | ci.yml 有 `fuzz` job（`:1048-1097`），`fuzz/fuzz_targets/` 6 个真实 target | ⚠️ **接入但不阻断**（`continue-on-error: true`） |
| 9 | loom-check 独立工作区 | `loom-check/` 存在，3 个不变量模型，README 记录"移除修复检查后 FAILED" | ✅ **有实际进展**（模型可抓到竞态） |
| 10 | 性能/soak 是否接入 CI | `benchmark` job 存在但 `continue-on-error`；`soak.yml` 每周 6h + `soak-smoke` 阻断式 | ⚠️ **soak 接入且阻断；benchmark 无回归门禁** |

---

## 12. 改进建议（按 ROI 排序）

### ROI-1：删除/修复现存幻影断言（1 天内，零风险）

- **问题**：`dialect_advanced_test.rs:90-91` 永真断言、`multi_tenant_security_test.rs:5-16` 字面量比较、`connection_tenant_security_test.rs:6-22` 测标准库、`pool_security_test.rs:6-18` 配置回显、`smallstring_differential.rs` 幻影差分。
- **证据**：见 §4.1。
- **改法**：
  - `dialect_advanced_test.rs:90-91` → 删除这两行断言，或改为断言 `full_text_search(&["title","body"], "keyword")` 非空、`full_text_search(&[], "keyword")` 为空（如各方言语义不支持空数组，则 `prop_assume`/返回 `Option`）。
  - `connection_tenant_security_test.rs` → 改为调用产品代码（`connection_tenant.rs` 的实际转义函数），断言其输出。
  - `multi_tenant_security_test.rs` / `pool_security_test.rs` → 删除或改为真实行为测试（如 `MultiTenantPool` 的租户隔离行为）。
  - `smallstring_differential.rs` → 要么真正在 `--features perf-smallstring` 开关下对拍输出，要么改名为 `smallstring_behavior.rs` 并删除"差分"字样。
- **验收标准**：`grep -rn "is_empty() || !" packages/ --include=*.rs` 零命中；`check-semantic-patterns.py` 增加永真断言规则后能抓到上述模式（见 ROI-3）。

### ROI-2：metrics-real 增加 README 版本号校验 / 将 check-publish-consistency 接入 CI

- **问题**：README 版本漂移（v10.12.0 vs Cargo.toml 10.13.0）无任何 CI job 拦截。
- **证据**：`scripts/check-metrics-real.py:97-118` 无版本模式；`check-publish-consistency.py` 不在 CI。
- **改法**：在 `check-metrics-real.py` 的 `CLAIM_PATTERNS` 增加 `(v\d+\.\d+\.\d+)` 与 `workspace.package.version` 比对；或把 `check-publish-consistency.py` 加入 ci.yml（与 `metrics-real` 并列，2 行）。
- **验收标准**：把 README 版本号改成错误值，`python scripts/check-metrics-real.py` exit 1；CI 有对应 job 名。

### ROI-3：给门禁脚本补自测并接入 CI

- **问题**：24 道门禁的脚本绝大多数无自测；唯一自测未进 CI。
- **证据**：§8.3 表。
- **改法**：
  - 为 `check-coverage.py`、`check-mutation-coverage.py`、`check-metrics-real.py`、`check-semantic-patterns.py`、`check-phantom-delivery.py` 各写 pytest 自测（参照 `tests/test_check_doc_sync.py` 的 `importlib` 加载模式）。
  - 在 ci.yml 增加 `checker-self-tests` job：`pip install pytest && python -m pytest tests/test_check_doc_sync.py scripts/test_check_sql_injection_auto.py -v`，并运行新增自测。
- **验收标准**：CI 出现该 job 且失败阻断；任一脚本改坏时 CI 红。

### ROI-4：CI mutation job 启用 `--test-package sz-orm-sqlx`

- **问题**：跨包变异盲区（core 的 pool 回归测试在 sz-orm-sqlx），能力已具备但 CI 未启用。
- **证据**：`scripts/collect-mutation-diff.sh:43,98`；`.github/workflows/ci.yml:996`。
- **改法**：ci.yml mutation job 的 `collect-mutation-diff.sh` 调用加 `--test-package sz-orm-sqlx`。
- **验收标准**：CI 日志出现 `test-package: sz-orm-sqlx`；变异报告中包含 sqlx 包测试杀死的 core 变异体。

### ROI-5：将 fuzz / benchmark / integration 从 continue-on-error 改为有条件阻断

- **问题**：三类高价值门禁失败不阻断合并。
- **证据**：`.github/workflows/ci.yml:410,700,1053`。
- **改法**：保留 `continue-on-error: true` 的同时，增加"artifact 为空即失败"的后续 step（如 fuzz 无 artifacts、benchmark 无 `benchmark-report.json`、integration 无测试日志）；对 PR 可先保持非阻断、对 main 分支 push 改为阻断。
- **验收标准**：故意让 fuzz target panic，PR 的 CI 显示红色 job（或至少 artifact 缺失被标记）。

### ROI-6：为 OWASP A01/A03 补真实 DB 行为断言（中长线）

- **问题**：OWASP 测试只断言构造期 SQL/参数，不验证"越权查询返回空集""注入执行失败"的行为副作用。
- **证据**：§4.3。
- **改法**：在 `integration_mysql`/`integration_pg` 套件中增加带租户上下文的真实 DB 越权查询用例（`SELECT` 应返回 0 行），以及黑名单注入串在真实 DB 上的执行失败断言。
- **验收标准**：新增集成测试在 CI integration-test job 中通过。

---

## 13. 证据索引与 audit-verify 输出

### 13.1 关键证据索引（file:line）

**测试文件**
- `packages/sz-orm-core/tests/property.rs:23-27`（proptest）
- `packages/sz-orm-core/tests/chaos_pool.rs:1-10`（混沌池）、`:819`（shutdown 竞态回归）
- `packages/sz-orm-core/tests/plan_cache_differential.rs:17-33`（差分）
- `packages/sz-orm-core/tests/security_attacks.rs:42-64`（安全攻击）
- `packages/sz-orm-core/tests/stress.rs:51-92`（压力）
- `packages/sz-orm-core/tests/fuzz.rs:31-84`（模糊）
- `packages/sz-orm-core/tests/smallstring_differential.rs:1-4,35-41`（幻影差分）
- `packages/sz-orm-core/tests/dialect_advanced_test.rs:90-91,180-181`（幻影断言）
- `packages/sz-orm-core/tests/multi_tenant_security_test.rs:5-16`（字面量比较）
- `packages/sz-orm-core/tests/connection_tenant_security_test.rs:5-22`（测标准库）
- `packages/sz-orm-core/tests/pool_security_test.rs:5-18`（配置回显）
- `packages/sz-orm-core/tests/owasp_a01_access_control.rs:41-65`、`owasp_a03_injection.rs:48-64`、`owasp_race_conditions.rs:21-282`（OWASP 断言层级）
- `packages/sz-orm-core/tests/tenant_concurrency.rs:33-63`、`packages/sz-orm-core/tests/chaos.rs:37-134`、`packages/sz-orm-core/tests/jepsen.rs:29-47`
- `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:34,44-69`（时序敏感降噪）
- `packages/sz-orm-core/tests/soak.rs:32-37,54`
- `packages/sz-orm-sqlx/tests/real_db_jepsen.rs:45,47,333`
- `packages/sz-orm-config/tests/config_diff_test.rs:109-312`
- `packages/sz-orm-core/tests/common/mod.rs:38-171`（InMemoryDb）、`:452-507`（Rng）

**门禁脚本**
- `scripts/check-metrics-real.py:81-91,97-118`（无版本校验）
- `scripts/check-coverage.py:30-104`（模块子集）、`:111-120`（find_cargo_bin）、`:211`（崩溃点）
- `scripts/check-semantic-patterns.py:32-61`（规则库）、`:80-113`（strip_tests）
- `scripts/check-phantom-delivery.py:40-114`（符号断言）
- `scripts/collect-mutation-diff.sh:12,30-36,43,98`（--test-package 支持但默认关闭）
- `scripts/run-gates-v10120-server.sh:25-27,51,80-82`（Gate 1/6/11 修复）
- `tests/test_check_doc_sync.py:1-14`（唯一门禁自测）
- `scripts/test_check_sql_injection_auto.py`（扫描器自测，13 通过）

**CI**
- `.github/workflows/ci.yml:53-65`（metrics-real）、`:262-287`（test）、`:337-401`（integration-test 阻塞）、`:406-410`（integration 不阻断）、`:696-700`（benchmark 不阻断）、`:880-917`（coverage）、`:925-1011`（mutation）、`:996`（mutation 无 --test-package）、`:1048-1053`（fuzz 不阻断）
- `.github/workflows/integration.yml:8-29,31-89,131`（定时/标签/service/串行）
- `.github/workflows/soak.yml:22-43,79-90`（每周 6h + 退化通知）

**其他**
- `file:///D:/www/test/鲜视达/rust/sz-orm/loom-check/src/lib.rs#L77-L100`（shutdown 竞态建模）
- `README.md:4,8,96,124,969`（测试数/94.4%/Jepsen 声称）
- `docs/assessment/2026-10-20-comprehensive-audit-report.md:18`（21,132 口径）
- `docs/assessment/2026-09-26-gate-review-report.md:32`（90.2% 基线）
- `docs/sz-orm-review-runbook.md:7`（旧路径 E:\vue，已过时）、`:287-289`（环境豁免规范）

### 13.2 audit-verify 自证输出

```powershell
.\scripts\audit-verify.ps1 docs\assessment\2026-10-27-sdet-review.md
```

实测输出（2026-10-27 本会话）：

```
==========================================
  审计证据验证: docs\assessment\2026-10-27-sdet-review.md
==========================================
✅ packages/sz-orm-core/tests/dialect_advanced_test.rs:90 (共 242 物理行)
✅ packages/sz-orm-core/tests/dialect_advanced_test.rs:91 (共 242 物理行)
✅ packages/sz-orm-core/tests/dialect_advanced_test.rs:180 (共 242 物理行)
✅ packages/sz-orm-core/tests/multi_tenant_security_test.rs:9 (共 16 物理行)
✅ packages/sz-orm-core/tests/multi_tenant_security_test.rs:15 (共 16 物理行)
✅ packages/sz-orm-core/tests/connection_tenant_security_test.rs:8 (共 22 物理行)
✅ packages/sz-orm-core/tests/pool_security_test.rs:8 (共 18 物理行)
✅ packages/sz-orm-core/tests/pool_security_test.rs:17 (共 18 物理行)
✅ packages/sz-orm-core/tests/dialect_advanced_test.rs:70 (共 242 物理行)
✅ packages/sz-orm-core/tests/dialect_advanced_test.rs:110 (共 242 物理行)
✅ packages/sz-orm-core/tests/query_aggregate_test.rs:48 (共 81 物理行)
✅ packages/sz-orm-core/tests/smallstring_differential.rs:1 (共 202 物理行)
✅ packages/sz-orm-config/tests/config_diff_test.rs:109 (共 470 物理行)
✅ packages/sz-orm-sqlx/tests/real_db_jepsen.rs:45 (共 599 物理行)
✅ packages/sz-orm-core/tests/tenant_concurrency.rs:33 (共 234 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/loom-check/src/lib.rs:77 (共 293 物理行)
✅ packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:34 (共 174 物理行)
✅ packages/sz-orm-core/tests/soak.rs:54 (共 294 物理行)
✅ packages/sz-orm-sqlx/tests/real_db_jepsen.rs:47 (共 599 物理行)
✅ packages/sz-orm-core/src/lib.rs:77 (共 1920 物理行)
✅ packages/sz-orm-core/tests/property.rs:23 (共 169 物理行)
✅ packages/sz-orm-core/tests/chaos_pool.rs:1 (共 865 物理行)
✅ packages/sz-orm-core/tests/plan_cache_differential.rs:17 (共 243 物理行)
✅ packages/sz-orm-core/tests/security_attacks.rs:42 (共 123 物理行)
✅ packages/sz-orm-core/tests/stress.rs:51 (共 584 物理行)
✅ packages/sz-orm-core/tests/fuzz.rs:31 (共 766 物理行)
✅ packages/sz-orm-core/tests/smallstring_differential.rs:1 (共 202 物理行)
✅ packages/sz-orm-core/tests/dialect_advanced_test.rs:90 (共 242 物理行)
✅ packages/sz-orm-core/tests/multi_tenant_security_test.rs:5 (共 16 物理行)
✅ packages/sz-orm-core/tests/connection_tenant_security_test.rs:5 (共 22 物理行)
✅ packages/sz-orm-core/tests/pool_security_test.rs:5 (共 18 物理行)
✅ packages/sz-orm-core/tests/owasp_a01_access_control.rs:41 (共 89 物理行)
✅ packages/sz-orm-core/tests/tenant_concurrency.rs:33 (共 234 物理行)
✅ packages/sz-orm-core/tests/chaos.rs:37 (共 998 物理行)
✅ packages/sz-orm-core/tests/jepsen.rs:29 (共 729 物理行)
✅ packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:34 (共 174 物理行)
✅ packages/sz-orm-core/tests/soak.rs:32 (共 294 物理行)
✅ packages/sz-orm-sqlx/tests/real_db_jepsen.rs:45 (共 599 物理行)
✅ packages/sz-orm-config/tests/config_diff_test.rs:109 (共 470 物理行)
✅ packages/sz-orm-core/tests/common/mod.rs:38 (共 961 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/loom-check/src/lib.rs:77 (共 293 物理行)

==========================================
  验证结果
==========================================
  ✅ 通过: 41
  ❌ 失败: 0
  ⚠️  警告: 0
==========================================
✅ 审计证据验证通过
```

---

*报告结束。本评测只读，未修改任何源码/文档/脚本；评测期间观察到 README 被并发进程更新（v10.12.0/21153 → v10.13.0/21161，2026-10-27 17:49），已在 §2.2 注明。*