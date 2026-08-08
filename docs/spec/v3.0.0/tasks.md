# sz-orm v3.0.0 任务分解文档

> 版本：v3.0.0（长期目标规划）
> 基线：v2.4.0（已完成：五方言集成测试 + 性能基准 + crates.io 44 包发布 2.3.0 + sz-pay 5139 测试零回归）
> 日期：2026-08-07
> 文档定位：任务分解（可执行任务清单），对应需求规格 `spec.md`（29 条 EARS 需求，6 组）与技术设计 `design.md`（2455 行，6 大模块、7 里程碑、15 风险）
> 任务总数：7 个里程碑 / 61 个子任务，覆盖全部 29 条需求（REQ-GDB-001~005 / REQ-WASM-001~005 / REQ-FDI-001~005 / REQ-AI-001~005 / REQ-DTX-001~005 / REQ-MB-001~004）
> 工程化基线：AGENTS.md 10 道门禁 + 五维审查 + 审计合规铁律（file:line 证据）

---

## 任务规划原则

1. **垂直切割**：按业务功能分组（多后端协同 / XA 事务 / 发布产物 / WASM / 图数据库 / AI 优化器 / 集成发布），非按技术层次分组
2. **可验收**：每个子任务标注对应需求编号、验收标准（含具体命令 + 期望结果）、门禁命令，可独立判定完成
3. **原子性**：一个子任务只做一件事，标注涉及文件路径，工作量 0.5~2 天
4. **有序性**：被依赖任务在前，按 design.md §2.9.1 实现顺序（多后端协同 → XA 事务 → 发布产物 → WASM → 图数据库 → AI 优化器 → 集成发布）
5. **门禁对齐**：每个任务验收标准必须包含门禁命令（参考 AGENTS.md 10 道门禁）
6. **审计合规**：每条结论附 file:line 证据，修复后必须运行 `cargo test` 验证

---

## 1. 任务总览

### 1.1 任务统计

| 里程碑 | 名称 | 任务数 | 需求组 | 周期 |
|--------|------|--------|--------|------|
| M1 | 多后端协同文档 | 5 | REQ-MB-001~004 | 1 周 |
| M2 | XA 事务一致性 | 8 | REQ-DTX-001~005 | 3 周 |
| M3 | 发布产物 | 8 | REQ-FDI-001~005 | 2 周 |
| M4 | WASM 完善 | 8 | REQ-WASM-001~005 | 3 周 |
| M5 | 图数据库支持 | 12 | REQ-GDB-001~005 | 4 周 |
| M6 | AI 优化器 | 11 | REQ-AI-001~005 | 3 周 |
| M7 | 集成验证 + 发布 | 9 | 全部（AC-ALL-1~7） | 1 周 |
| **合计** | | **61** | **29 条需求 + 7 总体验收** | **17 周** |

### 1.2 任务依赖关系图

```mermaid
graph TD
    M1[M1: 多后端协同文档<br/>5 任务]
    M2[M2: XA 事务一致性<br/>8 任务]
    M3[M3: 发布产物<br/>8 任务]
    M4[M4: WASM 完善<br/>8 任务]
    M5[M5: 图数据库支持<br/>12 任务]
    M6[M6: AI 优化器<br/>11 任务]
    M7[M7: 集成验证 + 发布<br/>9 任务]

    M1 --> M2
    M2 --> M3
    M3 --> M4
    M4 --> M5
    M5 --> M6
    M6 --> M7

    M1 -.-> M7
    M2 -.-> M7
    M3 -.-> M7
    M4 -.-> M7
    M5 -.-> M7
    M6 -.-> M7
```

**ASCII 版本**：

```
┌─────────────────────────────┐
│ M1: 多后端协同文档 (5 任务)  │
└──────────────┬──────────────┘
               ▼
┌─────────────────────────────┐
│ M2: XA 事务一致性 (8 任务)   │
└──────────────┬──────────────┘
               ▼
┌─────────────────────────────┐
│ M3: 发布产物 (8 任务)        │
└──────────────┬──────────────┘
               ▼
┌─────────────────────────────┐
│ M4: WASM 完善 (8 任务)       │
└──────────────┬──────────────┘
               ▼
┌─────────────────────────────┐
│ M5: 图数据库支持 (12 任务)   │
└──────────────┬──────────────┘
               ▼
┌─────────────────────────────┐
│ M6: AI 优化器 (11 任务)      │
└──────────────┬──────────────┘
               ▼
┌─────────────────────────────┐
│ M7: 集成验证 + 发布 (9 任务) │
└─────────────────────────────┘
```

### 1.3 与 design.md 里程碑映射关系

| 本文档里程碑 | design.md §2.9.2 里程碑 | design.md 交付物 | 一致性 |
|-------------|------------------------|-----------------|--------|
| M1: 多后端协同文档 | M1: 多后端协同文档 | multi_backend_readiness.md + dialect_constraints.md + sz_rust_integration_example.rs | ✅ |
| M2: XA 事务一致性 | M2: XA 事务一致性 | xa.rs + recovery.rs + suspension.rs + 集成测试 | ✅ |
| M3: 发布产物 | M3: 发布产物 | build_python_wheel.ps1 + build_napi.ps1 + publish_*.ps1 + pytest/jest | ✅ |
| M4: WASM 完善 | M4: WASM 完善 | js_bindings.rs + persistence.rs + error.rs + .cargo/config.toml | ✅ |
| M5: 图数据库支持 | M5: 图数据库支持 | packages/sz-orm-graph/ + Docker Neo4j + 集成测试 | ✅ |
| M6: AI 优化器 | M6: AI 优化器 | query_plan_optimizer.rs + explain_parser.rs + sql_sanitizer.rs | ✅ |
| M7: 集成验证 + 发布 | M7: 集成验证 + 发布 | 10 道门禁 + crates.io 发布 + 下游回归 + CHANGELOG | ✅ |

---

## 2. 逐里程碑任务分解

## 里程碑 M1：多后端协同文档

### 目标
提供多后端能力就绪清单验证文档（附 file:line 证据）、方言特性支持矩阵、sz-rust 协同集成示例，证明 sz-orm 上游已满足 sz-rust P2-1 启动条件，解锁下游透明适配层实现。

### 需求覆盖
REQ-MB-001, REQ-MB-002, REQ-MB-003, REQ-MB-004

### 任务列表

#### 任务 M1.1：创建多后端能力就绪清单文档
- **描述**：在 `docs/spec/v3.0.0/multi_backend_readiness.md` 新增就绪清单文档，逐项验证 `AnyBackend` 五方言枚举、`from_dsn()` 识别、`dialect()` 映射、`AnyPool`/`UnifiedPool` 连接工厂，每项附 file:line 证据
- **输入**：`packages/sz-orm-sqlx/src/any_driver.rs:57,80,117,129`、`packages/sz-orm-sqlx/src/unified_pool.rs:48`
- **输出**：`docs/spec/v3.0.0/multi_backend_readiness.md`（含 5 项验证项 + file:line 证据 + 验证结果 PASS/FAIL）
- **验收标准**：
  - 文档含 5 项验证项（AnyBackend 枚举 / from_dsn / dialect / AnyPool / UnifiedPool）
  - 每项附 file:line 证据且文件行真实存在（`scripts/audit-verify.ps1` 通过）
  - 命令：`Test-Path docs/spec/v3.0.0/multi_backend_readiness.md` 返回 True
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`scripts/audit-verify.ps1 docs/spec/v3.0.0/multi_backend_readiness.md`

#### 任务 M1.2：创建方言特性支持矩阵文档
- **描述**：在 `docs/spec/v3.0.0/dialect_constraints.md` 新增方言特性支持矩阵，汇总各方言特性支持范围（CRUD/事务/Eager Loading/ON DUPLICATE KEY UPDATE/RETURNING/SERIAL 等），标注约束
- **输入**：`packages/sz-orm-core/src/dialect.rs`、五方言集成测试 `packages/sz-orm-core/tests/smart_eager_integration_*.rs`
- **输出**：`docs/spec/v3.0.0/dialect_constraints.md`（含特性支持矩阵表 + 方言专属特性清单 + 约束提示）
- **验收标准**：
  - 文档含特性支持矩阵（行=特性，列=五方言，值=支持/不支持/需模拟）
  - 标注 MySQL ON DUPLICATE KEY UPDATE 为 MySQL 专属
  - 标注 RETURNING 为 PG/Oracle/MSSQL 支持，MySQL 需模拟
  - 命令：`Test-Path docs/spec/v3.0.0/dialect_constraints.md` 返回 True
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：文档审查

#### 任务 M1.3：创建 sz-rust 协同集成示例
- **描述**：在 `examples/sz_rust_integration_example.rs` 新增协同示例，展示 sz-rust 透明适配层仅依赖 sz-orm 公开 API（AnyBackend/AnyPool/UnifiedPool）完成统一访问，不触碰 sz-orm 内部实现
- **输入**：`packages/sz-orm-sqlx/src/any_driver.rs`（公开 API）
- **输出**：`examples/sz_rust_integration_example.rs`（含 DSN 识别 + 连接 + CRUD 示例，仅用公开 API）
- **验收标准**：
  - 示例仅 import sz-orm 公开 API（无 internal mod 引用）
  - 命令：`cargo build --example sz_rust_integration_example` 成功
  - 命令：`cargo clippy --example sz_rust_integration_example -- -D warnings` 零警告
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`cargo build --example sz_rust_integration_example` + `cargo clippy --example sz_rust_integration_example -- -D warnings`

#### 任务 M1.4：复用 v2.4.0 等价性测试基础设施验证五方言行为一致性
- **描述**：复用 v2.4.0 已交付的 `tests/common/equivalence.rs` + `schema_builder.rs` + `smart_eager_integration_*.rs`，验证 CRUD/事务/Eager Loading 三类用例在五方言下行为一致
- **输入**：`packages/sz-orm-core/tests/common/equivalence.rs`、`packages/sz-orm-core/tests/smart_eager_integration_{mysql,pg,sqlite,oracle,mssql}.rs`
- **输出**：五方言等价性测试执行报告（复用现有测试，无新增代码）
- **验收标准**：
  - 命令：`cargo test --workspace -- --ignored` 五方言集成测试全部通过（除 MSSQL 远程不可用跳过）
  - 五方言 CRUD/事务/Eager Loading 结果等价（行数/字段/值/嵌套深度一致）
  - 不支持的方言特性有 `dialect_constraints.md` 明确标注
- **依赖**：M1.2（方言约束文档）
- **预估**：中（1 天）
- **门禁**：`cargo test --workspace -- --ignored`

