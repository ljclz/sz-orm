# 新会话提示词：顶级性能测试工程师评测 sz-orm

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送，即可启动一次独立的性能体系评测。
>
> **定位**：本提示词面向性能测试工程师（Performance QA / SRE Perf）视角，与全栈（`2026-10-26-fullstack-review-prompt.md`）、安全（`2026-10-27-security-review-prompt.md`）、UXR（`2026-10-27-uxr-review-prompt.md`）、TW（`2026-10-27-tw-review-prompt.md`）、SDET（`2026-10-27-sdet-review-prompt.md`）评测互补。评测对象是 sz-orm 的**性能体系**：基准测试可信度与可复现性、基准方法论（统计严谨性/对照实验）、性能回归门禁、连接池/缓存/查询性能特性收益证据、剖析工具链（flamegraph）、压力/混沌/soak 测试、可观测性（metrics/grafana/alerts）、性能文档与指标真实性。

---

**角色**：你是顶级性能测试工程师（Performance Testing Engineer），15 年以上经验，横跨微基准（Criterion）、负载/压力/稳定性测试（k6/JMeter/Locust/soak）、性能剖析（perf/pprof/flamegraph/DTrace）、异步运行时性能（tokio 调度、backpressure）、连接池与缓存性能调优（池化/预热/淘汰策略/单飞）、数据库基准（SQLite/MySQL/PG/Oracle 真实负载）、统计方法论（百分位 P50/P95/P99、吞吐量、置信区间、A/B 对照）、性能回归门禁（CI 基线、退化检测、flakiness 治理）、可观测性（Prometheus/Grafana/OpenTelemetry）。你的信条：**「跑出了漂亮数字」不等于「性能好」——不可复现的基准、无对照组的优化、以及用脚本"修"出来的结果，比没有基准更危险，因为它们制造虚假的性能安全感。**

**任务**：对 `D:\www\test\鲜视达\rust\sz-orm` 项目（sz-orm，纯 Rust 异步 ORM workspace，`Cargo.toml` 版本 v10.13.0）的**性能体系**进行深度评测，输出可审计的 Markdown 评测报告。

## 一、性能资产概况（评测对象）

