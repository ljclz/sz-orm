# sz-orm 现有功能验证 SPEC（old-coder 流程）

> 日期：2026-08-15
> 流程：old-coder（SPEC → RED 证明 → GAUNTLET → EVIDENCE）
> 任务：**流程验证现有每项功能**——对既有功能逐项建立"功能 → 守卫测试 → 变异证明 → 门禁证据"的验证链，产出 EVIDENCE 报告。
> 模式：自主运行（无人实时审阅），`spec approval: not obtained (autonomous run)`，置信度相应降级声明。

---

## 0. 源状态声明（验证对象）

- 仓库：`E:\vue\test\鲜视达\rust\sz-orm`，分支 main，HEAD = `44f8ebd`（v4.7.0 门禁 22/23 落地）
- **工作树含并行会话未提交改动**：74 个文件（v4.8.0 安全修复，含 4 个未跟踪 owasp 测试文件）。验证针对**当前工作树**运行，与 HEAD 的差异在 EVIDENCE 中逐门禁记录（baseline 对比用 `git worktree add /tmp/szorm-head HEAD` 干净检出）。
- 工具链：cargo 1.97.1 / rustc 1.97.1（2026-07-14）；python 3.x（门禁脚本）
- 本次验证**零修改**：只读运行门禁 + 临时变异（变异后恢复，`git diff` 验证恢复完整性），不修复、不提交（并行会话文件边界，见 security-audit-fix-status 记忆）。

## 1. 功能清单（每项 = 验证单元）

### 1.1 v4.7.0 七项需求（当前版本主线，feature gate 默认关闭）

| ID | 功能 | feature gate | 实现模块 | 守卫测试（模块内 #[test]） | 验收标准 |
|----|------|--------------|----------|---------------------------|----------|
| F1 | 消息延迟队列与优先级调度（延迟投递 + 优先级 + 定时调度） | `delayed-priority-queue`（sz-orm-queue） | `packages/sz-orm-queue/src/delayed_priority.rs` | 19 个 | 全部通过；变异 1 处关键逻辑（如优先级比较/到期判断）测试必红 |
| F2 | 迁移前向兼容检查 + 沙箱预演 + 依赖图 | `forward-compat-sandbox`（sz-orm-core） | `packages/sz-orm-core/src/forward_compat_sandbox.rs` | 19 个 | 全部通过；变异 1 处（如破坏性变更判定）测试必红 |
| F3 | 批量 COPY 方言适配 + 并行分片 + 冲突解决 | `copy-parallel-shard`（sz-orm-batch） | `packages/sz-orm-batch/src/copy_parallel_shard.rs` | 25 个 | 全部通过；变异 1 处（如冲突策略分支）测试必红 |
| F4 | 租户资源配额 + RLS 增强 + 租户审计 | `tenant-quota-rls-enhanced`（sz-orm-core） | `packages/sz-orm-core/src/tenant_quota_rls.rs` | 35 个 | 全部通过；变异 1 处（如配额比较方向）测试必红 |
| F5 | 缓存预热 + 布隆穿透防护 + singleflight 击穿防护 | `cache-warmup-protection`（sz-orm-core） | `packages/sz-orm-core/src/cache_warmup_protection.rs` | 13 个 | 全部通过；变异 1 处（如布隆命中判定/击穿去重）测试必红 |
| F6 | 异常自愈 + 根因分析 + 关联分析 | `anomaly-remediation-rca`（sz-orm-observability） | `packages/sz-orm-observability/src/anomaly_remediation_rca.rs` | 20 个 | 全部通过；变异 1 处（如置信度阈值）测试必红 |
| F7 | 多云成本对比 + 容量预测 + 自动优化 | `multicloud-cost-forecast`（sz-orm-storage） | `packages/sz-orm-storage/src/multicloud_cost_forecast.rs` | 15 个 | 全部通过；变异 1 处（如最便宜 provider 选择）测试必红 |

### 1.2 v4.6.0 七项需求（已发布基线）

| ID | 功能 | feature gate | 实现模块 |
|----|------|--------------|----------|
| F8 | 消息死信队列自动重投递 | `dlx-auto-redelivery`（sz-orm-queue） | `packages/sz-orm-queue/src/dlx.rs` |
| F9 | 迁移零停机回滚 | `zero-downtime-rollback`（sz-orm-core） | `packages/sz-orm-core/src/rollback_zero_downtime.rs` |
| F10 | 批量事务原子性 | `batch-atomic`（sz-orm-batch） | `packages/sz-orm-batch/src/atomic.rs` |
| F11 | 异常检测 | `anomaly-detection`（sz-orm-observability） | `packages/sz-orm-observability/src/anomaly.rs` |
| F12 | 存储成本分析 | `cost-analysis`（sz-orm-storage） | `packages/sz-orm-storage/src/cost.rs` |
| F13 | 连接级多租户隔离 | `connection-level-tenant`（sz-orm-core） | `packages/sz-orm-core/src/connection_tenant.rs` |
| F14 | 进程级 L1 缓存 | `process-l1-cache`（sz-orm-core） | `packages/sz-orm-core/src/process_l1_cache.rs` |

验收标准：以上模块在各自 feature 下的模块内测试全部通过（数量在 EVIDENCE 中逐模块统计）。

### 1.3 核心能力声明（README 声称，默认 feature）