#### 任务 M1.5：验证 ADR-0001 sz-orm 仓库零修改
- **描述**：确认 M1 协同交付期间 sz-orm 仓库业务代码零修改（仅新增文档与示例文件），满足 ADR-0001 铁律
- **输入**：M1.1~M1.4 交付物
- **输出**：`git diff --name-only HEAD` 输出（仅含 docs/spec/v3.0.0/*.md + examples/sz_rust_integration_example.rs）
- **验收标准**：
  - 命令：`git diff --name-only HEAD` 输出仅含新增文档/示例，无 sz-orm-core/sz-orm-sqlx 业务代码变更
  - sz-rust 透明适配层仅依赖 sz-orm 公开 API（编译通过证明）
- **依赖**：M1.1, M1.2, M1.3, M1.4
- **预估**：低（0.5 天）
- **门禁**：`git diff --name-only HEAD` 审查

---

## 里程碑 M2：XA 事务一致性

### 目标
在 sz-orm-dtx 内扩展 XA 资源管理器适配，复用现有 2PC 状态机与日志，实现跨数据库原子提交（XA PREPARE/COMMIT 直连 DB 资源管理器）、崩溃恢复、悬挂事务检测，与既有 2PC/Saga/TCC/cross_shard 模式共存。

### 需求覆盖
REQ-DTX-001, REQ-DTX-002, REQ-DTX-003, REQ-DTX-004, REQ-DTX-005

### 任务列表

#### 任务 M2.1：实现 XaResource trait 与 XaParticipant
- **描述**：在 `packages/sz-orm-dtx/src/xa.rs` 新增 `XaResource` trait（xa_prepare/xa_commit/xa_rollback/resource_id/backend）与 `XaParticipant` 结构体（持有 `Arc<dyn XaResource>` + xid + state），实现直连 DB 资源管理器（非回调式）
- **输入**：`packages/sz-orm-dtx/src/lib.rs:151,174`（TransactionState/ParticipantState）、`packages/sz-orm-sqlx/src/any_driver.rs:129`（AnyPool 连接）
- **输出**：`packages/sz-orm-dtx/src/xa.rs`（XaResource trait + XaParticipant + XaError 枚举）
- **验收标准**：
  - XaResource trait 含 xa_prepare/xa_commit/xa_rollback/resource_id/backend 五方法
  - XaParticipant 持有真实 DB 连接（非回调式）
  - XaError 含 6 变体（PrepareFailed/CommitFailed/RollbackFailed/XaNotSupported/NotFound/DatabaseError）
  - 命令：`cargo build -p sz-orm-dtx --features xa` 成功
  - 命令：`cargo clippy -p sz-orm-dtx --features xa -- -D warnings` 零警告
- **依赖**：无
- **预估**：中（1 天）
- **门禁**：`cargo build -p sz-orm-dtx --features xa` + `cargo clippy -p sz-orm-dtx --features xa -- -D warnings`

#### 任务 M2.2：实现 XaCoordinator（XA 两阶段提交协调）
- **描述**：在 `packages/sz-orm-dtx/src/xa.rs` 新增 `XaCoordinator`，复用现有 2PC 状态机（TransactionState），实现 XA 两阶段提交（Prepare→全成功→Commit / 任一失败→Rollback），各阶段写 TransactionLogStore
- **输入**：M2.1（XaResource/XaParticipant）、`packages/sz-orm-dtx/src/lib.rs:45,258`（TransactionLogStore/DistributedTransaction）
- **输出**：XaCoordinator 结构体 + xa_two_phase_commit 方法
- **验收标准**：
  - XaCoordinator 复用 TransactionState 状态机（Active→Preparing→Prepared→Committing→Committed）
  - Prepare 阶段任一失败 → 回滚已 Prepare 的 + 标记 Failed
  - Prepare/Commit/Rollback 各阶段写 TransactionLogStore
  - 命令：`cargo test -p sz-orm-dtx --features xa xa_coordinator` 通过
- **依赖**：M2.1
- **预估**：高（2 天）
- **门禁**：`cargo test -p sz-orm-dtx --features xa xa_coordinator`

#### 任务 M2.3：实现 XaCapabilityChecker（后端 XA 能力校验）
- **描述**：在 `packages/sz-orm-dtx/src/xa.rs` 新增 `XaCapabilityChecker`，检测后端是否支持 XA（MySQL/PG/Oracle/MSSQL 支持，SQLite 不支持），不支持时返回 `XaNotSupported` 错误
- **输入**：`packages/sz-orm-sqlx/src/any_driver.rs:57`（AnyBackend 枚举）
- **输出**：XaCapabilityChecker + XaCapability 枚举（Supported/NotSupported）
- **验收标准**：
  - MySQL/PG/Oracle/MSSQL → Supported
  - SQLite → NotSupported { reason: "SQLite 不支持 XA 协议" }
  - 尝试注册 SQLite 为 XA 参与者 → 返回 XaError::XaNotSupported，事务不进入 Prepare
  - 命令：`cargo test -p sz-orm-dtx --features xa xa_capability` 通过
- **依赖**：M2.1
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-dtx --features xa xa_capability`

#### 任务 M2.4：实现 XaRecoveryCoordinator（崩溃恢复）
- **描述**：在 `packages/sz-orm-dtx/src/recovery.rs` 新增 `XaRecoveryCoordinator`，启动时调用 `log_store.read_pending()` 扫描未决事务，按日志状态执行补偿（Prepared→Commit / Preparing→Rollback / Committing→检查补全）
- **输入**：M2.2（XaCoordinator）、`packages/sz-orm-dtx/src/lib.rs:45`（TransactionLogStore.read_pending）
- **输出**：`packages/sz-orm-dtx/src/recovery.rs`（XaRecoveryCoordinator + RecoveryStrategy 枚举 + RecoveryResult）
- **验收标准**：
  - 启动时扫描 read_pending() 未决事务
  - Prepared → 继续 Commit；Preparing → Rollback；Committing → 检查补全
  - 所有未决事务收敛到终态（Committed/RolledBack），无悬挂残留
  - 命令：`cargo test -p sz-orm-dtx --features xa xa_recovery` 通过
- **依赖**：M2.2
- **预估**：高（2 天）
- **门禁**：`cargo test -p sz-orm-dtx --features xa xa_recovery`

#### 任务 M2.5：实现 SuspensionDetector（悬挂事务检测）
- **描述**：在 `packages/sz-orm-dtx/src/suspension.rs` 新增 `SuspensionDetector`（tokio 后台任务，周期扫描超时事务）+ `SuspensionConfig`（timeout 默认 30s + policy + check_interval）+ `SuspensionPolicy`（Commit/Rollback）
- **输入**：M2.2（XaCoordinator）、`packages/sz-orm-dtx/src/lib.rs:45`（TransactionLogStore）
- **输出**：`packages/sz-orm-dtx/src/suspension.rs`（SuspensionDetector + SuspensionConfig + SuspensionPolicy + SuspendedTransaction）
- **验收标准**：
  - 后台定时扫描（默认 5s 间隔）Prepare 后超时未决定的事务
  - 超时（默认 30s）→ 标记悬挂 + 按策略处理（Commit/Rollback）
  - 收敛到终态，写日志 Suspended-Resolved
  - 优雅关闭时停止检测（CancellationToken 控制）
  - 命令：`cargo test -p sz-orm-dtx --features xa suspension` 通过
- **依赖**：M2.2
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-dtx --features xa suspension`

#### 任务 M2.6：验证 XA 与既有 2PC/Saga/TCC/cross_shard 共存
- **描述**：验证 XA 事务通过 `DtxManager` 统一管理，与既有 2PC 回调式/Saga/TCC/cross_shard 模式并存，不破坏既有 API 签名
- **输入**：M2.1~M2.5、`packages/sz-orm-dtx/src/lib.rs:420`（DtxManager）、`packages/sz-orm-dtx/src/{saga,tcc,cross_shard}.rs`
- **输出**：共存验证测试
- **验收标准**：
  - XA 事务与既有 2PC 回调事务并行运行，独立协调、互不干扰
  - 既有 dtx API 签名不变（无 Breaking Change）
  - 命令：`cargo test -p sz-orm-dtx --features xa coexistence` 通过
  - 命令：`cargo test -p sz-orm-dtx`（默认 feature，无 xa）既有测试全部通过
- **依赖**：M2.1, M2.2, M2.3, M2.4, M2.5
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-dtx --features xa coexistence` + `cargo test -p sz-orm-dtx`

#### 任务 M2.7：编写 XA 集成测试（两库 XA 真实提交 + 崩溃恢复 + 悬挂超时）
- **描述**：在 `packages/sz-orm-dtx/tests/xa_integration.rs` 新增集成测试，覆盖两库 XA 真实提交（MySQL+PG）、Prepare 失败全局回滚、协调者崩溃后恢复、悬挂超时处理，`#[ignore]` 标注真实服务测试
- **输入**：M2.1~M2.6、本机 MySQL（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）+ PostgreSQL（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）
- **输出**：`packages/sz-orm-dtx/tests/xa_integration.rs`（含 5+ 测试函数）
- **验收标准**：
  - 两库各写一条数据并提交 XA 事务 → 两库同时提交成功
  - 制造某库 Prepare 失败 → 两库均无数据残留（全回滚）
  - Prepare 后模拟协调者崩溃 → 重启恢复收敛终态
  - 超时未决定 → 标记悬挂 + 按策略处理
  - 命令：`cargo test -p sz-orm-dtx --features xa --test xa_integration -- --ignored` 全部通过
- **依赖**：M2.6
- **预估**：高（2 天）
- **门禁**：`cargo test -p sz-orm-dtx --features xa --test xa_integration -- --ignored`

#### 任务 M2.8：配置 xa feature gate 与 Cargo.toml 依赖
- **描述**：在 `packages/sz-orm-dtx/Cargo.toml` 新增 `xa` feature gate（默认关闭），新增依赖 sz-orm-sqlx/async-trait/chrono（feature 隔离），修改 `src/lib.rs` 导出新模块
- **输入**：M2.1~M2.7
- **输出**：`packages/sz-orm-dtx/Cargo.toml`（新增 xa feature + 依赖）、`packages/sz-orm-dtx/src/lib.rs`（导出 xa/recovery/suspension 模块）
- **验收标准**：
  - `xa` feature 默认关闭，默认 feature 不引入额外依赖
  - 命令：`cargo build -p sz-orm-dtx`（默认 feature）成功，无 sz-orm-sqlx 依赖引入
  - 命令：`cargo build -p sz-orm-dtx --features xa` 成功
  - 命令：`cargo check --workspace --all-targets --all-features` 全组合编译通过
- **依赖**：M2.7
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-dtx` + `cargo build -p sz-orm-dtx --features xa` + `cargo check --workspace --all-targets --all-features`

---

## 里程碑 M3：发布产物

### 目标
补齐 sz-orm-python（PyO3/maturin）与 sz-orm-js（napi-rs）的构建、跨平台打包、发布流水线，产出可安装的 PyPI wheel 与 npm 包，并验证绑定层功能与 sz-orm-core 行为等价。

### 需求覆盖
REQ-FDI-001, REQ-FDI-002, REQ-FDI-003, REQ-FDI-004, REQ-FDI-005

### 任务列表

#### 任务 M3.1：实现 Python wheel 构建脚本
- **描述**：在 `scripts/build_python_wheel.ps1` 新增构建脚本，使用 maturin 三平台交叉编译（linux-x64/win32-x64/darwin-x64），产出 .whl 制品
- **输入**：`packages/sz-orm-python/pyproject.toml`、`packages/sz-orm-python/src/lib.rs`
- **输出**：`scripts/build_python_wheel.ps1` + 三平台 .whl 制品
- **验收标准**：
  - 三平台各产出 .whl 文件
  - 干净 venv `pip install <wheel>` + `python -c "import sz_orm"` 成功
  - 命令：`.\scripts\build_python_wheel.ps1` 三平台构建成功
- **依赖**：无
- **预估**：中（1 天）
- **门禁**：`.\scripts\build_python_wheel.ps1`

#### 任务 M3.2：实现 npm 包构建脚本
- **描述**：在 `scripts/build_napi.ps1` 新增构建脚本，使用 napi-rs 三平台构建（linux-x64-gnu/win32-x64-msvc/darwin-x64），产出 .node 二进制 + index.d.ts
- **输入**：`packages/sz-orm-js/package.json`、`packages/sz-orm-js/src/`
- **输出**：`scripts/build_napi.ps1` + 三平台 .node 二进制 + index.d.ts
- **验收标准**：
  - 三平台各产出 .node 文件 + index.d.ts
  - `npm install @sz-orm/core` + `require('@sz-orm/core')` 成功
  - 命令：`.\scripts\build_napi.ps1` 三平台构建成功
- **依赖**：无
- **预估**：中（1 天）
- **门禁**：`.\scripts\build_napi.ps1`

#### 任务 M3.3：编写 Python pytest 等价性测试套件
- **描述**：在 `packages/sz-orm-python/tests/` 新增 pytest 测试套件，覆盖 CRUD/事务/Eager Loading/异步查询，断言绑定层与 sz-orm-core 行为一致（参数化路径）
- **输入**：`packages/sz-orm-python/src/lib.rs`（PyModel/PyQueryBuilder/PyPool/PyTransaction）
- **输出**：`packages/sz-orm-python/tests/test_equivalence.py` + `test_async.py`
- **验收标准**：
  - 测试覆盖 CRUD（create/read/update/delete）+ 事务（commit/rollback）+ 异步（asyncio）
  - 断言绑定层使用参数化绑定（非裸 SQL）
  - 命令：`pytest packages/sz-orm-python/tests/` 全部通过
- **依赖**：M3.1
- **预估**：中（1.5 天）
- **门禁**：`pytest packages/sz-orm-python/tests/`

#### 任务 M3.4：编写 JS jest 等价性测试套件
- **描述**：在 `packages/sz-orm-js/tests/` 新增 jest 测试套件，覆盖 CRUD/事务/Eager Loading/异步查询，断言绑定层与 sz-orm-core 行为一致
- **输入**：`packages/sz-orm-js/src/`（model/query/pool/transaction 模块）
- **输出**：`packages/sz-orm-js/tests/equivalence.test.js` + `async.test.js`
- **验收标准**：
  - 测试覆盖 CRUD + 事务 + 异步（async/await）
  - 断言绑定层使用参数化绑定
  - 命令：`npx jest packages/sz-orm-js/tests/` 全部通过
- **依赖**：M3.2
- **预估**：中（1.5 天）
- **门禁**：`npx jest packages/sz-orm-js/tests/`

#### 任务 M3.5：实现 PyPI 发布脚本
- **描述**：在 `scripts/publish_pypi.ps1` 新增发布脚本，使用 maturin publish 发布 wheel 到 PyPI，校验后发布
- **输入**：M3.1（.whl 制品）、M3.3（pytest 通过）
- **输出**：`scripts/publish_pypi.ps1`
- **验收标准**：
  - 发布前校验 pytest 全部通过（未验证阻断发布）
  - PyPI token 通过环境变量 `PYPI_TOKEN` 传入，不硬编码
  - 命令：`.\scripts\publish_pypi.ps1 -DryRun` 干跑成功
- **依赖**：M3.1, M3.3
- **预估**：低（0.5 天）
- **门禁**：`.\scripts\publish_pypi.ps1 -DryRun`

#### 任务 M3.6：实现 npm 发布脚本
- **描述**：在 `scripts/publish_npm.ps1` 新增发布脚本，发布主包 @sz-orm/core + 平台子包，校验平台矩阵完整性后发布
- **输入**：M3.2（.node 二进制）、M3.4（jest 通过）
- **输出**：`scripts/publish_npm.ps1`
- **验收标准**：
  - 发布前校验三平台 .node 二进制完整 + jest 全部通过
  - 缺失平台 → 阻断发布 + 输出缺失平台列表
  - npm token 通过环境变量 `NPM_TOKEN` 传入
  - 命令：`.\scripts\publish_npm.ps1 -DryRun` 干跑成功
- **依赖**：M3.2, M3.4
- **预估**：低（0.5 天）
- **门禁**：`.\scripts\publish_npm.ps1 -DryRun`

#### 任务 M3.7：实现绑定层验证脚本（阻断未验证发布）
- **描述**：在 `scripts/verify_bindings.ps1` 新增验证脚本，执行 pytest + jest + 三平台加载测试，任一失败阻断发布
- **输入**：M3.3（pytest）、M3.4（jest）
- **输出**：`scripts/verify_bindings.ps1`
- **验收标准**：
  - 执行 pytest + jest，任一失败 → exit 1 + 输出失败用例明细
  - 三平台加载验证（pip install / npm install）
  - 命令：`.\scripts\verify_bindings.ps1` 全部通过时 exit 0
- **依赖**：M3.3, M3.4
- **预估**：低（0.5 天）
- **门禁**：`.\scripts\verify_bindings.ps1`

#### 任务 M3.8：配置 CI 矩阵（GitHub Actions 三平台并行构建）
- **描述**：在 `.github/workflows/bindings.yml` 新增 CI 矩阵配置，三平台（linux/win32/darwin x64）并行构建 + 测试 + 缓存 Cargo 编译产物
- **输入**：M3.1~M3.7
- **输出**：`.github/workflows/bindings.yml`
- **验收标准**：
  - 三平台并行构建矩阵
  - 缓存 Cargo 编译产物加速 CI
  - 任一平台失败 → CI 失败
  - 命令：CI 配置 YAML 语法校验通过
- **依赖**：M3.7
- **预估**：中（1 天）
- **门禁**：YAML 语法校验 + CI dry-run

---

## 里程碑 M4：WASM 完善

### 目标
在既有 sz-orm-wasm 包内扩展，补齐浏览器端能力：wasm32 目标编译、wasm-bindgen JS 互操作层、IndexedDB 持久化与恢复，复用现有 WasmDatabase/advanced 沙箱能力。

### 需求覆盖
REQ-WASM-001, REQ-WASM-002, REQ-WASM-003, REQ-WASM-004, REQ-WASM-005

### 任务列表

#### 任务 M4.1：配置 wasm32-unknown-unknown 编译目标
- **描述**：在 `.cargo/config.toml` 新增 wasm32 target 配置，验证 sz-orm-wasm 可通过 `wasm32-unknown-unknown` 目标编译
- **输入**：`packages/sz-orm-wasm/Cargo.toml`
- **输出**：`.cargo/config.toml`（wasm32 target 配置）
- **验收标准**：
  - 命令：`cargo build --target wasm32-unknown-unknown -p sz-orm-wasm` 成功
  - 产出 .wasm 文件
  - 产物 gzip 体积 ≤ 1MB（`wasm-opt` 优化后）
- **依赖**：无
- **预估**：中（1 天）
- **门禁**：`cargo build --target wasm32-unknown-unknown -p sz-orm-wasm`

#### 任务 M4.2：实现 js_bindings 模块（wasm-bindgen 导出）
- **描述**：在 `packages/sz-orm-wasm/src/js_bindings.rs` 新增 `#[wasm_bindgen]` 导出层，包装 WasmDatabase 方法为 JS 可调用（JsWasmDatabase：create_table/insert/query/update/delete），feature gate "js"
- **输入**：`packages/sz-orm-wasm/src/lib.rs:55`（WasmDatabase）
- **输出**：`packages/sz-orm-wasm/src/js_bindings.rs`（JsWasmDatabase + JsQueryResult）
- **验收标准**：
  - JsWasmDatabase 导出 create_table/insert/query/update/delete 五方法
  - wasm-bindgen 生成 .d.ts 覆盖全部导出函数签名
  - 命令：`cargo build --target wasm32-unknown-unknown -p sz-orm-wasm --features js` 成功
  - 命令：`wasm-bindgen` 生成绑定 + .d.ts
- **依赖**：M4.1
- **预估**：中（1.5 天）
- **门禁**：`cargo build --target wasm32-unknown-unknown -p sz-orm-wasm --features js`

#### 任务 M4.3：实现 persistence 模块（IndexedDB 持久化与恢复）
- **描述**：在 `packages/sz-orm-wasm/src/persistence.rs` 新增 `WasmPersistence` trait + `IndexedDbStore` 实现（通过 web-sys 调用 IndexedDB API）+ `PersistenceConfig`（db_name/storage_version/store_name），feature gate "persistence"
- **输入**：`packages/sz-orm-wasm/src/lib.rs:55`（WasmDatabase）
- **输出**：`packages/sz-orm-wasm/src/persistence.rs`（WasmPersistence trait + IndexedDbStore + PersistenceConfig）
- **验收标准**：
  - persist() 检查 IndexedDB 可用性 → 不可用返回 PersistenceError::Unavailable
  - persist() 事务级写入（一次持久化一批变更）
  - restore() 版本校验 → 不匹配返回 RestoreError
  - 命令：`cargo build --target wasm32-unknown-unknown -p sz-orm-wasm --features persistence` 成功
- **依赖**：M4.1
- **预估**：高（2 天）
- **门禁**：`cargo build --target wasm32-unknown-unknown -p sz-orm-wasm --features persistence`

#### 任务 M4.4：实现 error 模块（WasmPersistenceError）
- **描述**：在 `packages/sz-orm-wasm/src/error.rs` 新增 `WasmPersistenceError` 枚举（Unavailable/RestoreError/IndexedDbError/SerializationError）
- **输入**：M4.3
- **输出**：`packages/sz-orm-wasm/src/error.rs`
- **验收标准**：
  - WasmPersistenceError 含 4 变体
  - thiserror 派生，实现 std::error::Error
  - 命令：`cargo build -p sz-orm-wasm --features persistence` 成功
- **依赖**：M4.3
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-wasm --features persistence`

#### 任务 M4.5：验证内存资源限制（复用 advanced 沙箱）
- **描述**：验证 WASM 内存数据库达到配置上限时通过既有 `advanced` 沙箱能力（LimitedWasmDatabase + MemoryConfig）拒绝进一步写入，返回 MemoryLimitError，禁止无界内存增长
- **输入**：`packages/sz-orm-wasm/src/advanced.rs:33`（MemoryConfig/LimitedWasmDatabase/MemoryLimitError）
- **输出**：内存限制验证测试
- **验收标准**：
  - 内存使用达到上限后继续写入 → 写入被拒绝 + 错误 MemoryLimitError
  - 内存占用不再增长
  - 无 panic
  - 命令：`cargo test -p sz-orm-wasm memory_limit` 通过
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-wasm memory_limit`

#### 任务 M4.6：验证持久化不可用明确报告（非静默降级）
- **描述**：验证在不支持 IndexedDB 或 WASM 的环境（如纯内存 Node 测试）中运行持久化时，系统明确报告"持久化不可用"状态，禁止静默丢数据
- **输入**：M4.3, M4.4
- **输出**：不可用报告验证测试
- **验收标准**：
  - 无 IndexedDB 环境触发 persist() → 返回 Err(PersistenceError::Unavailable)
  - 数据仍保留在内存可读
  - 命令：`cargo test -p sz-orm-wasm persistence_unavailable` 通过
- **依赖**：M4.3, M4.4
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-wasm persistence_unavailable`

#### 任务 M4.7：编写 wasm-bindgen-test 测试套件
- **描述**：在 `packages/sz-orm-wasm/tests/` 新增 wasm-bindgen-test 测试，覆盖 JS 调用建表/增删改查 + persist/restore 链路，headless Chrome 执行
- **输入**：M4.2, M4.3
- **输出**：`packages/sz-orm-wasm/tests/js_bindings_test.rs`（wasm_bindgen_test 标注）
- **验收标准**：
  - 测试 JS 调用 create_table/insert/query 工作
  - 测试 persist → 重载 → restore 数据一致
  - 命令：`wasm-pack test --headless --chrome` 全部通过
- **依赖**：M4.2, M4.3
- **预估**：中（1.5 天）
- **门禁**：`wasm-pack test --headless --chrome`

#### 任务 M4.8：验证 WASM 产物 gzip 体积 ≤ 1MB
- **描述**：使用 `wasm-opt` 优化 + `twiggy` 分析体积 + gzip 体积断言脚本，验证产物 gzip 后体积 ≤ 1MB
- **输入**：M4.1（.wasm 产物）
- **输出**：体积断言脚本 + 体积报告
- **验收标准**：
  - `wasm-opt -Oz` 优化后 gzip 体积 ≤ 1MB
  - 体积超标 → 断言失败 + 输出体积明细
  - 命令：gzip 体积断言通过（≤ 1048576 字节）
- **依赖**：M4.1, M4.2, M4.3
- **预估**：低（0.5 天）
- **门禁**：gzip 体积断言脚本

---

## 里程碑 M5：图数据库支持

### 目标
新增独立包 sz-orm-graph，提供 Neo4j 图数据库的连接、参数化 Cypher 查询、结果类型化映射、声明式建模能力，不触碰 sz-orm-core/sz-orm-sqlx 既有 API。

### 需求覆盖
REQ-GDB-001, REQ-GDB-002, REQ-GDB-003, REQ-GDB-004, REQ-GDB-005

### 任务列表

#### 任务 M5.1：创建 sz-orm-graph 新包骨架
- **描述**：在 `packages/sz-orm-graph/` 新增包骨架（Cargo.toml + src/lib.rs + 模块结构 connection/query/model/mapping/error/validator），注册 workspace 成员
- **输入**：`Cargo.toml:2`（workspace members）
- **输出**：`packages/sz-orm-graph/Cargo.toml` + `packages/sz-orm-graph/src/lib.rs` + 模块文件
- **验收标准**：
  - 包名 sz-orm-graph，版本 0.1.0，edition 2021，rust-version 1.81
  - workspace members 新增 `packages/sz-orm-graph`
  - 命令：`cargo build -p sz-orm-graph` 成功
  - 命令：`cargo clippy -p sz-orm-graph -- -D warnings` 零警告
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-graph` + `cargo clippy -p sz-orm-graph -- -D warnings`

#### 任务 M5.2：实现 connection 模块（Bolt 协议连接）
- **描述**：在 `packages/sz-orm-graph/src/connection.rs` 新增 GraphConfig（Bolt DSN + 超时 + 池配置）+ GraphConnection（Bolt 连接句柄）+ GraphPool（连接池，复用 sz-orm-core Pool 抽象模式）
- **输入**：`packages/sz-orm-core/src/pool.rs`（Pool 抽象模式参考）、neo4j 驱动 crate（neo4rs）
- **输出**：`packages/sz-orm-graph/src/connection.rs`
- **验收标准**：
  - GraphConfig 含 dsn/connect_timeout_secs/query_timeout_secs/max_pool_size
  - GraphPool 复用 Pool 抽象模式（非类型依赖）
  - 连接错误 DSN 脱敏（不泄露密码）
  - 命令：`cargo build -p sz-orm-graph` 成功
- **依赖**：M5.1
- **预估**：中（1.5 天）
- **门禁**：`cargo build -p sz-orm-graph`

#### 任务 M5.3：实现 query 模块（Cypher 查询构造与执行）
- **描述**：在 `packages/sz-orm-graph/src/query.rs` 新增 CypherQuery（Cypher 文本 + 参数 Map）+ CypherQueryBuilder（链式构造）+ GraphResult（Node/Relationship/Scalar/Path 枚举）
- **输入**：M5.2（GraphConnection）
- **输出**：`packages/sz-orm-graph/src/query.rs`
- **验收标准**：
  - CypherQuery 含 cypher 文本 + parameters HashMap
  - GraphResult 含 Node/Relationship/Scalar/Path 四变体
  - GraphNode 含 id/labels/properties
  - GraphRelationship 含 id/rel_type/start_node_id/end_node_id/properties
  - 命令：`cargo build -p sz-orm-graph` 成功
- **依赖**：M5.2
- **预估**：中（1.5 天）
- **门禁**：`cargo build -p sz-orm-graph`

#### 任务 M5.4：实现 validator 模块（参数化校验 + SQL 透传拒绝）
- **描述**：在 `packages/sz-orm-graph/src/validator.rs` 新增 CypherValidator，强制参数化校验（`$param` 占位符）+ SQL 透传拒绝（检测 SELECT/INSERT/UPDATE/DELETE/CREATE TABLE/DROP TABLE 关键字）
- **输入**：M5.3（CypherQuery）
- **输出**：`packages/sz-orm-graph/src/validator.rs`
- **验收标准**：
  - 含 SQL 关键字的查询 → 返回 GraphError::SqlNotSupported
  - 含字面量拼接的查询 → 返回 GraphError::ParameterizationError
  - 注入载荷作为参数传入时被当作字面量
  - 命令：`cargo test -p sz-orm-graph cypher_validator` 通过
- **依赖**：M5.3
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-graph cypher_validator`

#### 任务 M5.5：实现 model 模块（声明式建模）
- **描述**：在 `packages/sz-orm-graph/src/model.rs` 新增 GraphNodeModel（标签 + 属性定义）+ GraphRelationModel（类型 + 方向 + 属性）+ GraphPropertyDef + RelationDirection + GraphValueType
- **输入**：M5.3（GraphResult）
- **输出**：`packages/sz-orm-graph/src/model.rs`
- **验收标准**：
  - GraphNodeModel 含 label + properties Vec
  - GraphRelationModel 含 rel_type + direction + from_label + to_label + properties
  - 模型可用于查询构造与结果映射，无需手写结构体字段映射
  - 命令：`cargo build -p sz-orm-graph` 成功
- **依赖**：M5.3
- **预估**：中（1 天）
- **门禁**：`cargo build -p sz-orm-graph`

#### 任务 M5.6：实现 mapping 模块（结果类型化映射）
- **描述**：在 `packages/sz-orm-graph/src/mapping.rs` 新增 ResultMapper（GraphResult → 用户结构）+ NodeMapper + RelationMapper，支持 serde 反序列化到 `#[derive(Deserialize)]` 用户结构
- **输入**：M5.3（GraphResult）
- **输出**：`packages/sz-orm-graph/src/mapping.rs`
- **验收标准**：
  - 节点 → Map<String, Value>，关系 → 含方向与类型，标量 → Value 变体
  - serde 反序列化到用户结构成功
  - 反序列化失败 → GraphError::MappingError 附缺失字段/类型差异明细
  - 命令：`cargo test -p sz-orm-graph result_mapper` 通过
- **依赖**：M5.3
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-graph result_mapper`

#### 任务 M5.7：实现 error 模块（GraphError）
- **描述**：在 `packages/sz-orm-graph/src/error.rs` 新增 GraphError 枚举（ConnectionError/QueryError/MappingError/SqlNotSupported/ParameterizationError/DriverError），DSN 脱敏
- **输入**：M5.2, M5.3, M5.4, M5.6
- **输出**：`packages/sz-orm-graph/src/error.rs`
- **验收标准**：
  - GraphError 含 6 变体
  - thiserror 派生，实现 std::error::Error
  - ConnectionError DSN 脱敏（不泄露密码）
  - 命令：`cargo build -p sz-orm-graph` 成功
- **依赖**：M5.6
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-graph`

#### 任务 M5.8：配置 Docker Neo4j 测试环境
- **描述**：在 `docker-compose.neo4j.yml` 新增 Neo4j 测试环境配置（neo4j:5-community + Bolt 7687 + HTTP 7474 + 测试凭证 neo4j/test123）
- **输入**：无
- **输出**：`docker-compose.neo4j.yml`
- **验收标准**：
  - Neo4j 容器启动成功
  - Bolt 协议端口 7687 可达
  - 测试连接 `neo4j://neo4j:test123@127.0.0.1:7687` 成功
  - 命令：`docker-compose -f docker-compose.neo4j.yml up -d` 成功
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`docker-compose -f docker-compose.neo4j.yml up -d`

#### 任务 M5.9：编写单元测试（CypherValidator/ResultMapper/GraphError）
- **描述**：在 `packages/sz-orm-graph/src/` 各模块新增单元测试，覆盖 CypherValidator（SQL 拒绝/参数化校验）、ResultMapper（类型映射）、GraphError（DSN 脱敏）
- **输入**：M5.4, M5.6, M5.7
- **输出**：各模块 `#[cfg(test)] mod tests`
- **验收标准**：
  - CypherValidator SQL 拒绝测试通过
  - ResultMapper 类型映射测试通过
  - GraphError DSN 脱敏测试通过
  - 命令：`cargo test -p sz-orm-graph` 全部通过
- **依赖**：M5.7
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-graph`

#### 任务 M5.10：编写集成测试（Neo4j 真实连接 + Cypher 查询 + 结果映射）
- **描述**：在 `packages/sz-orm-graph/tests/neo4j_integration.rs` 新增集成测试，覆盖 Neo4j 真实连接 + 参数化 Cypher 查询 + 结果类型化映射 + 声明式建模，`#[ignore]` 标注
- **输入**：M5.2~M5.7, M5.8（Neo4j Docker）
- **输出**：`packages/sz-orm-graph/tests/neo4j_integration.rs`
- **验收标准**：
  - Neo4j 连接成功，返回可复用连接句柄
  - 参数化 Cypher 查询执行成功，注入载荷被当作字面量
  - 节点/关系/标量结果类型化映射 + serde 反序列化成功
  - 图接口拒绝 SQL 输入，透传零请求
  - 命令：`cargo test -p sz-orm-graph --test neo4j_integration -- --ignored` 全部通过
- **依赖**：M5.8, M5.9
- **预估**：高（2 天）
- **门禁**：`cargo test -p sz-orm-graph --test neo4j_integration -- --ignored`

#### 任务 M5.11：性能测试（1000 节点图 P95 ≤ 500ms）
- **描述**：在 `packages/sz-orm-graph/tests/performance.rs` 新增性能测试，1000 节点图数据集，断言单次 Cypher 查询端到端 P95 延迟 ≤ 500ms
- **输入**：M5.10
- **输出**：`packages/sz-orm-graph/tests/performance.rs`
- **验收标准**：
  - 1000 节点图数据集构造成功
  - 单次 Cypher 查询 P95 延迟 ≤ 500ms
  - 超标 → 断言失败 + 输出实际 P95 值
  - 命令：`cargo test -p sz-orm-graph --test performance -- --ignored` 通过
- **依赖**：M5.10
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-graph --test performance -- --ignored`

#### 任务 M5.12：注册 workspace 成员 + 纳入发布拓扑
- **描述**：在 `Cargo.toml` workspace members 新增 `packages/sz-orm-graph`，在 `scripts/compute_topology.ps1` 拓扑排序中纳入新包（无下游依赖，可早期发布）
- **输入**：M5.1
- **输出**：`Cargo.toml`（members 新增）+ `scripts/compute_topology.ps1`（拓扑序含 sz-orm-graph）
- **验收标准**：
  - workspace members 含 `packages/sz-orm-graph`
  - 拓扑序输出含 sz-orm-graph，且其所有 sz-orm-* 依赖在其之前
  - 命令：`cargo check --workspace` 成功
  - 命令：`.\scripts\compute_topology.ps1` 输出含 sz-orm-graph
- **依赖**：M5.11
- **预估**：低（0.5 天）
- **门禁**：`cargo check --workspace` + `.\scripts\compute_topology.ps1`

---

## 里程碑 M6：AI 优化器

### 目标
在 sz-orm-ai 内新增"LLM 查询计划优化建议"能力，与现有规则型 QueryOptimizer 并存互补：调用 LLM 服务生成结构化优化建议，解析 EXPLAIN 计划纳入建议上下文，合并规则与 LLM 建议并标注来源，未配置 LLM 时自动降级纯规则引擎。

### 需求覆盖
REQ-AI-001, REQ-AI-002, REQ-AI-003, REQ-AI-004, REQ-AI-005

### 任务列表

#### 任务 M6.1：实现 HintSource 枚举与 UnifiedOptimizationHint
- **描述**：在 `packages/sz-orm-ai/src/query_plan_optimizer.rs` 新增 HintSource 枚举（Rule/Llm { model }）+ UnifiedOptimizationHint（title/description/severity/suggested_sql/source），扩展现有 QueryOptimizationHint 增加 source/model 字段（新类型，非修改现有，避免 Breaking Change）
- **输入**：`packages/sz-orm-ai/src/nl2sql.rs:1091`（QueryOptimizationHint）
- **输出**：`packages/sz-orm-ai/src/query_plan_optimizer.rs`（HintSource + UnifiedOptimizationHint + UnifiedQueryAnalysis）
- **验收标准**：
  - HintSource 含 Rule + Llm { model: String } 两变体
  - UnifiedOptimizationHint 含 source: HintSource 字段
  - 现有 QueryOptimizationHint 不变（无 Breaking Change）
  - 命令：`cargo build -p sz-orm-ai --features llm-optimizer` 成功
- **依赖**：无
- **预估**：中（1 天）
- **门禁**：`cargo build -p sz-orm-ai --features llm-optimizer`

#### 任务 M6.2：实现 OptimizerConfig（LLM 可配置性）
- **描述**：在 `packages/sz-orm-ai/src/query_plan_optimizer.rs` 新增 OptimizerConfig（api_key/api_base/model/timeout_secs/max_tokens/enable_llm），Default 时 api_key=None + enable_llm=false（降级纯规则）
- **输入**：M6.1
- **输出**：OptimizerConfig 结构体 + Default impl
- **验收标准**：
  - OptimizerConfig 含 api_key/api_base/model/timeout_secs/max_tokens/enable_llm 六字段
  - Default: api_key=None, enable_llm=false, timeout_secs=10, max_tokens=2000
  - 命令：`cargo test -p sz-orm-ai --features llm-optimizer optimizer_config` 通过
- **依赖**：M6.1
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-ai --features llm-optimizer optimizer_config`

#### 任务 M6.3：实现 ExplainPlanParser trait + 各方言实现
- **描述**：在 `packages/sz-orm-ai/src/explain_parser.rs` 新增 ExplainPlanParser trait（parse/dialect）+ ExplainSignal 枚举（FullTableScan/MissingIndex/UsingTempTable/UsingFilesort/IndexScan）+ 各方言实现（MySqlExplainParser/PgExplainParser/SqliteExplainParser/OracleExplainParser/MssqlExplainParser）+ ExplainParseError
- **输入**：`packages/sz-orm-ai/src/nl2sql.rs`（SchemaContext）
- **输出**：`packages/sz-orm-ai/src/explain_parser.rs`
- **验收标准**：
  - ExplainPlanParser trait 含 parse + dialect 两方法
  - ExplainSignal 含 5 变体（FullTableScan/MissingIndex/UsingTempTable/UsingFilesort/IndexScan）
  - 五方言各一实现
  - 解析失败返回 ExplainParseError（非静默忽略）
  - 命令：`cargo test -p sz-orm-ai --features llm-optimizer explain_parser` 通过
- **依赖**：无
- **预估**：高（2 天）
- **门禁**：`cargo test -p sz-orm-ai --features llm-optimizer explain_parser`

#### 任务 M6.4：实现 LlmOptimizer（LLM 建议引擎）
- **描述**：在 `packages/sz-orm-ai/src/query_plan_optimizer.rs` 新增 LlmOptimizer，调用 OpenAI 兼容 API 生成结构化优化建议，构造提示词（含 SQL + EXPLAIN 信号 + schema）→ 调用 LLM API → 解析结构化 JSON 响应 → 校验合法性
- **输入**：M6.1, M6.2, M6.3、`packages/sz-orm-ai/src/real_embedding.rs`（reqwest 客户端模式复用）
- **输出**：LlmOptimizer 结构体 + request 方法
- **验收标准**：
  - 构造提示词含 SQL + EXPLAIN 信号 + schema
  - 调用 OpenAI 兼容 API
  - 解析结构化 JSON 响应 → UnifiedOptimizationHint 列表
  - 非法 JSON/字段缺失 → 丢弃非法建议保留合法的 + 记录解析失败日志
  - 命令：`cargo build -p sz-orm-ai --features llm-optimizer` 成功
- **依赖**：M6.1, M6.2, M6.3
- **预估**：高（2 天）
- **门禁**：`cargo build -p sz-orm-ai --features llm-optimizer`

#### 任务 M6.5：实现 UnifiedQueryOptimizer（合并规则 + LLM 建议）
- **描述**：在 `packages/sz-orm-ai/src/query_plan_optimizer.rs` 新增 UnifiedQueryOptimizer（持有 rule_optimizer + llm_optimizer + config），实现 optimize 方法：规则分析（离线）→ EXPLAIN 解析 → LLM 建议（可选）→ 合并建议（标注来源）
- **输入**：M6.1~M6.4、`packages/sz-orm-ai/src/nl2sql.rs:1190`（QueryOptimizer）
- **输出**：UnifiedQueryOptimizer 结构体 + optimize 方法
- **验收标准**：
  - 规则分析始终执行（离线）
  - LLM 建议在 enable_llm + api_key 存在时执行
  - 合并规则 + LLM 建议，标注来源（HintSource::Rule/Llm）
  - 返回 UnifiedQueryAnalysis（含 hints + explain_signals + llm_available + llm_degraded_reason）
  - 命令：`cargo test -p sz-orm-ai --features llm-optimizer unified_optimizer` 通过
- **依赖**：M6.4
- **预估**：高（2 天）
- **门禁**：`cargo test -p sz-orm-ai --features llm-optimizer unified_optimizer`

#### 任务 M6.6：实现 SqlSanitizer（敏感字面量脱敏）
- **描述**：在 `packages/sz-orm-ai/src/sql_sanitizer.rs` 新增 SqlSanitizer，识别敏感字面量（password/token/Base64 token）并替换为占位符，发送 LLM 前脱敏
- **输入**：无
- **输出**：`packages/sz-orm-ai/src/sql_sanitizer.rs`
- **验收标准**：
  - 识别 password='...' / token='...' / Base64 token 字面量
  - 替换为 password='***' / token='***' / '***'
  - 命令：`cargo test -p sz-orm-ai sql_sanitizer` 通过
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-ai sql_sanitizer`

#### 任务 M6.7：实现 LLM 降级逻辑（未配置/不可用时自动降级纯规则）
- **描述**：在 UnifiedQueryOptimizer 中实现降级逻辑：未配置 API key / LLM 调用失败 / 超时 → 自动降级纯规则引擎，返回规则建议 + LLM 降级说明，不报错不阻塞
- **输入**：M6.5
- **输出**：降级逻辑（llm_available=false + llm_degraded_reason）
- **验收标准**：
  - 未配置 API key → 仅返回规则建议，llm_degraded_reason="未配置 LLM API key"
  - LLM 调用失败 → 返回规则建议 + 降级原因
  - 超时（>10s）→ 降级规则引擎
  - 不报错不阻塞
  - 命令：`cargo test -p sz-orm-ai --features llm-optimizer llm_degradation` 通过
- **依赖**：M6.5
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-ai --features llm-optimizer llm_degradation`

#### 任务 M6.8：验证 LLM SQL 零执行（suggested_sql 仅建议）
- **描述**：验证 LLM 重写 SQL 仅作为建议返回，系统零次执行 LLM 生成的 SQL（UnifiedQueryOptimizer 无 execute_sql 方法，suggested_sql 字段仅展示用途）
- **输入**：M6.5
- **输出**：零执行验证测试
- **验收标准**：
  - UnifiedQueryOptimizer 无 execute_sql 方法
  - suggested_sql 字段为 Option<String>，仅展示用途
  - LLM 返回重写 SQL → SQL 仅存在于建议结构中，系统零次执行
  - 命令：`cargo test -p sz-orm-ai --features llm-optimizer llm_zero_execute` 通过
- **依赖**：M6.5
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-ai --features llm-optimizer llm_zero_execute`

#### 任务 M6.9：编写单元测试（mock LLM、ExplainParser、Sanitizer、降级）
- **描述**：在 `packages/sz-orm-ai/src/` 各模块新增单元测试，覆盖 mock LLM 响应、ExplainParser 各方言解析、SqlSanitizer 脱敏、降级逻辑
- **输入**：M6.1~M6.8
- **输出**：各模块 `#[cfg(test)] mod tests`
- **验收标准**：
  - mock LLM 响应解析测试通过
  - 五方言 ExplainParser 解析测试通过
  - SqlSanitizer 脱敏测试通过
  - 降级逻辑测试通过
  - 命令：`cargo test -p sz-orm-ai --features llm-optimizer` 全部通过
- **依赖**：M6.8
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-ai --features llm-optimizer`

#### 任务 M6.10：编写集成测试（LLM 真实 API 调用）
- **描述**：在 `packages/sz-orm-ai/tests/llm_integration.rs` 新增集成测试，调用真实 OpenAI 兼容 API 生成优化建议，`#[ignore]` 标注，real feature
- **输入**：M6.9、OpenAI 兼容 API（需 API key）
- **输出**：`packages/sz-orm-ai/tests/llm_integration.rs`
- **验收标准**：
  - LLM 真实调用返回结构化建议
  - 建议含 severity/title/description/source/model
  - SQL 已脱敏（无敏感字面量）
  - 命令：`cargo test -p sz-orm-ai --features llm-optimizer --test llm_integration -- --ignored` 通过
- **依赖**：M6.9
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-ai --features llm-optimizer --test llm_integration -- --ignored`

#### 任务 M6.11：配置 llm-optimizer feature gate
- **描述**：在 `packages/sz-orm-ai/Cargo.toml` 新增 `llm-optimizer` feature gate（默认关闭，依赖 real feature + reqwest + sqlparser），修改 `src/lib.rs` 导出新模块
- **输入**：M6.1~M6.10
- **输出**：`packages/sz-orm-ai/Cargo.toml`（新增 llm-optimizer feature）+ `packages/sz-orm-ai/src/lib.rs`（导出 query_plan_optimizer/explain_parser/sql_sanitizer）
- **验收标准**：
  - `llm-optimizer` feature 默认关闭
  - 默认 feature 不引入 reqwest/sqlparser 依赖
  - 命令：`cargo build -p sz-orm-ai`（默认 feature）成功
  - 命令：`cargo build -p sz-orm-ai --features llm-optimizer` 成功
  - 命令：`cargo check --workspace --all-targets --all-features` 全组合编译通过
- **依赖**：M6.10
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-ai` + `cargo build -p sz-orm-ai --features llm-optimizer` + `cargo check --workspace --all-targets --all-features`

---

## 里程碑 M7：集成验证 + 发布

### 目标
全 workspace 10 道门禁通过、无 Breaking Change、Feature 隔离、crates.io 发布（sz-orm-graph 0.1.0 + 扩展包升级）、sz-pay/sz-rust 下游零回归、性能基准不回退、CHANGELOG 更新、29 条需求追溯核对。

### 需求覆盖
AC-ALL-1, AC-ALL-2, AC-ALL-3, AC-ALL-4, AC-ALL-5, AC-ALL-6, AC-ALL-7

### 任务列表

#### 任务 M7.1：全 workspace 10 道门禁通过
- **描述**：执行 AGENTS.md 定义的 10 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入扫描/Feature 全组合），任一 FAIL 阻断发布
- **输入**：M1~M6 全部交付物
- **输出**：门禁执行报告
- **验收标准**：
  - 命令：`cargo fmt --all -- --check` 通过
  - 命令：`cargo check --workspace --all-targets` 通过
  - 命令：`cargo clippy --workspace --all-targets -- -D warnings` 零警告
  - 命令：`cargo test --workspace` 全部通过
  - 命令：`cargo doc --workspace --no-deps --all-features` 通过
  - 命令：`cargo audit` + `cargo deny check` 通过
  - 命令：`cargo test --workspace -- --ignored` 真实服务集成测试通过
  - 命令：`grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` 无结果
  - 命令：`.\scripts\check-sql-injection.ps1` 通过
  - 命令：`cargo check --workspace --all-targets --all-features` 全组合编译通过
- **依赖**：M1, M2, M3, M4, M5, M6
- **预估**：中（1 天）
- **门禁**：`.\scripts\gate.ps1`（10 道门禁全通过）

#### 任务 M7.2：验证无 Breaking Change
- **描述**：确认 v3.0.0 无 Breaking Change，v2.4.0 公开 API（SmartEagerLoader/EagerLoader/StrategyResolver/N1Eliminator/AnyBackend/AnyPool/UnifiedPool/QueryOptimizer 等）全部保持不变
- **输入**：M1~M6
- **输出**：API 兼容性验证报告
- **验收标准**：
  - v2.4.0 公开 API 签名全部保持不变
  - 新增能力通过 feature gate 隔离（默认 feature 不引入额外依赖）
  - 命令：`cargo build --workspace`（默认 feature）成功，无 API 签名变更
- **依赖**：M7.1
- **预估**：低（0.5 天）
- **门禁**：API 兼容性审查

#### 任务 M7.3：验证 Feature 隔离
- **描述**：确认新增能力全部 feature gate 隔离（xa/llm-optimizer/js/persistence），默认 feature 不引入额外依赖
- **输入**：M2.8, M4.1, M6.11
- **输出**：Feature 隔离验证报告
- **验收标准**：
  - 默认 feature 不引入 sz-orm-sqlx（dtx）、reqwest/sqlparser（ai）、wasm-bindgen/web-sys（wasm）依赖
  - 命令：`cargo build --workspace`（默认 feature）依赖树无新增重依赖
  - 命令：`cargo check --workspace --all-targets --all-features` 全组合编译通过
- **依赖**：M7.2
- **预估**：低（0.5 天）
- **门禁**：`cargo build --workspace` + `cargo check --workspace --all-targets --all-features`

#### 任务 M7.4：crates.io 发布（sz-orm-graph 0.1.0 + 扩展包升级）
- **描述**：复用 v2.4.0 `scripts/publish_crates_io.ps1` 发布脚本，按拓扑序发布：sz-orm-graph 0.1.0（新增）+ sz-orm-ai/sz-orm-dtx/sz-orm-wasm 2.4.0（扩展升级）
- **输入**：M7.1~M7.3、`scripts/publish_crates_io.ps1`、`scripts/compute_topology.ps1`
- **输出**：crates.io 发布结果
- **验收标准**：
  - sz-orm-graph 0.1.0 在 crates.io 可见
  - sz-orm-ai/sz-orm-dtx/sz-orm-wasm 2.4.0 在 crates.io 可见
  - 发布前检查版本未已存在（已存在则跳过并记录）
  - 命令：`.\scripts\publish_crates_io.ps1 -DryRun` 干跑成功
- **依赖**：M7.3
- **预估**：中（1 天）
- **门禁**：`.\scripts\publish_crates_io.ps1 -DryRun`

#### 任务 M7.5：sz-pay 下游回归验证（5139 测试零回归）
- **描述**：将 sz-pay 依赖升级到 v3.0.0 版本，移除 [patch.crates-io] 本地覆盖，执行 `cargo test` 回归验证 5139 测试零回归
- **输入**：M7.4、`E:\vue\test\sz-pay\server\sz-rust\Cargo.toml`
- **输出**：sz-pay 回归测试报告
- **验收标准**：
  - sz-pay 依赖升级到 v3.0.0 版本
  - 移除 patch 段后 cargo build 成功
  - 命令：`cargo test --manifest-path <sz-pay Cargo.toml>` 5139 测试全部通过（零回归）
  - 回归失败 → 保留 patch 段 + 输出失败测试详情
- **依赖**：M7.4
- **预估**：中（1 天）
- **门禁**：`cargo test --manifest-path <sz-pay Cargo.toml>`

#### 任务 M7.6：sz-rust 下游回归验证
- **描述**：验证 sz-rust 透明适配层在 v3.0.0 升级后零回归，仅依赖 sz-orm 公开 API
- **输入**：M7.4、`E:\vue\test\鲜视达\rust\sz-rust`
- **输出**：sz-rust 回归测试报告
- **验收标准**：
  - sz-rust 依赖升级到 v3.0.0 版本
  - 命令：`cargo build --manifest-path <sz-rust Cargo.toml>` 成功
  - 命令：`cargo test --manifest-path <sz-rust Cargo.toml>` 通过（零回归）
- **依赖**：M7.4
- **预估**：中（1 天）
- **门禁**：`cargo test --manifest-path <sz-rust Cargo.toml>`

#### 任务 M7.7：性能基准不回退验证
- **描述**：复用 v2.4.0 性能基准套件（bench-comparison），验证 v3.0.0 不使 v2.4.0 已验收的性能基准回退（决策延迟 P99 ≤ 100μs、智能/手动比 ≤ 1.10、N+1 消除生效）
- **输入**：M7.5、`bench-comparison/benches/bench_smart_eager.rs`
- **输出**：性能基准对比报告
- **验收标准**：
  - 决策延迟 P99 ≤ 100μs
  - 智能/手动耗时比 ≤ 1.10
  - N+1 消除生效（消除后查询次数 < 消除前）
  - 命令：`cargo bench --workspace` 基准不回退
- **依赖**：M7.5
- **预估**：中（1 天）
- **门禁**：`cargo bench --workspace`

#### 任务 M7.8：更新 CHANGELOG.md v3.0.0
- **描述**：在 `CHANGELOG.md` 新增 v3.0.0 变更记录：图数据库支持、WASM 完善、maturin/napi 发布产物、AI 查询优化器、XA 事务一致性、多后端协同文档
- **输入**：M1~M6
- **输出**：`CHANGELOG.md`（v3.0.0 变更记录）
- **验收标准**：
  - CHANGELOG.md 含 v3.0.0 变更记录
  - 记录六项任务交付物 + 29 条需求覆盖 + 7 总体验收标准
  - 命令：`Test-Path CHANGELOG.md` 返回 True
- **依赖**：M7.7
- **预估**：低（0.5 天）
- **门禁**：文档审查

#### 任务 M7.9：需求追溯矩阵核对（29 条需求全覆盖）
- **描述**：核对 spec.md 第 7 章需求追溯矩阵，确认 29 条需求（REQ-GDB-001~005 / REQ-WASM-001~005 / REQ-FDI-001~005 / REQ-AI-001~005 / REQ-DTX-001~005 / REQ-MB-001~004）全部映射到任务且验收通过
- **输入**：M1~M7.8、`docs/spec/v3.0.0/spec.md` 第 7 章
- **输出**：需求追溯核对报告
- **验收标准**：
  - 29 条需求全部映射到任务（见下方需求覆盖核对表）
  - spec §9 验收标准总览中 AC-GDB-1~5 / AC-WASM-1~5 / AC-FDI-1~5 / AC-AI-1~5 / AC-DTX-1~5 / AC-MB-1~4 / AC-ALL-1~7 全部满足
  - 命令：需求追溯矩阵核对通过
- **依赖**：M7.8
- **预估**：低（0.5 天）
- **门禁**：需求追溯矩阵核对

---

## 3. 任务依赖矩阵

| 任务编号 | 依赖任务 | 关键路径 |
|---------|---------|---------|
| M1.1 | 无 | |
| M1.2 | 无 | |
| M1.3 | 无 | |
| M1.4 | M1.2 | |
| M1.5 | M1.1, M1.2, M1.3, M1.4 | |
| M2.1 | 无 | |
| M2.2 | M2.1 | ✓ |
| M2.3 | M2.1 | |
| M2.4 | M2.2 | ✓ |
| M2.5 | M2.2 | |
| M2.6 | M2.1~M2.5 | |
| M2.7 | M2.6 | ✓ |
| M2.8 | M2.7 | |
| M3.1 | 无 | |
| M3.2 | 无 | |
| M3.3 | M3.1 | |
| M3.4 | M3.2 | |
| M3.5 | M3.1, M3.3 | |
| M3.6 | M3.2, M3.4 | |
| M3.7 | M3.3, M3.4 | |
| M3.8 | M3.7 | |
| M4.1 | 无 | |
| M4.2 | M4.1 | |
| M4.3 | M4.1 | ✓ |
| M4.4 | M4.3 | |
| M4.5 | 无 | |
| M4.6 | M4.3, M4.4 | |
| M4.7 | M4.2, M4.3 | ✓ |
| M4.8 | M4.1, M4.2, M4.3 | |
| M5.1 | 无 | |
| M5.2 | M5.1 | |
| M5.3 | M5.2 | |
| M5.4 | M5.3 | |
| M5.5 | M5.3 | |
| M5.6 | M5.3 | |
| M5.7 | M5.6 | |
| M5.8 | 无 | |
| M5.9 | M5.7 | |
| M5.10 | M5.8, M5.9 | ✓ |
| M5.11 | M5.10 | ✓ |
| M5.12 | M5.11 | |
| M6.1 | 无 | |
| M6.2 | M6.1 | |
| M6.3 | 无 | |
| M6.4 | M6.1, M6.2, M6.3 | |
| M6.5 | M6.4 | ✓ |
| M6.6 | 无 | |
| M6.7 | M6.5 | |
| M6.8 | M6.5 | |
| M6.9 | M6.8 | |
| M6.10 | M6.9 | ✓ |
| M6.11 | M6.10 | |
| M7.1 | M1~M6 | ✓ |
| M7.2 | M7.1 | |
| M7.3 | M7.2 | |
| M7.4 | M7.3 | ✓ |
| M7.5 | M7.4 | ✓ |
| M7.6 | M7.4 | |
| M7.7 | M7.5 | |
| M7.8 | M7.7 | |
| M7.9 | M7.8 | ✓ |

### 关键路径（最长依赖链）

```
M2.1 → M2.2 → M2.4 → M2.7 → M7.1 → M7.4 → M7.5 → M7.9
```

**关键路径长度**：8 个任务（M2.1 → M2.2 → M2.4 → M2.7 → M7.1 → M7.4 → M7.5 → M7.9）

**关键路径说明**：XA 事务一致性（M2）的 XaResource → XaCoordinator → XaRecoveryCoordinator → 集成测试是技术最复杂的链路，后续集成发布（M7）依赖所有里程碑完成。关键路径决定项目最短工期。

---

## 4. 验收标准汇总

### 4.1 需求 → 任务 → 验收命令映射

| 需求编号 | 对应任务 | 验收命令 | 期望结果 |
|---------|---------|---------|---------|
| REQ-GDB-001 | M5.2, M5.10 | `cargo test -p sz-orm-graph --test neo4j_integration -- --ignored` | Neo4j 连接成功，脱敏错误 |
| REQ-GDB-002 | M5.4, M5.10 | `cargo test -p sz-orm-graph cypher_validator` | 参数绑定执行，注入载荷为字面量 |
| REQ-GDB-003 | M5.6, M5.10 | `cargo test -p sz-orm-graph result_mapper` | 结果映射 + serde 反序列化成功 |
| REQ-GDB-004 | M5.5, M5.10 | `cargo build -p sz-orm-graph` | 模型驱动查询与映射 |
| REQ-GDB-005 | M5.4, M5.10 | `cargo test -p sz-orm-graph cypher_validator` | 图接口拒绝 SQL，零透传 |
| REQ-WASM-001 | M4.1, M4.8 | `cargo build --target wasm32-unknown-unknown -p sz-orm-wasm` | 编译成功，gzip ≤ 1MB |
| REQ-WASM-002 | M4.2, M4.7 | `cargo build --target wasm32-unknown-unknown -p sz-orm-wasm --features js` | wasm-bindgen 绑定 + .d.ts |
| REQ-WASM-003 | M4.3, M4.7 | `wasm-pack test --headless --chrome` | IndexedDB 持久化 + 恢复 |
| REQ-WASM-004 | M4.5 | `cargo test -p sz-orm-wasm memory_limit` | 超限拒绝写入 |
| REQ-WASM-005 | M4.6 | `cargo test -p sz-orm-wasm persistence_unavailable` | 不可用时明确报告 |
| REQ-FDI-001 | M3.1, M3.5 | `.\scripts\build_python_wheel.ps1` | maturin wheel + pip install 成功 |
| REQ-FDI-002 | M3.2, M3.6 | `.\scripts\build_napi.ps1` | napi 产物 + npm install 成功 |
| REQ-FDI-003 | M3.3, M3.4, M3.7 | `.\scripts\verify_bindings.ps1` | 绑定层 == sz-orm-core 行为 |
| REQ-FDI-004 | M3.3, M3.4 | `pytest packages/sz-orm-python/tests/` + `npx jest packages/sz-orm-js/tests/` | asyncio/async 查询成功 |
| REQ-FDI-005 | M3.7 | `.\scripts\verify_bindings.ps1` | 测试失败阻断发布 |
| REQ-AI-001 | M6.4, M6.5, M6.10 | `cargo test -p sz-orm-ai --features llm-optimizer --test llm_integration -- --ignored` | 结构化建议 + 来源追溯 |
| REQ-AI-002 | M6.3, M6.9 | `cargo test -p sz-orm-ai --features llm-optimizer explain_parser` | EXPLAIN 解析纳入建议 |
| REQ-AI-003 | M6.1, M6.5 | `cargo test -p sz-orm-ai --features llm-optimizer unified_optimizer` | JSON 输出含 source/model |
| REQ-AI-004 | M6.2, M6.7 | `cargo test -p sz-orm-ai --features llm-optimizer llm_degradation` | 模型可配置，未配置降级 |
| REQ-AI-005 | M6.8 | `cargo test -p sz-orm-ai --features llm-optimizer llm_zero_execute` | LLM SQL 仅建议零执行 |
| REQ-DTX-001 | M2.1, M2.2, M2.7 | `cargo test -p sz-orm-dtx --features xa --test xa_integration -- --ignored` | 2PC 全有全无 |
| REQ-DTX-002 | M2.4, M2.7 | `cargo test -p sz-orm-dtx --features xa xa_recovery` | 日志恢复未决事务 |
| REQ-DTX-003 | M2.5, M2.7 | `cargo test -p sz-orm-dtx --features xa suspension` | 超时收敛终态 |
| REQ-DTX-004 | M2.6 | `cargo test -p sz-orm-dtx --features xa coexistence` | XA 与 2PC/Saga/TCC 共存 |
| REQ-DTX-005 | M2.3, M2.7 | `cargo test -p sz-orm-dtx --features xa xa_capability` | 拒绝不支持 XA 参与者 |
| REQ-MB-001 | M1.1 | `scripts/audit-verify.ps1 docs/spec/v3.0.0/multi_backend_readiness.md` | 就绪清单逐项验证 |
| REQ-MB-002 | M1.4 | `cargo test --workspace -- --ignored` | 五方言等价测试通过 |
| REQ-MB-003 | M1.3, M1.5 | `cargo build --example sz_rust_integration_example` + `git diff --name-only HEAD` | 适配层仅依赖公开 API |
| REQ-MB-004 | M1.2 | 文档审查 | 方言约束明确提示 |

### 4.2 总体验收标准映射

| 验收编号 | 对应任务 | 验收命令 |
|---------|---------|---------|
| AC-ALL-1（无 Breaking Change） | M7.2 | API 兼容性审查 |
| AC-ALL-2（全 workspace 测试通过） | M7.1 | `cargo test --workspace` |
| AC-ALL-3（clippy 零警告） | M7.1 | `cargo clippy --workspace --all-targets -- -D warnings` |
| AC-ALL-4（Feature 隔离） | M7.3 | `cargo build --workspace` + `cargo check --workspace --all-targets --all-features` |
| AC-ALL-5（下游零回归） | M7.5, M7.6 | `cargo test --manifest-path <sz-pay>` + `cargo test --manifest-path <sz-rust>` |
| AC-ALL-6（性能基准不回退） | M7.7 | `cargo bench --workspace` |
| AC-ALL-7（29 条需求全满足） | M7.9 | 需求追溯矩阵核对 |

---

## 5. 门禁检查清单

### 5.1 每个里程碑结束前的门禁命令

| 里程碑 | 必过门禁 | 命令 |
|--------|---------|------|
| M1 | 文档审查 + 示例编译 | `cargo build --example sz_rust_integration_example` + `scripts/audit-verify.ps1` |
| M2 | dtx 包编译 + 测试 + clippy | `cargo build -p sz-orm-dtx --features xa` + `cargo test -p sz-orm-dtx --features xa` + `cargo clippy -p sz-orm-dtx --features xa -- -D warnings` |
| M3 | 绑定层构建 + 等价性测试 | `.\scripts\build_python_wheel.ps1` + `.\scripts\build_napi.ps1` + `.\scripts\verify_bindings.ps1` |
| M4 | wasm32 编译 + bindgen-test + 体积断言 | `cargo build --target wasm32-unknown-unknown -p sz-orm-wasm --features js,persistence` + `wasm-pack test --headless --chrome` + gzip 体积断言 |
| M5 | graph 包编译 + 测试 + clippy + 集成测试 | `cargo build -p sz-orm-graph` + `cargo test -p sz-orm-graph` + `cargo clippy -p sz-orm-graph -- -D warnings` + `cargo test -p sz-orm-graph --test neo4j_integration -- --ignored` |
| M6 | ai 包编译 + 测试 + clippy + feature 隔离 | `cargo build -p sz-orm-ai --features llm-optimizer` + `cargo test -p sz-orm-ai --features llm-optimizer` + `cargo clippy -p sz-orm-ai --features llm-optimizer -- -D warnings` |
| M7 | 全量 10 道门禁 + 下游回归 | `.\scripts\gate.ps1` + `cargo test --manifest-path <sz-pay>` + `cargo test --manifest-path <sz-rust>` |

### 5.2 发布前全量门禁（M7.1）

| # | 门禁 | 命令 | 期望结果 |
|---|------|------|---------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` | 通过 |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` | 通过 |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` | 零警告 |
| 4 | test 单元/集成测试 | `cargo test --workspace` | 全部通过 |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` | 通过 |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` | 通过 |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` | 全部通过 |
| 8 | 禁止占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` | 无结果 |
| 9 | SQL 注入扫描 | `.\scripts\check-sql-injection.ps1` | 通过 |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` | 通过 |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD` | 仅含新增文件，无业务代码变更（ADR-0001） |

### 5.3 五维审查（每次 PR 必做）

| 维度 | 检查项 | 验证方式 |
|------|--------|---------|
| 正确性 | 29 条需求全覆盖 + 无占位实现 + 无 Breaking Change + 依赖无循环 | 需求追溯矩阵 + grep 占位 + API 兼容性审查 + 拓扑排序 |
| 可读性 | 文档结构清晰 + file:line 证据 + 图表辅助 | 文档审查 + `scripts/audit-verify.ps1` |
| 架构 | 模块职责单一 + 复用基础设施 + Feature 隔离 + 新增包最小化 | 架构审查 + `cargo check --all-features` |
| 安全性 | 参数化查询 + LLM 输出零执行 + SQL 脱敏 + unsafe 零容忍 + SQL 透传拒绝 | `.\scripts\check-sql-injection.ps1` + grep unsafe + 安全审查 |
| 性能 | 图查询 P95 ≤ 500ms + AI 建议 P95 ≤ 10s + WASM gzip ≤ 1MB + 基准不回退 | 性能测试 + `cargo bench --workspace` |

---

## 6. 风险标记汇总

| 风险 ID | 关联任务 | 风险描述 | 等级 | 缓解措施 |
|---------|---------|---------|------|---------|
| R-01 | M5.2 | Neo4j Bolt 驱动生态成熟度不足 | 高 | 锁定 neo4rs 驱动版本，独立包 feature 隔离，评估期先行 spike |
| R-02 | M4.2, M4.3 | WASM 生态对 tokio/异步支持有限 | 高 | 浏览器端用同步/轻异步执行路径，与服务器端 WASI 场景分离 |
| R-03 | M4.2 | wasm-bindgen 版本 API 变动频繁 | 中 | 锁定 wasm-bindgen 版本，绑定层最小化 |
| R-04 | M3.8 | maturin/napi 跨平台 CI 矩阵成本 | 中 | 复用 GitHub Actions 矩阵，三平台并行构建，缓存 Cargo 编译产物 |
| R-05 | M6.7 | LLM 服务不可用/限流导致建议不可用 | 中 | 自动降级规则引擎（REQ-AI-004） |
| R-06 | M6.8 | LLM 生成 SQL 存在安全/正确性风险 | 高 | 建议零自动执行 + 脱敏 + safety 模块校验 |
| R-07 | M2.3, M2.7 | XA 协议在不同数据库的实现差异 | 高 | 仅对明确支持 XA 的库开放，逐库集成测试 |
| R-08 | M2.5 | 分布式事务性能开销 | 中 | 超时与重试可配置，提供性能基准 |
| R-09 | M1.4 | sz-rust 适配层方言差异处理成本 | 中 | 五方言等价性测试 + 方言约束文档 |
| R-10 | 全部 | 长期目标优先级低，与日常迭代争抢资源 | 中 | 按收益/风险序逐步推进，独立里程碑 |
| R-11 | M4.8 | WASM 产物体积超 1MB | 中 | wasm-opt 优化 + twiggy 分析 + feature gate 隔离重依赖 + gzip 体积断言 |
| R-12 | M2.4 | 协调者崩溃后 XA 日志不完整 | 高 | TransactionLogStore 各阶段落日志，恢复时按日志状态收敛 |
| R-13 | M2.5 | XA Commit 阶段部分失败（不可回滚） | 高 | 标记悬挂 + 审计日志 + 人工介入或按策略补偿 |
| R-14 | M3.3, M3.4 | 绑定层与 sz-orm-core 行为不一致 | 高 | pytest/jest 等价性测试套件覆盖，任一失败阻断发布 |
| R-15 | M7.5, M7.6 | 下游 sz-pay/sz-rust 回归 | 高 | v3.0.0 无 Breaking Change（feature gate 隔离），sz-pay 5139 测试基线 + sz-rust 回归验证 |

**高严重度风险优先级**：R-01 = R-02 = R-06 = R-07 > R-12 = R-13 = R-14 = R-15

---

## 7. 需求覆盖核对表

| 需求编号 | 对应任务 | 覆盖状态 |
|---------|---------|---------|
| REQ-GDB-001 | M5.2 + M5.10 | ☐ |
| REQ-GDB-002 | M5.4 + M5.10 | ☐ |
| REQ-GDB-003 | M5.6 + M5.10 | ☐ |
| REQ-GDB-004 | M5.5 + M5.10 | ☐ |
| REQ-GDB-005 | M5.4 + M5.10 | ☐ |
| REQ-WASM-001 | M4.1 + M4.8 | ☐ |
| REQ-WASM-002 | M4.2 + M4.7 | ☐ |
| REQ-WASM-003 | M4.3 + M4.7 | ☐ |
| REQ-WASM-004 | M4.5 | ☐ |
| REQ-WASM-005 | M4.6 | ☐ |
| REQ-FDI-001 | M3.1 + M3.5 | ☐ |
| REQ-FDI-002 | M3.2 + M3.6 | ☐ |
| REQ-FDI-003 | M3.3 + M3.4 + M3.7 | ☐ |
| REQ-FDI-004 | M3.3 + M3.4 | ☐ |
| REQ-FDI-005 | M3.7 | ☐ |
| REQ-AI-001 | M6.4 + M6.5 + M6.10 | ☐ |
| REQ-AI-002 | M6.3 + M6.9 | ☐ |
| REQ-AI-003 | M6.1 + M6.5 | ☐ |
| REQ-AI-004 | M6.2 + M6.7 | ☐ |
| REQ-AI-005 | M6.8 | ☐ |
| REQ-DTX-001 | M2.1 + M2.2 + M2.7 | ☐ |
| REQ-DTX-002 | M2.4 + M2.7 | ☐ |
| REQ-DTX-003 | M2.5 + M2.7 | ☐ |
| REQ-DTX-004 | M2.6 | ☐ |
| REQ-DTX-005 | M2.3 + M2.7 | ☐ |
| REQ-MB-001 | M1.1 | ☐ |
| REQ-MB-002 | M1.4 | ☐ |
| REQ-MB-003 | M1.3 + M1.5 | ☐ |
| REQ-MB-004 | M1.2 | ☐ |

> 覆盖状态：☐ 待完成 / ✅ 已完成（任务执行时更新）
> **29 条需求全覆盖确认**：上表 29 行，每行对应任务非空，覆盖状态完整。

---

## 8. 里程碑交付物汇总

| 里程碑 | 交付物 | 文件路径 |
|--------|--------|---------|
| M1 | 多后端就绪清单 + 方言约束矩阵 + 协同示例 | `docs/spec/v3.0.0/multi_backend_readiness.md` + `docs/spec/v3.0.0/dialect_constraints.md` + `examples/sz_rust_integration_example.rs` |
| M2 | XA 事务（XaResource/XaCoordinator/恢复/悬挂）+ 集成测试 | `packages/sz-orm-dtx/src/{xa,recovery,suspension}.rs` + `packages/sz-orm-dtx/tests/xa_integration.rs` |
| M3 | 构建发布脚本 + 等价性测试 + CI 矩阵 | `scripts/{build_python_wheel,build_napi,publish_pypi,publish_npm,verify_bindings}.ps1` + `packages/sz-orm-python/tests/` + `packages/sz-orm-js/tests/` + `.github/workflows/bindings.yml` |
| M4 | WASM 浏览器端能力（bindgen + IndexedDB）+ 测试 | `packages/sz-orm-wasm/src/{js_bindings,persistence,error}.rs` + `.cargo/config.toml` + `packages/sz-orm-wasm/tests/js_bindings_test.rs` |
| M5 | sz-orm-graph 新包 + Neo4j 测试环境 + 集成测试 | `packages/sz-orm-graph/` + `docker-compose.neo4j.yml` + `packages/sz-orm-graph/tests/{neo4j_integration,performance}.rs` |
| M6 | AI 优化器（LLM + EXPLAIN + 降级）+ 测试 | `packages/sz-orm-ai/src/{query_plan_optimizer,explain_parser,sql_sanitizer}.rs` + `packages/sz-orm-ai/tests/llm_integration.rs` |
| M7 | 全门禁通过 + crates.io 发布 + 下游回归 + CHANGELOG | 门禁报告 + crates.io 发布结果 + sz-pay/sz-rust 回归报告 + `CHANGELOG.md` |

---

> **文档结束**
>
> **文档版本**：v3.0.0 任务分解（tasks 阶段）
> **对应需求**：`docs/spec/v3.0.0/spec.md`（29 条 EARS 需求，6 组）
> **对应设计**：`docs/spec/v3.0.0/design.md`（2455 行，6 大模块、7 里程碑、15 风险）
> **基线参考**：`docs/spec/v2.4.0/tasks.md`（v2.4.0 任务分解，7 主任务 / 35 子任务）
> **任务总数**：7 个里程碑 / 61 个子任务，覆盖全部 29 条需求 + 7 总体验收标准
> **关键路径**：M2.1 → M2.2 → M2.4 → M2.7 → M7.1 → M7.4 → M7.5 → M7.9（8 个任务）
> **总周期**：约 17 周（4 个月），按低优先级逐步推进，可与日常迭代并行
> **工程化基线**：AGENTS.md 10 道门禁 + 五维审查 + 审计合规铁律（file:line 证据）