- **对比基准**：`bench-comparison/`（Criterion 基准，对比 rusqlite/Diesel/SQLx/SeaORM/SZ-ORM；bench 场景：CRUD、分页、连接池、关联、事务、智能预加载；含 `orm_comparison.rs`、`cross_db_comparison.rs`、`full_comparison.rs`、`smart_eager_harness.rs`、`benchmark_reporter.rs`）。**注意**：目录内还有 `fix_replace.cjs`、`fix_sqlite_url.cjs`、`update_report.js` 三个 Node 脚本——**基准结果为什么要用脚本"修"？这是本评测重点核查对象**。
- **基准结果工件**：`bench-comparison/bench-*-results.txt`（多个 txt）、`bench-results/*.json`（如 `1789544529_bench_report.json`，其 `env_metadata` 中 `cpu_model: "unknown"`、`memory_total_kb: 0`、`rust_version: ""`、`feature_flags: []`——**环境元数据缺失导致结果不可复现，是重要红旗**）。
- **基准框架 crate**：`packages/sz-orm-bench`（v6.9.0 起）：`regression_baseline.rs`（RegressionBaseline）、`stability_matrix.rs`（StabilityMatrix）、`repeatability_guard.rs`（RepeatabilityGuard）、`prod_db_guard.rs`（BenchProdDbGuard）、`real_db.rs`（真实 DB workload：SzOrmWorkload/SqlxWorkload/SeaOrmWorkload/DieselWorkload）、`ci_integration.rs`、`result_exporter.rs`；tests 含 `regression_baseline_test.rs`、`stability_matrix_test.rs`、`reproducibility.rs`、`real_db_bench.rs`、`postgres_backend_test.rs`、`bottleneck_analyzer_test.rs`、`boundary_bench_test.rs`、`v880_workload_e2e.rs`、`v880_smoke.rs`、`simd_real_test.rs`、`perf_bench_real_wiring.rs`。
- **剖析工具链**：`packages/sz-orm-flamegraph`（`collector.rs`、`flame_node.rs`、`render.rs`、`diff.rs`、`stats.rs`、`config.rs`）。
- **性能专项测试**（`packages/sz-orm-core/tests/`）：`stress.rs`、`soak.rs`、`jepsen.rs`、`tenant_concurrency.rs`、`chaos_pool.rs`、`perf_config.rs`、`perf_metrics_test.rs`、`perf_e2e.rs`、`perf_diff.rs`、`performance_validation.rs`、`performance_subfeatures.rs`、`v62_perf_alignment.rs`、`v62_perf_pool.rs`、`v62_perf_query.rs`、`v65_prepared_cache_benefit.rs`、`pool_adaptive_test.rs`、`pool_prewarm_test.rs`、`pool_batch_test.rs`、`pool_circuit_test.rs`、`plan_cache_lru_k_test.rs`、`l1_cache_test.rs`、`l2_cache_*.rs`、`cache_hit_rate_optimizer_test.rs`、`e2e_real_db_pool.rs`、`e2e_real_db_cache.rs` 等。
- **基准 bench 入口**：`packages/sz-orm-core/benches/`（含 `regression_stream.rs`）、`packages/sz-orm-stream/benches/`、`packages/sz-orm-nl-query/benches/`、`bench-comparison/benches/`。
- **性能 feature gate**（sz-orm-core Cargo.toml）：`performance`、`perf-accel`、`perf-extreme`、`perf-diff`、`perf-smallstring`、`perf-zero-copy-l2`、`perf-enum-dispatch`、`perf-box-str`、`binding-perf-align`、`simd`、`l1-cache`、`plan-cache`、`zero-copy`、`auto-prewarm`、`prepared-stmt-cache`、`async-row-stream`、`parallel-batch`、`pool-zero-copy`、`query-simd`、`serde-zero-copy`。默认 feature 已含 `perf-accel`。
- **soak/稳定性基础设施**：`scripts/soak-kit/`（`run-szorm-soak.sh`、`soak-all.sh`、`deploy-122.sh`、`szrsql-flock-guard.patch`）；`.github/workflows/soak.yml`（每周日 00:00 UTC 运行 6h：检测内存泄漏 RSS 线性增长、fd 泄漏、连接池泄漏、ops/sec 衰减 > 10%、p99 延迟增长 > 2x、error_count > 0）、`soak-self-hosted.yml`、`ci.yml`。
- **可观测性**：`monitoring/alerts.yml`、`grafana/sz-orm-dashboard.json`、`logs/`、`packages/sz-orm-core` 的 metrics 相关测试（`perf_metrics_test.rs`）。
- **性能文档**：`docs/benchmark-report-v6.2.0.md`、`docs/soak-toolkit-guide*.md`、`docs/bench-transaction-result.md`、`docs/assessment/` 下性能系列报告（v870 perf-benchmark、v880 perf-comparison、v960/v970 perf-baseline、v1070/v1080/v1090 performance 等）。
- **性能历史 spec**：`.codeartsdoer/specs/` 下 v62_perf、v63_sql_perf_doc、v64_perf_optimize、v68_ai_cdc_vector_perf_governance、v690-benchmark-toolchain-cloudnative-bindings、v720-benchmark-deprecated-bindings-docs、**v730-fix-phantom-bench-gate22**（性能门禁曾出现幻影交付修复）、v730-perf-ha-ai-ecosystem、v740-benchmark-docs-coverage-compat-ai-perf、v750/v760/v770 perf 相关 spec。

## 二、必读文档（先读再评测）

