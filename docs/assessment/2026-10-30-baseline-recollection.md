# 2026-10-30 三条基线重采记录（覆盖率 / 变异 / 性能）

> 采集人：本会话（工作树 HEAD `02e2c79f`，含并行会话在途改动）
> 原则：所有数字来自本机真实命令输出，未做任何推算或补齐；无法采集的项显式登记为"未验证"。
> 证据分级：T = 真实命令输出，D = 文档/代码引用，W = 警告/未验证，C = 结论。

## 一、覆盖率（G22）

| 项 | 值 |
|----|----|
| 工具 | `cargo-llvm-cov 0.9.1` + `llvm-cov`（rustc 1.99.1 自带） |
| 采集命令（测试阶段） | `cargo llvm-cov --workspace --all-features --no-fail-fast --no-report` |
| 采集命令（报告阶段） | `llvm-cov export -format=lcov -instr-profile=D:/rust-target/llvm-cov-target/sz-orm.profdata @objs_all.txt` |
| 插桩二进制 | 779 个（`D:/rust-target/llvm-cov-target/debug/deps/*.exe`） |
| 仓库源码总行（含测试代码） | LF 405,273 / LH 337,616 → **83.31%** |
| 其中 `src/`（生产代码） | LF 311,563 / LH 265,452 → **85.20%**（946 文件） |
| 其中 `tests/` | LF 93,710 / LH 72,164 → 77.01%（683 文件） |
| 有插桩数据的包 | 72 / 72（无 0 行包） |
| 门槛（G22 = 60%） | 通过 |

**Windows 特有处置**：`cargo-llvm-cov report/--lcov` 会一次性把 779 个 `-object` 拼进命令行，超过 Windows 32767 字符的 `CreateProcess` 上限而失败（`could not execute process llvm-cov.exe export ...`）。改为把对象列表写入响应文件 `@objs_all.txt` 直接调用 `llvm-cov export`，输出同一份 lcov。

**与历史口径的差异（C）**：CHANGELOG 旧记录 `86.45%（69 包 / 40,276 行 / 34,819 命中）` 的分母只有本轮的 1/10，且不含 sz-orm-core——那是局部 lcov 合并口径，不是全工作空间。本轮已用全量结果取代（见 CHANGELOG 同条目）。

**本轮顺带修复的两处全特性测试缺陷（T）**：
- `packages/sz-orm-graph/src/connection.rs:558` [must-contain: ConnectionError]、`packages/sz-orm-graph/tests/in_memory_e2e.rs:45` [must-contain: DriverError] 与 `:61`：bolt/neo4j 连接失败断言只认 `DriverError`；启用 `neo4j-driver` 后走真实 TCP，端口拒绝返回 `ConnectionError` → 改为两者皆可。修复后 `cargo test -p sz-orm-graph --all-features --lib --test in_memory_e2e` = **200 + 17 passed**。
- `packages/sz-orm-core/tests/v10120_failover_integration.rs:301` [must-contain: half_open_probe]：半开探测用 1ms 间隔，"刚探测完不该再探测"的断言在插桩 + 并行下偶发失败（本轮实跑复现 1 次）→ 间隔放宽到 100ms，睡眠 150ms。修复后 71 passed。

## 二、变异测试（G20，增量）

| 项 | 值 |
|----|----|
| 命令 | `bash scripts/collect-mutation-diff.sh --diff /tmp/uncommitted_src.diff --package sz-orm-core --features tenant-quota-rls-enhanced,auto-prewarm` |
| 工具 | cargo-mutants 27.1.0，jobs=8 |
| caught / missed / timeout / unviable | 0 / 0 / 0 / 0 |
| 结论（C） | 本次 diff 在 sz-orm-core 的改动全部位于 `#[cfg(test)]` 模块与 `tests/`，没有可变异的生产代码行 → 脚本判定"无有效变异体 → 视为通过" |

**W（限制）**：该结果不能代表历史"杀率 100%"基线的回归情况——本轮没有产生生产代码改动，杀率无从重算；如需重算需对 `sz-orm-core/src/pool.rs` 等核心文件跑全量变异（333+ 变异体，超出单次会话预算）。此前记录的 Windows `cargo-mutants` diff 解析报错（`invalid escaped character`）本轮未复现。

## 三、性能基线（sqlite，pool-size 20）

