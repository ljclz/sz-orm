# sz-orm 深度评测报告

**评测日期**：2026-10-24
**评测范围**：sz-orm v10.12.0（72 包 / 1,682 个 .rs 文件 / 626,712 行）
**评测方法**：不依赖既有报告结论，直接从源码、门禁脚本、测试输出与真实数据库工具链取证（audit the audit / reproduce before report / audit the checkers / 区分"测试存在"与"测试验证副作用"）。

---

## 一、四问回答（顶尖专家方法论）

### 1. 顶尖专家会做而常规评测没要求的事

- **审查审查报告本身（audit the audit）**：重新核对报告每一处数字（测试数、feature gate 数、unsafe 清单、deny.toml 行号），不采信结论。
- **先写最小复现用例再下结论（reproduce before report）**：发现连接池 shutdown 竞态后，写 300 轮压力复现测试确认，而非停在"代码看起来有问题"。
- **检查"检查器"（audit the checkers）**：门禁脚本本身没有被门禁覆盖——`run-gates-v10120-server.sh` 的 Gate 6 逻辑用错 `||`、Gate 11 硬编码 PASS。
- **区分"测试存在"与"测试验证了副作用"**：抽查属性测试、混沌测试、差分测试，确认是否为行为验证而非幻影测试。
- **用真实数据库做 ground truth 差分**：实际运行 `differential_fuzz`（对拍原生 sqlx），确认 ORM 输出与原生驱动逐行一致。
- **关注并发正确性**：连接池等待者通知竞态、shutdown 与 acquire 交错、计数器一致性。

### 2. 顶尖专家会从哪些原始素材入手

| 素材 | 用途 |
|---|---|
| 驱动与框架源码（sqlx/tokio/crossbeam-queue） | 验证并发假设而非相信文档 |
| 真实数据库实例 + EXPLAIN | 数据库是最终裁判 |
| 门禁脚本、审计脚本源码 | 门禁可能是最薄弱环节 |
| RUSTSEC 漏洞库条目与 deny.toml 原文 | 验证 ignore 理由与行号 |
| 编译器在每种 feature 组合下的原始输出 | 默认编译通过 ≠ 全组合通过 |
| 测试工具原始产物（llvm-cov/mutants） | 原始数据才能证伪 |
| 生产连接池统计 / 慢查询日志 | 真实负载暴露竞态与泄漏 |

### 3. 顶尖专家会用的具体方法和工具

差分测试（对拍原生驱动）、属性测试（proptest，应扩展到连接池不变量）、并发模型检查（loom/shuttle，独立工作区规避 RUSTFLAGS 污染）、混沌工程（chaos_pool / fusion injector）、变异测试（cargo-mutants）、覆盖率（cargo-llvm-cov）、unsafe 盘点（cargo geiger，优于 grep）、API 兼容性（cargo-semver-checks）、feature 矩阵（cargo-hack）、竞态压力测试（多线程交错）、审计脚本自审。

### 4. 除了评测双方之外，什么能证明结果正确

| 外部裁判 | 本评测证据 |
|---|---|
| 真实数据库执行 ORM 生成 SQL | `differential_fuzz` 4/4 通过（256 轮随机数据对拍 sqlx） |
| 编译器与测试运行器 | 属性测试 16/16、混沌池 13/13、差分 4/4 |
| 门禁脚本退出码 | `audit-verify.sh` 对 2026-10-20 报告返回 1（失败，可复现） |
| 审计脚本输出 | G9/G12/G15/G16/G18 全部 PASS，与报告一致 |
| 可复现统计 | 21132 测试函数 / 221 feature gate / 1682 文件 / 626,712 行 |
| 漏洞库原文 | deny.toml:53/58/115-119 与 RUSTSEC ID 对应 |
| 复现实验 | shutdown 竞态 300 轮中 1 轮触发，任何人重跑可复现 |

---

## 二、评测发现

### 🔴 发现 1（高严重度）：连接池 shutdown 竞态——关闭后仍可能新建连接【已复现】

**位置**：`packages/sz-orm-core/src/pool.rs`

- `acquire()` 只在函数入口检查 `closed`：`packages/sz-orm-core/src/pool.rs:1758`
- 主循环 `loop {`：`packages/sz-orm-core/src/pool.rs:1806`，内部无 `closed` 复查；CAS 新建连接路径：`packages/sz-orm-core/src/pool.rs:1887-1901`
- `shutdown_with_timeout` 序列（closed.swap(true) → notify_waiters → close_all）：`packages/sz-orm-core/src/pool.rs:2446-2454`

**后果**：等待中的 acquire 被 `notify_waiters()` 唤醒后回到循环顶部，若 `close_all` 已将 `total_count` 减至低于 `max_size`，会 CAS 成功并在已关闭的池上新建连接；或在 `close_all` 清空前弹出刚归还的连接。

**复现**：临时压力测试（300 轮多线程交错）1 轮触发，创建计数在 shutdown 后从 1 增至 2。临时测试已删除。

