# 2026-10-27/28 性能工程师评测整改记录（ROI-1 ~ ROI-6）

> 来源：`docs/assessment/2026-10-27-perf-review.md`（顶级性能测试工程师独立评测）
> 整改日期：2026-10-27 ~ 2026-10-28
> 涉及包：`sz-orm-bench` / `sz-orm-observability` / `sz-orm-core` / `bench-comparison` / `.github/workflows` / `scripts/check-metrics-real.py` / README
> 验证：本文件所有 `file:line` 引用均可通过 `scripts/audit-verify.ps1` 校验（见文末输出）。

---

## 一、ROI-1：删除模拟基准路径，封存模拟报告（P0）

### 问题
`sz-orm-bench` 存在一套**模拟延迟模型**：`packages/sz-orm-bench/src/lib.rs` 的
`run_workload()` 用硬编码 `workload_latency_profile`（base/variance）乘以
`framework_latency_factor`（SzOrm=1.0/SeaOrm=1.15/Diesel=1.08/Sqlx=1.20）生成"延迟"，
天然保证 sz-orm 最快。评测证明 `bench-results/1789544529_bench_report.json` 的
20 组 raw_latencies 与该公式 **20/20 完全吻合**（§3.2），即该报告是模拟数据冒充基准结果。
且 `main.rs` 默认（无 `real-bench` feature）就会运行模拟并写报告。

### 整改
- **删除模拟函数**：`packages/sz-orm-bench/src/lib.rs`
  - 删除 `SeededRng`、`run_workload`、`workload_latency_profile`、`framework_latency_factor`、`run_full_benchmark`（原 `packages/sz-orm-bench/src/lib.rs:570-794`）
  - 删除 `SimdComparisonResult::run` / `is_simd_available` / `simulate_decode_throughput`（原 `packages/sz-orm-bench/src/lib.rs:816-851`）
  - 删除依赖上述函数的单元测试（`test_run_workload_*`、`test_run_full_benchmark`、`test_workload_reproducibility`、`test_seeded_rng_*`、`test_simd_comparison_run`、`test_simd_comparison_threshold`）
  - 模块文档注明历史（`packages/sz-orm-bench/src/lib.rs:1-16`）
- **CLI fail-closed**：`packages/sz-orm-bench/src/main.rs`
  - 无 `real-bench` feature 时打印错误并 `exit(1)`
  - `--simulate` 参数直接拒绝（`exit(1)`）
  - `simd` 子命令改为调用真实 `SimdComparisonResult::run_real`（真实 SQLite 数据集）
  - 新增 `--save-baseline` / `--compare-baseline`（见 ROI-2）
- **封存模拟报告**：删除 `bench-results/1789544529_bench_report.json`；新增 `bench-results/README.md` 说明报告真实性约定（`is_real_db=true` + `git_commit` 非空 + 记录命令）

### 验证
```
cargo run -q -p sz-orm-bench -- --simulate        → exit 1（拒绝模拟）
cargo run -q -p sz-orm-bench                      → exit 1（无 real-bench）
cargo run -q -p sz-orm-bench --features real-bench -- --db-backend sqlite --dataset-size 200 ...
    → 真实测量输出 + 报告 JSON + 基线 JSON（exit 0）
```

---

## 二、ROI-2：性能回归门禁 fail-closed（P0）

### 问题
- `ci.yml` Job 6 `continue-on-error: true` + `cargo bench ... || echo WARNING`：命令失败被吞掉，job 恒绿
- `cargo bench -p sz-orm-bench` 对无 `[[bench]]` 目标的包为空转
- `ci_integration.rs` 基线缺失时 `Pass`（`BENCH_BASELINE_MISSING` 放行）——fail-open

### 整改
- **`.github/workflows/ci.yml` Job 6 重写**：
  - 移除 `continue-on-error` 与 `|| echo WARNING`
  - 步骤 1：`cargo run --release -p sz-orm-bench --features real-bench -- --db-backend sqlite ... --save-baseline ci-sqlite`（真实基准，失败即 job 失败）
  - 步骤 2：`cargo test -p sz-orm-bench --features perf-bench-real --test perf_bench_real_wiring`（回归决策门禁）
  - 产物上传 `bench-results/`（90 天）
- **`ci_integration.rs`**：基线缺失 `decision` 从 `Pass` 改为 `BlockError`（`BENCH_BASELINE_MISSING`），注释与测试同步更新
- **`main.rs` 基线 CLI**：`--save-baseline <name>` 写 `bench-results/baseline_<name>.json`；`--compare-baseline <name>` 加载并对比，退化 ≥10% 打印退化列表并 `exit(1)`
- **SQLite 默认连接**：`sqlite::memory:` 每连接独立导致 `DatasetInitializer` 与 Workload 两个池不可见，CLI 自动改用临时文件（`?mode=rwc`）