| ID | 功能 | 验证方式 |
|----|------|----------|
| F15 | 连接池（自研无锁池，AtomicU32 + crossbeam ArrayQueue + Notify） | 门禁 4 全量测试 + chaos_pool 测试 |
| F16 | 查询构建（参数化 WHERE 铁律 + 防 SELECT *） | 门禁 4 + 门禁 9 SQL 注入扫描 + 门禁 16 |
| F17 | 多租户隔离（TenantContext / RLS / 列脱敏） | `tenant_concurrency.rs` + `e2e_real_db_multi_tenant.rs` |
| F18 | 迁移管理（Migration / dry-run） | `e2e_migration.rs` + `migration.rs` 模块测试 |
| F19 | L1/L2 缓存 + 缓存一致性 | `l1_cache_test.rs` + `l1_l2_db_test.rs` |
| F20 | 安全（JWT/OAuth2/MFA/RBAC，sz-orm-auth） | 门禁 21 安全攻击测试 |
| F21 | 密码学（AES-GCM/HMAC/PBKDF2，sz-orm-crypto） | 门禁 21 KAT 测试 |
| F22 | 五方言集成（MySQL/PG/SQLite/Oracle/DuckDB） | 门禁 7（--ignored 集成测试） |

### 1.4 必须 NOT 破坏（负向约束，contract clauses）

- N1 门禁 1-23 全绿（或如实记录 baseline 失败，零新增失败）
- N2 公开 API 签名不因本验证改变（本验证零修改源码）
- N3 既有测试基线不回退：`cargo test --workspace` 结果 vs HEAD 基线对比
- N4 并行会话文件边界：不触碰 74 个已修改文件与 4 个未跟踪 owasp 文件（不格式化、不修复、不提交）
- N5 无新增依赖、无环境变更（cargo/python 均已就绪，验证仅用现有工具）

## 2. RED 证明计划（证明测试非空洞）

对 F1~F7 各执行 1 次手工变异（共 7 次）：引入真实 bug（比较翻转/条件删除/边界偏移）→ 运行该 feature 的测试命令 → 记录必红输出 → `git revert` 式恢复 → `git diff` 验证恢复完整。变异点位与观察到的失败测试名逐条记入 EVIDENCE。门禁 20（cargo-mutants 子集杀率）作为工具化变异补充。

## 3. GAUNTLET 层（本项目 23 道门禁）

| 门禁 | 命令 | 记录方式 |
|------|------|----------|
| 1 fmt | `cargo fmt --all -- --check` | 退出码 + diff 文件清单 |
| 2 check | `cargo check --workspace --all-targets` | 退出码 |
| 3 clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 退出码 |
| 4 test | `cargo test --workspace` | 通过/失败计数 |
| 8 占位扫描 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` | 命中数 |
| 9 SQL 注入 | `scripts/check-sql-injection.ps1`（PowerShell） | 退出码 |
| 12 文档一致性 | `python scripts/check-doc-consistency.py` | 退出码 |
| 14 文档同步 | `python scripts/check-doc-sync.py --diff HEAD` | 退出码 |
| 15 幻影交付 | `python scripts/check-phantom-delivery.py` | PHANTOM-1/2 计数 + 接线断言 |
| 16 语义反模式 | `python scripts/check-semantic-patterns.py` | 硬/软命中 |
| 17 架构一致性 | `python scripts/check-architecture.py` | 退出码 |
| 18 度量真实 | `python scripts/check-metrics-real.py` | 退出码 |
| 19 发布一致 | `python scripts/check-publish-consistency.py` | 退出码 |
| 20 变异杀率 | `python scripts/check-mutation-coverage.py` | 杀率 % |
| 21 安全攻击 | auth security_attacks + crypto kat + core multi-tenant security_attacks | 通过计数 |
| 22 覆盖率 | `python scripts/check-coverage.py` | 行覆盖率 % |
| 23 未用依赖 | `python scripts/check-unused-deps.py` | 警告数 |

门禁 5/6/7/10/11/13 与本次验证相关性的判定：5（doc 构建）、6（audit/deny）、7（--ignored 集成需真 DB，本机有 MySQL/PG/Oracle，可选跑）、10（all-features 编译）、11（上游未修改——本仓库即上游，跳过并说明）、13（audit-verify 针对审计报告，本次 EVIDENCE 亦将跑）。未跑层在 EVIDENCE 逐条记录原因。

## 4. 已知 baseline（本 SPEC 撰写时已发现，EVIDENCE 需逐项复核）

- B1 门禁 1（fmt）：工作树 6 处 diff，全部位于并行会话未跟踪文件（owasp_a01/a02 测试文件 ×4 内 6 处）；HEAD 干净检出下待复核
- B2 门禁 15（幻影）：HEAD 与工作树均 ❌，PHANTOM-1 = 33（C 类有意保留组件，2026-08-13 审计分类），PHANTOM-2 = 147（HEAD）/ 160（工作树）；接线断言 4/4 通过。该红态为 HEAD 既有状态（44f8ebd 门禁脚本按 `n1>0` 即失败），非本次验证引入
- B3 门禁 16（语义）：通过（硬 0 / 软 4）

## 5. 产出

- `docs/assessment/2026-08-15-old-coder-verification-evidence.md`：EVIDENCE 报告（功能→测试映射、每层门禁命令+实际数字、变异证明记录、baseline 记录、可复现入口）
- 全部数字来自最后一次编辑后的 fresh run
