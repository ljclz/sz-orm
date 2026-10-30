# sz-orm v10.13.0 性能独立评测（v2）

> 评测日期：2026-10-28
> 评测对象：`D:\www\test\鲜视达\rust\sz-orm`，HEAD `13a22924`，`workspace.package.version = "10.13.0"`（`Cargo.toml:6`）
> 评测角色：独立性能测试工程师（不采信任何既有性能结论，全部复测）
> 证据分层：**L1** `[事实]` 代码/文档 `file:line`；**L2** `[实测]` 本会话命令输出（含退出码）；**L3** `[推断]` 专业判断，已显式标注
> 红线遵守：仓库只读（外部试验 crate 位于 `D:\temp\perf-eval-v2\`）；未运行 `bench-comparison/` 下任何 Node 脚本；未修改仓库基线；未重启/调参数据库；仅操作 `sz_orm_test`；密码以 `<local-password>` 占位。

---

## 1. 执行摘要

### 1.1 健康度总评：**45 / 100（🟡 偏低，不具备「性能可信」发布条件）**

| # | 维度 | 权重 | 得分 | 一句话依据 |
|---|------|------|------|------------|
| 1 | 基准可信度与可复现性 | 20% | 32 | `env_metadata` 仍是 `unknown/0/""/[]`；HEAD 仍含模拟报告与报告改写脚本 |
| 2 | 实验方法与统计严谨性 | 15% | 42 | 同代码三次复跑 p95 波动 85.8%~540.8%，10% 阈值淹没在噪声里 |
| 3 | 性能回归门禁 | 15% | 30 | CLI 层 fail-closed 成立，CI 层从未做过一次基线比对；HEAD 仍 `continue-on-error: true` |
| 4 | 核心链路性能实测 | 20% | 72 | 显式归还下与 sqlx 打平（p50 +4.1%），低并发默认路径慢 3.4× |
| 5 | 负载/压力/soak/容量 | 15% | 58 | 900s 满负载 8,955 万次操作 0 查询错误，但 49 次 acquire 超时 |
| 6 | 剖析与资源效率 | 5% | 45 | 有可行动热点分解，但产品内 flamegraph 无采样器/无 bin |
| 7 | 跨库/跨语言公平性 | 5% | 20 | 四库 8/8 无法复现；MySQL 鉴权失败、Oracle 工具不支持、PG 仅 sqlx 有结果 |
| 8 | 可观测性 | 5% | 25 | 13 个指标名注册齐全，但无生产调用点、无 exporter 服务，告警将「无数据」 |

### 1.2 三个最严重问题

1. **回归门禁在 CI 层从未真正成立（P0，ROI-2 未闭环）**
   `[事实]` HEAD 的 benchmark job 是 `continue-on-error: true` + `|| echo "WARNING: benchmark had errors (non-blocking)"`（`git show HEAD:.github/workflows/ci.yml` 第 696 行起）；工作区新版本改为 `continue-on-error: ${{ github.ref != 'refs/heads/main' }}`（`.github/workflows/ci.yml:765`），但 `Run real-DB benchmark` 只做 `--save-baseline ci-sqlite`（`.github/workflows/ci.yml:783`），**从未执行 `--compare-baseline`**；`Regression decision gate` 只跑接线单测（`.github/workflows/ci.yml:791`）。即使整改被提交，也只能拦住「基准崩溃」，拦不住任何性能退化。
   `[实测]` 门禁 B5 判定：**漏放**（见 §4）。

2. **性能元数据与统计口径不足以支撑任何结论（P0，v1 根因仍在）**
   `[事实]` `EnvMetadata::default()` 仍硬编码 `cpu_model: "unknown"`、`memory_total_kb: 0`、`rust_version: ""`、`feature_flags: vec![]`（`packages/sz-orm-bench/src/lib.rs:486`，字段 489-494）；`EnvMetadata::new` 未实现。
   `[实测]` 2026-10-28 19:48 生成的 PG 报告：`{"cpu_model":"unknown","cpu_cores":40,"memory_total_kb":0,"os_version":"windows","rust_version":"","feature_flags":[],"git_commit":"13a22924..."}`。
   `[实测]` 同一二进制、同参数、同机器三次运行（rounds=2000）：sz-orm 单行查询 p95 = 77 / 84 / 143 µs（+85.8%），batch_query p95 = 607 / 595 / 3890 µs（+540.8%）；即便 sqlx 自身 p95 也在 204 / 96 / 148 µs 间摆动（+112.5%）。

3. **默认连接池存在「双层上限 + 异步归还」结构性缺陷（P0，新增发现）**
   `[事实]` 外层 `Pool` 的 `max_size` 由用户配置，内层 sqlx 池硬编码 `.max_connections(10)`（`packages/sz-orm-sqlx/src/any.rs:170`；MySQL `:875`、PG `:1748` 同）→ 实际可用连接数 = `min(max_size, 10)`，但外层按 `max_size` 记账并放行等待者。
   `[事实]` `impl Drop for PooledConnection` 会把归还动作 `spawn` 到运行时（`packages/sz-orm-core/src/pool.rs:440`）。
   `[实测]` 串行 2000 次 acquire/release：`max=16` 走 Drop 归还 **在第 213/429 次 acquire 超时失败**（`status(idle=10,active=10,max=16)`），显式 `pool.release()` 2000 次仅 1.95 ms；`max=4` 时 Drop 归还 17.2 ms vs 显式 1.95 ms（8.8×）。
   `[实测]` 任何 `pool_size > 10` 的配置在首轮即产生 **恰好 `pool_size − 10` 次 acquire 错误**（p16 → 6 次；p64 → 54 次），15 分钟 soak 也复现 49 次 acquire 超时 + 6 次操作超时。

### 1.3 三个最高 ROI 改进

| ROI | 改进 | 预期收益 | 成本 |
|-----|------|---------|------|
| 1 | **CI 改为「先比对、后更新」**：`--compare-baseline ci-sqlite` + 下载上一轮基线产物 + 缺 workload 判失败 + 移除 `continue-on-error` | 把 0 检出的回归门禁变成真门禁；同时消灭 17%~22% 的误报 | 1 人日 |
| 2 | **元数据与统计口径硬性化**：`EnvMetadata` 采集真实 CPU/内存/rustc/feature/DB 版本；rounds ≥ 100；3 轮取中位；门限按实测 CV 自适应（当前 SQLite 单行 p95 CV ≈ 26%，10% 阈值必然误报） | 让「换机器能否复现」变成可回答的问题 | 2 人日 |
| 3 | **连接池容量一致性**：内层 `max_connections` 与外层 `max_size` 对齐（或初始化时显式钳制并暴露真实容量）；`Drop` 归还改为同步/内联归还；`PoolStatus` 记账自洽 | 消除 `max_size>10` 时的确定性超时；低并发 p50 从 93µs 降到 55µs（-41%） | 2 人日 |

### 1.4 上线前必须补的 5 个性能验收项

| # | 验收项 | 通过判据 |
|---|--------|----------|
| A1 | 回归门禁真阻断 | main 上引入 15% 退化必红；缺 1 个 workload 必红；PR 也执行只读比对（不阻断但留痕） |
| A2 | 元数据与统计基线 | 报告 `env_metadata` 六字段无 `unknown/0/""`；同一构建 3 轮 p95 极差 ≤ 15%，否则门限按 CV 自适应 |
| A3 | 池容量一致 | 任意 `max_size`（1~256）下 acquire 错误 = 0；`idle + active ≤ max` 恒成立；串行 10k 次 acquire/release 无超时 |
| A4 | 声称可对照 | 每条性能声称附「对照组 + 命令 + 5 次重复 + 效应量」；`perf-accel` 等默认开启的 feature 必须在热路径可见 |
| A5 | Soak 可观测 | Linux 侧 RSS/fd 真实采样非 0；6h 一次跑通并归档；RSS 斜率与延迟漂移进入判定 |

---

## 2. 环境与可复现性卡（实验 A）

| 字段 | 值 | 采集方式 |
|------|-----|---------|
| CPU | 2 × Intel Xeon E5-2640 v4，20 核 / 40 线程（报告里 `cpu_cores=40` 为逻辑核） | `wmic cpu` / 报告字段 |
| 主频策略 | Windows 高性能电源计划，未锁频（未见降频事件） | 电源计划面板 |
| 内存 | 63.9 GB | `wmic ComputerSystem` |
| 磁盘 | Samsung SSD 870 EVO（SATA） | `wmic diskdrive` |
| OS | Windows 10 Pro 19045（x64） | `ver` |
| rustc / cargo | 1.99.1（edition 2021，声明 MSRV 1.81） | `rustc -V` / `cargo -V` |
| profile | `--release`（`opt-level=3`，`debug=false`） | 自建 harness `[profile.release]` |
| 仓库 | HEAD `13a22924`，但**工作区脏**：295 个变更文件，其中 42 个源文件 `+2698/−704` | `git status --short` / `git diff --stat` |
| 生效 feature | 默认集：`simd/plan-cache/l1-cache/zero-copy/auto-prewarm/prepared-stmt-cache/perf-accel…`（[sz-orm-core/Cargo.toml:14](file:///D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L14)） | 依赖解析 |
| DB 版本 | SQLite（sqlx 0.9 内置，文件库 WAL）；PostgreSQL 18（本机 5432）；MySQL 9.6（3306，鉴权失败）；Oracle 23ai Free（1521，bench 工具不支持） | 连接测试 |
| 测试 crates | 外部 `D:\temp\perf-eval-v2\harness`（path 依赖 core/sqlx/cabi + sqlx 0.9） | `Cargo.toml` |
| 构建缓存 | `CARGO_HOME=/d/temp/cargo-home2`；`CARGO_TARGET_DIR=/d/temp/pe-target`（bench）/`pe-target-harness`（harness） | 环境变量 |
| 隔离措施 | 全部压测在 `sz_orm_test` 与 `%TEMP%` 文件库；bench 工具 CWD 固定到 `D:\temp\perf-eval-v2\bench-results`，不写仓库 | 命令 |

**可复现性告警（必须写在报告里）**：

- `[实测]` 评测期间仓库工作区被**并发修改**：`scripts/check-metrics-real.py` 08:5x 首跑因 README 测试数 21183 vs 21193 不一致而失败（退出码 1），20:0x 复跑已变为通过（退出码 0）——说明另一会话在同期编辑 README。
- `[实测]` 同一时段另有一组 `sz-orm-bench` 运行在 `D:\temp\perf-eval-v2\` 落地（`1793188214/1793188228_bench_report.json`、`g.sqlite`，时间 19:50），与本评测无关，构成 CPU/磁盘噪声源。
- `[推断]` 上述两点使「同机三次复跑」的方差被高估；但即使扣除噪声，SQLite 单行 p95 的 CV 仍 ≈ 26%（77/84/143 µs 的样本标准差 30.5 µs），10% 阈值依旧不可用。

复现命令（关键几条）：

```bash
# 门禁 B1/B2
cargo run -q --release -p sz-orm-bench -- --rounds 5                      # 期望 exit 1
cargo run -q --release -p sz-orm-bench --features real-bench -- --simulate --rounds 5  # 期望 exit 1
# 门禁 B3/B4
cargo run -q --release -p sz-orm-bench --features real-bench -- --rounds 5 --compare-baseline nonexistent_baseline
cargo run -q --release -p sz-orm-bench --features real-bench -- --rounds 2000 --save-baseline sq_r1
# 核心链路 / Soak / ABI（外部 harness）
perf-harness-v2.exe sqlite szorm_rel 16 16 8 10000 rel_c16_p16
perf-harness-v2.exe sqlite szorm_rel 64 16 900 10000 soak900
```

---

## 3. 历史整改复核表

### 3.1 ROI-1 ~ ROI-6

| ROI | 声明 | 本轮证据 | 裁定 |
|-----|------|---------|------|
| ROI-1 删除模拟基准路径 | `--simulate` 不可达、模拟报告封存 | `[实测]` `--simulate` → exit 1，提示「已于 v10.13.0 移除」；不带 `real-bench` → exit 1。`[事实]` 但 **HEAD 仍提交着** `bench-results/1789544529_bench_report.json`（v7.1.0 旧格式，无 `is_real_db`、`git_commit` 为空），工作区已删除但未提交 | **部分闭环** |
| ROI-2 CI 门禁 fail-closed | 移除 `continue-on-error`、加真实基准与接线测试 | `[事实]` HEAD job 仍是 `continue-on-error: true` + `\|\| echo "WARNING..."`；工作区新 job 只 `--save-baseline` 不 `--compare-baseline`；接线测试是决策逻辑单测 | **未闭环** |
| ROI-3 消灭幻影性能测试 | 诚实命名执行器、去 sleep、断言真实 | `[事实]` `ControlledExecutor`（`is_real_db=false`）与 `RealSqliteExecutor` 拆分清晰（`packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:9`）；v65 去 sleep 属实。`[实测]` 但 `performance_validation` 的 compare_eq 标量基准测得 **3 ns/50k 元素**（死代码消除），decode 两条相同路径测出 2.35× 差异 | **部分闭环** |
| ROI-4 可观测性指标对齐 | 13 指标注册且与告警/面板一致 | `[事实]` `register_alert_metrics` 存在（`packages/sz-orm-observability/src/alert_metrics.rs:204`），经 `lib.rs:62` 导出；`[实测]` 全仓除测试外无调用点，`PrometheusExporter::export()` 只返回字符串，无 HTTP 端点 | **部分闭环**（名对齐、接线未落） |
| ROI-5 报告生成器修复 + 工件清理 | 删除 `update_report.js`、清空模拟报告 | `[事实]` **HEAD 仍存在** `bench-comparison/update_report.js`（`git ls-tree HEAD` 可见），工作区才删除；`bench-results/` 仍含 7.1.0/7.2.0 两份旧报告 | **未闭环（交付态）** |
| ROI-6 性能声称纳入 G18 | `check-metrics-real.py` 拦截无证据性能数字 | `[实测]` G18 退出码 0（UTF-8 控制台）/ 1（GBK 控制台直接 `UnicodeEncodeError` 崩溃）；规则只是「同一行含 `cargo`/`docs/`/日期等 token」（`scripts/check-metrics-real.py:151`、`:257`），无法识别 `89.3%` 与 mock soak | **部分闭环** |

### 3.2 协议 §5 的 12 项

| # | 验证点 | 证据 | 裁定 |
|---|--------|------|------|
| 1 | `--simulate` 彻底不可达 / 残留 `framework_latency_factor` | 见 ROI-1；工作区已无模拟模型代码 | **部分闭环**（工作区闭环，交付态未） |
| 2 | `update_report.js` 是否删除 | HEAD 存在，工作区已删（未提交） | **未闭环（交付态）** |
| 3 | CI 是否真阻断 / 工件是否含完整元数据 | 见 ROI-2；工件元数据缺 CPU/内存/rustc/feature（§5.2） | **未闭环** |
| 4 | 幻影测试 / `is_simulated` / v65 去 sleep / 断言真实性 | 命名已诚实；`is_real_db` 字段存在；compare_eq 基准被 DCE（3 ns） | **部分闭环** |
| 5 | 13 指标注册与告警一致 / 是否仍「手动接入」 | 名称一致；仍手动接入 | **部分闭环** |
| 6 | 「prepared cache 降幅 89.3%」 | 5 次复跑：89.2 / 89.3 / 90.1 / 90.6 / 93.0%，均值 90.44%（95% CI ±1.92）；但度量是「6 次真实 prepare 的最大值 vs 1000 次查表」，且默认池未接线 | **算法层可复现，端到端不可外推** |
| 7 | 「四库 8/8 PASS」 | MySQL `1045 Access denied`；Oracle：bench 工具明确不支持；diesel 未启用；PG 工具产物只有 sqlx 6 条 | **无法验证** |
| 8 | `env_metadata` 是否仍 `unknown/0/""` | 今日新报告仍全缺（§5.2） | **未闭环（P0）** |
| 9 | 基准公平性 | sz-orm 走 `Drop` 归还、sqlx 走签名期归还；`BENCHMARK_REPORT.md` 用 in-memory `cache=shared`，CLI 用文件库（`bench-comparison/BENCHMARK_REPORT.md:5`） | **未闭环** |
| 10 | soak.yml 阈值是否 fail-closed / 防 flaky | `[事实]` 测试失败 → 步骤失败 → `Notify on failure` 里 `exit 1`（`.github/workflows/soak.yml:68`、`:90`），阻断成立；`[实测]` 但 Windows 下 soak 报告 `rss=0MB fd=0`，内存/句柄泄漏规则永不触发；`PoolLeak` 规则「`active != idle`」在 `idle=8,active=8` 时靠巧合通过 | **部分闭环** |
| 11 | 工件卫生 | `*.profraw` 已被 `.gitignore:60` 忽略（仍有 2 个文件在盘）；仓库根有 139 字节的杂散未跟踪文件 `0`；`bench-results/` 仅 16K 但含旧版本报告 | **部分闭环** |
| 12 | G18 能否拦住 README 性能声称 | 见 ROI-6：能拦「无任何证据 token」的行，拦不住「有 token 但不可复现」的行 | **部分闭环** |

---

## 4. 门禁 fail-closed 验证矩阵（实验 B）

| # | 场景 | 命令 | 退出码 | 关键输出 | 判定 |
|---|------|------|--------|----------|------|
| B1 | 无 `real-bench` feature | `cargo run -q --release -p sz-orm-bench -- --rounds 5` | **1** | 「sz-orm-bench 需要 --features real-bench 才能运行真实 DB 基准」 | ✅ fail-closed |
| B2 | `--simulate` | `--features real-bench -- --simulate --rounds 5` | **1** | 「--simulate（模拟延迟模型）已于 v10.13.0 移除」 | ✅ fail-closed |
| B3 | 基线缺失 | `--compare-baseline nonexistent_baseline` | **1** | 「fail-closed 阻断（BENCH_BASELINE_MISSING）」（`packages/sz-orm-bench/src/main.rs:118`） | ✅ fail-closed |
| B4 | 人为劣化基线（p95×0.7） | `--rounds 2000 --compare-baseline degraded` | **1** | 12 个 workload 命中，退化 16.6%~416.1% | ✅ fail-closed |
| B5 | CI 语义 | 读 HEAD + 工作区 `ci.yml` | 不适用 | 见下 | ❌ **漏放** |

### 4.1 B4 的详细输出（`--compare-baseline degraded`，18 个 workload）

```
❌ 检测到性能退化（≥10%），阻断合入：
  - SingleRowQuery: 基线 P95=53.9µs → 当前 84.0µs（退化 55.8%）
  - BatchQuery: 基线 P95=424.9µs → 当前 595.0µs（退化 40.0%）
  - ComplexJoin: 基线 P95=49.7µs → 当前 109.0µs（退化 119.3%）
  - Transaction: 基线 P95=386.4µs → 当前 662.0µs（退化 71.3%）
  - SimdCompare: 基线 P95=182.7µs → 当前 270.0µs（退化 47.8%）
  …（共 12 项，最大 416.1%）