### 验证
```
cargo test -p sz-orm-bench --features perf-bench-real --test perf_bench_real_wiring → 8 passed
  其中 test_ci_bench_block_when_baseline_missing_fail_closed 验证基线缺失阻断
```

---

## 三、ROI-3：消灭幻影性能测试（P1）

### 3.1 perf_bench_real_wiring：真实 SQLite 执行器
原 `RealSqliteExecutor::execute` 直接返回硬编码 `vec![150,180,...]` 并标记 `is_real_db=true`
（幻影接线测试）。重构：
- `ControlledExecutor`（受控数据注入）：轮换返回预置延迟向量，`is_real_db=false`，仅用于验证 `CiBenchIntegration` 决策逻辑（Block/Pass/BlockUnstable/BaselineMissing）
- `RealSqliteExecutor`（真实执行）：连接 SQLite 临时文件 → `DatasetInitializer::init` 建表插数 → `RealDbExecutor::run` 执行真实查询负载 → `is_real_db=true`，断言非空正数延迟
- 文件：`packages/sz-orm-bench/tests/perf_bench_real_wiring.rs`

### 3.2 稳定性矩阵 is_simulated
`stability_matrix.rs` `run()` 的 availability/recovery 是固定参考值（非真实混沌测量）。
`MatrixEntry` 新增 `is_simulated` 字段：
- `run()` → 全部 `is_simulated: true`
- `run_with_injector()` → `is_simulated: !report.is_real_chaos`
- 测试 `test_stability_matrix_run_entries_marked_simulated` 断言全部标 true

### 3.3 v65 预编译缓存收益：移除注入 sleep
原测试在 miss 路径后注入 `tokio::time::sleep(200µs)` 人为制造收益再断言降幅 ≥40%。
现改为真实测量：
- miss 成本 = 缓存未命中查找 + **真实 SQLite 未命中查询全流程**（sqlx `AssertSqlSafe` 动态 SQL 查询）
- hit 成本 = 1000 次缓存命中均值
- 实测：miss 339.8µs vs hit 33µs，降幅 90.3% ≥ 40% ✅
- 文件：`packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs`

### 3.4 performance_validation：诚实断言
- `compare_in`：批量 HashSet O(n) vs 标量线性 O(n·m)，存在算法级差异 → 新增 `assert!(speedup > 1.0)`（实测 24.27x）
- `compare_eq` / `decode`：`simd.rs` 明确标注 batch 与标量算法等价（`batch_decode_integers` 恒走标量路径、`batch_compare_eq` 为展开实现），只断言正确性，不制造伪收益断言（评测中 debug 构建 compare_eq batch 慢 25%，断言会误报）
- 文件：`packages/sz-orm-core/tests/performance_validation.rs`

---

## 四、ROI-4：可观测性指标对齐落地（P1）

### 问题
`monitoring/alerts.yml` 与 `grafana/sz-orm-dashboard.json` 引用的 13 个 `sz_orm_*` 指标在源码中不存在；
评测报告"无 HTTP 端点"结论有误（`start_metrics_server` 早已存在），已在本文件更正。

### 整改
- **新增 `packages/sz-orm-observability/src/alert_metrics.rs`**：
  - `ALERT_METRIC_DEFS`：13 个指标（errors_total / ops_total / slo_* / soak_*）
  - `ALERT_LATENCY_HISTOGRAM = "sz_orm_latency_us"` + 16 个 bucket（渲染 `*_bucket{le=}` / `_sum` / `_count`，供 `histogram_quantile`）
  - `register_alert_metrics(registry) -> AlertMetricHandles`：注册全部指标并返回写入句柄（**需手动接入**：查询路径 `record_latency_us`/`inc_ops`/`inc_errors`、soak 运行方 `set_soak_snapshot`、SLO 监控 `set_slo`）
  - 契约测试 `metric_names_match_alerts_and_dashboard`：从 alerts.yml 与 dashboard 提取全部指标名，断言渲染输出覆盖——防止再次漂移
- **`prometheus_exporter.rs`**：`CloudNativeObservabilityExporter::export` 移除硬编码 `latency_ms: 10.0`（改为实测校验耗时），语义澄清为"类型校验通过"

### 验证
```
cargo test -p sz-orm-observability --lib → 47 passed（含 3 个 alert_metrics 测试）
```

---

## 五、ROI-5：基准报告生成器修复 + 工件清理（P2）

### 问题
- `full_comparison.rs` `ReportGuard::drop` 用空 `records` 生成空报告，且因 `Box::leak` 永不执行（双重幻影）
- `fix_replace.cjs` 仅 1 行注释（空壳）；`update_report.js` 直接改写 BENCHMARK_REPORT.md（报告篡改）
- `bench-results-fixed.txt` 空文件；`bench-mysql-results.txt` 截断

