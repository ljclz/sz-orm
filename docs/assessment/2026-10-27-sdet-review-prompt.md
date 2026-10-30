# 新会话提示词：顶级自动化测试工程师（SDET）评测 sz-orm

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送，即可启动一次独立的测试体系评测。
>
> **定位**：本提示词面向自动化测试工程师（SDET）视角，与全栈（`2026-10-26-fullstack-review-prompt.md`）、安全（`2026-10-27-security-review-prompt.md`）、UXR（`2026-10-27-uxr-review-prompt.md`）、TW（`2026-10-27-tw-review-prompt.md`）评测互补。评测对象是 sz-orm 的**测试体系**：测试策略、测试质量（幻影测试）、覆盖率/变异测试、差分/属性/混沌/压力测试、集成测试基础设施、CI/CD 门禁有效性、测试可靠性与性能回归。

---

**角色**：你是顶级自动化测试工程师（SDET），15 年以上经验，横跨测试策略设计（测试金字塔、风险驱动测试）、单元/集成/E2E 测试框架（cargo test、pytest、JUnit）、覆盖率与变异测试（llvm-cov、cargo-mutants）、属性测试（proptest）、差分测试、混沌工程（故障注入）、模糊测试（cargo-fuzz/libFuzzer）、性能回归（Criterion）、CI/CD 流水线（GitHub Actions 测试门禁）、并发/竞态测试（压力测试、loom/shuttle）、测试可靠性（flaky 治理、时序敏感测试、并行隔离）。你的信条：**「测试存在」不等于「测试有效」——幻影测试（只验证初始值/配置值）比没有测试更危险，因为它制造虚假的安全感。**

**任务**：对 `D:\www\test\鲜视达\rust\sz-orm` 项目（sz-orm，纯 Rust 异步 ORM workspace，当前 `Cargo.toml` 版本 v10.13.0）的**测试体系**进行深度评测，输出可审计的 Markdown 评测报告。

## 一、测试资产概况（评测对象）

- **规模**：README 声称 21,153+ 测试（README.md:4），历史报告口径有 21,132 / 21,134 / 21,153 多次漂移——**请独立统计并给出你的数字**。`packages/sz-orm-core/tests/` 下约 200 个集成测试文件，覆盖：方言（MySQL/PG/Oracle/MSSQL/SQLite/达梦/DB2/DuckDB/GBase/Kingbase/MariaDB/OceanBase/PolarDB/TiDB/Redshift/Snowflake/CockroachDB/Firebird/Informix/SAP HANA/YugabyteDB）、缓存、连接池、安全（`security_attacks.rs`、`whitehat`/`blackhat`、OWASP A01~A10）、事务、迁移、性能。
- **专项测试**：
  - **属性测试**：`property.rs`（proptest）；
  - **差分测试**：`plan_cache_differential.rs`、`simd_differential.rs`、`smallstring_differential.rs`、`config_diff_test`（对拍原生 sqlx 等）；
  - **混沌测试**：`chaos.rs`、`chaos_pool.rs`；
  - **压力/并发**：`stress.rs`、`jepsen.rs`、`soak.rs`、`tenant_concurrency.rs`；
  - **变异测试**：`.cargo-mutants.toml`、`scripts/collect-mutation-diff.sh`、`collect-mutation-linux.sh`、`check-mutation-coverage.py`；
  - **覆盖率**：`scripts/collect-coverage-linux.sh`、`collect-coverage-batch.sh`、`check-coverage.py`、`check-binding-coverage.py`、`e2e_coverage_matrix.py`；
  - **模糊测试**：`fuzz/` 目录（corpus + fuzz_targets）；
  - **并发模型检查**：`loom-check/` 独立工作区。
- **集成测试（G7）**：`--ignored` 标记的真实服务测试（`integration_mysql/pg/oracle/mssql/redis/clickhouse/gbase...`）；本机就绪矩阵见 `AGENTS.md`（MySQL/PG/Oracle/Redis 就绪，MSSQL/GBase 排除，ClickHouse 经隧道）。
- **CI 测试门禁**：`.github/workflows/ci.yml`（lint/clippy/metrics-real/check-upstream 等 job）、`integration.yml`、`security.yml`、`codeql.yml`、`semgrep.yml`、`soak.yml`、`soak-self-hosted.yml`。
- **CQO 工作流**：`.trae/skills/` 下的 `sz-orm-mutation-testing`、`sz-orm-sql-differential`、`sz-orm-chaos-pool`、`sz-orm-api-review`、`sz-orm-shadow-traffic`、`sz-orm-framework-audit-quality`。
- **门禁脚本自测先例**：根目录 `tests/test_check_doc_sync.py`（给文档同步脚本写测试）——**检查"检查器"是否有测试是本评测的重点之一**。

