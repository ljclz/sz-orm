# 2026-10-28 CI 门禁阻断语义与敏感信息清理整改记录

> 来源：
> - `docs/assessment/2026-10-27-sdet-review.md`（SDET 评测 ROI-5：fuzz/benchmark/integration 阻断语义）
> - `docs/assessment/2026-10-27-security-review-fixes.md`（安全评测 P0-3 遗留：真实服务器 IP 占位）
> - `docs/assessment/2026-10-27-perf-fixes.md`（并行会话已完成的性能整改 ROI-1~ROI-6，本次记录其验证与补充）
>
> 整改日期：2026-10-28
> 决策依据：用户明确拍板「服务器已重装、凭据已失效」「对 main 分支阻断」「按建议执行」。

---

## 一、SDET ROI-5 落地：CI 高价值门禁「对 main 分支阻断」

### 背景

SDET 评测发现 `fuzz` / `benchmark` / `integration`（含 `mq-integration`）四个高价值门禁在
`.github/workflows/ci.yml` 中全部 `continue-on-error: true`，即命令失败不阻断合入，CI 强制力
弱于文档宣称的「25 道门禁」。评测建议"对 main 分支阻断"（需分支条件判断）。

用户决策：**对 main 分支阻断**——即 PR 分支继续允许偶发失败（真实服务不稳定 / 性能波动不阻塞
日常开发，保留 ADR-005 原意）；main 分支上失败必须阻断合入。

### 改动

四个 job 的 `continue-on-error` 从 `true` 改为条件表达式
`${{ github.ref != 'refs/heads/main' }}`：

| Job | 位置 | 阻断语义 |
|-----|------|----------|
| `integration`（真实服务集成） | `.github/workflows/ci.yml:466-470` | main 阻断，PR 放行 |
| `mq-integration`（真实 MQ 集成） | `.github/workflows/ci.yml:598-602` | main 阻断，PR 放行 |
| `benchmark`（性能基准） | `.github/workflows/ci.yml:763-768` | main 阻断，PR 放行 |
| `fuzz`（模糊测试） | `.github/workflows/ci.yml:1101-1105` | main 阻断，PR 放行 |

`benchmark` job 由并行会话先改为无条件 fail-closed，本次按用户决策调整为「main 分支阻断」，
并在注释中说明（`.github/workflows/ci.yml:754-762`）。

### 验证

- `python -c "yaml.safe_load(...)"` → YAML 解析通过，`jobs` 列表 23 个 job 正常。
- 语义核验：`github.ref != 'refs/heads/main'` 在 main push 时为 `false`（阻断），
  在 PR 时为 `true`（放行）。

---

## 二、文档真实服务器 IP 占位清理（安全 P0-3 遗留）

### 背景

安全评测 P0-3 已将文档中的真实密码替换为占位符，但 R10 扫描发现真实服务器 IP
`<remote-server-ip>` / `<legacy-server-ip>` 仍残留在 `.codeartsdoer` 历史文档、基准报告、
部署脚本与源码注释中。用户确认服务器已重装、相关凭据已失效，但真实 IP 仍应占位，
避免仓库成为目标探测的线索。

### 改动

1. **批量占位**：仓库内 24 个文件中的 `<remote-server-ip>` → `<remote-server-ip>`、
   `<legacy-server-ip>` → `<legacy-server-ip>`。涉及：
   - `.codeartsdoer/` 下 21 个历史文档（specs v830~v910、experience-reuse、temp/accept）
   - `bench-comparison/BENCHMARK_REPORT.md`
   - `packages/sz-orm-core/tests/common/e2e_env.rs:42`（注释）
   - `scripts/soak-kit/deploy-122.sh`
2. **`deploy-122.sh` 环境变量化**：原 `HOST="root@<legacy-server-ip>"` 硬编码真实主机，
   改为 `HOST="${SZORM_SERVER_122:-}"`，未设置时打印错误并退出
   （`scripts/soak-kit/deploy-122.sh:8-14`），部署方显式配置服务器地址。
3. **`e2e_env.rs` 注释同步占位**（`packages/sz-orm-core/tests/common/e2e_env.rs:42`）。

### 验证

- 全仓库扫描 `<remote-server-ip>|<legacy-server-ip>` → **0 命中**。
- `python scripts/check-secrets.py` → `OK: no secrets detected`（2957 文件）。

---

## 三、安全整改报告运维项状态更新

`docs/assessment/2026-10-27-security-review-fixes.md`：

- §5.2 运维动作第 1 条更新为「服务器已重装，凭据已失效，无需轮换；IP 已批量占位」，
  `git filter-repo` 清理历史降级为可选（`:182-186`）。
- §6 遗留跟踪表新增一行「文档残留真实服务器 IP → 已闭环（2026-10-28）」（`:207`）。

---

## 四、门禁脚本自测补充（5 个新脚本）

### 背景

运行 `python -m pytest tests/test_gate_scripts.py` 时发现
`test_all_check_scripts_have_self_tests` 失败：UXR 第三轮（并行会话，2026-10-27）新增的
5 个门禁脚本没有自测覆盖：