### 整改
- **`full_comparison.rs`**：删除 `ReportGuard`，改为自定义 `main()` 顺序运行 9 个 bench 后调用 `generate_report_from_criterion()`——解析 `target/criterion/<id>/new/{estimates,samples}.json` 生成真实数据报告（`records` 来自本次运行实际测量）；无数据时明确跳过并告警
- **删除**：`fix_replace.cjs`、`update_report.js`、`bench-results-fixed.txt`、`bench-mysql-results.txt`
- **保留**：`fix_sqlite_url.cjs`（源码修复，非结果篡改；报告中说明其用途）

### 验证
```
cd bench-comparison && cargo check --bench full_comparison → OK
```

---

## 六、ROI-6：性能声称纳入门禁 18（P2）

### 问题
`check-metrics-real.py` 只校验包/测试/feature/方言数字（并行会话已补版本一致性），性能数字声称（"8/8 PASS"、"提升 X%"）无任何校验。

### 整改
`scripts/check-metrics-real.py` 新增：
- `PERF_CLAIM_PATTERNS`：匹配 `N/N PASS`、`提升/快于/慢于/退化 X%`、`X% faster/slower/improvement/regression`、`X% 吞吐衰减/throughput decay`
- `PERF_CLAIM_EVIDENCE_RE`：声称行必须同行含 `cargo/复现/reproduce/docs|assessment/|SOAK_DURATION/实测/采集/日期/详见/see docs` 之一
- `scan_perf_claims()`：报告性检查（不参与 `--fix`——证据无法自动生成）
- 退出码合并 `not issues and not perf_issues`

### 自测
```
（注入）删除 README.zh.md:1075 行尾"（复现方式同上）" → 脚本报 "1075 性能声称缺少证据标记"
（恢复后）→ ✅ README 性能声称均附带证据标记
```

---

## 七、验证汇总

| 检查 | 结果 |
|------|------|
| `cargo test -p sz-orm-bench` | 18 passed |
| `cargo test -p sz-orm-bench --features perf-bench-real --test perf_bench_real_wiring` | 8 passed |
| `cargo test -p sz-orm-bench --features stability-matrix --test stability_matrix_test` | 6 passed |
| `cargo test -p sz-orm-bench --features real-bench --test regression_baseline_test --test reproducibility` | 7 passed |
| `cargo test -p sz-orm-observability --lib` | 47 passed |
| `cargo test -p sz-orm-core --test performance_validation --features simd` | 6 passed（compare_in 24.27x） |
| `cargo test -p sz-orm-core --test v65_prepared_cache_benefit --features prepared-stmt-cache -- --ignored` | 3 passed（降幅 90.3%） |
| `cargo clippy -p sz-orm-bench --all-targets`（默认/全 feature） | 通过（-D warnings） |
| `cargo clippy -p sz-orm-observability --all-targets` | 通过（-D warnings） |
| `cargo fmt -p sz-orm-bench -p sz-orm-observability -- --check` | 通过 |
| 门禁 G12/G14/G15/G16/G17/G18/G24 | 全部通过（G14 `check-doc-sync.py --diff HEAD` 需 git，本机 git 位于 `D:\Git`（未入 PATH），临时加入 PATH 后运行通过） |
| `python scripts/check-metrics-real.py` | 通过（含性能声称检查） |
| `cargo run -p sz-orm-bench --features real-bench` 端到端 | 报告+基线落盘（exit 0） |
| `cargo run -p sz-orm-bench -- --simulate` / 无 feature | exit 1 |

---

## 八、关键 file:line 证据索引

### ROI-1（模拟基准路径移除）
| 证据 | 位置 |
|------|------|
| 模块文档注明模拟历史 | `packages/sz-orm-bench/src/lib.rs:1-16` |
| `SimdComparisonResult` 仅保留 `meets_threshold`（run 已删） | `packages/sz-orm-bench/src/lib.rs:807-823` |
| `test_bench_report_with_results` 改用真实结果构造（不再调用 run_full_benchmark） | `packages/sz-orm-bench/src/lib.rs:901-916` |
| CLI 拒绝 `--simulate` / 无 real-bench 报错退出 | `packages/sz-orm-bench/src/main.rs:18-57` |
| 真实 SIMD 对比（run_real） | `packages/sz-orm-bench/src/main.rs:202-226` |
| 基线保存/对比 CLI（退化 ≥10% exit 1） | `packages/sz-orm-bench/src/main.rs:78-133` |
| 临时文件连接串替代 `:memory:` | `packages/sz-orm-bench/src/main.rs:63-74` |
| bench-results 真实性约定 | `bench-results/README.md:1-33` |