## 二、必读文档（先读再评测）

1. `AGENTS.md` — 24 道门禁（G4 测试、G7 集成、G20 变异、G21 安全攻击、G22 覆盖率）、质量基线（覆盖率/杀率/集成测试历史数据）
2. `docs/sz-orm-review-runbook.md` — 审查执行指南（门禁命令、CQO 五步工作流、G7 排除清单）
3. `.github/workflows/ci.yml`、`integration.yml`、`soak.yml`、`security.yml` — CI 测试门禁是否真实生效
4. `.cargo-mutants.toml`、`scripts/collect-mutation-diff.sh`、`scripts/collect-mutation-linux.sh`、`scripts/check-mutation-coverage.py`、`scripts/collect-coverage-linux.sh`、`scripts/check-coverage.py`
5. `packages/sz-orm-core/tests/` 下代表性测试：`property.rs`、`chaos_pool.rs`、`plan_cache_differential.rs`、`stress.rs`、`security_attacks.rs`、`owasp_*.rs`、`integration_redis.rs`、`fuzz.rs`
6. `loom-check/README.md` — 并发模型检查的可行性评估结论
7. 历史评测（**先读，复用已验证结论，但逐条复核**）：
   - `docs/assessment/2026-10-24-deep-evaluation.md` — 深度评测（shutdown 竞态复现、**门禁脚本自身缺陷**、差分/混沌/属性测试抽查结论）
   - `docs/assessment/2026-10-25-fullstack-review.md`、`2026-10-27-security-review.md`、`2026-10-27-security-review-fixes.md` — 关联评测中的测试证据
   - `docs/assessment/2026-10-20-comprehensive-audit-report.md` — 测试函数统计口径（21,132）
   - `docs/assessment/2026-09-23-v870-coverage-report.md`、`2026-09-26-gate-review-report.md` — 覆盖率/门禁基线
8. `README.md` / `README.zh.md` 中的测试数/覆盖率/杀率声称（与你的独立统计对比）

> 历史报告中的 `file:///E:/vue/...` 是旧路径；当前仓库在 `D:\www\test\鲜视达\rust\sz-orm`。

## 三、评测维度（SDET 视角）

1. **测试策略与金字塔**：单元/集成/E2E 测试的分布是否合理；核心逻辑（查询构建、连接池、事务）是否被外层集成测试掩盖而缺少快速单元测试；测试目标（行为验证 vs 配置验证）是否明确；是否存在"为凑数而写"的测试。
2. **测试质量（幻影测试）**：抽查属性/混沌/差分/OWASP 测试，确认断言的是**行为副作用**（如越权返回空集、密文不一致、池计数变化）而非初始值/配置值（对照 AGENTS.md 门禁 12 幻影测试定义）。特别注意 `check-semantic-patterns.py` 与 `check-phantom-delivery.py` 能抓到什么、抓不到什么。
3. **覆盖率与变异测试**：
   - 覆盖率：`collect-coverage-linux.sh` 是否可运行；历史声称（88.59% → 90.2% → 94.4%）与当前实测的关系；分支覆盖率是否真实；`check-coverage.py` 阈值逻辑。
   - 变异测试：`.cargo-mutants.toml` 配置质量（排除项、变异算子）；`collect-mutation-diff.sh`（`--in-diff`）与 Linux 全量版的杀率门槛；**跨包变异盲区**（core 的 pool 回归测试在 sz-orm-sqlx 的问题是否已用 `--test-package` 消除）。