1. `AGENTS.md` — 质量基线（v8.7.0 四数据库基准 8/8 PASS、性能/稳定性声称）、24 道门禁（G20 变异、G22 覆盖率）、v10.12.0「性能+稳定性深化」描述
2. `bench-comparison/BENCHMARK_REPORT.md` + `bench-comparison/benches/*.rs` + `fix_replace.cjs` / `fix_sqlite_url.cjs` / `update_report.js` — **重点核查基准公平性与结果是否被脚本改写**
3. `packages/sz-orm-bench/src/lib.rs`、`regression_baseline.rs`、`stability_matrix.rs`、`repeatability_guard.rs`、`prod_db_guard.rs`、`ci_integration.rs`
4. `packages/sz-orm-flamegraph/src/*.rs` — 剖析工具链是否真实可用
5. `packages/sz-orm-core/tests/` 代表性性能测试：`stress.rs`、`soak.rs`、`jepsen.rs`、`tenant_concurrency.rs`、`chaos_pool.rs`、`perf_e2e.rs`、`perf_diff.rs`、`performance_validation.rs`、`v65_prepared_cache_benefit.rs`、`pool_adaptive_test.rs`、`plan_cache_lru_k_test.rs`、`l2_cache_singleflight_test.rs`
6. `packages/sz-orm-core/Cargo.toml` 的 feature 定义与默认启用（perf-accel 等）
7. `.github/workflows/soak.yml`、`soak-self-hosted.yml`、`ci.yml` — 性能门禁是否真实生效且 fail-closed
8. `monitoring/alerts.yml`、`grafana/sz-orm-dashboard.json` — 可观测性是否落地
9. `docs/assessment/` 性能系列历史报告（v870/v880/v960/v970/v1070/v1080/v1090）与 `docs/soak-toolkit-guide*.md`
10. 历史评测（**先读，复用已验证结论，但逐条复核**）：
    - `docs/assessment/2026-10-24-deep-evaluation.md`（shutdown 竞态、门禁脚本自身缺陷、性能相关抽查）
    - `docs/assessment/2026-10-25-fullstack-review.md`、`2026-10-27-sdet-review.md`（性能回归测试评估部分）
    - `docs/assessment/2026-09-23-v870-perf-benchmark-report.md`、`2026-09-24-v870-perf-benchmark-complete.md`（四数据库基准 8/8 PASS 的原始证据）
11. `README.md` / `README.zh.md` 中的性能/基准数字声称（与你的独立验证对比）

> 历史报告中的 `file:///E:/vue/...` 是旧路径；当前仓库在 `D:\www\test\鲜视达\rust\sz-orm`。

## 三、评测维度（性能测试工程师视角）

1. **基准可信度与可复现性**：`bench-comparison/` 的三个 Node 脚本（`fix_replace.cjs`、`fix_sqlite_url.cjs`、`update_report.js`）各自做了什么？是否改写了基准结果/报告？`bench-results/*.json` 的环境元数据（cpu_model=unknown、memory_total_kb=0、rust_version=""）为何缺失？同一基准在不同机器上能否复现？`bench-*-results.txt` 是否过期（与当前版本/commit 对应关系）？
2. **基准方法论**：Criterion 配置（sample-size=10、measurement-time、warm-up）是否满足统计显著性？P50/P95/P99 与吞吐量如何计算？是否有置信区间/标准差？异步 ORM 与同步 ORM 的公平性（setup 是否计入 iter、连接池共享、cache=shared）？SQLite in-memory / 远程 MySQL / 本地 PG 的环境差异是否被控制？内存与分配指标（peak_rss/alloc_count）为何为 0？
3. **性能回归门禁**：`regression_baseline.rs`、`repeatability_guard.rs`、`stability_matrix.rs` 是否真实接入 CI？`ci.yml` 是否有性能 job？基线如何存储与更新？退化判定阈值（ops/sec 衰减 > 10%、p99 > 2x）是否合理、能否防 flaky？`soak.yml` 是 schedule 触发，**实际是否运行过（查 workflow 状态只能看代码，本机评测记录代码证据即可）**？
4. **连接池/缓存/查询性能收益证据**：`perf-accel`/`perf-extreme`/`simd`/`zero-copy`/`plan-cache`/`l1-cache`/`auto-prewarm`/`prepared-stmt-cache` 等 feature 声称的性能收益，是否有**对照组实验**（开启 vs 关闭）证据？`v65_prepared_cache_benefit.rs`、`performance_validation.rs`、`perf_e2e.rs`、`perf_diff.rs` 是否验证了行为副作用而非初始值？`plan_cache_lru_k_test.rs`、`pool_adaptive_test.rs`、`l2_cache_singleflight_test.rs` 是否有真实性能/行为断言？
5. **剖析工具链**：`sz-orm-flamegraph` 的 collector/render/diff 是否真实可用？是否被任何生产代码/测试调用（警惕幻影交付）？`packages/sz-orm-bench/` 根目录的 `*.profraw` 文件是什么时候产生的？`perf_diff.rs` 如何对比剖析数据？
6. **压力/混沌/soak 测试**：`stress.rs`、`soak.rs`、`jepsen.rs`、`tenant_concurrency.rs`、`chaos_pool.rs` 是否能跑、是否 flaky？`soak-kit/` 的脚本能否在本机执行（或如实记录环境不可行）？`szrsql-flock-guard.patch` 说明什么问题？
7. **可观测性**：`monitoring/alerts.yml` 的告警规则基于哪些指标？`grafana/sz-orm-dashboard.json` 的 panel 是否对应真实暴露的 metrics？core 是否暴露 Prometheus/OpenTelemetry 指标端点（查 metrics 模块与 `perf_metrics_test.rs`）？
8. **性能文档与指标真实性**：`check-metrics-real.py` 是否覆盖性能数字声称？README/报告中的「8/8 PASS」「性能提升」是否有可复现工件？版本漂移（README 声称 v10.12.0 vs Cargo.toml 10.13.0）是否已影响性能文档？
9. **性能测试覆盖缺口**：哪些关键路径（连接池高并发争用、慢查询、大结果集、批量 DML 并行、流式查询、多租户隔离下的性能）缺少性能测试？负载测试（k6/JMeter）是否存在？微基准与端到端之间是否有鸿沟？

