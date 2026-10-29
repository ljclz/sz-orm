# sz-orm 性能体系深度评测报告（Performance Engineering Review）

> 评测人：顶级性能测试工程师（Performance Testing Engineer，15+ 年经验）
> 评测日期：2026-10-28
> 评测对象：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.0，Rust 2021，workspace 72 成员）
> 评测范围：基准可信度与可复现性、基准方法论、性能回归门禁、连接池/缓存/查询性能收益证据、剖析工具链、压力/混沌/soak 测试、可观测性、性能文档与指标真实性
> 评测方法：Audit the audit（不采信历史数字）、Check the checkers（检查门禁自身）、Reproduce before report（最小复现/重复运行确认后再下结论）、只读评测（不修改任何源码/文档/脚本）
>
> **证据分层**：L1 `[事实]`（file:line / 命令输出）、L2 `[实测]`（本会话真实运行输出）、L3 `[推断]`（基于证据的专业判断，标注）。

---

## 1. 执行摘要

### 性能体系健康度总评：**🟡 中等偏低（存在 P0 级可信度缺陷，门禁 fail-open）**

sz-orm 拥有**数量庞大且多数可运行**的性能测试资产（stress/soak/chaos/jepsen/SingleFlight/可复现性/回归基线/稳定性矩阵等，本会话实测 10 个套件全部通过），真实 DB 基准框架（`real-bench`）在 SQLite/PostgreSQL 上**真实可用**。但性能体系存在 3 个 P0 级可信度问题：

1. **存在由"模拟延迟模型"生成的基准结果报告**被当作真实基准工件存放（`bench-results/1789544529_bench_report.json`），其原始延迟与 `run_workload()` 的硬编码 profile **完全吻合**（L2 数学验证），环境元数据缺失（`cpu_model=unknown`、`memory_total_kb=0`、`rust_version=""`）。这是"制造虚假性能安全感"的典型形态。
2. **基准报告被 Node 脚本直接改写**（`update_report.js` 向 `BENCHMARK_REPORT.md` 插入/修改数据），且 `full_comparison.rs` 的报告生成器用**空 records** 生成空报告文件——报告链路本身不可信。
3. **CI 性能门禁 fail-open**：`ci.yml` 的 benchmark job `continue-on-error: true` + `|| echo WARNING` 吞掉一切错误，且 `cargo bench -p sz-orm-bench` 在当前工作区因 `libsqlite3-sys` 依赖冲突**根本无法解析**（L2 实测 FAILED）——CI 性能 job 事实上是"无论成败都放行"的空转。

### 3 个最严重问题