### ROI-2（CI 门禁 fail-closed）
| 证据 | 位置 |
|------|------|
| CI Job 6 重写：真实基准 + 回归门禁，无 continue-on-error | `.github/workflows/ci.yml:719-748` |
| 基线缺失阻断（BlockError + 测试） | `packages/sz-orm-bench/src/ci_integration.rs:181-190`、`packages/sz-orm-bench/src/ci_integration.rs:366-379` |
| 模块文档说明基线缺失 fail-closed | `packages/sz-orm-bench/src/ci_integration.rs:1-8` |

### ROI-3（幻影性能测试消除）
| 证据 | 位置 |
|------|------|
| `ControlledExecutor`（受控注入，is_real_db=false） | `packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:50-91` |
| `RealSqliteExecutor`（真实 SQLite 执行链路） | `packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:110-152` |
| 真实产物断言（is_real_db/非空延迟） | `packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:228-247` |
| 稳定性矩阵 `is_simulated` 字段与赋值 | `packages/sz-orm-bench/src/stability_matrix.rs:19-35`、`:85-101`、`:129-141` |
| 矩阵测试断言全部标 true | `packages/sz-orm-bench/tests/stability_matrix_test.rs:42-50` |
| v65 真实 SQLite 未命中查询测量（移除注入 sleep） | `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:63-83` |
| v65 收益断言（≥40%） | `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:113-117` |
| compare_in 加速比断言（>1.0） | `packages/sz-orm-core/tests/performance_validation.rs:75-109` |
| compare_eq/decode 只断言正确性（注释说明） | `packages/sz-orm-core/tests/performance_validation.rs:30-72` |

### ROI-4（可观测性指标对齐）
| 证据 | 位置 |
|------|------|
| 告警指标定义（13 个） | `packages/sz-orm-observability/src/alert_metrics.rs:58-118` |
| 延迟直方图注册 | `packages/sz-orm-observability/src/alert_metrics.rs:121-127` |
| 注册函数与写入句柄 | `packages/sz-orm-observability/src/alert_metrics.rs:150-197` |
| 契约测试（文件引用 vs 注册表） | `packages/sz-orm-observability/src/alert_metrics.rs:245-271` |
| 模块在 lib.rs 导出 | `packages/sz-orm-observability/src/lib.rs:56-66` |
| `export` 硬编码移除 | `packages/sz-orm-observability/src/prometheus_exporter.rs:235-253` |

### ROI-5（报告生成器修复）
| 证据 | 位置 |
|------|------|
| 从 Criterion 落盘数据生成报告 | `bench-comparison/benches/full_comparison.rs:51-119` |
| 自定义 main 顺序运行全部 bench 后生成报告 | `bench-comparison/benches/full_comparison.rs:405-431` |
| 报告生成函数（真实 records 入参） | `bench-comparison/benches/full_comparison.rs:120-146` |
| 被删除工件清单 | `bench-comparison/`（bench-results-fixed.txt / fix_replace.cjs / update_report.js / bench-mysql-results.txt 已移除） |

### ROI-6（性能声称纳入门禁 18）
| 证据 | 位置 |
|------|------|
| 性能声称模式与证据标记定义 | `scripts/check-metrics-real.py:146-177` |
| 扫描函数 | `scripts/check-metrics-real.py:179-188` |
| main 中报告性能声称问题并合并退出码 | `scripts/check-metrics-real.py:236-248`、`:282-287` |
| 版本一致性检查（并行会话已加） | `scripts/check-metrics-real.py:81-95`、`:141-145` |

### README / CHANGELOG
| 证据 | 位置 |
|------|------|
| README 头部版本与测试数（已同步） | `README.md:4`、`README.zh.md:4` |
| 性能整改条目 | `CHANGELOG.md:74-98` |

### audit-verify 输出

```
==========================================
  审计证据验证: docs/assessment/2026-10-27-perf-fixes.md
==========================================
  ✅ 通过: 29
  ❌ 失败: 0
  ⚠️  警告: 0
==========================================
✅ 审计证据验证通过
```

### 环境备注

- 本机 git 安装于 `D:\Git\cmd\git.exe`（git 2.56.0），未加入 PATH；调用需用完整路径或临时 `$env:PATH += ';D:\Git\cmd'`。
- 依赖 git 的门禁（`scripts/check-doc-sync.py --diff HEAD`、`check-upstream-unmodified.sh`）在临时加入 PATH 后正常运行通过。
- 本机 MySQL `root:test123` 凭据失效（`Access denied`）且缺少 Oracle ODPI-C 客户端库，故 `real_db_bench -- --ignored` 的 MySQL/Oracle 用例无法本机复现（环境问题，非测试缺陷）。