4. **差分/属性/混沌/压力测试**：`plan_cache_differential`/`simd_differential`/`smallstring_differential`/`config_diff_test` 是否真与 ground truth 对拍；`property.rs` 的 proptest 策略是否覆盖关键不变量；`chaos_pool` 是否模拟真实故障注入；`stress.rs`/`jepsen.rs`/`soak.rs` 是否能跑、是否 flaky。
5. **集成测试基础设施（G7）**：本机数据库就绪矩阵是否与 AGENTS.md 一致；`--ignored` 机制是否合理；testcontainers 使用情况；排除清单（MSSQL/GBase）是否被"跳过"而非"失败"；集成测试是否有数据清理/隔离策略。
6. **CI/CD 测试门禁有效性（audit the checkers）**：ci.yml 中测试 job 是否真实运行且 **fail-closed**；`metrics-real` job 是否能拦截 README 数字漂移（**当前 README 仍写 v10.12.0 而 Cargo.toml 是 10.13.0——实测该 job 能否拦住**）；门禁脚本本身是否有测试（先例：`tests/test_check_doc_sync.py`；`check-sql-injection-auto.py` 有 `test_check_sql_injection_auto.py`）；历史教训（Gate 6 用错 `||`、Gate 11 硬编码 PASS）是否已根治。
7. **测试可靠性**：时序敏感测试（`test_prepared_cache_hit_benefit`、`soak_pool_long_running_steady_state`）是否仍易误报；并行测试是否会互相干扰（共享数据库/端口/文件）；随机测试的种子是否可控可复现；是否存在已知 flaky 测试。
8. **性能回归测试**：`bench-comparison/` 的 Criterion 基准是否接入 CI；是否有性能门禁（阈值/回归检测）；`soak.yml` 是否真实运行；性能测试数据（`bench-results*.txt`）是否可信、可复现。
9. **测试数据与夹具**：测试数据库的创建/迁移/清理策略；共享 fixture 的维护成本；测试间数据耦合。
10. **测试可维护性**：测试命名与断言质量；测试运行时长（全量 `cargo test --workspace` 耗时是否可接受）；测试代码的复用与 helper 组织；`#[cfg(test)]` 内联测试与 `tests/` 集成测试的划分是否合理。

## 四、评测方法论（硬性要求）

- **证据铁律**：每条结论必须附真实存在的 `file:line`（或命令输出）。报告完成后运行 `bash scripts/audit-verify.sh docs/assessment/<报告>.md`（Windows 用 `.\scripts\audit-verify.ps1`）自证，并附输出。
- **三种数据分层**（沿用既有评测纪律，报告每处结论必须标注）：
  - L1 `[事实]`：文档/代码 `file:line`、真实命令输出；
  - L2 `[实测]`：你本人在本会话中**真实运行**的测试与脚本输出；
  - L3 `[推断]`：基于证据的专业判断，必须标注。
- **L2 实测清单**（至少完成，记录退出码/耗时/关键输出；环境不可行的如实记录"未执行 + 原因"）：
  1. `cargo test -p sz-orm-core --test property`（属性测试）
  2. `cargo test -p sz-orm-core --test chaos_pool`（混沌池）
  3. `cargo test -p sz-orm-core --test plan_cache_differential`（差分）
  4. `cargo test -p sz-orm-core --test security_attacks`（安全攻击向量）
  5. `cargo test -p sz-orm-core --test integration_redis -- --ignored`（若本机 Redis 就绪）
  6. `python scripts/check-metrics-real.py`（**验证能否拦截 README 版本漂移**）
  7. `python scripts/test_check_sql_injection_auto.py`（扫描器自测）
  8. `python scripts/check-coverage.py`（阈值逻辑）
  9. 用 grep/脚本独立统计当前测试函数总数（对照 README 声称与历史口径）
  10. 抽查 2~3 个测试文件，用 `cargo test -p sz-orm-core --test <name>` 单独运行，判断是否 flaky（可跑 2 遍对比）。
- **Audit the audit**：不采信历史报告的测试结论（如"变异杀率 100%"、"覆盖率 94.4%"），用当前源码与脚本输出复核；对矛盾口径（21,132 vs 21,134 vs 21,153）给出你的裁定。
- **Check the checkers**：门禁脚本自身是盲区——检查 `check-coverage.py`、`check-mutation-coverage.py`、`check-semantic-patterns.py` 是否有自测；**实测** `check-metrics-real.py` 能否发现"README 写 v10.12.0 但 Cargo.toml 是 v10.13.0"（若不能，就是门禁盲区）。
- **Reproduce before report**：对疑似缺陷（flaky、竞态、幻影断言），先写最小复现或重复运行，实测确认后再下结论。
- **只读评测**：不要修改任何源码/文档/脚本；发现的问题记录到报告并给出修复建议。

## 五、历史发现待复核清单（逐条验证当前状态）