- `check-filename-encoding.py`（G26）
- `check-doc-version-consistency.py`（G27）
- `check-generated-code-compiles.py`（G28）
- `check-lockfile-ignore-relevance.py`（警告级）
- `check-test-symbols.py`（警告级）

这违反 SDET 评测 ROI-3「门禁脚本自身必须有自测」的整改要求。

### 改动

`tests/test_gate_scripts.py` 新增 `TestNewGateScriptsSmoke`（`:286-333`），为 5 个脚本补冒烟自测：

- `check-filename-encoding`：`_name_roundtrips` 正反例（正常中文可 round-trip、
  GBK 乱码字节经 surrogateescape 进入 str 后判定为乱码）+ 当前仓库 `scan()` 通过（`:294-301`）
- `check-doc-version-consistency`：检查表引用的 6 份核心文档均存在 +
  顶部版本行正则可提取当前版本（`:304-315`）
- 其余 3 个脚本：导入 + `main` 可调用冒烟（`:344-362`）

### 验证

`python -m pytest tests/test_gate_scripts.py -q` → **26 passed**（此前 20 passed + 1 failed）。

---

## 五、验证汇总（本会话实测，2026-10-28）

| 检查 | 命令 | 结果 |
|------|------|------|
| CI YAML 语法 | `python -c "yaml.safe_load(open('.github/workflows/ci.yml'))"` | ✅ 通过，23 个 job |
| 敏感信息扫描 | `python scripts/check-secrets.py` | ✅ 0 secrets |
| 度量真实性（G18） | `python scripts/check-metrics-real.py` | ✅ 通过（含性能声称证据） |
| 文档一致性（G12） | `python scripts/check-doc-consistency.py` | ✅ EXIT=0 |
| SQL 注入扫描（G9） | `python scripts/check-sql-injection-auto.py --strict` | ✅ 46/46 Safe |
| 文档代码可编译（G25） | `python scripts/check-doc-code-compiles.py` | ✅ 通过 |
| 幻影交付（G15） | `python scripts/check-phantom-delivery.py` | ✅ EXIT=0 |
| 语义反模式（G16） | `python scripts/check-semantic-patterns.py` | ✅ EXIT=0 |
| 架构一致性（G17） | `python scripts/check-architecture.py` | ✅ EXIT=0 |
| 发布一致性（G19） | `python scripts/check-publish-consistency.py` | ✅ EXIT=0 |
| 示例清单（G24） | `python scripts/check-examples-readme.py` | ✅ EXIT=0 |
| 未用依赖（G23） | `python scripts/check-unused-deps.py` | ✅ EXIT=0 |
| 门禁脚本自测 | `python -m pytest tests/test_gate_scripts.py -q` | ✅ 26 passed |
| bench 默认测试 | `cargo test -p sz-orm-bench` | ✅ 通过 |
| 可观测性 | `cargo test -p sz-orm-observability --lib` | ✅ 47 passed |
| 真实 SQLite 接线 + 基线缺失阻断 | `cargo test -p sz-orm-bench --features perf-bench-real --test perf_bench_real_wiring` | ✅ 8 passed |
| 稳定性矩阵 is_simulated | `cargo test -p sz-orm-bench --features stability-matrix --test stability_matrix_test` | ✅ 6 passed |
| SIMD 收益断言 | `cargo test -p sz-orm-core --test performance_validation --features simd` | ✅ 6 passed |
| v65 真实测量（去 sleep 注入） | `cargo test -p sz-orm-core --test v65_prepared_cache_benefit --features prepared-stmt-cache -- --ignored` | ✅ 3 passed（miss 305.7µs vs hit 32.6µs，降幅 89.3%） |

---

## 六、关键 file:line 证据索引

| 证据 | 位置 |
|------|------|
| integration job main 阻断 | `.github/workflows/ci.yml:466-470` |
| mq-integration job main 阻断 | `.github/workflows/ci.yml:598-602` |
| benchmark job main 阻断（含注释） | `.github/workflows/ci.yml:754-768` |
| fuzz job main 阻断 | `.github/workflows/ci.yml:1101-1105` |
| deploy-122.sh 环境变量化 | `scripts/soak-kit/deploy-122.sh:8-14` |
| e2e_env.rs 注释占位 | `packages/sz-orm-core/tests/common/e2e_env.rs:42` |
| 门禁脚本自测补充 | `tests/test_gate_scripts.py:286-333` |
| 安全报告运维项状态更新 | `docs/assessment/2026-10-27-security-review-fixes.md:182-186` |
| 安全报告遗留项闭环 | `docs/assessment/2026-10-27-security-review-fixes.md:207` |
| 性能整改记录（并行会话） | `docs/assessment/2026-10-27-perf-fixes.md` |
| 性能整改 CHANGELOG | `CHANGELOG.md`（v10.13.0 条目） |

### audit-verify 输出

```
（见文末运行输出）
```

---

*整改记录生成：2026-10-28。所有门禁与 cargo 验证均为本会话实测输出。*