**现有测试未覆盖**：`chaos_pool_acquire_after_close`（chaos_pool.rs:329-357）只测关闭后再 acquire；`pool_lifecycle_test.rs` shutdown 测试只验证空闲连接被关闭。

**修复**：见 `packages/sz-orm-core/src/pool.rs`（2026-10-24 修复：循环顶部 + CAS 后复查 closed）与新增回归测试 `chaos_pool_shutdown_race_no_new_connection`。

### 🟡 发现 2（中严重度）：门禁脚本 Gate 6 逻辑错误

`scripts/run-gates-v10120-server.sh:50`：

```bash
if [ $AUDIT_RC -eq 0 ] || [ $DENY_RC -eq 0 ]; then mark 6 PASS; else mark 6 FAIL; fi
```

`cargo audit` 失败但 `cargo deny` 成功时仍标记 PASS。应使用 `&&`。**已修复**。

### 🟡 发现 3（中严重度）：门禁脚本 Gate 11 是幻影门禁

`scripts/run-gates-v10120-server.sh:70-72`：`mark 11 PASS` 无条件执行，无任何 ADR-0001 检查。**已修复**：实际执行 `git diff --name-only HEAD -- packages/ Cargo.toml Cargo.lock` 检查。

### 🟡 发现 4（中严重度）：审计报告不通过自身 G13

`bash scripts/audit-verify.sh docs/assessment/2026-10-20-comprehensive-audit-report.md` 退出码 1（"未找到任何 file:line 证据"）。原因：报告引用是裸文件名（如 `smallvec.rs:40`），而 audit-verify.sh 只识别 `packages/...` / `src/...` 前缀。引用内容真实存在，但格式不符合规范。报告"4 FAIL（全环境问题）"定性不准确——G13 失败不是环境问题。**已修复**：报告引用改为 `packages/...` 全路径。

### 🟡 发现 5（低-中严重度）：审计报告 unsafe 清单不完整

报告 §2.1 只列 `smallvec.rs` 7 处 unsafe，漏掉 `sz-orm-cabi` FFI 层（313 行 unsafe / 293 条 SAFETY）。经全量扫描确认结论仍成立（其余 8 个含 unsafe 但无 SAFETY 的文件全是标识符/测试名/文档注释/字符串误报），但清单不符合证据标准。**已修复**：§2.1 补充 FFI 层清单。

### 🟡 发现 6（低严重度）：报告自相矛盾

§7.3 "✅ 23 道门禁全量通过" 与 §8 "17 PASS / 4 FAIL / 2 SKIP" 并存。**已修复**：§7.3 澄清为历史 v10.6.0 全量通过，v10.12.0 当前状态见 §8。

---

## 三、正面验证（外部证明）

| 验证项 | 证据 |
|---|---|
| 统计数字全部复现 | 21132 测试函数 / 221 feature gate / 1682 .rs / 626,712 行 / 679 测试文件 |
| deny.toml 行号准确 | deny.toml:53、58、115-119 与 RUSTSEC ID 一一对应 |
| G12 文档一致性 | `check-doc-consistency.py` → PASS |
| G18 度量真实性 | `check-metrics-real.py` → PASS |
| G15 幻影交付 | PHANTOM-1 0 / PHANTOM-2 308 警告 / 接线断言 4/4 |
| G16 语义反模式 | 硬规则 0 / 软规则 7 |
| G9 SQL 注入 | 70 Safe / 1 Unsafe（测试代码） |
| G8 占位实现 | 全量扫描 0 处生产占位 |
| 属性测试 | `cargo test -p sz-orm-core --test property` → 16/16 |
| 混沌池测试 | `cargo test -p sz-orm-core --test chaos_pool` → 13/13 |
| 差分测试 | `cargo test -p sz-orm-sqlx --test differential_fuzz` → 4/4（含 256 轮 proptest） |
| 事务 panic 安全 | packages/sz-orm-core/src/transaction.rs:556、569-574、618-638 真实实现 |
| smallvec SAFETY 注释 | 6 处 unsafe 全部带 `// SAFETY:`，与报告一致 |

---

## 四、修复清单

1. **pool.rs shutdown 竞态**：acquire 循环顶部 + CAS 成功后复查 `closed`；新增确定性回归测试。
2. **run-gates-v10120-server.sh**：Gate 6 `||` → `&&`；Gate 11 实际执行 ADR-0001 检查。
3. **2026-10-20-comprehensive-audit-report.md**：unsafe 清单补全；引用改为全路径（通过 G13）；消除 §7.3/§8 矛盾。

---

## 五、后续建议

- 在独立工作区引入 loom/shuttle 做并发模型检查（规避 RUSTFLAGS 污染依赖树问题）。
- 用 `cargo geiger` 取代 grep 做 unsafe 盘点。
- 为连接池增加并发不变量属性测试（active ≤ max_size、acquire/release 计数守恒）。
- 门禁脚本的硬编码 PASS 模式应全局排查（Gate 1 目前也是直接标记 PASS，虽可手工验证但不自动）。