```

### 4.2 B5：CI 为什么仍然拦不住（逐条）

| 漏洞 | 证据 | 后果 |
|------|------|------|
| HEAD job 永不失败 | `git show HEAD:.github/workflows/ci.yml`（benchmark job 第 696 行起）：`continue-on-error: true` + `cargo bench … \|\| echo "WARNING…"` | 合入 main 也不阻断 |
| 工作区 job 只存不比 | `.github/workflows/ci.yml:783`（`--save-baseline ci-sqlite`）、`:791`（只跑 `perf_bench_real_wiring`） | 每轮覆盖基线，永远没有「基线与当前」的对比 |
| PR 仍不阻断 | `.github/workflows/ci.yml:765`：`continue-on-error: ${{ github.ref != 'refs/heads/main' }}` | 退化只有在合并后才发现 |
| 工件不回流 | 只有 `upload-artifact`（`:796`），无 `download-artifact` | 上轮基线从不被消费 |
| 上游 job 可跳过 | `needs: [test]`（`:762`） | test 失败/取消时 benchmark 不运行，且不算失败 |
| 缺 workload 静默跳过 | `RegressionBaseline::compare` 只遍历基线条目，当前结果缺失即跳过（`packages/sz-orm-bench/src/regression_baseline.rs:92`）；单测明确断言缺 workload 时比较数为 0（`:200`） | 跑 1 个 workload 也能「通过」 |

### 4.3 阈值可用性：同代码复跑的误报率

`[实测]` 三次完全相同的构建与参数（rounds=2000，pool=10，dataset=10000），互相比对：

| 对比 | ≥10% 退化数 | 明细（p95 µs） |
|------|------------|----------------|
| r1 → r2 | **4 / 18 (22%)** | sz-orm complex_join 71→109 (+53.5%)；transaction 552→662 (+19.9%)；sqlx pool_concurrency 113→142 (+25.7%)；simd_compare 193→697 (+261.3%) |
| r2 → r3 | **4 / 18 (22%)** | sz-orm single_row 84→143 (+70.4%)；batch 595→3890 (+553.8%)；complex_join 109→123 (+12.8%)；sqlx single_row 96→148 (+54.2%) |
| r1 → r3 | **3 / 18 (17%)** | sz-orm single_row 77→143 (+85.8%)；batch 607→3890 (+540.8%)；complex_join 71→123 (+73.2%) |

`[推断]` 单次运行要 18 个 workload **全部**低于 10% 才算通过，按上述单项误报率 17%~22% 估算，整体通过概率约 `(1-0.2)^18 ≈ 1.8%`；一旦门禁真接上，几乎每轮都会红。这就是「门禁不敢接」的根因，也是必须先解决统计口径的原因。

---

## 5. 基准可信度评估

### 5.1 三类工件的可信度

| 来源 | 内容 | 问题 | 可信度 |
|------|------|------|--------|
| `bench-results/1789562783_bench_report.json`（已提交） | v7.2.0，15 个 workload，`is_real_db=true` | 版本落后 3 个 minor；`env_metadata` 全缺；`alloc_count=0`（内存指标未采集） | 低 |
| `bench-results/1789544529_bench_report.json`（HEAD 存在） | v7.1.0 旧格式，无 `is_real_db`，`git_commit=""` | 模拟时代遗留，与 `bench-results/README.md`「仅允许真实测量结果入库」冲突 | 不可用 |
| `bench-comparison/BENCHMARK_REPORT.md`（2026-07-23） | Criterion，sample-size=10 | 自述「sample-size=10 统计置信度低」（`:125`）；SQLite 用 `cache=shared` 内存库（`:5`），与 CLI 文件库不可比；宣称「Drop 修复后可自动归还」（`:37`）与实测的 8.8× 开销/超时矛盾 | 低 |

### 5.2 元数据缺口（P0 未闭环）

`[实测]` 本轮全部新报告的 `env_metadata`：

```json
{"cpu_model":"unknown","cpu_cores":40,"memory_total_kb":0,"os_version":"windows",
 "rust_version":"","feature_flags":[],"git_commit":"13a22924a77a2bec758d598d0f39e389dedbd67e"}
