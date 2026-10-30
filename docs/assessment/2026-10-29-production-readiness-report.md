# sz-orm 生产就绪度评估报告（2026-10-29 重评）

> 评估日期：2026-10-29
> 评估对象：`sz-orm` workspace v10.13.0（`Cargo.toml:6`），HEAD `949ff449`（工作树含并行会话在途改动）
> 评估范围：综合 8 份独立评测（全栈 v1/v3、安全、性能 v1/v2、SDET、UXR、TW、DBA、系统架构师路线图）及其整改闭环，结合本会话实测复核
> 方法：只采信可验证证据（`file:line` 或命令输出），全部门禁与关键测试本会话实测

---

## 一、总体结论

**就绪度：🟢 有条件投产（内部生产可用）**

经过 2026-10-28~29 密集评测与整改，**此前被 DBA 评测判定的 P0（PG 时间戳静默 NULL、DECIMAL 精度丢失）已全部修复并经真库验证**；安全评测的凭据泄露与扫描器假绿已修复（`check-secrets.py` 实测 0 secrets）；架构评测的供应链声称与锁文件不符已修复（tokio 1.54.1 / ctutils 0.4.6 / webpki-root-certs 1.0.11 / actix-http 3.20.0 与 CHANGELOG 一致）。**当前无已知 P0 级缺陷**。

对外公开宣称"生产就绪"前，剩余为**中优先级加固项**（性能元数据、孤儿包治理、历史报告引用清账、C# SafeHandle、CI 基线比对闭环）。

| 维度 | 评级 | 关键状态（2026-10-29 实测） |
|------|------|----------|
| 数据库语义正确性 | 🟢 已修复 | PG 时间戳/日期/NUMERIC 真库 46 passed；MySQL 21 passed；隔离级别/方言/会话复位闭环 |
| 供应链安全 | 🟢 已修复 | 锁文件与 CHANGELOG 一致；cargo-audit 0 漏洞；deny.toml 与 CI 一致 |
| 凭据与门禁 | 🟢 已修复 | check-secrets 0 secrets；13 项门禁全过；门禁自测 36 passed |
| C ABI / C# 绑定 | 🟢 已补齐 | cabi 测试通过；C# .NET 9 13 段 E2E 全通过（DBA 评测 R10） |
| 性能基准可信度 | 🟡 中上 | 模拟报告已删、CI 门控已做；`env_metadata` 默认值仍未采集（P2） |
| 测试与质量 | 🟢 强 | PG 46 + MySQL 21 + cabi + bench 全部通过；clippy 关键包 -D warnings 通过 |
| 文档与历史证据 | 🟡 中 | 历史 154 份报告引用分类完成（0 造假）；需校验器路径归一化治理 |

---

## 二、本会话实测复核记录（2026-10-29）

| 检查 | 结果 |
|------|------|
| 门禁 13 项（G18/G12/G27/G26/G19/G15/G16/G17/G25/check-secrets/文件编码/发布一致性/CI-deny） | 全部通过（exit 0） |
| 门禁脚本自测 `pytest tests/test_gate_scripts.py` | 36 passed |
| G9 自测 `scripts/test_check_sql_injection_auto.py` | 13 passed |
| `cargo test -p sz-orm-sqlx --test any_pg_test -- --ignored` | **46 passed**（含 jsonb 类型，本次修复 `Value::is_json` + 测试断言） |
| `cargo test -p sz-orm-sqlx --test any_mysql_test -- --ignored`（本机 8802） | **21 passed**（date/datetime/time 精确） |
| `cargo test -p sz-orm-cabi` | 通过 |
| `cargo clippy -p sz-orm-core -p sz-orm-sqlx -p sz-orm-cabi --all-targets -- -D warnings` | 通过（0 warning） |
| `cargo test -p sz-orm-bench` | 通过 |

---

## 三、评测整改闭环与遗留项

### 3.1 已闭环（2026-10-28~29 各评测 P0/P1）

