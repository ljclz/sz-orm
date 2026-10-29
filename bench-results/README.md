# bench-results 目录说明

本目录存放 sz-orm-bench 真实 DB 基准的 JSON 报告与回归基线。**仅允许真实测量结果入库**。

## 报告真实性约定（2026-10-27 性能评测 ROI-1 整改）

- v10.13.0 起，`sz-orm-bench` 已**移除模拟延迟模型**（`run_workload` / `workload_latency_profile` /
  `framework_latency_factor` / `run_full_benchmark` / `SeededRng` / `SimdComparisonResult::run`）。
  任何基准测量必须通过 `real_db::run_workload_real`（`--features real-bench`）连接真实 DB 执行
  （ADR-003：禁止回退模拟延迟）。无 `real-bench` feature 时二进制直接报错退出。
- 提交到本目录的 `*_bench_report.json` 必须满足：
  1. `results[].is_real_db == true`；
  2. `env_metadata.git_commit` 非空（可复现锚点）；
  3. 附带产生该报告的命令（记录在提交说明或交付文档中）。
- 回归基线文件名固定为 `baseline_<name>.json`，由 `--save-baseline <name>` 生成、
  `--compare-baseline <name>` 消费（退化 ≥10% 退出码 1，fail-closed）。

## 现存文件

| 文件 | 性质 | 说明 |
|------|------|------|
| `1789562783_bench_report.json` | 真实 DB 测量 | v7.2.0 交付记录引用的真实基准结果（见 `docs/assessment/2026-09-16-v7.2.0-delivery-record.md`） |
| `1789544529_bench_report.json` | **已删除（模拟数据）** | 曾为模拟延迟模型产物：其 `raw_latencies` 与已删除的 `workload_latency_profile` + `framework_latency_factor` 公式 20/20 完全吻合，且 `git_commit` 为空、`is_real_db` 缺失。2026-10-27 性能评测（`docs/assessment/2026-10-27-perf-review.md` §3.2）判定为非真实测量后移除 |

## 环境元数据完整性要求

`env_metadata` 中 `cpu_model` / `memory_total_kb` / `rust_version` / `feature_flags` 由
`sz-orm-bench` 生成器填充；如出现 `unknown` / `0` / 空值，说明生成器未采集到构建环境，
该报告不可复现，不应作为性能结论依据。