```

- `rust_version` 为空的原因：`env!("CARGO_PKG_RUST_VERSION")`（`packages/sz-orm-bench/src/lib.rs:493`）读的是**本包**的 `rust-version`，而版本声明在 `[workspace.package]`（`Cargo.toml:6` 起），成员包未继承 → 取到空串。
- `git_commit` 只记录 **HEAD**，而构建来源是脏工作区（42 个源文件 +2698/−704）→ 复现锚点失真。
- 报告完全没有 DB 版本字段：`RegressionBaseline` 里的 `db_version` 只是 `"sqlite"` 这样的后端名。
- `alloc_count`/`alloc_bytes` 恒为 0 → 内存分配维度无数据。

### 5.3 报告生成器与脚本

`[事实]` 工作区删除了 `bench-comparison/update_report.js` 与 `fix_replace.cjs`，但 **HEAD 仍保留**；`fix_sqlite_url.cjs` 仍在（仅改连接串，不改数字）。
`[实测]` 当前 `sz-orm-bench` 的产物路径为 50 行内直接 `write_to_file`（`packages/sz-orm-bench/src/lib.rs:567`），无后处理；即「生成器不再改写数字」在工作区成立。

### 5.4 跨机复现性

`[推断]` 不可复现。原因按影响排序：① 元数据缺失，无法确认基线机器；② 未固定 CPU 亲和性/频率；③ 单次采样、p95 取 10 个样本（CI 用 `--rounds 10`，p95 ≈ 最大值）；④ sz-orm 的归还策略依赖调用方写法（`drop` vs `release`），同一 SQL 可差 3.4×。任何一条都足以让另一台机器上的数字对不上。

---

## 6. 实验设计与统计

### 6.1 假设与对照

| 假设 | 对照组 | 样本量 | 重复 |
|------|--------|--------|------|
| H1 sz-orm 与裸 sqlx 单行查询开销差 < 10% | 同 SQL/同 schema/同数据量/同池大小/显式归还 | 各 2000 次（hotspots） | 1 组（+梯度 8s×多档） |
| H2 归还策略（Drop vs 显式）影响吞吐 | 同二进制内两种实现 | 每轮 6s，60 万+ 操作 | **5 次** |
| H3 prepared cache 降幅 ≈ 89.3% | 命中 vs 未命中（真实 sqlite prepare） | 1000 命中 + 6 未命中 | **5 次** |
| H4 SIMD 批量路径有收益 | 标量路径同数据 | 100k 元素 × 20 迭代 | **5 次** |
| H5 门禁阈值 10% 可区分退化与噪声 | 同代码三次复跑 | 18 workload × 3 | 3 次 |
| H6 长稳无泄漏 | 15 分钟满负载 | 8,955 万次操作 | 1 次（+仓库 soak 5 次） |

### 6.2 统计结果（含置信区间与效应量）

| 组 | 指标 | 结果 | 95% CI / 效应量 |
|----|------|------|-----------------|
| 归还策略 | 有效 QPS（ops/6s） | Drop 106.8k；显式 102.9k | Drop [106.3, 107.3]；显式 [100.6, 105.1]；差值 +3.8%，Cohen's d = 2.99（效应显著但量级小） |
| 归还策略 | 串行单次成本 | Drop 8.6 µs/op；显式 0.98 µs/op | 8.8×（n=2000，无重叠） |
| prepared cache | 耗时降幅 | 89.2 / 89.3 / 90.1 / 90.6 / 93.0% | 均值 90.44% ± 1.92 |
| SIMD | decode 批量/标量 | 0.922（同代码，测量偏差） | 5 次中位 |
| SIMD | compare_eq 批量/标量 | 1.384（批量更慢） | 5 次中位 |
| SIMD | compare_in 批量/标量（set=64） | 0.96×（批量更慢） | 5 次中位 |
| SIMD | compare_in 批量/标量（set=500，仓库测试） | 6.89×（算法差异，单次） | 仓库 `--release` 实跑 |

### 6.3 公平性检查表

| 检查项 | `sz-orm-bench` CLI | 本评测 harness | 判定 |
|--------|--------------------|----------------|------|
| 同 SQL / 同 schema / 同数据量 | ✅（10000 行，同 DDL） | ✅（10000 行） | 公平 |
| 同池大小 | ✅（pool=10 所有框架） | ✅（p16 对齐） | 公平 |
| 预热 | 3 轮（`warmup_rounds=3`） | 2 s | 基本公平 |
| 连接归还策略 | ❌ sz-orm 依赖 `Drop`，sqlx 签名期归还 | ✅ 两实现都显式 | **不公平（CLI）** |
| 内存 vs 文件库 | 文件库 | 文件库 WAL | 公平 |
| `BENCHMARK_REPORT.md` | in-memory `cache=shared` | 不适用 | **与 CLI 不可比** |
| 预编译语句 | 两者均走 sqlx 语句缓存；sz-orm 的 `prepared-stmt-cache` 未接线 | 同 | 公平（但 feature 无效） |
| 计时源 | `Instant`（单调时钟） | `Instant` | 公平 |
| 采样 | 2000 轮；CI 只用 10 轮 | 全量延迟保留（n=2k~82 万） | **CI 不足** |

---

## 7. 核心链路实测

### 7.1 单次查询成本分解（SQLite 文件库，c1，2000 样本）

| 步骤 | sz-orm p50 / p95 / p99 (µs) | 裸 sqlx p50 / p95 / p99 (µs) |
|------|------------------------------|------------------------------|
| 仅 acquire | **0.9 / 1.0 / 1.0** | 10.0 / 19.9 / 26.0 |
| acquire + `SELECT 1` | 28.7 / 36.0 / 46.4 | — |
| acquire + 0 行查询（`id=-1`） | 39.2 / 47.5 / 62.6 | 43.6 / 60.8 / 70.8 |
| acquire + 1 行查询（`id∈[0,10000)`） | **45.4 / 54.1 / 70.1** | **43.6 / 65.6 / 79.7** |

`[实测]` 结论：显式归还下，单行查询 ORM 开销 = **+1.8 µs（+4.1%）p50**，p95/p99 反而优于裸 sqlx（-17.8% / -12.0%）。分项：SQLite 引擎+驱动 28.7 µs（63%）；语句执行+索引定位 +10.5 µs（23%）；行解码 +6.2 µs（14%）；池获取 0.9 µs（2%）。

### 7.2 吞吐与尾延迟（SQLite，8 s 窗口，归一化 QPS = ops/8s）

| 实现 | 并发/池 | QPS | p50 | p95 | p99 | acquire 错误 | 超时 |
|------|---------|-----|-----|-----|-----|--------------|------|
| 显式归还 | 1 / 1 | 16,603 | 55 | 80 | 103 | 0 | 0 |
| 显式归还 | 4 / 4 | 59,153 | 64 | 93 | 110 | 0 | 0 |
| 显式归还 | 16 / 16 | **101,801** | 99 | 131 | 150 | 6 | 0 |
| 显式归还 | 64 / 64 | 101,019 | 99 | 133 | 153 | 54 | 0 |
| Drop 归还 | 1 / 1 | 4,861 | 93 | 278 | 1,610 | 0 | 0 |
| Drop 归还 | 4 / 4 | 39,612 | 95 | 145 | 193 | 0 | 0 |
| Drop 归还 | 16 / 16 | 29,585 | 120 | 780 | 4,715 | 6 | 0 |
| Drop 归还 | 64 / 64 | 82,770 | 111 | 189 | 266 | 54 | 0 |
| 裸 sqlx | 1 / 1 | 14,022 | 58 | 124 | 158 | 0 | 0 |
| 裸 sqlx | 16 / 16 | 78,332 | 198 | 272 | 342 | 0 | 0 |
| 裸 sqlx | 64 / 16 | 82,560 | 758 | 931 | 1,104 | 0 | 0 |
| 裸 sqlx | 64 / 64 | 63,920 | 975 | 1,203 | 1,359 | 0 | 0 |

关键读数：`c1/p1` 时 Drop 归还比显式归还慢 **3.4×**（4.9k vs 16.6k QPS），与池内差异 8.8 µs 及一次任务调度吻合；`c16` 之后管线掩盖了归还开销，Drop 反而略快（见 §10 组 3）；裸 sqlx 在 `p64` 时因每连接一个后台任务而吞吐回落到 63.9k。

### 7.3 事务 / 批量 / 流式 / 缓存

| 项目 | 结果 | 说明 |
|------|------|------|
| transaction（工具 2000 轮） | p50 393/504/391 µs（三次） | 三方差距在噪声内 |
| batch_query | p50 456/457/815 µs | p95 607→3890 µs，方差 540% |
| prepared cache 单测 | 命中 33.4~35.2 µs vs 未命中 316~482 µs | 算法层成立；默认池未接线（`packages/sz-orm-core/src/connection_ext.rs:41` 仅测试调用） |
| L1 / plan cache | **未执行** | 默认 `Pool`/`any.rs` 查询路径零引用，无运行时开关，无法构造对照（见 §10） |
| 流式背压（async-row-stream） | **未执行** | 与本轮容量问题无关，且 8 s 窗口无法稳定测流式背压；列为未执行项 |

---

## 8. 并发梯度与容量包线

### 8.1 SQLite 曲线（40 逻辑核，文件库 WAL，1 行主键查询）

```
QPS(k)  ┌──────────────────────────────────────────────
102     │            ●──●──●──●   sz-orm(显式归还) 平台 ~101k
 83     │        ●        ○──○      sqlx 平台 ~82k
 59     │    ●              
 48     │  ●    ○
 16     │●
 5      │▲  sz-orm(Drop) c1 仅 4.9k
        └───┬────┬────┬────┬────┬─── 并发 1/4/16/64/256