| 项 | 值 |
|----|----|
| 命令 | `cargo run --release -p sz-orm-bench --features real-bench -- --db-backend sqlite --db-connection "sqlite://bench-local-20261030.sqlite?mode=rwc" --dataset-size 1000 --pool-size 20 --rounds 10 --save-baseline local-20261030-pool20` |
| 报告 | `bench-results/1793347278_bench_report.json` |
| 基线 | `bench-results/baseline_local-20261030-pool20.json` |
| 环境指纹 | `Intel64 Family 6 Model 79 / 40 cores / 66,994,108 KB / rustc 1.99.1 / git 02e2c79f` |

P50 延迟（μs，dataset 1000，rounds 10）：

| 工作负载 | sz-orm | sea-orm | sqlx |
|----------|--------|---------|------|
| single_row_query | 53.5 | 174.0 | 58.0 |
| batch_query | 677.0 | 377.0 | 385.5 |
| complex_join | 66.5 | 81.0 | 62.5 |
| transaction | 409.0 | 375.5 | 354.0 |
| pool_concurrency | 50.5 | 125.0 | 57.0 |
| simd_compare | 147.5 | 191.5 | 153.0 |

**C（判读）**：`pool-size 20`（> 10）路径正常产出数据，连接池并发（`pool_concurrency`）P50 50.5μs、吞吐 15,748 ops/s，是三者中最快的一档；`batch_query` P50 高于 sea-orm/sqlx，是本轮唯一明显落后项，需单独定位（登记为性能观察项，不作为门禁阻断）。diesel 未启用（`real-bench-diesel` 依赖不在本地缓存），6 个工作负载均为 diesel 失败并跳过。

**Drop 归还路径（T）**：性能基准里没有专门的 Drop 归还工作负载（`pool_concurrency` 只覆盖获取/释放并发）。真正覆盖归还语义的是真库测试 `pg_drop_uncommitted_tx_auto_rollback_on_release`（本轮实跑通过），以及单元测试层 `sz-orm-core` 的连接池归还用例。

## 四、真库测试（统一入口）

| 项 | 值 |
|----|----|
| 入口 | `bash scripts/run-real-db-tests.sh pg mysql`（凭据经 `SZ_ORM_PG_URL` / `SZ_ORM_MYSQL_URL` 环境变量注入，未写入仓库） |
| PG（18 PostgreSQL:5432 / sz_orm_test） | sqlx real_db_pool_tests **10 passed**、core prewarm_integration **1 passed**、core smart_eager_integration_pg **7 passed** |
| MySQL（宝塔 8.0.29:8802 / sz_orm_test） | sqlx real_db_pool_tests **12 passed**、core prewarm_integration **1 passed**、core smart_eager_integration_mysql **7 passed** |
| 汇总 | 运行 6 组 / 失败 0 / 跳过 0（退出码 0） |

**W（未验证）**：e2e 套件（`SZ_ORM_E2E_*`，依赖第二台服务器的 MySQL/PG/Oracle）、Oracle 26ai、MSSQL、ClickHouse 本轮未执行；`scripts/run-real-db-tests.sh e2e oracle` 在当前环境会显式 SKIP 并以退出码 2 结束（"什么都没跑"不算通过）。

## 五、门禁与自测（本轮实跑）

| 检查 | 结果 |
|------|------|
| Python 门禁：G12 文档一致性 / G14 文档同步 / G15 幻影交付 / G16 语义反模式 / G17 架构 / G18 度量真实性 / G19 发布一致性 / G23 未用依赖 / G24 示例清单 / G25 文档代码可编译 / G26 CI-deny 同步 / G27 门禁自检 / G28 安全文档版本 / G29 幻影测试引用 / G30 幻影测试 / G31 CI 阻断语义 / G32 文件名编码 / G33 文档版本 / check-secrets / check-unsafe-audit / check-sql-injection-auto / G35 文档链接 / G36 CHANGELOG 版本线 | 全部 rc=0 |
| 门禁自测 `pytest tests/` | **159 passed** |
| `cargo check`（改动 crate，all-features/all-targets） | sz-orm-graph、sz-orm-bench rc=0；sz-orm-python 默认与 `extension-module` 两种形态 rc=0 |
| `cargo fmt --check`（改动 crate） | 通过 |

*生成时间：2026-10-30。本文件所有 `file:line` 引用可按 `bash scripts/audit-verify.sh` 复核。*