| # | 严重度 | 问题 | 证据 |
|---|--------|------|------|
| P0-1 | 伪造基准结果 | `bench-results/1789544529_bench_report.json` 是模拟模型输出，非真实测量 | [§3.2](#32-json-元数据缺失与模拟数据铁证) L2 数学验证 |
| P0-2 | 报告被脚本篡改 | `update_report.js` 直接改写 BENCHMARK_REPORT.md | [§3.1](#31-node-脚本行为核查) |
| P0-3 | 性能门禁 fail-open | CI benchmark job 吞错 + `cargo bench` 无法运行 + 基线缺失放行 | [§5](#5-性能回归门禁评估) L2 |

### 3 个最高 ROI 改进

1. **删除模拟基准路径**：`main.rs` 默认走 `--simulate`，且 `bench-results/*.json` 是模拟产物。删除 `run_workload()`/`framework_latency_factor`/模拟报告文件，`main.rs` 无 `real-bench` feature 时直接报错退出（fail-closed）。验收：`cargo run -p sz-orm-bench` 不再产生任何 JSON 报告，除非显式 `--features real-bench`。
2. **CI 性能门禁改为 fail-closed**：`ci.yml` benchmark job 去掉 `continue-on-error` 与 `|| echo WARNING`；增加基线对比步骤（调用 `RegressionBaseline::compare`），退化 ≥10% 阻断 PR；`cargo bench` 失败必须使 job 失败。同时修复 `libsqlite3-sys` links 冲突（统一 rusqlite 版本）。验收：故意制造退化时 PR 被阻断；`cargo bench` 在 CI 真正产出结果。
3. **建立真实 A/B 收益断言**：`performance_validation.rs` 的 SIMD 对比加上 `assert!(speedup > 1.0)`（或 1.05）；删除 `v65_prepared_cache_benefit.rs` 中注入 200µs sleep 造收益的做法，改为用真实 prepared statement 路径测量；`perf_bench_real_wiring.rs` 的 `RealSqliteExecutor` 改为真正连接 SQLite 执行查询。验收：性能收益测试无需"模拟成本注入"即可通过。

---

## 2. 性能测试资产盘点

| 资产 | 路径 | 状态（L1/L2） |
|------|------|--------------|
| Criterion 对比基准 | `bench-comparison/benches/*.rs`（13 个 bench） | 存在，`full_comparison.rs` 配置统计严谨（sample=100/CI 0.95），但 ReportGuard 生成空报告（[§4.4](#44-报告生成器幻影)） |
| 基准结果工件 | `bench-comparison/bench-*.txt`、`bench-results/*.json` | `1789544529` 为模拟数据（P0）；`1789562783` 含真实 DB 结果但元数据仍缺失；`bench-results-fixed.txt` 为空文件（P2） |
| Node 修复脚本 | `bench-comparison/fix_replace.cjs`、`fix_sqlite_url.cjs`、`update_report.js` | `update_report.js` 改写报告（P0）；`fix_replace.cjs` 仅 1 行注释（P2）；`fix_sqlite_url.cjs` 修复源码非结果（无害） |
| 基准框架 crate | `packages/sz-orm-bench`（9 个 src 模块 + 10 个测试） | 结构完整；模拟路径仍存在且默认可达（P1）；CI 集成 fail-open（P0） |
| 剖析工具链 | `packages/sz-orm-flamegraph`（7 个模块） | 库可用；未被 core/sqlx 查询路径调用（P2）；CLI 有演示命令 |
| 性能专项测试 | `packages/sz-orm-core/tests/`（stress/soak/jepsen/chaos_pool/perf_* 等 30+） | 本会话实测 10 套件全部通过（L2）；但多项"性能验证"缺少性能断言（P1） |
| 压力/混沌/soak 基础设施 | `scripts/soak-kit/`、`.github/workflows/soak.yml`、`soak-self-hosted.yml` | soak 冒烟 CI 每次运行（10s），6h 周更 fail-closed；`szrsql-flock-guard.patch` 为外部工具补丁 |
| 可观测性 | `monitoring/alerts.yml`、`grafana/sz-orm-dashboard.json`、`sz-orm-observability` | 告警/面板引用的指标名在源码中不存在，无 HTTP metrics 端点（P1） |
| 性能文档 | `docs/assessment/` 下 15+ 份性能报告、`docs/soak-toolkit-guide*.md` | 存在但含旧路径 `E:/vue/...`；"8/8 PASS"本机无法完整复现（P2，环境相关） |

---

## 3. 基准可信度评估

### 3.1 Node 脚本行为核查

**L1 `[事实]` `update_report.js` 直接改写基准报告（P0）**：
- `bench-comparison/update_report.js:5-19`：读取 `BENCHMARK_REPORT.md`，在标记 `参数绑定将 SZ-ORM 与 sqlx 在 SELECT BY ID` 前插入两行 SELECT ALL 差距数据（"sz-orm 192.60 ms vs sqlx 128.28 ms（慢 50.2%）"等），并修改差距描述（`:22-24` 将 `剩余差距` 改为 `在 SELECT ALL 场景提升 16.5%。剩余差距`）。
- 该脚本插入的文本与 `BENCHMARK_REPORT.md:430-431` 的正文**逐字一致**——报告内容确实被脚本改写。
- 脚本引用旧路径 `e:/vue/test/鲜视达/rust/sz-orm/bench-comparison/BENCHMARK_REPORT.md`（`:2`），是历史遗留。

**L1 `[事实]` `fix_replace.cjs` 是空壳**：`bench-comparison/fix_replace.cjs:1` 仅有一行注释 `// 修复 replace 参数类型（char`，无任何代码。文件名宣称"修复"但实际无逻辑。

**L1 `[事实]` `fix_sqlite_url.cjs` 修改源码而非结果**：`bench-comparison/fix_sqlite_url.cjs:9-22` 将 `benches/cross_db_comparison.rs` 中的 SQLite URL 从 `sqlite://{}` 改为 `sqlite://file:{}` 以兼容 Windows 盘符。这是**源码修复**，不涉及结果篡改，但暴露了"基准源码需要脚本事后修补"的流程缺陷。

**L3 `[推断]`**：`update_report.js` 的存在表明报告数字曾通过脚本"手工"补入，而非由基准程序一次性生成。这使 `BENCHMARK_REPORT.md` 的 SELECT ALL 章节（§2.3/§10.4）可信度严重受损——无法区分哪些数字是实测、哪些是脚本插入。

### 3.2 JSON 元数据缺失与模拟数据铁证

**L1 `[事实]` 元数据缺失根因**：`packages/sz-orm-bench/src/lib.rs:483` `EnvMetadata::default()` 将 `cpu_model` 硬编码为 `"unknown"`，`:485` `memory_total_kb` 为 `0`，`:487` `rust_version` 取 `env!("CARGO_PKG_RUST_VERSION", "unknown")`（构建时未设置则为空串），`:489` `feature_flags` 为空，`:494-511` git commit 通过 `git rev-parse` 获取（评测机 git 位于 `D:\Git` 但未加入 PATH，`capture_git_commit` 找不到 git → 空串；这是**评测环境 PATH 问题**，并非仓库缺陷）。因此 `bench-results/*.json` 的 `env_metadata` 缺失**不是数据丢失，而是生成器从未采集**。

**L2 `[实测]` 第一个 JSON 是模拟模型输出（P0 铁证）**：
解析 `bench-results/1789544529_bench_report.json`（`sz_orm_version: "7.1.0"`、`git_commit: ""`）全部 20 组 `raw_latencies` 后，得到 min/max 范围：

| workload | sz-orm | sea-orm | diesel | sqlx |
|----------|--------|---------|--------|------|
| single_row_query | 50–59 | 57–66 | 54–63 | 60–69 |
| batch_query | 501–596 | 576–671 | 541–636 | 601–696 |
| complex_join | 2001–2395 | 2301–2695 | 2161–2555 | 2401–2795 |
| transaction | 301–349 | 346–394 | 325–373 | 361–409 |
| pool_concurrency | 20–24 | 23–27 | 21–25 | 24–28 |

而 `lib.rs:746-759` `workload_latency_profile`（base/variance）与 `lib.rs:761-768` `framework_latency_factor`（SzOrm=1.0/SeaOrm=1.15/Diesel=1.08/Sqlx=1.20）给出的公式为 `base × factor + noise(0..variance)`。逐一验证：sz-orm single=50+[0..10)=50..59 ✓；sea-orm single=57.5+[0..10)=57..67 ✓（观测 57–66）；sqlx complex=2400+[0..400)=2400..2799 ✓（观测 2401–2795）…… **20/20 全部落在模拟区间内**。结论：该 JSON 是 `run_workload()` 模拟函数生成的"假基准报告"。

**L2 `[实测]` 第二个 JSON 是真实 DB 结果但元数据仍缺失**：`bench-results/1789562783_bench_report.json` 含 `git_commit: "578926448c4fafc09285411d1557a02152a65e17"`、部分结果 `is_real_db: true`、`peak_rss_kb` 非零、`raw_latencies` 高方差（如 `[153, 558, 178, 129, 132, 128, 57, 50, 32, 32]`）——符合真实测量特征；但 `cpu_model` 仍为 `"unknown"`、`memory_total_kb` 仍为 `0`、`rust_version` 仍为 `""`、`alloc_count/alloc_bytes` 为 `0`（`lib.rs:606-612` `MemoryMetrics::capture()` 恒置 0）。

**L3 `[推断]`**：仓库同时存放模拟报告与真实报告，且无任何标注区分（如 `is_simulated: true`），任何人误用 `1789544529` 都会得出"sz-orm 全面最快"的错误结论——这正是该 JSON 中 sz-orm 全场景第一的原因（模拟因子保证）。这是**最严重的可信度问题**。

### 3.3 结果工件真实性

- **L1 `[事实]` `bench-comparison/bench-results-fixed.txt` 为空文件**（5 字节，仅 BOM+空白）。名为 "fixed" 的结果文件无内容。
- **L1 `[事实]` `bench-comparison/bench-mysql-results.txt` 截断**（964 字节），只含 INSERT 1000 的 Trial 1，无完整结果。
- **L1 `[事实]` `bench-comparison/bench-pg-results.txt` / `bench-real-db-results.txt`** 为完整真实 DB 结果（3 trials + median），与 BENCHMARK_REPORT.md §7/§8 表格一致。
- **L1 `[事实]` `bench-comparison/bench-results.txt`（21KB）** 为 Criterion 原始输出（含置信区间如 `[3.3380 ms 3.4870 ms 3.6598 ms]`），是 BENCHMARK_REPORT.md SQLite 表格的原始来源（2026-07-23，v1.0.0，`E:\` 旧路径）。
- **L3 `[推断]` 工件过期**：上述 txt 均为 2026-07-23（v1.0.0 时代）产物，当前代码 v10.13.0；BENCHMARK_REPORT.md 无 commit 对应关系，无法确认其数据对应哪个版本。

### 3.4 可复现性实测

- **L2 `[实测]` `cargo test -p sz-orm-bench --test reproducibility --features real-bench`**：**3 passed; 0 failed**（sqlx/sea-orm/sz-orm 三框架，SQLite 真实文件库，3 次吞吐偏差阈值 30%/警告 5%）。`packages/sz-orm-bench/tests/reproducibility.rs:65-75` 定义断言。可复现性框架本身**真实可用**。
- **L2 `[实测]` `cargo bench -p sz-orm-bench -- --test` 与 `cargo bench -p sz-orm-core -- --test`**：**均 FAILED**，错误为 `libsqlite3-sys` links 冲突：`sz-orm-core` → `rusqlite 0.32`（需 `libsqlite3-sys ^0.30`）与 `sz-orm-studio` → `rusqlite 0.31`（需 `libsqlite3-sys ^0.28`）同时链接 `sqlite3` 原生库，Cargo 拒绝解析。**这直接证明 `ci.yml` 的 `cargo bench -p sz-orm-bench --features real-bench` 在相同解析条件下无法运行**（CI 用 `|| echo WARNING` 吞错）。注：本会话因执行该命令触发了 Cargo.lock 重解析并使其进入不一致状态，我已用 git 索引中的已提交 blob 恢复 `Cargo.lock`（L2：恢复后 `cargo test -p sz-orm-bench --test regression_baseline_test` 重新通过）。此依赖冲突是仓库**真实存在**的问题（详见 §12-ROI-2）。

---

## 4. 基准方法论评估

### 4.1 Criterion 配置

- **L1 `[事实]` `bench-comparison/benches/full_comparison.rs`** 使用 `Criterion::default().sample_size(100).warm_up_time(3s).measurement_time(10s).confidence_level(0.95)`——统计严谨。
- **L1 `[事实]` `bench-comparison/benches/orm_comparison.rs` / `cross_db_comparison.rs`** 无显式 Criterion 配置，依赖默认值；但 BENCHMARK_REPORT.md:5 声明 SQLite 部分使用 `sample-size=10, measurement-time=2s, warm-up-time=1s`（CLI 覆盖）。**sample-size=10 的统计功效极低**，BENCHMARK_REPORT.md:125 局限性也承认"统计置信度低于默认 100 样本"。置信区间存在（Criterion 输出含 CI），但报告只取 median，未呈现 CI/标准差。

### 4.2 公平性

- **L1 `[事实]` BENCHMARK_REPORT.md §1.3 记录了 v0.2→v0.4 公平性修复**：异步 ORM setup 移出 `b.iter`、`cache=shared`、`Drop for PooledConnection` 自动归还、`SzOrmCtx: Clone`。`orm_comparison.rs:9-14` 注释确认。设计方向正确。
- **L1 `[事实]` 异步 vs 同步对比在报告 §3.1 有明确 caveat**（同步无 runtime 开销）。OK。
- **L3 `[推断]` 潜在不公平点**：异步 ORM 用连接池（`max_connections(10)`）而 rusqlite 用单连接——池 acquire/release 开销计入异步 ORM 的每次 iter；报告 §3.2 的排名在"异步 ORM 间"公平，但与 rusqlite/Diesel 同步基线不完全对等（报告已声明）。

### 4.3 环境控制与内存指标

- **L2 `[实测]` MySQL/PG 连通性**：本机 MySQL 3306、PG 5432、Oracle 1521 端口均开放；但 `root:test123` 连接 MySQL 返回 `Access denied for user 'root'@'localhost'`（凭据与 AGENTS.md 不符，环境问题）；`postgres:test123` 连接 PG 18.4 成功；Oracle 缺 64 位 ODPI-C 客户端库（`DPI-1047`）。
- **L1 `[事实]` `bench-results/*.json` 的 `peak_rss_kb` 在模拟报告中为 0，真实报告中部分非零；`alloc_count/alloc_bytes` 恒为 0**（`lib.rs:606-612` `MemoryMetrics::capture()` 仅采 RSS，分配计数无实现）。内存与分配指标实际不可用。

### 4.4 报告生成器幻影

- **L1 `[事实]` `bench-comparison/benches/full_comparison.rs:41-42`**：`ReportGuard::drop()` 创建 `let records: Vec<BenchmarkRecord> = Vec::new();` 并调用 `generate_report_files(&records, &dialects)`——**records 恒为空**。`generate_report_files`（`:85-107`）会用空 records 写出 `benchmark-report.md/csv/json`（含表头无数据）。基准退出时生成的报告文件是**空报告**。`BenchmarkReporter::audit()`（`benchmark_reporter.rs:184-225`）能检测到缺失维度，但**从未被调用**。

---

## 5. 性能回归门禁评估

### 5.1 CI 接入（fail-open）

- **L1 `[事实]` `.github/workflows/ci.yml:696-772` Job 6 `benchmark`**：
  - `:700` `continue-on-error: true`（性能波动不阻断 PR，注释 ADR-005）；
  - `:762` `cargo bench -p sz-orm-bench --features real-bench -- --nocapture || echo "WARNING: benchmark had errors (non-blocking)"`——**命令失败也被吞掉**；
  - 无任何基线对比、退化判定、阈值检查步骤；产物仅上传 `target/criterion/` 与 `benchmark-report.json`（`:770-771`）。
- **L1 `[事实]` `packages/sz-orm-bench/src/ci_integration.rs:177-186`**：基线缺失时 `decision = CiBenchDecision::Pass`（`BENCH_BASELINE_MISSING` 放行）——**fail-open**。一个退化门禁在"没有基线"时应阻断或告警，而不是放行。
- **L2 `[实测]` `cargo bench -p sz-orm-bench` 在本机因 `libsqlite3-sys` 冲突无法解析**（见 §3.4）。若 CI 环境同样触发，`|| echo WARNING` 会掩盖，job 仍绿。**性能门禁事实上不生效**。
- **L1 `[事实]` `ci.yml:783-814` Job 6.5 `soak-smoke`**（每次 push 运行 10s soak 冒烟）无 continue-on-error，是**真正 fail-closed** 的性能稳定性门禁——这是 CI 中唯一实际阻断的性能相关检查。

### 5.2 门禁组件自身测试（L2 全部通过）

| 测试 | 命令（L2 实测） | 结果 |
|------|----------------|------|
| 回归基线 | `cargo test -p sz-orm-bench --test regression_baseline_test` | **4 passed / 0 failed** |
| 稳定性矩阵 | `cargo test -p sz-orm-bench --test stability_matrix_test --features stability-matrix` | **5 passed / 0 failed** |
| CI 接线 | `cargo test -p sz-orm-bench --test perf_bench_real_wiring --features perf-bench-real` | **3 passed / 0 failed** |

- **L1 `[事实]` 但 `perf_bench_real_wiring.rs`（原 `:55` 硬编码延迟实现，已于 2026-10-28 ROI-3 整改移除，见 `docs/assessment/2026-10-27-perf-fixes.md` §3.1）**：评测时其"真实 DB 执行器" `RealSqliteExecutor::execute` 直接返回硬编码 `vec![150u64, 180, 200, 220, 250, 300, 350, 400, 450, 500]`，**未连接任何 DB**，却设置 `is_real_db = true`，并声称"验证全链路：… → 真实 DB 执行 → …"。这是**幻影接线测试**——测试通过只证明代码路径可调，不证明真实 DB 基准链路可用。直接违反项目自身"禁止幻影交付/幻影测试"铁律（AGENTS.md 硬约束 11/12）。整改后：`packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:50-91` 的 `ControlledExecutor` 诚实标注 `is_real_db=false`，`:110-152` 的 `RealSqliteExecutor` 执行真实 SQLite 查询（见 fixes 文档）。
- **L1 `[事实]` `stability_matrix.rs:73-85`**：`StabilityMatrixRunner::run()` 对 `connection_exhaust/slow_query_storm` 硬编码 `availability=0.0/recovery=2.5`，其余故障 `50.0/5.0`，且 `degradation_triggered=true`、`degradation_correct=true` 恒真——**稳定性矩阵是模拟数据**，非真实混沌实验。`run_with_injector`（`:111-154`）才使用 `sz_orm_fusion::ChaosInjector`，但 `stability_matrix_test` 5 个测试均只验证默认 run 的 JSON 结构与硬编码值。

### 5.3 阈值合理性

- **L1 `[事实]` soak 退化阈值**（`tests/soak.rs:32-37` 与 `soak.yml:86-90`）：RSS 增长 >50MB、fd 增长 >10、pool_active!=pool_idle、ops/sec 衰减 >10%、p99 增长 >2x、error>0。**p99 增长 2x 作为"慢退化"阈值偏宽**（性能工程常用 20-30% 即告警），但 soak 是长时泄漏检测而非微退化门禁，可接受。
- **L1 `[事实]` `repeatability_guard.rs:66-68`**：QPS 偏差 ≤5% 判稳定。`compute_max_deviation_pct`（`:94-105`）用 `(max-min)/max`，对单次离群值敏感；`reproducibility.rs:67` 的真实测试阈值放宽到 30%（真实 DB 波动），二者口径不一致但各自合理。

---

## 6. 连接池/缓存/查询性能特性评估

### 6.1 feature 收益是否有对照组实验

- **L1 `[事实]` 有真实 A/B 断言的测试**：
  - `tests/performance_subfeatures.rs:156-158`（plan-cache）：`assert!(speedup > 1.0, "cached lookup should be faster than fresh parse")`——真实计时 + 弱阈值。
  - `tests/real_db_bench.rs`（--ignored，历史 v960 报告记载）含 `bench_arena_alloc_count_v960`（断言 arena < Vec×0.5）等带阈值的性能断言——但本会话未运行该套件全部（需真 DB）。
- **L1 `[事实]` 测了但不断言的测试**：
  - `tests/performance_validation.rs:30-47`（SIMD decode）：计算 `speedup` 并 `eprintln!`，但**只断言标量=SIMD 正确性**，无 `assert!(speedup > 1)`。SIMD 即使比标量慢也通过。
  - `tests/performance_subfeatures.rs:12-36`（L1 hit vs miss）、`:205-262`（零拷贝列式 vs 行式）：计时后仅打印，无断言。
  - `tests/perf_e2e.rs:16-77`：只断言指标计数器非空/正确性，不测性能。
- **L1 `[事实]` 收益被"造"出来的测试**：`tests/v65_prepared_cache_benefit.rs:64-65` 在 miss 路径后**人为注入 `tokio::time::sleep(Duration::from_micros(200))`** 模拟 prepare 成本，然后断言"耗时降幅 ≥ 40%"（`:106-109`）。收益是测试自己注入的，不是缓存行为的实测收益。注释（`:62-64`）坦承"用固定短延迟模拟其保守下限"。**这属于用脚本"修"出漂亮数字的典型反模式**。
- **L1 `[事实]` 有真实行为副作用的测试**（正面）：`tests/l2_cache_singleflight_test.rs` 验证 10 并发只回源 1 次（`:56-60`）、缓存命中不加载（`:218-223`）、不同键不互锁（`:258-262`）——是真实的行为断言。

### 6.2 结论

**L3 `[推断]`**：`perf-accel`/`simd`/`plan-cache`/`l1-cache` 等 feature 的"性能收益"缺乏**带阈值的对照组断言**作为回归门禁；仅有的真实断言阈值过弱（>1.0）或被测试注入。收益证据主要存在于 `--ignored` 的 real_db_bench 套件中（本机因 DB 凭据/客户端库无法全量复现）。

---

## 7. 性能剖析工具链评估

- **L1 `[事实]` `sz-orm-flamegraph` 库实现完整**：`collector.rs`（QueryTracer/PhaseRecorder 真实计时）、`render.rs`（SVG/folded）、`diff.rs`、`stats.rs`、`config.rs`，有单元测试。
- **L1 `[事实]` 未被查询路径调用**：在 `packages/sz-orm-core/src/*.rs`、`packages/sz-orm-sqlx/src/*.rs` 中搜索 `trace_execute`/`PhaseRecorder`/`QueryTracer`/`sz_orm_flamegraph`，**无匹配**。即 ORM 的真实查询执行不经过 flamegraph 计时。
- **L1 `[事实]` 实际调用点**：`cli/src/main.rs:2754-2772` `cmd_flamegraph`（`flamegraph` feature）用 `QueryTracer::trace_execute` 包裹两个**普通闭包**（`"SELECT * FROM users".to_string()` 与 `42usize`），生成 SVG/folded 并打印长度——是**演示命令**，不剖析真实查询。`sz-orm-observability/diagnosis/advisor/explain` 仅引用 `QueryPhaseTiming`/`Phase` 类型（多在 `#[cfg(test)]` 或序列化层）。
- **L1 `[事实]` `packages/sz-orm-bench/*.profraw`**：3 个文件（23KB 各），`.gitignore` 已含 `*.profraw`（已缓解）；来源为某次带 `-C instrument-coverage` 的构建（时间戳 2026-10-17），与 `performance_validation` 或 bench 运行相关。
- **L3 `[推断]`**：flamegraph 工具链处于"库可用、未接入生产查询路径"状态。若宣称"剖析工具链已集成"，属于幻影交付；当前文档需明确"提供组件（需手动接入）"。

---

## 8. 压力/混沌/soak 测试评估

### 8.1 可运行性（L2 全部通过）

| 套件 | 命令（L2） | 结果 |
|------|-----------|------|
| 压力 | `cargo test -p sz-orm-core --test stress` | **34 passed / 0 failed**（1.02s） |
| Soak 冒烟 | `cargo test -p sz-orm-core --test soak` | **23 passed / 0 failed / 1 ignored**（10.27s，`soak_smoke_10s` 真实跑 10s） |
| 混沌池 | `cargo test -p sz-orm-core --test chaos_pool` | **14 passed / 0 failed** |
| perf-e2e | `cargo test -p sz-orm-core --test perf_e2e --features perf-accel` | **4 passed / 0 failed / 1 ignored** |
| 性能验证 | `cargo test -p sz-orm-core --test performance_validation --features simd` | **6 passed / 0 failed**（44.13s，真实跑 200 次迭代计时） |
| 预编译缓存收益 | `cargo test -p sz-orm-core --test v65_prepared_cache_benefit --features prepared-stmt-cache -- --ignored` | **3 passed / 0 failed** |

- **L1 `[事实]` `stress.rs`** 覆盖连接池并发 acquire/release、max_size 强制、突发负载、长事务、混合负载、耗尽恢复、高频切换、close_all、稳态、并发 reap、工厂故障、随机持有时间——不变量断言真实（`active <= max_size`、无借出、超时区间 900ms–2s 等）。
- **L1 `[事实]` `soak.rs`** 用 Mock 连接工厂但真实监控 RSS/fd/p99/ops/error，检测 6 类退化并 fail-closed（`assert!(regressions.is_empty())`）。
- **L1 `[事实]` `chaos_pool.rs`** 覆盖饥荒攻击、事务泄漏、关闭后获取、健康检查等，断言真实。
- **L1 `[事实]` `jepsen.rs`**（未在本会话全量运行）是事务状态机/嵌套 savepoint/故障注入的正确性测试。

### 8.2 soak-kit 脚本

- **L1 `[事实]` `scripts/soak-kit/`** 面向 122 服务器部署（`deploy-122.sh`、`run-szorm-soak.sh`），本机无该服务器，**无法在本机执行**（如实记录：未执行，环境不可行）。
- **L1 `[事实]` `szrsql-flock-guard.patch`** 是外部项目 `szrsql-bin` 的补丁（`std::mem::forget(lock_file)` 防止 flock 句柄提前释放导致双实例），与 sz-orm 本体无关，是 soak 部署工具链的一部分。

### 8.3 flaky 评估

**L3 `[推断]`**：`stress_long_running_steady_state`（1s 稳态）与 `soak_pool_long_running_steady_state`（6h）时序敏感；AGENTS.md 已将后者列入高负载机器易误报清单。`performance_validation` 的 200 次迭代计时在 44s 内完成，未 flaky。总体稳定性好。

---

## 9. 可观测性评估

- **L1 `[事实]` `monitoring/alerts.yml` 引用的指标在源码中不存在**：在 `packages/**/src/*.rs`、`cli/src/*.rs` 全量搜索 `sz_orm_soak_pool_idle`、`sz_orm_soak_rss_bytes`、`sz_orm_soak_fd_count`、`sz_orm_latency_us_bucket`、`sz_orm_errors_total`、`sz_orm_ops_total`、`sz_orm_slo_short_burn_rate`——**全部无匹配**（findstr exit 1）。
- **L1 `[事实]` `grafana/sz-orm-dashboard.json`** 16 个 panel 引用上述不存在的指标；`templating` 引用 Prometheus datasource，但**仓库内无 Prometheus HTTP `/metrics` 端点实现**（搜索 axum/warp/hyper/TcpListener 无命中；`sz-orm-observability/src/prometheus_exporter.rs` 是库对象，可渲染文本但无服务端点）。
- **L1 `[事实]` `packages/sz-orm-observability/src/prometheus_exporter.rs:180-207` `register_sz_orm_metrics` 注册的指标名**（`sz_orm_pool_active`、`sz_orm_query_qps`、`sz_orm_query_latency_p50` 等）与 alerts/dashboard 的指标名**不一致**。
- **L1 `[事实]` `packages/sz-orm-observability/src/prometheus_exporter.rs:240-250` `CloudNativeObservabilityExporter::export` 返回硬编码 `export_success: true, latency_ms: 10.0`**，无真实导出——同类幻影模式。
- **L3 `[推断]`**：告警规则与 Grafana 面板是"纸面配置"——即使部署 Prometheus+Grafana，也采集不到这些指标。可观测性层未落地。

---

## 10. 性能文档与指标真实性评估

- **L2 `[实测]` `python scripts/check-metrics-real.py`**（门禁 18）：
  - 第一次运行：**exit 1**，实际 `测试标注=21171`，README/README.zh 共 7 处声称 `21161`（顶部、徽章、测试总数、总计行）→ **漂移被成功拦截**（证明检查器有效）。
  - 第二次运行（并行会话已更新 README 后）：**exit 0**，实际 `21182` 与 README 一致。
  - **L1 `[事实]` 局限性**：`check-metrics-real.py:97-118` 的 `CLAIM_PATTERNS` 只覆盖包数/测试数/feature 数/方言数，**不覆盖任何性能数字声称**（"8/8 PASS"、"性能提升 X%"、ops/sec、p99 等），也不检查文档中的版本号与 `Cargo.toml` 的漂移。因此性能报告中的数字（如"比 sqlx 快 33%~63%"）无任何自动校验。
- **L1 `[事实]` 版本漂移**：AGENTS.md 头部明确 `版本：10.13.0`；README.md:4 也写 `v10.13.0`；Cargo.toml workspace 版本 10.13.0。**当前无版本漂移**（prompt 提到的"README 声称 v10.12.0"在本仓库当前状态不成立——可能是历史状态）。
- **L1 `[事实]` 历史性能报告**（`docs/assessment/2026-09-23-v870-perf-benchmark-report.md`、`2026-09-24-v870-perf-benchmark-complete.md` 等）引用 `file:///E:/vue/...` 旧路径（prompt 已说明）；其"8/8 PASS"是 `cargo test -p sz-orm-bench --features real-bench --test real_db_bench -- --ignored` 的**集成测试通过数**，不是性能对比数字——口径需要区分。
- **L2 `[实测]` "8/8 PASS" 当前本机无法完整复现**：
  - SQLite：`real_db_bench` 非 ignored **5 passed / 0 failed**（真实运行）；
  - PostgreSQL：`postgres_backend_test -- --ignored` **2 passed / 0 failed**（真实连接 PG 18.4）；
  - MySQL：`test_run_workload_real_mysql` **FAILED**（`Access denied for user 'root'@'localhost'`，凭据不符）；
  - Oracle：3 个 Oracle 测试 **FAILED**（`DPI-1047: Cannot locate a 64-bit Oracle Client library`，本机缺 ODPI-C）。
  - 因此"8/8 PASS"的复现门槛较高（需 MySQL 凭据与 Oracle 客户端库），本机环境不满足。这属于**环境问题**而非测试自身缺陷（测试断言正确，失败在连接层），但说明 AGENTS.md 声称的"本机数据库就绪"与实际不符。

---

## 11. 历史发现复核结果表

| # | 历史发现 | 当前证据 | 状态 |
|---|---------|---------|------|
| 1 | `bench-comparison/` 三个 Node 脚本"修复"基准 | `update_report.js` 直接改写 BENCHMARK_REPORT.md（L1）；`fix_replace.cjs` 空壳；`fix_sqlite_url.cjs` 修复源码 | **仍存在（P0：报告被脚本改写）** |
| 2 | `bench-results/*.json` env_metadata 严重缺失 | 根因是 `EnvMetadata::default()` 硬编码 unknown/0（L1）；且 `1789544529` 是模拟模型输出（L2 数学验证） | **仍存在（P0：模拟数据当基准）** |
| 3 | v730 性能门禁幻影交付修复 | CI 集成组件存在且测试通过，但 `perf_bench_real_wiring` 的"真实执行器"是硬编码（L1）；CI benchmark job fail-open（L1） | **部分缓解→仍存在（幻影接线测试）** |
| 4 | AGENTS.md「四数据库基准 8/8 PASS」 | `real_db_bench` 套件真实存在；本机 SQLite 5/5、PG 2/2 通过，MySQL/Oracle 因环境失败（L2） | **部分缓解（测试真实，本机复现不完整）** |
| 5 | README/文档版本漂移 | 当前 AGENTS.md/README/Cargo.toml 均为 10.13.0；`check-metrics-real.py` 可拦截测试数漂移（L2 实测 exit 1 拦截成功），但不覆盖性能数字/版本 | **已缓解（当前无漂移）；检查器覆盖不足仍存在** |
| 6 | soak.yml 每周 6h、检测泄漏/退化 | soak.yml/soak-self-hosted.yml fail-closed（L1）；soak-smoke 每次 CI 运行（L1）；退化阈值合理但 p99 2x 偏宽 | **存在且真实生效（可执行性 L2 通过）** |
| 7 | perf-accel 等 feature 收益证据 | `performance_validation` 测而不断言；`v65` 注入成本造收益；`performance_subfeatures` 仅 plan-cache 有 >1.0 断言（L1） | **仍存在（缺真实对照断言）** |
| 8 | sz-orm-flamegraph 是否被真实调用 | 被 cli/observability/diagnosis/advisor/explain 引用，但**未被 core/sqlx 查询路径调用**；仅 CLI 演示（L1） | **仍存在（未接入真实查询路径）** |
| 9 | `packages/sz-orm-bench/*.profraw` 污染 | `.gitignore` 已含 `*.profraw`（L1） | **已缓解** |
| 10 | Criterion 基准是否接入 CI 回归门禁 | CI benchmark job `continue-on-error` + 吞错 + 无基线对比（L1）；`cargo bench` 本机无法运行（L2） | **仍存在（门禁 fail-open）** |

---

## 12. 改进建议（按 ROI 排序）

### ROI-1（P0）：删除模拟基准路径，封存模拟报告

- **问题**：`main.rs:18` 检测 `--simulate`，`main.rs:39-62` 无 `real-bench` feature 时默认跑模拟并写 `bench-results/`；`bench-results/1789544529_bench_report.json` 是模拟数据。
- **证据**：`lib.rs:713-768`（模拟函数与硬编码因子）、§3.2 L2。
- **改法**：① 删除 `run_workload`/`workload_latency_profile`/`framework_latency_factor`/`run_full_benchmark` 模拟路径（或加 `#[cfg(feature = "simulated-bench")]` 并默认关闭）；② `main.rs` 无 `real-bench` feature 时打印错误并以非零退出；③ 在 `bench-results/` 删除或重命名两个 JSON，并添加 `README` 说明哪些是真实测量；④ `EnvMetadata` 增加 `is_simulated` 字段（`BenchResult` 已有 `is_real_db`，报告级再加 `generation_mode`）。
- **验收**：`cargo run -p sz-orm-bench` 不产生任何 JSON；`bench-results/` 仅含 `is_real_db=true` 的报告；`grep framework_latency_factor` 无命中。

### ROI-2（P0）：CI 性能门禁 fail-closed + 修复依赖冲突

- **问题**：`ci.yml:700` `continue-on-error: true`；`:762` `|| echo WARNING` 吞错；`ci_integration.rs:177-186` 基线缺失放行；`cargo bench` 因 `libsqlite3-sys` links 冲突无法解析（L2）。
- **证据**：§3.4、§5.1。
- **改法**：① 去掉 `continue-on-error` 与 `|| echo WARNING`；② 在 benchmark job 中增加基线保存/对比步骤（`cargo test -p sz-orm-bench --features perf-bench-real --test perf_bench_real_wiring` + 真实 DB），退化 ≥10% 时 `exit 1`；③ `CiBenchIntegration` 基线缺失时改为 `BlockError`（fail-closed）；④ 统一 `sz-orm-core`/`sz-orm-studio` 的 rusqlite 版本（都升到 0.32 或都降 0.31），消除 links 冲突。
- **验收**：`cargo bench -p sz-orm-bench --features real-bench` 在 CI 成功执行并产出真实结果；人为制造退化（修改 baseline 或注入慢路径）时 PR 被阻断。

### ROI-3（P1）：消灭幻影性能测试

- **问题**：`perf_bench_real_wiring.rs` 原 `:55` 硬编码延迟冒充真实 DB（已整改移除，见 `docs/assessment/2026-10-27-perf-fixes.md` §3.1）；`stability_matrix.rs:73-85` 硬编码矩阵数据；`v65_prepared_cache_benefit.rs:64-65` 注入成本造收益；`performance_validation.rs:30-47` 测而不断言。
- **证据**：§5.2、§6.1。
- **改法**：① `RealSqliteExecutor` 改为真正 `sqlite::memory:` 连接并执行 `SzOrmWorkload`/`SqlxWorkload`（复用 `real_db.rs`）；② `StabilityMatrixRunner::run()` 删除，只保留 `run_with_injector`（真实 chaos），或明确标注 `is_simulated=true`；③ `v65` 移除注入 sleep，改用真实 prepare 路径测量并保留 40% 断言（若无法达标则说明 feature 无收益，应修 feature 而非修测试）；④ `performance_validation` 三个 SIMD 测试增加 `assert!(speedup > 1.0)`。
- **验收**：`cargo test -p sz-orm-bench --features perf-bench-real --test perf_bench_real_wiring` 在无 SQLite 时失败；`v65` 不注入 sleep 仍通过；`performance_validation` 在 SIMD 慢于标量时失败。

### ROI-4（P1）：可观测性指标落地

- **问题**：`monitoring/alerts.yml` 与 `grafana/sz-orm-dashboard.json` 引用不存在的指标；无 `/metrics` 端点。
- **证据**：§9。
- **改法**：① 在 `sz-orm-observability` 增加 axum/hyper `GET /metrics` 端点（feature gate），暴露 `register_sz_orm_metrics` 的指标；② 统一指标名（alerts/dashboard 与 exporter 使用同一命名空间，如 `sz_orm_soak_*` 由 soak 测试导出）；③ 删除 `CloudNativeObservabilityExporter` 的硬编码返回值。
- **验收**：启动示例服务后 `curl /metrics` 返回 `sz_orm_soak_pool_idle` 等指标；alerts.yml 表达式在 Prometheus 中可解析。

### ROI-5（P2）：报告生成器与工件清理

- **问题**：`full_comparison.rs:41-42` 空 records 生成空报告；`bench-results-fixed.txt` 空文件；`bench-mysql-results.txt` 截断；`fix_replace.cjs` 空壳。
- **证据**：§3.1、§4.4。
- **改法**：① `ReportGuard` 改为从 Criterion 全局收集器读取真实 records（或删除该 guard，改为 bench 函数显式写报告）；② 删除空文件/空壳脚本；③ 重跑并重新生成 `bench-*-results.txt`，标注 commit。
- **验收**：`cargo bench --bench full_comparison` 退出后 `benchmark-report.md` 含非空记录；`bench-comparison/` 无空文件。

### ROI-6（P2）：性能数字纳入 check-metrics-real

- **问题**：`check-metrics-real.py` 不覆盖性能数字声称与版本漂移。
- **证据**：§10。
- **改法**：在 `CLAIM_PATTERNS` 增加性能模式（如 `四数据库基准 \d+/\d+ PASS`、`比 sqlx 快 \d+%`、`提升 \d+%`），并在 `real_metrics()` 中提供对应的源码统计口径（如 real_db_bench 通过的测试数）；或对无法自动核对的性能数字要求标注采集命令与日期，脚本校验格式。
- **验收**：README 手写"性能提升 50%"被脚本拦截。

---

## 13. 证据索引与 audit-verify

### 13.1 关键 file:line 索引

| 证据 | 位置 |
|------|------|
| 模拟延迟模型 `run_workload` | `packages/sz-orm-bench/src/lib.rs:713-744` |
| 硬编码延迟 profile | `packages/sz-orm-bench/src/lib.rs:746-759` |
| 硬编码框架因子 | `packages/sz-orm-bench/src/lib.rs:761-768` |
| EnvMetadata 默认 unknown/0 | `packages/sz-orm-bench/src/lib.rs:483-491` |
| git commit 采集 | `packages/sz-orm-bench/src/lib.rs:494-511` |
| MemoryMetrics 分配计数恒 0 | `packages/sz-orm-bench/src/lib.rs:604-613` |
| main 默认模拟路径 | `packages/sz-orm-bench/src/main.rs:18-36, 39-62` |
| CI 集成基线缺失放行 | `packages/sz-orm-bench/src/ci_integration.rs:177-186` |
| 稳定性矩阵硬编码 | `packages/sz-orm-bench/src/stability_matrix.rs:73-85` |
| 幻影"真实执行器" | `packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:50-91`（原 `:55-69` 硬编码实现已整改移除，见 fixes） |
| 报告改写脚本 | `bench-comparison/update_report.js:5-24` |
| 空壳脚本 | `bench-comparison/fix_replace.cjs:1` |
| SQLite URL 修复 | `bench-comparison/fix_sqlite_url.cjs:9-22` |
| 空 records 报告生成器 | `bench-comparison/benches/full_comparison.rs:41-42, 85-107` |
| Criterion 严谨配置 | `bench-comparison/benches/full_comparison.rs`（configure_criterion） |
| 空结果文件 | `bench-comparison/bench-results-fixed.txt`（5 字节） |
| 截断 MySQL 结果 | `bench-comparison/bench-mysql-results.txt`（964 字节） |
| CI 性能 job fail-open | `.github/workflows/ci.yml:696-772`（`:700` continue-on-error、`:762` 吞错） |
| soak-smoke fail-closed | `.github/workflows/ci.yml:783-814` |
| 周更 6h soak | `.github/workflows/soak.yml:22-90` |
| 告警引用不存在指标 | `monitoring/alerts.yml:16-162` |
| Grafana 面板引用不存在指标 | `grafana/sz-orm-dashboard.json` |
| 指标注册（不同命名） | `packages/sz-orm-observability/src/prometheus_exporter.rs:180-207` |
| 硬编码导出成功 | `packages/sz-orm-observability/src/prometheus_exporter.rs:240-250` |
| SIMD 测而不断言 | `packages/sz-orm-core/tests/performance_validation.rs:30-47` |
| 注入成本造收益 | `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:64-65, 106-109` |
| plan-cache 真实弱断言 | `packages/sz-orm-core/tests/performance_subfeatures.rs:156-158` |
| SingleFlight 行为断言（正面） | `packages/sz-orm-core/tests/l2_cache_singleflight_test.rs:56-60, 218-223` |
| 可复现性真实测试 | `packages/sz-orm-bench/tests/reproducibility.rs:65-75` |
| 门禁 18 覆盖范围 | `scripts/check-metrics-real.py:97-118` |
| flamegraph 未被 core 调用 | 搜索 `packages/sz-orm-core/src`、`packages/sz-orm-sqlx/src` 无 `trace_execute/PhaseRecorder` 命中 |
| CLI flamegraph 演示 | `cli/src/main.rs:2754-2772` |
| profraw 已 gitignore | `.gitignore`（`*.profraw`） |
| README 版本/测试数 | `README.md:4`（v10.13.0 · 21182+ tests） |

### 13.2 L2 实测命令输出汇总

```
cargo test -p sz-orm-bench --test regression_baseline_test
  → test result: ok. 4 passed; 0 failed

cargo test -p sz-orm-bench --test stability_matrix_test --features stability-matrix
  → test result: ok. 5 passed; 0 failed

cargo test -p sz-orm-bench --test reproducibility --features real-bench
  → test result: ok. 3 passed; 0 failed（编译 8m21s，测试 2.30s）

cargo test -p sz-orm-bench --test perf_bench_real_wiring --features perf-bench-real
  → test result: ok. 3 passed; 0 failed

cargo test -p sz-orm-bench --features real-bench --test real_db_bench
  → test result: ok. 5 passed; 0 failed; 4 ignored

cargo test -p sz-orm-bench --features real-bench --test real_db_bench -- --ignored
  → test result: FAILED. 0 passed; 4 failed（MySQL Access denied + Oracle DPI-1047）

cargo test -p sz-orm-bench --features real-bench --test postgres_backend_test -- --ignored
  → test result: ok. 2 passed; 0 failed

cargo test -p sz-orm-core --test stress
  → test result: ok. 34 passed; 0 failed

cargo test -p sz-orm-core --test soak
  → test result: ok. 23 passed; 0 failed; 1 ignored（soak_smoke_10s 真实 10s）

cargo test -p sz-orm-core --test chaos_pool
  → test result: ok. 14 passed; 0 failed

cargo test -p sz-orm-core --test perf_e2e --features perf-accel
  → test result: ok. 4 passed; 0 failed; 1 ignored

cargo test -p sz-orm-core --test performance_validation --features simd
  → test result: ok. 6 passed; 0 failed（44.13s）

cargo test -p sz-orm-core --test v65_prepared_cache_benefit --features prepared-stmt-cache -- --ignored
  → test result: ok. 3 passed; 0 failed

python scripts/check-metrics-real.py
  → 第一次：❌ 存在不一致数字声称（21161 vs 21171），exit 1
  → 第二次（并行会话更新 README 后）：✅ 通过，exit 0

cargo bench -p sz-orm-bench -- --test
  → FAILED: libsqlite3-sys links 冲突（rusqlite 0.32 vs 0.31）
```

### 13.3 audit-verify 脚本输出

```
==========================================
  审计证据验证: docs\assessment\2026-10-27-perf-review.md
==========================================
✅ packages/sz-orm-bench/src/lib.rs:483 (共 1211 物理行)
✅ packages/sz-orm-bench/tests/reproducibility.rs:65 (共 181 物理行)
✅ packages/sz-orm-bench/src/ci_integration.rs:177 (共 452 物理行)
⚠️  src/main.rs:2754 — 未找到对应文件（跳过）
✅ packages/sz-orm-observability/src/prometheus_exporter.rs:180 (共 393 物理行)
✅ packages/sz-orm-observability/src/prometheus_exporter.rs:240 (共 393 物理行)
✅ packages/sz-orm-bench/src/lib.rs:713 (共 1211 物理行)
✅ packages/sz-orm-bench/src/lib.rs:746 (共 1211 物理行)
✅ packages/sz-orm-bench/src/lib.rs:761 (共 1211 物理行)
✅ packages/sz-orm-bench/src/lib.rs:483 (共 1211 物理行)
✅ packages/sz-orm-bench/src/lib.rs:494 (共 1211 物理行)
✅ packages/sz-orm-bench/src/lib.rs:604 (共 1211 物理行)
✅ packages/sz-orm-bench/src/main.rs:18 (共 262 物理行)
✅ packages/sz-orm-bench/src/ci_integration.rs:177 (共 452 物理行)
✅ packages/sz-orm-bench/src/stability_matrix.rs:73 (共 190 物理行)
✅ packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:50 (共 399 物理行；原 55-69 硬编码实现已整改移除，见 fixes 文档)
✅ packages/sz-orm-observability/src/prometheus_exporter.rs:180 (共 393 物理行)
✅ packages/sz-orm-observability/src/prometheus_exporter.rs:240 (共 393 物理行)
✅ packages/sz-orm-core/tests/performance_validation.rs:30 (共 156 物理行)
✅ packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:64 (共 174 物理行)
✅ packages/sz-orm-core/tests/performance_subfeatures.rs:156 (共 374 物理行)
✅ packages/sz-orm-core/tests/l2_cache_singleflight_test.rs:56 (共 296 物理行)
✅ packages/sz-orm-bench/tests/reproducibility.rs:65 (共 181 物理行)
⚠️  src/main.rs:2754 — 未找到对应文件（跳过）

==========================================
  验证结果
==========================================
  ✅ 通过: 22
  ❌ 失败: 0
  ⚠️  警告: 2
==========================================
✅ 审计证据验证通过
```

**警告说明**：2 条 `src/main.rs:2754` 警告来自报告对 `cli/src/main.rs:2754-2772` 的引用。`audit-verify.ps1` 的路径解析分支仅覆盖 `packages/` 前缀与裸 `src/`（映射到 `packages/sz-orm-core/src/`），不识别 `cli/` 前缀，因此将 `cli/src/main.rs:2754` 中的 `src/main.rs` 误匹配到不存在的 `packages/sz-orm-core/src/main.rs` 而"跳过"。实际文件 `cli/src/main.rs` 存在且 2754 行为 `cmd_flamegraph` 函数体（L1 已读取验证）。该警告是**脚本路径解析局限**，非虚假引用。

---

## 附：过程说明与局限

1. **只读约束**：评测未修改任何源码/文档/脚本。唯一副作用：执行 prompt 要求的 `cargo bench` 时触发了 `Cargo.lock` 重解析并使其不一致，已用 git 索引中的已提交 blob 恢复（L2 验证恢复后测试重新通过）。此冲突本身是仓库真实缺陷，已列入 ROI-2。
2. **并行会话**：本机存在另一评测会话并行运行 cargo 操作，曾导致构建锁争用与 README 数字更新；本报告所有 L2 输出均以本会话实际捕获为准。
3. **未执行项**（如实记录）：`soak-kit` 部署脚本（需 122 服务器）、6h soak（需 6 小时）、`real_db_bench` 的 MySQL/Oracle 部分（本机凭据/客户端库不满足）、`cargo bench`（依赖冲突失败）。`docs/assessment/2026-10-27-sdet-review.md` 在评测时尚未产出，无法复用其性能结论。
4. **网络说明**：cargo 使用 `tuna` 镜像源；`cargo bench` 失败时的错误信息包含索引更新行为，属正常网络解析。