```

| 指标 | 结论 |
|------|------|
| 饱和点 | 并发 ≥16、池 ≥16 即饱和（~101k QPS）；并发 256 不再提升 |
| Little's Law 自洽 | c16/p16：QPS 101.8k × p50 99 µs ≈ **10.1 个在飞请求**（均值 97 µs → 9.9），与 16 个 worker 下约 10 个连接实际忙碌一致 ✅ |
| 过载行为 | c256/p16：QPS 98.9k 但 p95 14.1 ms / p99 26.8 ms——只有排队，没有背压/拒绝；唯一兜底是 10 s acquire 超时 |
| 容量包线（建议） | 单机 1 行主键查询：**稳态 80k QPS、峰值 100k QPS**；池 16、并发 16~32；池 >10 必须先把内层上限修掉 |

### 8.2 PostgreSQL 18（localhost，dataset 5000，pool 16）

| 实现 | 并发 | QPS | p50 | p95 | p99 |
|------|------|-----|-----|-----|-----|
| sz-orm（Drop） | 1 | 6,034 | 158 | 206 | 269 |
| sz-orm（Drop） | 16 | 16,428 | 347 | 1,179 | 6,288 |
| sz-orm（Drop） | 64 | **29,858** | 1,663 | 3,820 | 4,733 |
| sz-orm（Drop） | 256 | 14,436 | 14,180 | 47,255 | 79,997 |
| 裸 sqlx | 1 | 2,334 | 409 | 544 | 720 |
| 裸 sqlx | 16 | 10,320 | 1,064 | 3,217 | 8,835 |
| 裸 sqlx | 64 | 10,709 | 4,863 | 12,314 | 19,615 |
| 裸 sqlx | 256 | 7,536 | 28,892 | 66,523 | 109,277 |

`[实测]` PG 上 sz-orm 在 c1/c64 分别快 2.6×/2.8×。`[推断]` 差异更像「sqlx 侧每请求重新准备语句」而非 ORM 优化，**需单独立项复核**，不应写进对外营销结论。

### 8.3 容量包线小结

- SQLite 单机：**80k QPS 稳态 / 100k 峰值**（40 逻辑核，8~10 核忙）。
- PG 单机：**~30k QPS 峰值**（localhost 回环，无网络开销；真实部署需按 RTT 折算）。
- 每核有效吞吐：SQLite ≈ 10~12k QPS/核；PG ≈ 3k QPS/核。
- 结论：**当前瓶颈不在 ORM 层**，而在 SQLite 驱动/引擎与连接池配置；修池后可释放 3.4× 的默认路径损失。

---

## 9. Soak 与 flaky 分析

### 9.1 15 分钟满负载 soak（`soak900`：SQLite，c64/p16，显式归还，900 s）

```
RESULT,soak900,Sqlite,SzOrmRel,64,16,89552491,99327,108,3412,6600,636,157471,49,0,6,...
        ↑ 总操作 8,955 万   ↑ QPS   ↑p50 ↑p95  ↑p99  ↑mean ↑max  ↑acquire_err ↑query_err ↑timeout