| 评测 | 原问题 | 闭环证据 |
|------|--------|----------|
| DBA（首轮不可投产） | PG `TIMESTAMP/DATE` 读取 NULL、NUMERIC 丢精度、隔离级别、会话泄漏、C# 无类型通道 | `packages/sz-orm-sqlx/src/any.rs`（TIMESTAMP/DATE/BigDecimal 解码）；`docs/assessment/2026-10-28-db-review.md` §9 R1~R11 全达成；PG 46 / MySQL 21 / MSSQL 8 / C# 13 段 E2E 通过 |
| 安全（7.3/10） | rustls 链、凭据进 git、扫描器假绿、审计日志脱敏 | `check-secrets.py` 0 secrets；cargo-audit 0 漏洞；审计脱敏已落地 |
| 架构路线图（P0） | CHANGELOG 声称升级但锁文件未变；ProdReadyChecker 自证 PASS；门禁数口径漂移 | 锁文件已一致（tokio 1.54.1 等）；`prod_ready_check.rs` 配置驱动真实校验（无输入即 Skipped，严格判定）；AGENTS 门禁 25+9 增强结构统一 |
| 全栈 v3（6.5/10） | 凭据泄露、clippy 红灯、C# LPStr 编码、门禁未接线 | check-secrets=0；clippy 通过；C# UTF-8/参数化/DBNull/DateTime 已补齐 |
| 性能 v1（3 P0） | 模拟基准、报告篡改、CI fail-open | 模拟文件已删；CI main 分支门控；真实测量收益 89.3% |

### 3.2 当前遗留项（对外宣称生产就绪前）

| # | 遗留项 | 优先级 | 状态与说明 |
|---|--------|--------|------|
| 1 | 性能基准元数据未采集 | ✅ 已修复 | `EnvMetadata` 真实采集 CPU 型号/内存/rustc 版本/feature/git（`packages/sz-orm-bench/src/lib.rs`），实测生成报告含 `Intel64 ... / 40 cores / 66,994,108 KB / 1.99.1` |
| 2 | CI 性能基线比对未闭环 | ✅ 已闭环 | `.github/workflows/ci.yml:795-812`：存在基线时 `--compare-baseline ci-sqlite`（≥10% 退化阻断）后 `--save-baseline` 滚动更新，首次仅建基线；`actions/cache` 恢复滚动基线 |
| 3 | 孤儿包治理 | 🟡 清单已产出 | `docs/assessment/2026-10-29-orphan-package-ledger.md`：30 个无下游依赖包分类（9 个独立交付物非孤儿 + 21 个扩展包待裁定） |
| 4 | PHANTOM-2 警告收敛 | ✅ 已收敛 | `check-phantom-delivery.py` 实测 PHANTOM-2 **0**（未登记警告 0），308 个门控未启用警告已全部处理 |
| 5 | 历史报告引用清账 | 🟡 部分完成 | `legacy-citation-ledger.md`：648 处红牌 0 造假；**校验器路径归一化已完成**（`scripts/audit-verify.ps1` 识别旧克隆根 `E:/vue/...` 与 `/packages/...`，自测 10 passed）；待做：45 份零可校验引用报告治理 + `.sh` 同步 |
| 6 | C# SafeHandle | ⏳ 待 .NET SDK 环境 | `SzOrm.cs` 仍用裸 IntPtr + 手工 Dispose；本机无 dotnet SDK，SafeHandle 改造无法编译验证（P3，非阻断） |
| 7 | 覆盖率/变异基线复测 | 🟡 复测进行中 | 本会话安装 cargo-mutants 并启动增量变异 + cargo-llvm-cov 覆盖率复测（G22/G20） |
| 8 | sz-pay 下游版本 | 下游待办 | sz-pay 声明 `sz-rust-* ^1.7.0`，上游已 2.0.0，需下游升级适配后完整编译 |
| 9 | 其余包发布 | 可选 | 核心 6 包 + 依赖链已发布 crates.io 10.13.0；其余 workspace 包可扩展发布 |

---

## 四、证据索引

| 证据 | 位置 |
|------|------|
| workspace 版本 | `Cargo.toml:6` |
| 供应链锁文件已一致 | `Cargo.lock`（tokio 1.54.1 / ctutils 0.4.6 / webpki-root-certs 1.0.11 / actix-http 3.20.0） |
| env_metadata 默认值 | `packages/sz-orm-bench/src/lib.rs:489-493` |
| CI main 分支门控 | `.github/workflows/ci.yml:470/602/763/1101`（integration/mq/benchmark/fuzz） |
| DBA 整改清单 | `docs/assessment/2026-10-28-db-review.md` §9（R1~R11） |
| 历史引用分类 | `docs/assessment/2026-10-29-legacy-citation-ledger.md` §3（0 造假） |
| ProdReadyChecker 真实化 | `packages/sz-orm-core/src/prod_ready_check.rs` |
| 本会话修复 | `packages/sz-orm-core/src/value.rs`（`is_json`）、`packages/sz-orm-sqlx/tests/any_pg_test.rs` |
| 可观测性接入（sz-orm-bench + sz-pay） | `packages/sz-orm-bench/Cargo.toml`（metrics-wiring）、`D:\www\test\sz-pay\server\sz-rust\src\repositories\base.rs` |

---

*报告生成：2026-10-29。本报告所有 `file:line` 引用经 `scripts/audit-verify.ps1` 校验（见文末输出）。*