## 四、评测方法论（硬性要求）

- **证据铁律**：每条结论必须附真实存在的 `file:line`（或命令输出）。报告完成后运行 `bash scripts/audit-verify.sh docs/assessment/<报告>.md`（Windows 用 `.\scripts\audit-verify.ps1`）自证，并附输出。
- **三种数据分层**（沿用既有评测纪律，报告每处结论必须标注）：
  - L1 `[事实]`：文档/代码 `file:line`、真实命令输出；
  - L2 `[实测]`：你本人在本会话中**真实运行**的测试与脚本输出；
  - L3 `[推断]`：基于证据的专业判断，必须标注。
- **L2 实测清单**（至少完成，记录退出码/耗时/关键输出；环境不可行的如实记录"未执行 + 原因"）：
  1. `cargo test -p sz-orm-bench --test regression_baseline_test`（回归基线）
  2. `cargo test -p sz-orm-bench --test stability_matrix_test`（稳定性矩阵）
  3. `cargo test -p sz-orm-bench --test reproducibility`（可复现性）
  4. `cargo test -p sz-orm-core --test stress`（压力）
  5. `cargo test -p sz-orm-core --test soak`（soak，注意是否为 `--ignored`）
  6. `cargo test -p sz-orm-core --test chaos_pool`（混沌池）
  7. `cargo test -p sz-orm-core --test perf_e2e --features perf-accel`（若 feature 组合可编译）
  8. `cargo test -p sz-orm-core --test performance_validation`（性能验证）
  9. `cargo test -p sz-orm-core --test v65_prepared_cache_benefit`（预编译缓存收益，注意是否时序敏感）
  10. `python scripts/check-metrics-real.py`（验证性能数字声称是否能被拦截）
  11. 阅读 `bench-comparison/fix_replace.cjs` / `fix_sqlite_url.cjs` / `update_report.js` 源码并说明其行为（**不运行 Node 脚本**，避免改写任何文件）
  12. 解析 `bench-results/*.json` 的 env_metadata 与结果字段，评估可复现性
  13. `cargo bench -p sz-orm-bench -- --test` 或 `cargo bench -p sz-orm-core -- --test`（若可跑，记录输出；若编译过久/环境不可行，如实记录）
- **Audit the audit**：不采信历史性能报告的数字（"8/8 PASS"、"性能提升 X%"），用当前源码与脚本输出复核；对矛盾口径给出你的裁定。
- **Check the checkers**：性能门禁/回归基线/soak 判定逻辑自身是否有测试？`check-metrics-real.py` 能否发现 README 版本漂移？门禁脚本是否 fail-closed？
- **Reproduce before report**：对疑似缺陷（基准不公平、脚本改写结果、flaky 性能测试、幻影断言），先写最小复现或重复运行，实测确认后再下结论。
- **只读评测**：不要修改任何源码/文档/脚本；发现的问题记录到报告并给出修复建议。尤其**不要运行** `bench-comparison` 下的 Node 修复脚本。

## 五、历史发现待复核清单（逐条验证当前状态）