```

| 采样点 | RSS | 句柄 | active / idle |
|--------|-----|------|---------------|
| t=60 s | 117,320 KB | 233 | 16 / 1 |
| t=300 s | 492,764 KB | 233 | 16 / 0 |
| t=600 s | 964,784 KB | 233 | 16 / 0 |
| t=780 s | 1,257,256 KB | 233 | 16 / 0 |
| 结束 | 1,420,772 KB | 233 | — |

| 趋势 | 数据 | 判定 |
|------|------|------|
| 吞吐 | 每 60 s 桶 5.92M→6.12M ops，QPS 98.3k→101.9k（+3.7%） | 无衰减 ✅ |
| p50 | 111 → 105 µs | 稳定 ✅ |
| p99 | 5,772 → 6,924 µs（+20%） | 有上行趋势，未超 2× 门限 ⚠️ |
| 句柄 | 233 → 233 | 无泄漏 ✅ |
| RSS | 19.8 MB → 1.42 GB，线性 +1.6 MB/s | **测量器开销，非库泄漏**：run_phase 对每次成功操作保留 16B 延迟（`lats` + `buckets` 双份），8,955 万 × 16B = 1.43 GB，与实测完全吻合 |
| 错误 | acquire 49 次 / 操作超时 6 次 / 查询错误 0 | 内层上限 10 的确定性后果 ⚠️ |

`[推断]` 该 soak 证明「稳态吞吐与查询正确性」没问题，但**不能**证明库本身无 RSS 泄漏（测量器掩盖），也暴露了高并发下 acquire 超时。

### 9.2 flaky 复现率（5 次重复）

| 套件 | 5 次结果 | flaky 率 |
|------|----------|----------|
| `stress`（14 个用例） | 5/5 全绿 | 0 / 70 |
| `chaos_pool`（34 个用例） | 5/5 全绿 | 0 / 170 |
| `soak`（60 s） | 5/5 通过，总操作 2201~2284 万，错误 0，p99 53~100 µs | 0 / 5 |
| `v65_prepared_cache_benefit`（ignored） | 5/5 通过 | 0 / 5（但见 §10 方法问题） |

`[事实]` 仓库 soak 在 Windows 上 `rss=0MB fd=0`（`packages/sz-orm-core/tests/common/soak.rs` 的采样器在该平台取不到值），因此 6 条退化规则里内存/句柄两条**永不触发**；`PoolLeak` 规则（`active != idle`）在终态 `idle=8,active=8` 时通过属于巧合。

---

## 10. Feature A/B 对照

### 10.1 结构性问题：默认 feature 无法 A/B

`[事实]` `sz-orm-core` 默认开启 39 个 feature，含 `simd/plan-cache/l1-cache/prepared-stmt-cache/perf-accel`（[sz-orm-core/Cargo.toml:14](file:///D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L14)）；`perf-accel = ["simd","zero-copy","auto-prewarm","plan-cache","zero-copy-deep"]`（[sz-orm-core/Cargo.toml:225](file:///D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L225)）；而 `sz-orm-sqlx` 通过 **workspace 依赖**引入 core（[sz-orm-sqlx/Cargo.toml:14](file:///D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/Cargo.toml#L14)），feature 并集恒为「开」。
`[事实]` 更关键的是：默认 `Pool` 与 `any.rs` 的查询路径**没有任何** `simd::` / `plan_cache` / `l1_cache` / `prepared` 引用（`packages/sz-orm-core/src/pool.rs` 仅在注释里出现 "prepared statement"，`packages/sz-orm-sqlx/src/any.rs:374` 起为 sqlx 自带语句绑定）。
`[推断]` 因此 `perf-accel` 对被测热路径的**结构效应为 0**，A/B 不是「测不出差异」，而是「不存在差异」；任何「默认开启性能加速」的表述都应改为「提供组件（需手动接入）」。

### 10.2 组 1：prepared-stmt-cache（5 次重复，仓库内测试）

| 轮次 | 未命中（6 次真实 prepare 最大值） | 命中均值（1000 次） | 降幅 |
|------|----------------------------------|---------------------|------|
| 1 | 357.1 µs | 35.2 µs | 90.1% |
| 2 | 354.1 µs | 33.4 µs | 90.6% |
| 3 | 316.0 µs | 34.1 µs | 89.2% |
| 4 | 317.2 µs | 33.8 µs | 89.3% |
| 5 | 482.5 µs | 34.0 µs | 93.0% |
| **统计** | — | — | **90.44% ± 1.92（95% CI）** |

判定：声称 89.3% **可复现**，但不是「端到端查询提速 89.3%」，而是「查表 vs prepare」；默认池未接线，端到端收益 = 0。测试本身用「6 次最大值」做分母（`packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:1` 门控），偏差方向对被测项有利。

### 10.3 组 2：simd（5 次重复，外部 harness，100k 元素）

| 子项 | 标量 | 批量 | 比值 | 结论 |
|------|------|------|------|------|
| decode_integers | 77,630 ns | 71,560 ns | 0.922 | 两条路径**是同一函数**（`packages/sz-orm-core/src/simd.rs:113` 直接委托标量），差异纯属测量偏差 |
| compare_eq | 79,085 ns | 109,440 ns | 1.384 | 手工 8 展开**更慢** |
| compare_in（set=64） | 1,667,433 ns | 1,733,333 ns | 0.96 | 每次调用重建 HashSet（`simd.rs:180`）导致更慢 |

`[实测]` 仓库自带测试反而给出 compare_in 6.89×（set=500）——这是**算法差异**（哈希 vs 500×50k 次比较），不是 SIMD；同一次运行里 decode 的 batch/scalar 相差 2.35×，且 compare_eq 标量基准只有 3 ns/50k（编译器把结果消掉了），说明测试方法无法支撑任何「加速」结论。

### 10.4 组 3：连接归还策略（同二进制，5 次重复，c16/p16，6 s）

| 轮次 | Drop 归还 QPS | 显式归还 QPS |
|------|---------------|--------------|
| 1 | 107,248 | 102,068 |
| 2 | 106,231 | 101,101 |
| 3 | 106,732 | 102,181 |
| 4 | 106,958 | 103,035 |
| 5 | 106,862 | 105,908 |
| 均值 | **106,806**（sd 0.37k, CV 0.34%） | **102,858**（sd 1.83k, CV 1.78%） |

`[实测]` Drop 归还快 3.8%（d=2.99，CI 不重叠）——高并发下把归还挪出热路径是有利的；但代价是：串行场景 8.8× 的归还开销、低并发 3.4× 吞吐损失（§7.2）、`PoolStatus` 记账漂移（`idle+active > max`）和 `max_size>10` 时的确定性 acquire 超时。

---

## 11. 剖析与资源热点

### 11.1 工具可用性（限制如实记录）

| 工具 | 状态 | 说明 |
|------|------|------|
| `sz-orm-flamegraph` | ❌ 不可用 | 只有 `lib`（`packages/sz-orm-flamegraph/src/collector.rs:39` 基于 `Instant` 的分阶段计时），**无采样器、无 bin**，不能产出火焰图 |
| `cargo flamegraph` / `perf` | ❌ 不可用 | Windows 无 perf；未安装 WPR/ETW 采集链 |
| 手工分解（本报告采用） | ✅ | 在外部 harness 内按「acquire / SELECT 1 / 0 行 / 1 行」分层计时，n=2000 |

### 11.2 Top 热点（SQLite，单行查询 p50 45.4 µs）

| # | 热点 | 成本 | 占比 | 可行动项 |
|---|------|------|------|----------|
| 1 | SQLite 语句编译+执行（`SELECT 1` 基线） | 28.7 µs | 63% | 复用 prepared statement（缓存已实现但未接线）；WAL 下可评估 `synchronous` 等级 |
| 2 | 表查询额外开销（定位+取行） | +10.5 µs | 23% | 确认 `id` 主键索引命中；避免 `SELECT *` 与列类型重复解析 |
| 3 | 行解码 + 参数绑定 | +6.2 µs | 14% | `Value` 枚举匹配与 `Row` 组装可做列类型直读；`zero-copy` 未接入此路径 |
| 4 | 连接归还（默认 Drop 路径） | +8.6 µs/op（串行） | 相当于 +19% | 见 §14 P0-3 |
| 5 | C ABI 边界（跨语言） | +25.6 µs/op（+56%） | — | 见 §12.3 |

---

## 12. 跨库/跨语言公平性与开销

### 12.1 四库复核结论

| 库 | 工具支持 | 本轮结果 | 裁定 |
|----|----------|---------|------|
| SQLite | ✅ sz-orm/sea-orm/sqlx | 三框架均产出真实数据（§7） | 可复现 |
| PostgreSQL | 仅 sqlx | sz-orm/sea-orm 报错；工具产物 6 条全是 sqlx；我用外部 harness 补测 sz-orm（§8.2） | 部分 |
| MySQL | 仅 sqlx | `1045 Access denied for user 'root'@'localhost'`（文档密码 `<local-password>` 不匹配当前实例） | **无法验证** |
| Oracle | 无 | 工具明确「sz-orm 后端暂不支持 MySQL/PostgreSQL/Oracle 基准」；sea-orm 无 oracle 驱动；diesel 未启用 | **无法验证** |

`[事实]` 因此 `AGENTS.md` 的「v8.7.0 四库 8/8 PASS」在当前版本无法通过 bench CLI 复现；该数字来自测试套件（不在本轮范围），README 若用它支撑「四库性能领先」属于**证据错配**。

### 12.2 对比公平性

- `[事实]` `bench-comparison/BENCHMARK_REPORT.md:37` 宣称「PooledConnection Drop 修复，drop 时自动归还」，与本轮实测（8.8× 开销、记账漂移、超时）冲突，该结论应撤回或限定条件。
- `[事实]` 该报告是 SQLite **内存库 `cache=shared`**（`:5`）+ sample-size=10（`:125`），与 CLI 的文件库、2000 轮不可比。
- `[事实]` 报告称 sz-orm `select_by_id` 25.48 µs vs sqlx 17.05 µs（`:56`，慢 49%）。`[实测]` 我在同机同 SQL 下测得 sz-orm 45.4 µs vs sqlx 43.6 µs（+4.1%，含真实文件 IO）。`[推断]` 旧结论的 49% 差距主要来自旧版本与内存库配置，不能代表 v10.13.0。

### 12.3 C ABI / 跨语言开销（37 个导出）

| 调用 | p50 | p95 | p99 | 说明 |
|------|-----|-----|-----|------|
| `sz_orm_version` | 0.10 µs | 0.10 | 0.10 | 常量返回 |
| `sz_orm_ping` | 35.6 µs | 51.2 | 68.2 | `block_on` + 池获取 |
| `sz_orm_query`（1 行，JSON 115 B） | **71.0 µs** | 98.0 | 137.0 | 进程内同 SQL 45.4 µs → **+56%** |

`[事实]` 每次调用含 `CStr::to_str().to_string()`、`serde_json::to_string`、`CString::into_raw`、`Box` 分配（`packages/sz-orm-cabi/src/lib.rs:378` 起）。
`[实测]` `dotnet` 不存在（`which dotnet` 退出码 2）→ C# P/Invoke 未执行，列入 §15 未执行项。
`[推断]` 对 115 B 结果集而言，JSON 序列化+字符串拷贝是主要边界成本；高频小查询场景建议提供二进制列式返回或句柄复用。

---

## 13. 可观测性评估

| 项 | 状态 | 证据 |
|----|------|------|
| 指标名与告警/面板一致 | ✅ | 13 个名称在 `packages/sz-orm-observability/src/alert_metrics.rs:204` 注册，测试断言与 `alerts.yml`/dashboard 对齐 |
| 生产调用点 | ❌ | 全仓仅测试调用；`lib.rs:62` 只是再导出 |
| Exporter 服务 | ❌ | `PrometheusExporter::export()` 返回 `String`，无 HTTP/TCP 监听；`register_sz_orm_metrics(exporter)` 需手动调用 |
| 池指标 | ⚠️ | `PoolStatus`（`packages/sz-orm-core/src/pool.rs:717`）与 `PoolMetrics`（`:749`）字段齐全，但 soak 采样出现 `active=16` 而内层上限 10、`idle=8,active=8` 等不自洽 |
| 慢查询定位 | ⚠️ | 无自动埋点证据；需手动接 `sz-orm-observability` |
| Soak 指标 | ❌ | Windows 下 RSS/fd 恒为 0（§9.2），告警/门禁拿到的是空数据 |

`[推断]` 若把当前组件直接接到生产告警，`sz_orm_*` 系列将长期处于 `No data`，等于没有监控；应先把「注册 + 导出 + 埋点」三件事做成默认路径。

---

## 14. 优先整改清单

| # | 优先级 | 问题 | 证据 | 改法 | 验收标准 |
|---|--------|------|------|------|----------|
| P0-1 | 阻断 | CI 从不比对基线；HEAD 仍 fail-open | `.github/workflows/ci.yml:765/783/791`；`git show HEAD:.github/workflows/ci.yml` | `download-artifact` 取上轮基线 → `--compare-baseline ci-sqlite` → 通过后再 `--save-baseline`；移除 `continue-on-error`；缺 workload 判失败 | main 上人为 +15% 必红；删 1 个 workload 必红；PR 留痕不阻断 |
| P0-2 | 阻断 | 元数据全缺 + 统计口径不足 | `packages/sz-orm-bench/src/lib.rs:486`；今日报告 JSON | 真实采集 CPU/内存/rustc/feature/DB 版本；rounds ≥100；3 轮中位；门限按 CV 自适应 | 报告六字段无 `unknown/0/""`；同代码 3 轮 p95 极差 ≤15% |
| P0-3 | 阻断 | 内层池上限 10 + Drop 异步归还 | `packages/sz-orm-sqlx/src/any.rs:170`；`packages/sz-orm-core/src/pool.rs:440` | 内外上限对齐；Drop 改为内联归还或至少在 acquire 中同步回收；`PoolStatus` 记账自洽 | 任意 pool 1~256：acquire 错误 0、`idle+active≤max`、串行 10k 次无超时 |
| P1-1 | 高 | prepared cache / SIMD / plan-cache 未接热路径 | `packages/sz-orm-core/src/connection_ext.rs:41`；`packages/sz-orm-core/src/simd.rs:113` | 在 `PooledConnection::query` 接入 `PreparedStatementCache`；SIMD 路径要么删除要么接 decode | 端到端开关缓存 p50 提升 ≥20%（对照 5 次） |
| P1-2 | 高 | 可观测性未接线 | `alert_metrics.rs:204`；exporter 无端点 | 默认注册 + 提供 `serve(addr)`；soak 用真实 RSS/fd | 进程启动后 `/metrics` 可抓；告警不再 `No data` |
| P1-3 | 高 | 工件与脚本未提交/未清理 | HEAD 含 `update_report.js`、`1789544529` | 提交工作区删除；`bench-results/` 只保留当前版本 | `git ls-tree HEAD bench-comparison/update_report.js` 为空 |
| P2-1 | 中 | G18 只查 token | `scripts/check-metrics-real.py:257` | 要求性能行带「对照 + 重复次数 + 复现命令」结构化块；识别无对照的百分比 | 能拦住「89.3%（无对照）」与「1h soak（mock）」 |
| P2-2 | 中 | C ABI JSON 边界成本 | `packages/sz-orm-cabi/src/lib.rs:378` | 增加列式/二进制返回或句柄复用 | 1 行查询跨语言开销 ≤ +20% |
| P2-3 | 中 | 四库声称与工具能力错配 | 工具对 PG/MySQL/Oracle 的 sz-orm 不支持 | README 明确「仅 SQLite 可 CLI 基准」；或补齐驱动 | 文档与工具输出一致 |
| P2-4 | 中 | `audit-verify` 把 `packages/*/Cargo.toml:NNN` 误解析为根清单 | [scripts/audit-verify.ps1:137](file:///D:/www/test/鲜视达/rust/sz-orm/scripts/audit-verify.ps1#L137) 的根级正则 `(?<![\w.-])(Cargo\.toml):(\d+)` 会吃掉子目录前缀；本轮首测因此误报 3 处失败（§15.4） | 先匹配 `packages/[^:]+\.toml:(\d+)` 再回退根级；或对 `*/Cargo.toml` 强制要求绝对 `file:///` 引用 | 子目录清单引用能定位到目标包；`test_audit_verify` 增加该用例 |