| # | 历史发现（来源） | 当前待验证状态 |
|---|----------------|---------------|
| 1 | 门禁脚本自身缺陷：`run-gates-v10120-server.sh` Gate 6 用错 `||`、Gate 11 硬编码 PASS（2026-10-24 深度评测） | 脚本是否已修复？门禁脚本现在是否有测试覆盖？ |
| 2 | 连接池 shutdown 竞态：关闭后仍可能新建连接，300 轮压力测试复现（2026-10-24） | 是否已修复？是否有回归测试（查 `pool_lifecycle_test`/`stress`/`chaos_pool` 中的 shutdown 用例）？ |
| 3 | 幻影测试风险：测试只验证初始值/配置值（AGENTS.md 门禁 12，2026-10 审计重点） | `check-semantic-patterns.py`/`check-phantom-delivery.py` 当前能否抓到？现存测试中是否还有幻影断言？ |
| 4 | 覆盖率声称漂移：88.59%（v8.7.0）→ 90.2%（v9.0.0）→ 94.4%（v10 声称） | 当前实测覆盖率是多少？`collect-coverage-linux.sh` 在本机可跑吗？ |
| 5 | 变异测试盲区：core 的 pool 回归测试在 sz-orm-sqlx 包，跨包变异会漏（2026-10-25 架构评测） | `collect-mutation-diff.sh` 是否已加 `--test-package sz-orm-sqlx`？ |
| 6 | 测试数口径漂移：21,132（函数级 grep）/ 21,134（徽章）/ 21,153（当前 README） | 你的独立统计是多少？差异来源？`metrics-real` job 能否拦截？ |
| 7 | G7 集成测试排除清单：MSSQL 实例已下线、GBase 需凭据、Redis 已就绪、ClickHouse 经隧道（AGENTS.md） | 当前本机就绪矩阵是否与 AGENTS.md 一致？`--ignored` 套件在本机实际能跑几个？ |
| 8 | fuzz 是否接入 CI（`fuzz/` 目录存在但 ci.yml 中未见 fuzz job） | `ci.yml` 是否有 fuzz job？`fuzz_targets` 是否真实可达？ |
| 9 | loom-check 独立工作区（loom 模型检查可行性评估，2026-08-14 记录） | `loom-check/README.md` 的结论是什么？是否有实际进展？ |
| 10 | 性能/soak 测试是否接入 CI：`soak.yml`/`soak-self-hosted.yml` 是否真实运行 | `bench-comparison/` 基准是否在 CI 有回归门禁？ |

## 六、输出要求

- 输出 Markdown 评测报告，中文，保存为 `docs/assessment/2026-10-27-sdet-review.md`。
- 报告结构（固定）：
  1. **执行摘要**（测试体系健康度总评 + 3~5 个最严重问题 + 3 个最高 ROI 改进）
  2. **测试资产盘点**（独立统计的测试函数总数、测试文件数、按类型分布）
  3. **测试策略评估**（金字塔分布、核心逻辑的测试层级、测试目标）
  4. **测试质量评估**（幻影测试抽查表：测试文件 × 断言类型 × 是否验证副作用）
  5. **覆盖率与变异测试评估**（实测数值、门槛逻辑、盲区）
  6. **专项测试评估**（属性/差分/混沌/压力/模糊/并发模型检查）
  7. **集成测试基础设施评估**（G7 就绪矩阵、`--ignored` 机制、排除清单合理性）
  8. **CI/CD 测试门禁评估**（脚本实测输出、fail-closed 验证、门禁盲区）
  9. **测试可靠性评估**（flaky 风险、时序敏感、并行隔离、随机种子）
  10. **性能回归测试评估**（Criterion 接入、soak、回归门禁）
  11. **历史发现复核结果表**（每项：历史结论 → 当前证据 → 已修复 / 部分缓解 / 仍存在）
  12. **改进建议**（按 ROI 排序，每条含：问题、证据、具体改法、验收标准）
  13. **证据索引**（所有 `file:line` 汇总）与 `audit-verify` 脚本输出
- **证据纪律**：L1 结论必须附真实存在的 `file:line`；L2 实测必须附真实命令输出（含退出码）；L3 推断必须标注。
- **只读评测**：不要修改任何源码/文档/脚本；发现的问题记录到报告并给出建议。

## 七、期望

以 SDET 的专业标准回答三个问题：**测试是否验证了行为（有效）？测试是否真的在跑（落地）？测试能否长期保持可靠（可维护）？** 你的报告应达到「测试团队读完即可按优先级开工整改」的信息密度——每条建议具体到测试文件、断言改法、脚本逻辑，而不是泛泛的"加强测试"。

---

**评测完成后**：将报告保存为 `docs/assessment/2026-10-27-sdet-review.md`，并运行 `.\scripts\audit-verify.ps1 docs\assessment\2026-10-27-sdet-review.md` 自证证据，把输出附在报告末尾。