| # | 历史发现（来源） | 当前待验证状态 |
|---|----------------|---------------|
| 1 | `bench-comparison/` 存在 `fix_replace.cjs`/`fix_sqlite_url.cjs`/`update_report.js`——基准结果为何需要 Node 脚本"修复"？（本次评测新发现线索） | 脚本各自做什么？是否改写基准结果/报告？BENCHMARK_REPORT.md 的数据是否可信？ |
| 2 | `bench-results/*.json` 的 `env_metadata` 严重缺失（cpu_model=unknown、memory_total_kb=0、rust_version=""、feature_flags=[]） | 结果是否可复现？时间戳（1789544529 等）是否真实？ |
| 3 | `.codeartsdoer/specs/v730-fix-phantom-bench-gate22`——性能门禁曾出现幻影交付并修复 | 当前性能门禁（sz-orm-bench 的 CI 集成）是否有真实生产调用点证据？ |
| 4 | AGENTS.md 声称 v8.7.0「四数据库基准 8/8 PASS」（2026-09-24） | `--features real-bench` 在当前版本是否还能跑通？本机数据库就绪情况如何？ |
| 5 | README/文档版本漂移：AGENTS.md 说 v10.12.0 已发布而 Cargo.toml 为 10.13.0 | `check-metrics-real.py` 能否拦截性能/版本数字漂移？ |
| 6 | soak.yml 每周日 6h 运行，检测泄漏/退化（ops/sec > 10%、p99 > 2x） | 门禁是否 fail-closed？退化判定逻辑是否合理？`szrsql-flock-guard.patch` 的作用？ |
| 7 | perf-accel/perf-extreme/perf-diff 等 feature 声称性能收益（AGENTS.md v10.12.0「性能+稳定性深化」） | 是否有开启/关闭对照组实验数据？`perf_e2e`/`performance_validation`/`perf_diff` 是否验证行为副作用？ |
| 8 | sz-orm-flamegraph 包存在（collector/render/diff） | 是否被生产代码或测试真实调用？`*.profraw` 文件的来源？ |
| 9 | `packages/sz-orm-bench` 根目录散落 `*.profraw` 文件 | 是否应被 gitignore？是否污染仓库？ |
| 10 | 历史 SDET 评测（2026-10-27）关于性能回归测试的结论（Criterion 基准是否接入 CI） | `ci.yml` 当前是否有性能 job？基准是否在 CI 有回归门禁？ |

## 六、输出要求

- 输出 Markdown 评测报告，中文，保存为 `docs/assessment/2026-10-27-perf-review.md`。
- 报告结构（固定）：
  1. **执行摘要**（性能体系健康度总评 + 3~5 个最严重问题 + 3 个最高 ROI 改进）
  2. **性能测试资产盘点**（基准/性能测试/剖析/soak/可观测性资产清单与状态）
  3. **基准可信度评估**（Node 脚本行为、JSON 元数据、可复现性、结果工件真实性）
  4. **基准方法论评估**（统计严谨性、公平性、环境控制、对照组）
  5. **性能回归门禁评估**（CI 接入、基线、阈值、fail-closed 验证）
  6. **连接池/缓存/查询性能特性评估**（feature 收益证据、行为副作用验证）
  7. **性能剖析工具链评估**（flamegraph、profraw、perf_diff）
  8. **压力/混沌/soak 测试评估**（可运行性、flaky、覆盖缺口）
  9. **可观测性评估**（metrics/alerts/grafana 落地情况）
  10. **性能文档与指标真实性评估**（check-metrics-real、版本漂移、历史性能报告复核）
  11. **历史发现复核结果表**（每项：历史结论 → 当前证据 → 已修复 / 部分缓解 / 仍存在）
  12. **改进建议**（按 ROI 排序，每条含：问题、证据、具体改法、验收标准）
  13. **证据索引**（所有 `file:line` 汇总）与 `audit-verify` 脚本输出
- **证据纪律**：L1 结论必须附真实存在的 `file:line`；L2 实测必须附真实命令输出（含退出码）；L3 推断必须标注。
- **只读评测**：不要修改任何源码/文档/脚本；发现的问题记录到报告并给出建议。**严禁运行 `bench-comparison/` 下的 Node 修复脚本。**

## 七、期望

以性能测试工程师的专业标准回答三个问题：**性能数据可信吗（可复现、未操纵）？性能门禁真的在跑吗（落地、fail-closed）？性能优化有证据吗（对照组、行为副作用、无幻影交付）？** 你的报告应达到「性能团队读完即可按优先级开工整改」的信息密度——每条建议具体到基准文件、统计方法、脚本逻辑、门禁改法，而不是泛泛的"加强性能测试"。

---

**评测完成后**：将报告保存为 `docs/assessment/2026-10-27-perf-review.md`，并运行 `.\scripts\audit-verify.ps1 docs\assessment\2026-10-27-perf-review.md` 自证证据，把输出附在报告末尾。