---

## 15. 证据索引、`audit-verify` 输出与未执行项

### 15.1 仓库内证据（L1）

| 主题 | 引用 |
|------|------|
| 版本 | `Cargo.toml:6` |
| 池 Drop 归还 / 状态 / 指标 | `packages/sz-orm-core/src/pool.rs:440`、`:717`、`:749` |
| 内层池上限 10 | `packages/sz-orm-sqlx/src/any.rs:170`、`:875`、`:1748` |
| 元数据硬编码 | `packages/sz-orm-bench/src/lib.rs:486`、`:493`、`:542` |
| 基线比对逻辑 | `packages/sz-orm-bench/src/regression_baseline.rs:92`、`:107`、`:200` |
| 基线缺失 fail-closed | `packages/sz-orm-bench/src/main.rs:118`、`:134` |
| CI benchmark job | `.github/workflows/ci.yml:761`、`:765`、`:783`、`:791`、`:796` |
| Soak workflow | `.github/workflows/soak.yml:68`、`:90` |
| Soak 退化规则 | `packages/sz-orm-core/tests/common/soak.rs:233`、`:272` |
| Soak 归还注释 | `packages/sz-orm-core/tests/soak.rs:106` |
| SIMD 实现 | `packages/sz-orm-core/src/simd.rs:113`、`:180` |
| prepared cache 门控测试 | `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:1` |
| 缓存未接线 | `packages/sz-orm-core/src/connection_ext.rs:41`、`packages/sz-orm-core/src/pool_elastic.rs:1066` |
| SIMD 性能断言 | `packages/sz-orm-core/tests/performance_validation.rs:75` |
| feature 纠缠 | [sz-orm-core/Cargo.toml:14](file:///D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L14)、[sz-orm-core/Cargo.toml:225](file:///D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L225)；[sz-orm-sqlx/Cargo.toml:14](file:///D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/Cargo.toml#L14) |
| 可观测性注册 | `packages/sz-orm-observability/src/alert_metrics.rs:204`、`packages/sz-orm-observability/src/lib.rs:62` |
| flamegraph 计时器 | `packages/sz-orm-flamegraph/src/collector.rs:39` |
| C ABI 查询 | `packages/sz-orm-cabi/src/lib.rs:378` |
| 旧基准报告 | `bench-comparison/BENCHMARK_REPORT.md:5`、`:37`、`:56`、`:125` |
| README soak 声称 | `README.zh.md:359`、`README.zh.md:1061` |
| ROI 记录 | `docs/assessment/2026-10-27-perf-fixes.md:10`、`:43`、`:68`、`:99`、`:120`、`:139` |
| 四库 8/8 声称 | `docs/assessment/2026-09-24-v870-perf-benchmark-complete.md:140` |
| 既有 DB 复核（旁证） | `docs/assessment/2026-10-28-db-review.md:178`、`:413`、`:464` |
| G18 规则 | `scripts/check-metrics-real.py:151`、`:257` |
| 接线测试重构 | `packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:9` |

### 15.2 本会话测得的外部工件（L2，位于仓库外）

| 工件 | 内容 |
|------|------|
| `D:\temp\perf-eval-v2\harness\` | 外部 harness（SQLite/PG，szorm / szorm_rel / szorm_wide / sqlx） |
| `D:\temp\perf-eval-v2\gradient.csv` | 33 组 SQLite + 10 组 PG 并发梯度 |
| `D:\temp\perf-eval-v2\szorm_rel.csv` | 8 组显式归还梯度 |
| `D:\temp\perf-eval-v2\release_ab.csv` / `release_ab.sh` | 归还策略 5×2 次 A/B |
| `D:\temp\perf-eval-v2\hotspots.log` | 2000 样本链路分解 + sqlx 对照 |
| `D:\temp\perf-eval-v2\soak_real_900.log` | 900 s soak（SAMPLE/BUCKET/RESULT） |
| `D:\temp\perf-eval-v2\soak_5x.log` | 仓库 soak 5 次 |
| `D:\temp\perf-eval-v2\flaky_stress_chaos.log` | stress/chaos_pool 各 5 次 |
| `D:\temp\perf-eval-v2\v65_runs.log` | prepared cache 5 次 |
| `D:\temp\perf-eval-v2\simd_ab.log` | SIMD 3 项 ×5 次 |
| `D:\temp\perf-eval-v2\pool_leak*.log` | 归还策略串行对照 |
| `D:\temp\perf-eval-v2\cabi_ab.log` | C ABI 往返 |
| `D:\temp\perf-eval-v2\bench-results\baseline_sq_r{1,2,3}.json` | 同代码 3 次基准（方差/误报分析） |
| `D:\temp\perf-eval-v2\bench-results\pg_tool_1793188087.json` | PG 工具产物（仅 sqlx 6 条） |

### 15.3 未执行项清单

| 项 | 原因 |
|----|------|
| MySQL 真实基准 | `1045 Access denied`，文档密码 `<local-password>` 与当前实例不匹配；按红线未调参/未改账号 |
| Oracle 真实基准 | bench 工具明确不支持 sz-orm/sqlx 的 Oracle 后端；sea-orm 无驱动 |
| `perf-accel`/`simd`/`prepared-stmt-cache` 的 cargo feature 级 on/off | feature 并集恒开（workspace 依赖），且热路径零引用；改为结构分析 + 单测级 A/B |
| C# P/Invoke | 本机无 `dotnet`（退出码 2） |
| flamegraph / ETW 采样 | 产品内无采样器/bin；Windows 无 perf，未部署 WPR |
| 6 小时 soak | 单次 15 分钟替代（协议允许如实记录） |
| 流式背压 / L1 / plan cache 端到端 | 默认路径未接线，无运行时开关，无法构造对照 |
| `bench-comparison/` Criterion 复跑 | 红线禁止运行该目录下 Node 脚本；且其 in-memory 配置与 CLI 不可比 |

### 15.4 `audit-verify` 自证输出

**命令**（`ExecutionPolicy Bypass` 因本机默认策略禁止运行 `.ps1`；UTF-8 控制台避免 GBK 吞行）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command `
  "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; .\scripts\audit-verify.ps1 docs\assessment\2026-10-28-perf-review-v2.md; exit \$LASTEXITCODE"
```

`[实测]` 两轮结果（同一脚本、同一报告，差异仅在于附录内嵌日志是否参与解析）：

| 轮次 | 被测内容 | 通过 | 失败 | 退出码 | 裁定 |
|---|---|---|---|---|---|
| ① 定稿前 | 报告本体（§1–§15.3 共 50 处引用） | **50** | **0** | **0** | **权威结果：全部证据有效** |
| ② 定稿后 | 最终文件（50 处本体 + 50 处内嵌日志） | 100 | 2 | 1 | 2 处**伪失败**：附录日志中「sz-orm-core 清单第 225 行」被 §P2-4 的同一正则二次解析为根清单（根清单仅 91 行） |

> 轮次 ② 的两处失败不指向任何新增证据：报告正文对第 225 行的引用使用绝对 `file:///` 链接（轮次 ① 已逐条 ✅），失败来自**附录把该行当纯文本重新打印**。这与 §P2-4 是同一缺陷的第二个表现面。轮次 ② 的原始输出见 §15.6。

首轮（修正前）曾报 `失败 3`，全部来自对包内清单**第 15 行**的引用被脚本的**根级文件正则**（[scripts/audit-verify.ps1:137](file:///D:/www/test/鲜视达/rust/sz-orm/scripts/audit-verify.ps1#L137)，`(?<![\w.-])(Cargo\.toml):(\d+)`）捕获为**仓库根清单的第 15 行** —— 该行为空行。两处缺陷已修：

1. 行号本身错了 1 行：`default = [...]` 实际在包内清单第 **14** 行，第 15 行是 `owasp-pentest-suite = []`；
2. 引用形态改为绝对 `file:///` 链接（`#L14` 锚点），绕过该正则误吞，且能被脚本按字面校验到**目标包**清单。

> 该首轮失败本身就是一条评测证据：**门禁 13 对子目录清单的行号引用会把证据指向错误文件**，属于审计工具自身的静默错判（不是 fail-open，但会误伤真实引用）。已列入 §14 P2-4。

### 15.5 audit-verify 执行输出

```text
==========================================
  审计证据验证: docs\assessment\2026-10-28-perf-review-v2.md
==========================================
✅ Cargo.toml:6 (共 91 物理行)
✅ packages/sz-orm-bench/src/lib.rs:486 (共 994 物理行)
✅ packages/sz-orm-sqlx/src/any.rs:170 (共 2954 物理行)
✅ packages/sz-orm-core/src/pool.rs:440 (共 5638 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml:14 (共 936 物理行)
✅ packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:9 (共 399 物理行)
✅ packages/sz-orm-observability/src/alert_metrics.rs:204 (共 347 物理行)
✅ packages/sz-orm-bench/src/main.rs:118 (共 379 物理行)
✅ packages/sz-orm-bench/src/regression_baseline.rs:92 (共 211 物理行)
✅ packages/sz-orm-bench/src/lib.rs:493 (共 994 物理行)
✅ Cargo.toml:6 (共 91 物理行)
✅ packages/sz-orm-bench/src/lib.rs:567 (共 994 物理行)
✅ packages/sz-orm-core/src/connection_ext.rs:41 (共 398 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml:14 (共 936 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml:225 (共 936 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/Cargo.toml:14 (共 111 物理行)
✅ packages/sz-orm-sqlx/src/any.rs:374 (共 2954 物理行)
✅ packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:1 (共 195 物理行)
✅ packages/sz-orm-core/src/simd.rs:113 (共 1344 物理行)
✅ packages/sz-orm-flamegraph/src/collector.rs:39 (共 198 物理行)
✅ packages/sz-orm-cabi/src/lib.rs:378 (共 3518 物理行)
✅ packages/sz-orm-observability/src/alert_metrics.rs:204 (共 347 物理行)
✅ packages/sz-orm-core/src/pool.rs:717 (共 5638 物理行)
✅ packages/sz-orm-bench/src/lib.rs:486 (共 994 物理行)
✅ packages/sz-orm-sqlx/src/any.rs:170 (共 2954 物理行)
✅ packages/sz-orm-core/src/pool.rs:440 (共 5638 物理行)
✅ packages/sz-orm-core/src/connection_ext.rs:41 (共 398 物理行)
✅ packages/sz-orm-core/src/simd.rs:113 (共 1344 物理行)
✅ packages/sz-orm-cabi/src/lib.rs:378 (共 3518 物理行)
✅ Cargo.toml:6 (共 91 物理行)
✅ packages/sz-orm-core/src/pool.rs:440 (共 5638 物理行)
✅ packages/sz-orm-sqlx/src/any.rs:170 (共 2954 物理行)
✅ packages/sz-orm-bench/src/lib.rs:486 (共 994 物理行)
✅ packages/sz-orm-bench/src/regression_baseline.rs:92 (共 211 物理行)
✅ packages/sz-orm-bench/src/main.rs:118 (共 379 物理行)
✅ packages/sz-orm-core/tests/common/soak.rs:233 (共 676 物理行)
✅ packages/sz-orm-core/tests/soak.rs:106 (共 294 物理行)
✅ packages/sz-orm-core/src/simd.rs:113 (共 1344 物理行)
✅ packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:1 (共 195 物理行)
✅ packages/sz-orm-core/src/connection_ext.rs:41 (共 398 物理行)
✅ packages/sz-orm-core/src/pool_elastic.rs:1066 (共 1802 物理行)
✅ packages/sz-orm-core/tests/performance_validation.rs:75 (共 171 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml:14 (共 936 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml:225 (共 936 物理行)
✅ D:/www/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/Cargo.toml:14 (共 111 物理行)
✅ packages/sz-orm-observability/src/alert_metrics.rs:204 (共 347 物理行)
✅ packages/sz-orm-observability/src/lib.rs:62 (共 836 物理行)
✅ packages/sz-orm-flamegraph/src/collector.rs:39 (共 198 物理行)
✅ packages/sz-orm-cabi/src/lib.rs:378 (共 3518 物理行)
✅ packages/sz-orm-bench/tests/perf_bench_real_wiring.rs:9 (共 399 物理行)

==========================================
  验证结果
==========================================
  ✅ 通过: 50
  ❌ 失败: 0
  ⚠️  警告: 0
==========================================
✅ 审计证据验证通过
```

**裁定**：`[实测]` 轮次 ① 的 50 项 `file:line` 证据全部指向真实存在、行号在范围内且非空的行 → 证据层 L1 引用可审计。**边界**：脚本只校验「文件存在 + 行号有效 + 非空」，不校验「该行内容支撑该结论」；§7/§8/§9/§10 的 `[实测]` 数字由 §15.2 的外部工件日志与命令复算，不依赖本脚本。

### 15.6 定稿后复跑（附录自引用）

对**最终文件**（含本附录）再跑一次同一命令：

```text
==========================================
  验证结果
==========================================
  ✅ 通过: 100
  ❌ 失败: 2
  ⚠️  警告: 0
==========================================
❌ 审计证据验证未通过 — 存在 2 处无效引用
```

`[实测]` 退出码 **1**。两处失败形如 `❌ <包内清单路径>:225 — 行号超出范围（文件共 91 物理行）`，即 §15.5 内嵌日志中**被打印出来的**「sz-orm-core 清单第 225 行」一行被 §P2-4 的正则当成**根清单**重新解析。对照：轮次 ① 中同一行以 `file:///` 绝对链接形式被正确校验为「共 936 物理行」。

**结论**：这不构成新增虚假证据，而是审计工具的**文本复用盲区**——报告引用附录的原始输出时，工具会把附录当正文再扫一遍。修复方向同 §P2-4：让路径解析优先匹配 `packages/` 前缀，并对代码块（fenced block）内的行跳过校验。完整原始日志见 `D:\temp\perf-eval-v2\audit_verify_final.log`（轮次 ②）与 `D:\temp\perf-eval-v2\audit_verify.log`（轮次 ①）。

---

**报告状态**：完稿。全部 15 节交付，A–J 硬性实验已执行项均附命令/退出码/工件路径，未执行项见 §15.3 并注明原因。按红线要求：未运行 `bench-comparison/` 下任何 Node 脚本，未修改仓库内基线文件，未重启/调参数据库，仅操作 `sz_orm_test`，密码统一为 `<local-password>`。**未提交、未推送。**
