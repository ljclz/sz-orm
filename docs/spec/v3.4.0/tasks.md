# sz-orm v3.4.0 编码任务规划文档

> 版本：v3.4.0（测试覆盖补齐 + 架构改进 + 性能优化落地 + 编译期类型安全增强 + 文档与生态建设 + sz-pay 生产案例深化）
> 基线：v3.3.0（已完成：分布式缓存一致性 + GraphQL 查询支持 + 多租户与数据隔离 + AI 自然语言查询增强，6,327 测试基线）
> 日期：2026-08-08
> 文档定位：编码任务规划（What to do），对应需求规格 `docs/spec/v3.4.0/spec.md`（31 条 EARS 需求）与技术设计 `docs/spec/v3.4.0/design.md`（6 里程碑 + 10 聚合 feature gate）
> 任务粒度：每个任务可在 1-3 小时内完成，单个任务不超过 500 行代码变更
> 工程化铁律：禁止占位实现 / unsafe 零容忍 / 参数化查询 / API 向后兼容 / Feature 隔离 / 五方言行为一致 / ADR-0001 不改上游 / 审计合规铁律（每结论附 file:line 证据）

---

# 1. 任务总览

## 1.1 任务统计

| 里程碑 | 方向 | 主任务数 | 子任务数 | 关联需求 |
|--------|------|---------|---------|---------|
| M1 测试覆盖补齐 | 方向 1 | 7 | 42 | REQ-TC-001~005 |
| M2 架构改进 | 方向 2 | 7 | 26 | REQ-AR-001~006 |
| M3 性能优化落地 | 方向 3 | 9 | 32 | REQ-PF-001~007 |
| M4 编译期类型安全增强 | 方向 4 | 6 | 22 | REQ-TS-001~005 |
| M5 文档与生态建设 | 方向 5 | 7 | 18 | REQ-DOC-001~004 |
| M6 sz-pay 案例 + 集成验证与发布 | 方向 6 + 全方向 | 8 | 20 | REQ-PC-001~004 + AC-ALL-1~10 |
| **合计** | — | **44** | **160** | **31 条 REQ + 10 条 AC-ALL** |

## 1.2 里程碑分布

```
M1 测试覆盖补齐 (3 周)         ──→ M2 架构改进 (2 周) ──→ M3 性能优化 (2 周)
                                                              │
                                                              ↓
M6 集成验证与发布 (1 周) ←── M5 文档生态 (2 周) ←── M4 类型安全 (2 周)
```

- **关键路径**：M1 → M2 → M3 → M4 → M5 → M6（串行 12 周）
- **并行机会**：
  - M1 内部：18 扩展包测试可并行（不同包独立），MySQL INSERT IGNORE 修复独立，sz-orm-es/config real feature 独立
  - M2 内部：313 pub API 文档补齐与 async trait 评估可并行，README 更新与 query-builder 指南可并行
  - M3 内部：5 项性能优化可并行（不同模块），6 组基准对比可并行
  - M4 内部：Schema derive 扩展、Column<T>、typed DSL 三者可并行
  - M5 内部：三份迁移指南可并行
- **总周期**：关键路径 12 周（串行）；并行开发下可压缩至 8-10 周

## 1.3 Feature Gate 矩阵

### 1.3.1 10 个聚合 Feature gate

| 聚合 Feature | 所属包 | 默认 | 依赖 | 关联里程碑 |
|---------|--------|------|------|-----------|
| `test-coverage` | sz-orm-core | 关闭 | 无（仅标识） | M1 |
| `arch-improvement` | sz-orm-core | 关闭 | 无（仅标识） | M2 |
| `perf-smallstring` | sz-orm-core | 关闭 | compactstr（optional） | M3 |
| `perf-enum-dispatch` | sz-orm-core | 关闭 | 无 | M3 |
| `perf-zero-copy-l2` | sz-orm-core | 关闭 | 复用既有 zero-copy | M3 |
| `perf-box-str` | sz-orm-core | 关闭 | 无 | M3 |
| `type-safe-columns` | sz-orm-macros + sz-orm-core | 关闭 | 无（复用既有 proc-macro） | M4 |
| `doc-completion` | sz-orm-core | 关闭 | 无（纯文档） | M2/M5 |
| `migration-guide` | sz-orm-core | 关闭 | 无（纯文档） | M5 |
| `sz-pay-example` | examples | 关闭 | 无（复用既有 examples 依赖） | M6 |

### 1.3.2 6 个细粒度 Feature gate

| 细粒度 Feature | 所属包 | 默认 | 依赖 | 聚合到 |
|---------|--------|------|------|--------|
| `real` | sz-orm-es | 关闭 | elasticsearch（optional，占位） | test-coverage + arch-improvement |
| `real-consul` | sz-orm-config | 关闭 | reqwest（optional） | test-coverage |
| `real-nacos` | sz-orm-config | 关闭 | reqwest（optional） | test-coverage |
| `typed-schema` | sz-orm-macros | 关闭 | 无 | type-safe-columns |
| `typed-column` | sz-orm-core | 关闭 | 无 | type-safe-columns |
| `typed-dsl` | sz-orm-core | 关闭 | 无 | type-safe-columns |

---

# 2. M1 测试覆盖补齐（REQ-TC-001~005）

> **目标**：为 18 个零测试扩展包补齐单元测试（覆盖率 ≥ 60%），修复 MySQL INSERT IGNORE 测试缺陷，为 sz-orm-es 增加 `real` feature 占位 + 真实 ES 集成测试，为 sz-orm-config 增加真实 Consul/Nacos 客户端实现 + 集成测试，不修改既有公开 API。
> **周期**：3 周
> **关联设计**：design.md §2.1
> **关联验收**：AC-TC-1~6（spec §9.1）

## 2.1 M1-T1：Feature gate 配置与基础设施

- [ ] **M1-T1.1** 在 `packages/sz-orm-core/Cargo.toml` 的 `[features]` 新增 `test-coverage = []` 聚合 gate（仅用于门禁矩阵标识，不引入依赖）
  - 关联需求：REQ-TC-001~005
  - 关联设计：design.md §2.1.5
  - 验收：`cargo check -p sz-orm-core` 通过，feature 默认关闭
  - 依赖：无

- [ ] **M1-T1.2** 在 `packages/sz-orm-es/Cargo.toml` 新增 `[features] real = []` 占位 + `elasticsearch = { version = "8.5", optional = true }` optional 依赖
  - 关联需求：REQ-TC-003
  - 关联设计：design.md §2.1.5
  - 验收：`cargo check -p sz-orm-es` 默认 mock 行为不变；`cargo check -p sz-orm-es --features real` 编译通过
  - 依赖：M1-T1.1

- [ ] **M1-T1.3** 在 `packages/sz-orm-config/Cargo.toml` 新增 `real-consul = ["dep:reqwest"]` / `real-nacos = ["dep:reqwest"]` feature + `reqwest = { version = "0.12", features = ["json"], optional = true }` optional 依赖
  - 关联需求：REQ-TC-004
  - 关联设计：design.md §2.1.5
  - 验收：默认内存行为不变；`cargo check -p sz-orm-config --features real-consul` 编译通过
  - 依赖：M1-T1.1

## 2.2 M1-T2：18 个扩展包测试补齐（REQ-TC-001）

> 18 个零测试扩展包分 4 批补齐，每批独立可并行。各包新增 tests/ 目录与测试文件，覆盖正常路径 + 边界 + 错误处理，覆盖率 ≥ 60%。

### 2.2.1 第 1 批：核心基础设施包（config/auth/crypto/audit/batch）

- [ ] **M1-T2.1** 为 `packages/sz-orm-config` 补齐单元测试：`tests/config_test.rs` 覆盖 `ConsulConfigCenter` 内存配置读写/监听/事件通知（正常 + 边界 + 错误路径）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-config` 通过；`cargo tarpaulin -p sz-orm-config` 行覆盖率 ≥ 60%
  - 依赖：M1-T1.3

- [ ] **M1-T2.2** 为 `packages/sz-orm-auth` 补齐单元测试：`tests/auth_test.rs` 覆盖 JWT 生成/验证/RBAC 权限/OAuth2/MFA（正常 + 过期/无效 token + 权限拒绝）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-auth` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.3** 为 `packages/sz-orm-crypto` 补齐单元测试：`tests/crypto_test.rs` 覆盖 AES-256-GCM 加解密/HMAC/SHA-256/密钥管理（正常 + 空输入 + 错误密钥）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-crypto` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.4** 为 `packages/sz-orm-audit` 补齐单元测试：`tests/audit_test.rs` 覆盖 `SqlAuditor` 审计日志记录/查询/脱敏（正常 + 空日志 + 超长字段）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-audit` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.5** 为 `packages/sz-orm-batch` 补齐单元测试：`tests/batch_test.rs` 覆盖批量 INSERT/UPDATE/UPSERT（正常 + 空批量 + 超批量 + 冲突）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-batch` 通过；覆盖率 ≥ 60%
  - 依赖：无

### 2.2.2 第 2 批：数据操作包（rw/masking/logger/health/es）

- [ ] **M1-T2.6** 为 `packages/sz-orm-rw` 补齐单元测试：`tests/rw_test.rs` 覆盖读写分离路由（主写/从读/强制主/故障切换）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-rw` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.7** 为 `packages/sz-orm-masking` 补齐单元测试：`tests/masking_test.rs` 覆盖 `DataMasker` 脱敏规则（Phone/Email/IdCard/BankCard/Name/Address/Ip/Imei/Plate/Custom 全变体 + 边界）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-masking` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.8** 为 `packages/sz-orm-logger` 补齐单元测试：`tests/logger_test.rs` 覆盖结构化日志输出/级别过滤/格式化（正常 + 空消息 + 超长）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-logger` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.9** 为 `packages/sz-orm-health` 补齐单元测试：`tests/health_test.rs` 覆盖健康检查/断路器状态转换（Closed/Open/HalfOpen + 故障恢复）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-health` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.10** 为 `packages/sz-orm-es` 补齐 Mock 单元测试：`tests/es_mock_test.rs` 覆盖 `MockEsBackend` 索引/搜索/聚合/过滤（正常 + 空结果 + 不存在索引）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-es` 通过；覆盖率 ≥ 60%
  - 依赖：M1-T1.2

### 2.2.3 第 3 批：可观测性与 Web 集成包（grpc/swagger/tracing/observability/back）

- [ ] **M1-T2.11** 为 `packages/sz-orm-grpc` 补齐单元测试：`tests/grpc_test.rs` 覆盖 gRPC 抽象接口/消息序列化（正常 + 空消息 + 类型不匹配）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-grpc` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.12** 为 `packages/sz-orm-swagger` 补齐单元测试：`tests/swagger_test.rs` 覆盖 OpenAPI 规格生成/路径/参数/响应（正常 + 空路径 + 重复定义）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-swagger` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.13** 为 `packages/sz-orm-tracing` 补齐单元测试：`tests/tracing_test.rs` 覆盖分布式追踪 span 创建/传播/采样（正常 + 空上下文 + 采样率边界）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-tracing` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.14** 为 `packages/sz-orm-observability` 补齐单元测试：`tests/observability_test.rs` 覆盖 Prometheus 指标/SLO 计算/告警（正常 + 空指标 + 超阈值）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-observability` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.15** 为 `packages/sz-orm-back` 补齐单元测试：`tests/back_test.rs` 覆盖备份/恢复/校验（正常 + 空备份 + 损坏文件）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-back` 通过；覆盖率 ≥ 60%
  - 依赖：无

### 2.2.4 第 4 批：低代码/WASM/Web 框架/多语言绑定包（lc/wasm/axum/actix/js/python）

- [ ] **M1-T2.16** 为 `packages/sz-orm-lc` 补齐单元测试：`tests/lc_test.rs` 覆盖低代码声明解析/模型生成/查询构造（正常 + 无效声明 + 未知类型）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-lc` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.17** 为 `packages/sz-orm-wasm` 补齐单元测试：`tests/wasm_test.rs` 覆盖 WASM 查询执行/序列化（正常 + 空查询 + 序列化错误）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-wasm` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.18** 为 `packages/sz-orm-axum` 补齐单元测试：`tests/axum_test.rs` 覆盖 axum Web 集成/路由/中间件（正常 + 空路由 + 错误处理）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-axum` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.19** 为 `packages/sz-orm-actix` 补齐单元测试：`tests/actix_test.rs` 覆盖 actix Web 集成/路由/中间件（正常 + 空路由 + 错误处理）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-actix` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.20** 为 `packages/sz-orm-js` 补齐单元测试：`tests/js_test.rs` 覆盖 JavaScript 绑定/FFI 接口/序列化（正常 + 空输入 + 类型不匹配）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-js` 通过；覆盖率 ≥ 60%
  - 依赖：无

- [ ] **M1-T2.21** 为 `packages/sz-orm-python` 补齐单元测试：`tests/python_test.rs` 覆盖 Python 绑定/FFI 接口/序列化（正常 + 空输入 + 类型不匹配）
  - 关联需求：REQ-TC-001
  - 关联设计：design.md §2.1.2
  - 验收：`cargo test -p sz-orm-python` 通过；覆盖率 ≥ 60%
  - 依赖：无

## 2.3 M1-T3：MySQL INSERT IGNORE 测试缺陷修复（REQ-TC-002）

- [ ] **M1-T3.1** 定位 `packages/sz-orm-core/tests/integration_mysql.rs:1267` 测试 `test_mysql_insert_or_ignore_duplicate`，修改测试表 DDL 为 `name` 列添加 UNIQUE 约束（`CREATE TABLE ... (name VARCHAR(...) UNIQUE, ...)`）
  - 关联需求：REQ-TC-002
  - 关联设计：design.md §2.1.3（MySQL INSERT IGNORE 测试修复）
  - 验收：`cargo test -p sz-orm-core --test integration_mysql test_mysql_insert_or_ignore_duplicate` 通过，`affected_rows = 0`（修复前 = 1）
  - 依赖：无

- [ ] **M1-T3.2** 运行 `cargo test -p sz-orm-core --test integration_mysql` 全部通过，确认 UNIQUE 约束修复未引入其它测试回归
  - 关联需求：REQ-TC-002
  - 关联设计：design.md §2.1.7
  - 验收：integration_mysql 全部测试通过，无回归
  - 依赖：M1-T3.1

## 2.4 M1-T4：sz-orm-es real feature 占位 + 真实 ES 集成测试（REQ-TC-003）

- [ ] **M1-T4.1** 创建 `packages/sz-orm-es/tests/real_es_integration.rs`，添加 `#[cfg(feature = "real")]` + `#[ignore]` 标注，编写真实 ES 集成测试覆盖索引创建/文档索引/搜索/聚合/过滤
  - 关联需求：REQ-TC-003
  - 关联设计：design.md §2.1.3（sz-orm-es 真实 ES 集成测试）
  - 验收：默认 `cargo test -p sz-orm-es` 跳过真实 ES 测试；`cargo test -p sz-orm-es --features real -- --ignored` 在真实 ES 环境下通过
  - 依赖：M1-T1.2, M1-T2.10

- [ ] **M1-T4.2** 创建 `packages/sz-orm-es/tests/mock_real_diff.rs`，编写 Mock 与真实 ES 行为差分测试，验证语义一致（索引/搜索/聚合/过滤结果对比）
  - 关联需求：REQ-TC-003
  - 关联设计：design.md §2.1.3
  - 验收：Mock 与真实行为差分测试覆盖索引/搜索/聚合/过滤；语义一致
  - 依赖：M1-T4.1

## 2.5 M1-T5：sz-orm-config 真实 Consul/Nacos 实现（REQ-TC-004）

- [ ] **M1-T5.1** 创建 `packages/sz-orm-config/src/consul_client.rs`，实现真实 Consul HTTP API 客户端（基于 reqwest）：`get_config` / `set_config` / `watch` / `register_service`，ACL Token 认证
  - 关联需求：REQ-TC-004
  - 关联设计：design.md §2.1.3（sz-orm-config 真实 Consul 客户端）
  - 验收：`cargo check -p sz-orm-config --features real-consul` 编译通过；ACL Token 认证实现
  - 依赖：M1-T1.3

- [ ] **M1-T5.2** 创建 `packages/sz-orm-config/src/nacos_client.rs`，实现真实 Nacos HTTP API 客户端（基于 reqwest）：`get_config` / `set_config` / `watch` / `register_service`，Username+Password 认证
  - 关联需求：REQ-TC-004
  - 关联设计：design.md §2.1.3（sz-orm-config 真实 Nacos 客户端）
  - 验收：`cargo check -p sz-orm-config --features real-nacos` 编译通过；Username+Password 认证实现
  - 依赖：M1-T1.3

- [ ] **M1-T5.3** 创建 `packages/sz-orm-config/tests/real_consul_integration.rs`，添加 `#[cfg(feature = "real-consul")]` + `#[ignore]`，编写真实 Consul 集成测试覆盖配置读写/监听/服务发现
  - 关联需求：REQ-TC-004
  - 关联设计：design.md §2.1.4
  - 验收：`cargo test -p sz-orm-config --features real-consul -- --ignored` 在真实 Consul 环境下通过
  - 依赖：M1-T5.1

- [ ] **M1-T5.4** 创建 `packages/sz-orm-config/tests/real_nacos_integration.rs`，添加 `#[cfg(feature = "real-nacos")]` + `#[ignore]`，编写真实 Nacos 集成测试覆盖配置读写/监听/服务发现
  - 关联需求：REQ-TC-004
  - 关联设计：design.md §2.1.4
  - 验收：`cargo test -p sz-orm-config --features real-nacos -- --ignored` 在真实 Nacos 环境下通过
  - 依赖：M1-T5.2

- [ ] **M1-T5.5** 编写内存实现与真实实现行为差分测试：配置读写/监听/服务发现行为一致（含配置变更通知）
  - 关联需求：REQ-TC-004
  - 关联设计：design.md §2.1.4
  - 验收：内存/真实行为一致（配置变更通知测试覆盖）
  - 依赖：M1-T5.3, M1-T5.4

## 2.6 M1-T6：覆盖率验证与无效覆盖拒绝（REQ-TC-005）

- [ ] **M1-T6.1** 对 18 个扩展包逐一运行 `cargo tarpaulin -p <package>`，收集覆盖率报告，确认各包行覆盖率 ≥ 60%
  - 关联需求：REQ-TC-001, REQ-TC-005
  - 关联设计：design.md §2.1.7
  - 验收：18 包覆盖率报告附证据，各包 ≥ 60%
  - 依赖：M1-T2.21

- [ ] **M1-T6.2** 代码审查 18 扩展包测试，拒绝仅 `assert!(true)` 的无效覆盖（确保测试真实验证包内核心逻辑）
  - 关联需求：REQ-TC-005
  - 关联设计：design.md §2.1.7
  - 验收：无效覆盖测试被拒绝；测试真实验证包内核心逻辑
  - 依赖：M1-T6.1

## 2.7 M1-T7：门禁验证

- [ ] **M1-T7.1** 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
  - 关联需求：REQ-TC-001~005
  - 关联设计：design.md §2.1.5
  - 验收：fmt 通过、编译通过、clippy 零警告
  - 依赖：M1-T6.2

- [ ] **M1-T7.2** 运行 `cargo test --workspace` 全部通过，确认测试数 ≥ 6,327 + 新增数；扫描 `todo!` / `unimplemented!` / `unreachable!` / `unsafe` 零容忍
  - 关联需求：REQ-TC-001~005
  - 关联设计：design.md §2.1.5
  - 验收：测试全通过；测试数 ≥ 6,327 + 新增数；禁止占位实现；unsafe 零容忍
  - 依赖：M1-T7.1

---

# 3. M2 架构改进（REQ-AR-001~006）

> **目标**：为 Mock 包增加 `real` feature 占位、补齐 313 个 pub API 文档并移除 docs.rs cfg 跳过、更新 README 成熟度声明、评估 async trait 风格统一、编写 sz-orm-query-builder 选择指南，不修改既有公开 API（除文档注释）。
> **周期**：2 周
> **关联设计**：design.md §2.2
> **关联验收**：AC-AR-1~6（spec §9.2）
> **依赖**：M1 测试覆盖就绪

## 3.1 M2-T1：Feature gate 配置

- [ ] **M2-T1.1** 在 `packages/sz-orm-core/Cargo.toml` 新增 `arch-improvement = []` + `doc-completion = []` 聚合 gate（仅用于门禁矩阵标识）
  - 关联需求：REQ-AR-001~006
  - 关联设计：design.md §2.2.5
  - 验收：feature 默认关闭，不引入依赖
  - 依赖：M1-T7.2

## 3.2 M2-T2：Mock 包 real feature 占位（REQ-AR-001）

- [ ] **M2-T2.1** 确认 sz-orm-es `real` feature 占位已在 M1-T1.2 完成；验证默认 mock 行为不变，`cargo check -p sz-orm-es --features real` 编译通过
  - 关联需求：REQ-AR-001
  - 关联设计：design.md §2.2.3
  - 验收：AC-AR-1 real feature 占位，默认 mock 不变，编译通过
  - 依赖：M2-T1.1

## 3.3 M2-T3：313 个 pub API 文档补齐（REQ-AR-002）

> 分批补齐：优先公开 API（pub fn/pub struct/pub trait）> 内部 API（pub(crate)）> 测试 API，每批约 20-30 个，附进度跟踪。

- [ ] **M2-T3.1** 定位 313 个缺文档 pub API（通过 `cargo doc --workspace --no-deps` 警告或 `cargo +nightly rustdoc -- -D missing_docs` 扫描），生成清单并按公开 > 内部 > 测试排序
  - 关联需求：REQ-AR-002
  - 关联设计：design.md §2.2.3
  - 验收：313 个缺文档 API 清单生成，排序合理
  - 依赖：M2-T1.1

- [ ] **M2-T3.2** 第 1 批（公开 API，约 100 个）：为 `packages/sz-orm-core/src/` 核心 pub API（QueryBuilder/Dialect/Value/Pool/L2Cache 等公开类型与方法）补齐 `///` 文档注释（功能描述 + 参数 + 返回值 + 示例 + 错误）
  - 关联需求：REQ-AR-002
  - 关联设计：design.md §2.2.3
  - 验收：第 1 批 `cargo doc --workspace --no-deps` 新增警告数为 0
  - 依赖：M2-T3.1

- [ ] **M2-T3.3** 第 2 批（内部 API，约 100 个）：为 `pub(crate)` 内部 API 补齐 `///` 文档注释
  - 关联需求：REQ-AR-002
  - 关联设计：design.md §2.2.3
  - 验收：第 2 批 `cargo doc --workspace --no-deps` 新增警告数为 0
  - 依赖：M2-T3.2

- [ ] **M2-T3.4** 第 3 批（测试 API + 剩余，约 113 个）：为测试 API 与剩余 pub API 补齐 `///` 文档注释
  - 关联需求：REQ-AR-002
  - 关联设计：design.md §2.2.3
  - 验收：第 3 批 `cargo doc --workspace --no-deps` 新增警告数为 0
  - 依赖：M2-T3.3

- [ ] **M2-T3.5** 移除 `packages/sz-orm-core/src/lib.rs:406` 的 `#![cfg_attr(docsrs, warn(missing_docs))]`，改为全局 `#![warn(missing_docs)]`
  - 关联需求：REQ-AR-002
  - 关联设计：design.md §2.2.6
  - 验收：`cargo doc --workspace --no-deps` 无 missing-docs 警告；docs.rs 文档完整
  - 依赖：M2-T3.4

## 3.4 M2-T4：README 成熟度声明更新（REQ-AR-003）

- [ ] **M2-T4.1** 修改 `README.md:46`，移除"当前处于原型阶段，尚无生产案例、无第三方审计、无社区采用"过时声明
  - 关联需求：REQ-AR-003
  - 关联设计：design.md §2.2.3
  - 验收：过时声明已移除
  - 依赖：M2-T1.1

- [ ] **M2-T4.2** 在 README 补充 sz-pay 生产案例：7 个包（sz-orm-core/sqlx/config/auth/macros/queue/scheduler）、297 处引用、5139 测试零回归、crates.io 拉取 2.3.0；更新项目状态为"早期生产可用（内部项目）"
  - 关联需求：REQ-AR-003
  - 关联设计：design.md §2.2.3
  - 验收：AC-AR-3 README 含 sz-pay 案例，状态更新为"早期生产可用（内部项目）"，声明与评估报告 §5.1 一致
  - 依赖：M2-T4.1

## 3.5 M2-T5：async trait 风格统一评估（REQ-AR-004）

- [ ] **M2-T5.1** 编写性能基准对比：`#[async_trait]` 宏展开开销 vs 手动解糖开销（criterion 基准证据），对比 `packages/sz-orm-core/src/pool.rs:42` Connection trait 手动解糖与 `#[async_trait]` 版本
  - 关联需求：REQ-AR-004
  - 关联设计：design.md §2.2.3
  - 验收：基准对比附 criterion 证据（中位数 + 置信区间）
  - 依赖：M2-T1.1

- [ ] **M2-T5.2** 编写迁移影响分析：列出涉及的手动解糖 trait（Connection trait + 其他）+ 调用方列表 + Breaking Change 评估 + HRTB 技术原因分析
  - 关联需求：REQ-AR-004
  - 关联设计：design.md §1.2.3
  - 验收：迁移影响分析完整，含 trait 列表 + 调用方 + HRTB 技术原因
  - 依赖：M2-T5.1

- [ ] **M2-T5.3** 编写 `docs/async_trait_style_evaluation.md` 评估文档：性能基准对比 + 迁移影响分析 + 学习成本评估 + 推荐方案（可能是"保持手动解糖 + 文档说明原因"）
  - 关联需求：REQ-AR-004
  - 关联设计：design.md §2.2.3
  - 验收：AC-AR-4 评估文档含基准/迁移/学习成本/推荐方案；附 file:line 证据
  - 依赖：M2-T5.2

## 3.6 M2-T6：sz-orm-query-builder 选择指南（REQ-AR-005）

- [ ] **M2-T6.1** 编写能力对比表：`sz-orm-query-builder::Query`（独立 SQL 构造器，sea-query 风格）vs `sz-orm-core::QueryBuilder<M>`（绑定 Model，编译期校验），对比支持的查询类型（SELECT/INSERT/UPDATE/DELETE/JOIN/聚合）+ 方言支持 + 特性（类型安全/参数化/软删除）
  - 关联需求：REQ-AR-005
  - 关联设计：design.md §1.2.6
  - 验收：能力对比表完整（查询类型/方言/特性）
  - 依赖：M2-T1.1

- [ ] **M2-T6.2** 编写性能基准对比：两者 SQL 构造吞吐量基准（criterion 证据）+ 适用场景说明 + 迁移建议
  - 关联需求：REQ-AR-005
  - 关联设计：design.md §2.2.3
  - 验收：基准对比附 criterion 证据；适用场景与迁移建议清晰
  - 依赖：M2-T6.1

- [ ] **M2-T6.3** 编写 `docs/query_builder_selection_guide.md` 选择指南文档：能力对比表 + 适用场景 + 性能基准 + 迁移建议
  - 关联需求：REQ-AR-005
  - 关联设计：design.md §2.2.3
  - 验收：AC-AR-5 选择指南含能力对比/适用场景/基准/迁移建议；附 file:line 证据
  - 依赖：M2-T6.2

## 3.7 M2-T7：门禁验证

- [ ] **M2-T7.1** 运行 `cargo doc --workspace --no-deps` 无警告 + `cargo test --workspace --doc` doctest 通过，验证文档与代码实际行为一致
  - 关联需求：REQ-AR-002, REQ-AR-006
  - 关联设计：design.md §2.2.7
  - 验收：AC-AR-6 cargo doc 无警告，doctest 通过，文档与代码一致
  - 依赖：M2-T3.5, M2-T6.3

- [ ] **M2-T7.2** 运行 `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` 全部通过
  - 关联需求：REQ-AR-001~006
  - 关联设计：design.md §2.2.5
  - 验收：fmt 通过、clippy 零警告、测试全通过
  - 依赖：M2-T7.1

---

# 4. M3 性能优化落地（REQ-PF-001~007）

> **目标**：评估并落地 5 项性能优化（SmallString/CompactString、enum dispatch、zero-copy L2 推广、Box<str>、result_map 宏生成评估），完善 6 组对比基准，全部通过 feature gate 隔离，不修改既有公开 API。
> **周期**：2 周
> **关联设计**：design.md §2.3
> **关联验收**：AC-PF-1~7（spec §9.3）
> **依赖**：M2 文档补齐就绪

## 4.1 M3-T1：Feature gate 配置

- [ ] **M3-T1.1** 在 `packages/sz-orm-core/Cargo.toml` 新增 `perf-smallstring = ["dep:compactstr"]` / `perf-enum-dispatch = []` / `perf-zero-copy-l2 = ["zero-copy"]` / `perf-box-str = []` feature，新增 `compactstr = { version = "0.8", optional = true }` optional 依赖
  - 关联需求：REQ-PF-001~007
  - 关联设计：design.md §2.3.5
  - 验收：4 个 feature 默认关闭；`cargo check -p sz-orm-core --features perf-smallstring` 引入 compactstr
  - 依赖：M2-T7.2

## 4.2 M3-T2：query.rs SmallString/CompactString（REQ-PF-001）

- [ ] **M3-T2.1** 在 `packages/sz-orm-core/src/query.rs` 的 SQL 构造路径（`build_select` / `build_insert` / `build_update` / `build_delete`）添加 `#[cfg(feature = "perf-smallstring")]` 分支，使用 `CompactString` 替代 `String`（短字符串 ≤ 23 字节内联存储）
  - 关联需求：REQ-PF-001
  - 关联设计：design.md §2.3.3
  - 验收：既有 String 路径不变（`#[cfg(not(feature = "perf-smallstring"))]`）；QueryBuilder 公开 API 返回类型不变
  - 依赖：M3-T1.1

- [ ] **M3-T2.2** 编写 SmallString 差分测试：优化前 vs 优化后生成 SQL 完全一致（覆盖 SELECT/INSERT/UPDATE/DELETE + 短/长字符串场景）
  - 关联需求：REQ-PF-001, REQ-PF-007
  - 关联设计：design.md §2.3.4
  - 验收：差分测试通过，SQL 完全一致
  - 依赖：M3-T2.1

- [ ] **M3-T2.3** 编写 SmallString 基准对比：`packages/sz-orm-core/benches/smallstring_bench.rs`，对比 CompactString vs String SQL 构造吞吐量（短字符串场景），criterion 基准附中位数 + 置信区间
  - 关联需求：REQ-PF-001
  - 关联设计：design.md §2.3.7
  - 验收：AC-PF-1 短字符串场景吞吐量 ≥ 基线 1.15x（基准证据）
  - 依赖：M3-T2.2

## 4.3 M3-T3：dialect.rs enum dispatch（REQ-PF-002）

- [ ] **M3-T3.1** 在 `packages/sz-orm-core/src/dialect.rs` 新增 `enum DialectKind { MySQL, PostgreSQL, SQLite, Oracle, MSSQL }`，通过 match 分发替代 `Box<dyn Dialect>` vtable 查找，添加 `#[cfg(feature = "perf-enum-dispatch")]` 条件编译
  - 关联需求：REQ-PF-002
  - 关联设计：design.md §2.3.3
  - 验收：既有 `Box<dyn Dialect>` 路径不变；Dialect trait 公开 API 不变
  - 依赖：M3-T1.1

- [ ] **M3-T3.2** 编写 enum dispatch 差分测试：enum dispatch vs Box<dyn Dialect> 五方言行为完全一致（覆盖各方言 DDL/SQL 生成）
  - 关联需求：REQ-PF-002, REQ-PF-007
  - 关联设计：design.md §2.3.4
  - 验收：五方言行为一致（差分测试证据）
  - 依赖：M3-T3.1

- [ ] **M3-T3.3** 编写 enum dispatch 基准对比：`packages/sz-orm-core/benches/enum_dispatch_bench.rs`，对比 enum dispatch vs Box<dyn Dialect> 分发开销，criterion 基准附中位数 + 置信区间
  - 关联需求：REQ-PF-002
  - 关联设计：design.md §2.3.7
  - 验收：AC-PF-2 方言分发开销 ≤ 基线 0.7x（基准证据）
  - 依赖：M3-T3.2

## 4.4 M3-T4：l2_cache.rs zero-copy 推广（REQ-PF-003）

- [ ] **M3-T4.1** 在 `packages/sz-orm-core/src/l2_cache.rs` 的序列化/反序列化路径添加 `#[cfg(feature = "perf-zero-copy-l2")]` 分支，推广既有 `BorrowedValue` + `ColumnarResultSet`（`packages/sz-orm-core/src/value_borrowed.rs`）到 L2 缓存路径
  - 关联需求：REQ-PF-003
  - 关联设计：design.md §2.3.3
  - 验收：既有序列化路径不变；L2Cache 公开 API 不变；与既有 Redis 后端兼容（序列化格式不变）
  - 依赖：M3-T1.1

- [ ] **M3-T4.2** 编写 zero-copy L2 差分测试：zero-copy vs 普通序列化结果完全一致 + 兼容性测试（既有 Redis 缓存数据可读）
  - 关联需求：REQ-PF-003, REQ-PF-007
  - 关联设计：design.md §2.3.4
  - 验收：差分测试通过；兼容性测试覆盖
  - 依赖：M3-T4.1

- [ ] **M3-T4.3** 编写 zero-copy L2 基准对比：`packages/sz-orm-core/benches/zero_copy_l2_bench.rs`，对比 zero-copy vs 普通序列化分配计数
  - 关联需求：REQ-PF-003
  - 关联设计：design.md §2.3.7
  - 验收：AC-PF-3 L2 缓存序列化/反序列化分配 ≤ 基线 0.5x（分配计数证据）
  - 依赖：M3-T4.2

## 4.5 M3-T5：value.rs Box<str>（REQ-PF-005）

- [ ] **M3-T5.1** 在 `packages/sz-orm-core/src/value.rs` 新增 `Value::BoxedStr(Box<str>)` 变体（`#[cfg(feature = "perf-box-str")]`），用于不需要修改字符串的场景，节省 8 字节/值 capacity 字段
  - 关联需求：REQ-PF-005
  - 关联设计：design.md §2.3.3
  - 验收：既有 `Value::String(String)` 变体不变；Value 枚举公开 API 不变
  - 依赖：M3-T1.1

- [ ] **M3-T5.2** 编写 Box<str> 基准对比：`size_of::<Value>()` 对比 Box<str> vs String 变体内存占用
  - 关联需求：REQ-PF-005
  - 关联设计：design.md §2.3.7
  - 验收：AC-PF-5 Value 枚举内存占用减少（size_of 证据）
  - 依赖：M3-T5.1

## 4.6 M3-T6：result_map.rs 宏生成评估（REQ-PF-004）

- [ ] **M3-T6.1** 编写性能基准对比：宏生成（编译期）vs 反射式取值（运行时）开销基准（criterion 证据）
  - 关联需求：REQ-PF-004
  - 关联设计：design.md §2.3.3
  - 验收：基准对比附 criterion 证据
  - 依赖：M3-T1.1

- [ ] **M3-T6.2** 编写 `docs/result_map_macro_evaluation.md` 评估文档：性能基准对比 + 迁移影响分析 + 类型安全收益 + 推荐方案
  - 关联需求：REQ-PF-004
  - 关联设计：design.md §2.3.3
  - 验收：AC-PF-4 评估文档含基准/迁移/类型安全收益/推荐方案；附 file:line 证据
  - 依赖：M3-T6.1

## 4.7 M3-T7：6 组对比基准完善（REQ-PF-006）

- [ ] **M3-T7.1** 完善 `packages/sz-orm-core/benches/` 基准：新增 6 组对比基准（zero-copy vs 普通 / simd vs 标量 / plan-cache vs 无缓存 / SmallString vs String / enum dispatch vs Box<dyn> / Box<str> vs String），每组附加速比 + 中位数 + 置信区间
  - 关联需求：REQ-PF-006
  - 关联设计：design.md §2.3.7
  - 验收：AC-PF-6 6 组对比基准完善，每组附加速比证据
  - 依赖：M3-T2.3, M3-T3.3, M3-T4.3, M3-T5.2

- [ ] **M3-T7.2** 将 6 组基准纳入 CI 定期运行（CI 配置定期执行 `cargo bench --workspace`）
  - 关联需求：REQ-PF-006
  - 关联设计：design.md §2.3.7
  - 验收：CI 定期运行基准
  - 依赖：M3-T7.1

## 4.8 M3-T8：差分测试与正确性验证（REQ-PF-007）

- [ ] **M3-T8.1** 运行全 workspace 测试（各性能优化 feature 组合）：`cargo test --workspace --features perf-smallstring` / `--features perf-enum-dispatch` / `--features perf-zero-copy-l2` / `--features perf-box-str` / `--all-features`，确认既有 6,327+ 测试全部通过
  - 关联需求：REQ-PF-007
  - 关联设计：design.md §2.3.7
  - 验收：AC-PF-7 既有 6,327+ 测试全部通过，无正确性回归
  - 依赖：M3-T7.2

## 4.9 M3-T9：门禁验证

- [ ] **M3-T9.1** 运行 `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + 扫描 `todo!` / `unimplemented!` / `unreachable!` / `unsafe` 零容忍
  - 关联需求：REQ-PF-001~007
  - 关联设计：design.md §2.3.5
  - 验收：fmt 通过、clippy 零警告、禁止占位实现、unsafe 零容忍
  - 依赖：M3-T8.1

---

# 5. M4 编译期类型安全增强（REQ-TS-001~005）

> **目标**：扩展既有 `#[derive(Schema)]` 宏生成编译期列名常量，引入 `Column<T>` 类型安全列引用，完善 typed_ast.rs Diesel 风格表达式 DSL，全部通过 feature gate `type-safe-columns` 隔离，不修改既有公开 API，编译期完成类型检查，运行时零额外开销。
> **周期**：2 周
> **关联设计**：design.md §2.4
> **关联验收**：AC-TS-1~5（spec §9.4）
> **依赖**：M3 性能基准就绪

## 5.1 M4-T1：Feature gate 配置

- [ ] **M4-T1.1** 在 `packages/sz-orm-macros/Cargo.toml` 新增 `type-safe-columns = []` + `typed-schema = []` feature；在 `packages/sz-orm-core/Cargo.toml` 新增 `type-safe-columns = ["sz-orm-macros/type-safe-columns"]` + `typed-column = []` + `typed-dsl = []` feature
  - 关联需求：REQ-TS-001~005
  - 关联设计：design.md §2.4.5
  - 验收：feature 默认关闭；`cargo check -p sz-orm-core --features type-safe-columns` 编译通过
  - 依赖：M3-T9.1

## 5.2 M4-T2：Schema derive 列名常量扩展（REQ-TS-001）

- [ ] **M4-T2.1** 在 `packages/sz-orm-macros/src/derive/` 扩展 Schema derive，添加 `#[cfg(feature = "type-safe-columns")]` 分支，为每个结构体字段生成 `pub const FIELD_NAME: &'static str = "field_name"` 常量（如 `impl User { pub const ID: &'static str = "id"; pub const NAME: &'static str = "name"; }`）
  - 关联需求：REQ-TS-001
  - 关联设计：design.md §2.4.3
  - 验收：启用 feature 后生成列名常量；未启用时既有 derive 输出不变
  - 依赖：M4-T1.1

- [ ] **M4-T2.2** 编写 trybuild 编译测试：验证引用存在列编译通过，引用不存在列（如 `User::NON_EXISTENT`）编译失败
  - 关联需求：REQ-TS-001
  - 关联设计：design.md §2.4.7
  - 验收：AC-TS-1 列名拼写错误编译期暴露
  - 依赖：M4-T2.1

- [ ] **M4-T2.3** 实现复杂类型支持处理：对泛型/生命周期/trait object 等不支持字段跳过并告警（告警含字段名与类型）
  - 关联需求：REQ-TS-001
  - 关联设计：design.md §2.4.3
  - 验收：不支持字段告警跳过；用户可手动标注覆盖
  - 依赖：M4-T2.2

## 5.3 M4-T3：Column<T> 类型安全列引用（REQ-TS-002）

- [ ] **M4-T3.1** 创建 `packages/sz-orm-core/src/column.rs` 模块，定义 `Column<T: Schema> { name: &'static str, _marker: PhantomData<T> }` 泛型结构体，添加 `#[cfg(feature = "type-safe-columns")]` 条件编译守卫
  - 关联需求：REQ-TS-002
  - 关联设计：design.md §2.4.3
  - 验收：`Column<T>` 关联表类型 T 保证列引用属于指定表
  - 依赖：M4-T1.1

- [ ] **M4-T3.2** 实现 `Column<T>` 方法：`Column::<User>::new("id") -> Column<User>`（构造）、`Column<User>::name() -> &'static str`（获取列名）、`impl<T> Deref for Column<T>`（可解引用为 `&str` 支持混用）
  - 关联需求：REQ-TS-002
  - 关联设计：design.md §2.4.3
  - 验收：`Column<T>` 可解引用为 `&str` 支持混用
  - 依赖：M4-T3.1

- [ ] **M4-T3.3** 在 `packages/sz-orm-core/src/query.rs` 的 QueryBuilder 新增 `where_eq<T>(col: Column<T>, value: Value) -> Self` 重载（`#[cfg(feature = "type-safe-columns")]`），既有 `&str` API 不变
  - 关联需求：REQ-TS-002
  - 关联设计：design.md §2.4.6
  - 验收：既有 `&str` API 不变；新增 `Column<T>` 重载
  - 依赖：M4-T3.2

- [ ] **M4-T3.4** 编写 trybuild 编译测试：验证 `Column<User>` 用于 Order 查询（跨表引用）编译失败
  - 关联需求：REQ-TS-002
  - 关联设计：design.md §2.4.7
  - 验收：AC-TS-2 跨表列引用编译期暴露
  - 依赖：M4-T3.3

## 5.4 M4-T4：typed_ast.rs Diesel 风格 DSL（REQ-TS-003）

- [ ] **M4-T4.1** 在 `packages/sz-orm-core/src/typed_ast.rs` 完善 Diesel 风格 DSL：表引用（`users::table` -> `TableRef<User>`，`users::id` -> `Column<User>`）+ 表达式方法（`col.eq(value)` / `col.gt(value)` / `col.lt(value)` / `col.like(pattern)` / `col.in_(values)`）+ 表达式组合（`expr.and(expr)` / `expr.or(expr)` / `expr.not()`）
  - 关联需求：REQ-TS-003
  - 关联设计：design.md §2.4.3
  - 验收：DSL 表达式编译期类型安全
  - 依赖：M4-T3.2

- [ ] **M4-T4.2** 实现 `Expr::to_sql() -> (String, Vec<Value>)` 方法：生成 SQL 片段 + 参数（参数化绑定，禁止字符串拼接）
  - 关联需求：REQ-TS-003, REQ-TS-005
  - 关联设计：design.md §2.4.3
  - 验收：列名/表名参数化绑定或编译期常量内联，值参数化绑定
  - 依赖：M4-T4.1

- [ ] **M4-T4.3** 在 QueryBuilder 新增 `where_expr(expr: Expr) -> Self` 方法（`#[cfg(feature = "type-safe-columns")]`），既有 QueryBuilder API 不变
  - 关联需求：REQ-TS-003
  - 关联设计：design.md §2.4.6
  - 验收：既有 QueryBuilder API 不变
  - 依赖：M4-T4.2

- [ ] **M4-T4.4** 编写 DSL 与 QueryBuilder 差分测试：DSL 生成 SQL 与 QueryBuilder 生成 SQL 完全一致（覆盖 eq/gt/lt/like/in + and/or/not 组合）
  - 关联需求：REQ-TS-003
  - 关联设计：design.md §2.4.7
  - 验收：AC-TS-3 生成 SQL 与 QueryBuilder 行为一致（差分测试覆盖）
  - 依赖：M4-T4.3

## 5.5 M4-T5：零运行时开销验证（REQ-TS-004）

- [ ] **M4-T5.1** 编写零运行时开销基准对比：typed vs `&str` 基准对比运行时开销零差异（criterion 基准证据）
  - 关联需求：REQ-TS-004
  - 关联设计：design.md §2.4.7
  - 验收：AC-TS-4 运行时开销零差异（基准证据）；类型检查在编译期完成
  - 依赖：M4-T4.4

- [ ] **M4-T5.2** 运行 SQL 注入扫描：验证 `Column<T>` / typed DSL 生成 SQL 无字符串拼接，参数化绑定正确
  - 关联需求：REQ-TS-005
  - 关联设计：design.md §2.4.7
  - 验收：AC-TS-5 SQL 注入扫描通过
  - 依赖：M4-T5.1

## 5.6 M4-T6：门禁验证

- [ ] **M4-T6.1** 运行 `cargo fmt --all -- --check` + `cargo check -p sz-orm-core --features type-safe-columns` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo test --workspace --features type-safe-columns`
  - 关联需求：REQ-TS-001~005
  - 关联设计：design.md §2.4.5
  - 验收：fmt 通过、编译通过、clippy 零警告、测试全通过；禁止占位实现；unsafe 零容忍
  - 依赖：M4-T5.2

---

# 6. M5 文档与生态建设（REQ-DOC-001~004）

> **目标**：补齐 313 个 pub API 文档（与 M2 统一交付）、更新 README 成熟度声明（与 M2 统一交付）、编写 Diesel/SeaORM/SQLx 三份迁移指南（含概念映射表 + API 对照表 + 示例代码 + 常见陷阱），降低外部用户迁移成本。
> **周期**：2 周
> **关联设计**：design.md §2.5
> **关联验收**：AC-DOC-1~4（spec §9.5）
> **依赖**：M4 类型安全就绪

## 6.1 M5-T1：Feature gate 配置

- [ ] **M5-T1.1** 在 `packages/sz-orm-core/Cargo.toml` 新增 `migration-guide = []` 聚合 gate（仅用于门禁矩阵标识，纯文档不引入依赖）
  - 关联需求：REQ-DOC-001~004
  - 关联设计：design.md §2.5.5
  - 验收：feature 默认关闭，不引入依赖
  - 依赖：M4-T6.1

## 6.2 M5-T2：313 pub API 文档与 README 统一交付确认（REQ-DOC-001, REQ-DOC-002）

- [ ] **M5-T2.1** 确认 313 pub API 文档补齐已在 M2-T3 完成；确认 README 成熟度更新已在 M2-T4 完成；运行 `cargo doc --workspace --no-deps` 无警告验证
  - 关联需求：REQ-DOC-001, REQ-DOC-002
  - 关联设计：design.md §2.5.6
  - 验收：AC-DOC-1 cargo doc 无警告；AC-DOC-2 README 更新完成
  - 依赖：M5-T1.1

## 6.3 M5-T3：Diesel 迁移指南（REQ-DOC-003）

- [ ] **M5-T3.1** 编写 `docs/migration/diesel_to_sz_orm.md`：概念映射表（Diesel `schema.rs` → sz-orm `#[derive(Schema)]`、Diesel `QueryDsl` → sz-orm `QueryBuilder`、Diesel `BelongsTo` → sz-orm `Relation`）+ API 对照表（Diesel `users::id.eq(1)` → sz-orm typed DSL）+ 示例代码（CRUD + 关联查询 + 事务 + 迁移）+ 常见陷阱（异步/同步差异、类型映射差异、方言差异）
  - 关联需求：REQ-DOC-003
  - 关联设计：design.md §2.5.3
  - 验收：概念映射表 + API 对照表 + 示例代码 + 常见陷阱完整
  - 依赖：M5-T1.1

- [ ] **M5-T3.2** 将 Diesel 迁移指南示例代码纳入 doctest 验证（`cargo test --workspace --doc`）
  - 关联需求：REQ-DOC-003
  - 关联设计：design.md §2.5.7
  - 验收：示例代码可编译（doctest 通过）
  - 依赖：M5-T3.1

## 6.4 M5-T4：SeaORM 迁移指南（REQ-DOC-003）

- [ ] **M5-T4.1** 编写 `docs/migration/seaorm_to_sz_orm.md`：概念映射表（SeaORM `Entity` → sz-orm `Model`、SeaORM `ActiveModel` → sz-orm `Model + fill`、SeaORM `QueryFilter` → sz-orm `QueryBuilder::where_eq`）+ API 对照表 + 示例代码 + 常见陷阱
  - 关联需求：REQ-DOC-003
  - 关联设计：design.md §2.5.3
  - 验收：概念映射表 + API 对照表 + 示例代码 + 常见陷阱完整
  - 依赖：M5-T1.1

- [ ] **M5-T4.2** 将 SeaORM 迁移指南示例代码纳入 doctest 验证
  - 关联需求：REQ-DOC-003
  - 关联设计：design.md §2.5.7
  - 验收：示例代码可编译（doctest 通过）
  - 依赖：M5-T4.1

## 6.5 M5-T5：SQLx 迁移指南（REQ-DOC-003）

- [ ] **M5-T5.1** 编写 `docs/migration/sqlx_to_sz_orm.md`：概念映射表（SQLx `query!` 宏 → sz-orm `sql_string!` 宏 + `QueryBuilder`、SQLx `FromRow` → sz-orm `#[derive(FromQueryResult)]`、SQLx `Pool` → sz-orm `Pool`）+ API 对照表 + 示例代码 + 常见陷阱（编译时 SQL 校验差异、类型映射差异、连接池差异）
  - 关联需求：REQ-DOC-003
  - 关联设计：design.md §2.5.3
  - 验收：概念映射表 + API 对照表 + 示例代码 + 常见陷阱完整
  - 依赖：M5-T1.1

- [ ] **M5-T5.2** 将 SQLx 迁移指南示例代码纳入 doctest 验证
  - 关联需求：REQ-DOC-003
  - 关联设计：design.md §2.5.7
  - 验收：示例代码可编译（doctest 通过）
  - 依赖：M5-T5.1

## 6.6 M5-T6：doctest 与文档一致性验证（REQ-DOC-004）

- [ ] **M5-T6.1** 运行 `cargo doc --workspace --no-deps` 无警告 + `cargo test --workspace --doc` doctest 全部通过
  - 关联需求：REQ-DOC-004
  - 关联设计：design.md §2.5.7
  - 验收：AC-DOC-4 cargo doc 无警告，doctest 通过
  - 依赖：M5-T3.2, M5-T4.2, M5-T5.2

## 6.7 M5-T7：门禁验证

- [ ] **M5-T7.1** 运行 `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`
  - 关联需求：REQ-DOC-001~004
  - 关联设计：design.md §2.5.5
  - 验收：fmt 通过、clippy 零警告、测试全通过
  - 依赖：M5-T6.1

---

# 7. M6 sz-pay 案例 + 集成验证与发布（REQ-PC-001~004 + AC-ALL-1~10）

> **目标**：将 sz-pay 使用模式抽取为 `examples/sz_pay_pattern.rs`（脱敏版），更新 README，执行 10 feature 全组合编译、五方言集成测试、sz-pay/sz-rust 零回归验证、性能基准不回退验证、10 道门禁全通过，完成 v3.4.0 版本发布。
> **周期**：1 周
> **关联设计**：design.md §2.6 + §2.8
> **关联验收**：AC-PC-1~4（spec §9.6）+ AC-ALL-1~10（spec §9.7）
> **依赖**：M1~M5 全方向就绪

## 7.1 M6-T1：sz-pay 案例抽取（REQ-PC-001）

- [ ] **M6-T1.1** 在 `examples/Cargo.toml` 新增 `sz-pay-example = []` feature + `[[bin]] name = "sz_pay_pattern" path = "src/bin/sz_pay_pattern.rs"` 配置
  - 关联需求：REQ-PC-001
  - 关联设计：design.md §2.6.5
  - 验收：examples/Cargo.toml 新增 bin 配置，既有 bin 不变
  - 依赖：M5-T7.1

- [ ] **M6-T1.2** 创建 `examples/src/bin/sz_pay_pattern.rs`，抽取 sz-pay 使用 sz-orm 的典型模式（脱敏版）：连接池配置（`PoolConfigBuilder` + 脱敏连接串 `mysql://user:pass@127.0.0.1:3306/db`）、SQL 执行（`QueryBuilder::new::<User>().where_eq("id", 1).get()`）、错误映射（`DbError` 错误码处理）、消息队列（`sz-orm-queue`）、定时调度（`sz-orm-scheduler`）
  - 关联需求：REQ-PC-001
  - 关联设计：design.md §2.6.3
  - 验收：示例展示连接池/SQL 执行/错误映射/消息队列/定时调度典型用法
  - 依赖：M6-T1.1

- [ ] **M6-T1.3** 执行脱敏审查 + 密钥扫描（`grep -r "password\|secret\|key" examples/`），确认无真实密钥/连接串/业务数据
  - 关联需求：REQ-PC-004
  - 关联设计：design.md §2.6.7
  - 验收：AC-PC-4 密钥扫描通过，案例可公开
  - 依赖：M6-T1.2

- [ ] **M6-T1.4** 验证案例可独立编译运行：`cargo build --bin sz_pay_pattern` + `cargo run --bin sz_pay_pattern`（需本地 DB）
  - 关联需求：REQ-PC-001
  - 关联设计：design.md §2.6.7
  - 验收：AC-PC-1 案例可独立编译运行
  - 依赖：M6-T1.3

## 7.2 M6-T2：README 更新基于 sz-pay 案例（REQ-PC-002）

- [ ] **M6-T2.1** 确认 README 成熟度更新已在 M2-T4 完成；补充 examples/sz_pay_pattern.rs 案例引用
  - 关联需求：REQ-PC-002
  - 关联设计：design.md §2.6.6
  - 验收：AC-PC-2 README 含 sz-pay 生产使用证据 + 案例引用
  - 依赖：M6-T1.4

## 7.3 M6-T3：10 feature 全组合编译验证（AC-ALL-4, AC-ALL-8）

- [ ] **M6-T3.1** 运行 `cargo check --workspace --all-targets --all-features`，验证 10 聚合 + 6 细粒度 feature 全组合编译通过（纳入既有门禁 10 Feature 全组合编译）
  - 关联需求：AC-ALL-4, AC-ALL-8
  - 关联设计：design.md §2.7.3
  - 验收：全组合编译通过；feature 正交性验证（可独立启用）
  - 依赖：M6-T2.1

## 7.4 M6-T4：五方言集成测试（AC-ALL-7）

- [ ] **M6-T4.1** 运行五方言集成测试：`cargo test --workspace -- --ignored`（MySQL/PostgreSQL/SQLite/Oracle/MSSQL 真实服务集成测试），验证五方言行为一致
  - 关联需求：AC-ALL-7
  - 关联设计：design.md §2.8.2
  - 验收：五方言集成测试全部通过；行为一致
  - 依赖：M6-T3.1

## 7.5 M6-T5：sz-pay/sz-rust 零回归验证（AC-ALL-5）

- [ ] **M6-T5.1** 在 sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）运行 `cargo test`，验证 5139 测试零回归（ADR-0001 不修改下游代码，仅验证 sz-orm 升级兼容性）
  - 关联需求：AC-ALL-5
  - 关联设计：design.md §2.6.6
  - 验收：AC-ALL-5 sz-pay 5139 测试零回归
  - 依赖：M6-T4.1

## 7.6 M6-T6：性能基准不回退验证（AC-ALL-6）

- [ ] **M6-T6.1** 运行既有性能基准，验证 v3.3.0 性能基准不回退（冷启动 P95 ≤ 20ms、计划缓存命中率 ≥ 80%、零拷贝分配减少 ≥ 50%、SIMD 吞吐量 ≥ 2x、跨实例失效延迟 ≤ 50ms、GraphQL N+1 查询次数 ≤ 2、多租户隔离开销 ≤ 5μs/查询、AI 建议延迟 ≤ 10s P95）
  - 关联需求：AC-ALL-6
  - 关联设计：design.md §2.8.2
  - 验收：AC-ALL-6 性能基准不回退
  - 依赖：M6-T5.1

## 7.7 M6-T7：10 道门禁全通过（AC-ALL-8, AC-ALL-9）

- [ ] **M6-T7.1** 运行 AGENTS.md 定义的全部 10 道门禁：
  1. `cargo fmt --all -- --check`
  2. `cargo check --workspace --all-targets`
  3. `cargo clippy --workspace --all-targets -- -D warnings`
  4. `cargo test --workspace`
  5. `cargo doc --workspace --no-deps --all-features`
  6. `cargo audit` + `cargo deny check`
  7. `cargo test --workspace -- --ignored`
  8. `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'`（零结果）
  9. `scripts/check-sql-injection.ps1`
  10. `cargo check --workspace --all-targets --all-features`
  11. `git diff --name-only HEAD`（ADR-0001 上游仓库未修改检查）
  - 关联需求：AC-ALL-8, AC-ALL-9
  - 关联设计：AGENTS.md 10 道门禁
  - 验收：10 道门禁全通过；每结论附 file:line 证据（审计合规铁律）
  - 依赖：M6-T6.1

## 7.8 M6-T8：版本发布（AC-ALL-1~10）

- [ ] **M6-T8.1** 更新 `Cargo.toml` workspace.package.version 为 `3.4.0`，运行 `cargo check --workspace` 确认版本更新无编译错误
  - 关联需求：AC-ALL-1~10
  - 关联设计：design.md §2.8.2
  - 验收：版本更新为 3.4.0，编译通过
  - 依赖：M6-T7.1

- [ ] **M6-T8.2** 验证 31 条 REQ 全部满足（REQ-TC-001~005 + REQ-AR-001~006 + REQ-PF-001~007 + REQ-TS-001~005 + REQ-DOC-001~004 + REQ-PC-001~004），生成验收报告附 file:line 证据
  - 关联需求：AC-ALL-10
  - 关联设计：spec §9.7
  - 验收：AC-ALL-10 31 条 REQ 全部满足；验收报告附证据
  - 依赖：M6-T8.1

- [ ] **M6-T8.3** 发布 sz-orm-core 3.4.0 到 crates.io（`cargo publish --dry-run` 验证后 `cargo publish`），更新 `服务器信息.md` 发布记录
  - 关联需求：AC-ALL-1~10
  - 关联设计：design.md §2.8.2
  - 验收：crates.io 发布成功；发布记录更新
  - 依赖：M6-T8.2

---

# 8. 依赖关系图

## 8.1 里程碑间依赖

```
M1 测试覆盖补齐 ──→ M2 架构改进 ──→ M3 性能优化 ──→ M4 类型安全 ──→ M5 文档生态 ──→ M6 集成验证与发布
```

## 8.2 关键任务依赖链

```
M1-T1.1 (feature gate) ──→ M1-T1.2 (sz-orm-es real) ──→ M1-T4.1 (真实 ES 集成测试)
                        ──→ M1-T1.3 (sz-orm-config real) ──→ M1-T5.1 (Consul 客户端) ──→ M1-T5.3 (Consul 集成测试)

M1-T7.2 (M1 门禁) ──→ M2-T1.1 (M2 feature gate) ──→ M2-T3.1 (313 API 清单) ──→ M2-T3.5 (移除 docs.rs cfg)
                                                                ──→ M2-T7.1 (M2 门禁)

M2-T7.2 (M2 门禁) ──→ M3-T1.1 (M3 feature gate) ──→ M3-T2.1 (SmallString) ──→ M3-T8.1 (差分测试)
                                                ──→ M3-T3.1 (enum dispatch) ──→ M3-T8.1
                                                ──→ M3-T4.1 (zero-copy L2) ──→ M3-T8.1
                                                ──→ M3-T5.1 (Box<str>) ──→ M3-T8.1
                                                                    ──→ M3-T9.1 (M3 门禁)

M3-T9.1 (M3 门禁) ──→ M4-T1.1 (M4 feature gate) ──→ M4-T2.1 (Schema 列名常量) ──→ M4-T6.1 (M4 门禁)
                                                ──→ M4-T3.1 (Column<T>) ──→ M4-T6.1
                                                ──→ M4-T4.1 (typed DSL) ──→ M4-T6.1

M4-T6.1 (M4 门禁) ──→ M5-T1.1 (M5 feature gate) ──→ M5-T3.1 (Diesel 指南) ──→ M5-T7.1 (M5 门禁)
                                                ──→ M5-T4.1 (SeaORM 指南) ──→ M5-T7.1
                                                ──→ M5-T5.1 (SQLx 指南) ──→ M5-T7.1

M5-T7.1 (M5 门禁) ──→ M6-T1.1 (案例 feature) ──→ M6-T1.2 (sz_pay_pattern.rs) ──→ M6-T7.1 (10 门禁) ──→ M6-T8.3 (发布)
```

## 8.3 并行机会

| 里程碑 | 可并行任务 | 说明 |
|--------|-----------|------|
| M1 | M1-T2.1~T2.21（18 扩展包测试） | 不同包独立，可分配多人并行 |
| M1 | M1-T3（INSERT IGNORE）与 M1-T4（ES）与 M1-T5（Config） | 三个方向独立 |
| M2 | M2-T3（313 API 文档）与 M2-T5（async trait 评估）与 M2-T6（query-builder 指南） | 三个方向独立 |
| M3 | M3-T2（SmallString）与 M3-T3（enum dispatch）与 M3-T4（zero-copy）与 M3-T5（Box<str>） | 四项优化不同模块独立 |
| M4 | M4-T2（Schema derive）M4-T3（Column<T>）M4-T4（typed DSL） | 三项不同模块独立 |
| M5 | M5-T3（Diesel）M5-T4（SeaORM）M5-T5（SQLx） | 三份指南独立 |

---

# 9. 验收标准对照表

| 验收标准 | 对应任务 | 验证方式 |
|---------|---------|---------|
| AC-TC-1（18 包测试通过 + 覆盖率 ≥ 60%） | M1-T2.1~T2.21, M1-T6.1 | `cargo test -p <package>` + `cargo tarpaulin -p <package>` |
| AC-TC-2（INSERT IGNORE 修复） | M1-T3.1, M1-T3.2 | `cargo test -p sz-orm-core --test integration_mysql test_mysql_insert_or_ignore_duplicate` |
| AC-TC-3（sz-orm-es real + 真实 ES） | M1-T4.1, M1-T4.2 | `cargo test -p sz-orm-es --features real -- --ignored` |
| AC-TC-4（sz-orm-config real-consul/nacos） | M1-T5.1~T5.5 | `cargo test -p sz-orm-config --features real-consul -- --ignored` |
| AC-TC-5（覆盖率 ≥ 60% + 无效覆盖拒绝） | M1-T6.1, M1-T6.2 | tarpaulin 报告 + 代码审查 |
| AC-TC-6（全 workspace 测试 ≥ 6,327 + 新增） | M1-T7.2 | `cargo test --workspace` |
| AC-AR-1（real feature 占位） | M2-T2.1 | `cargo check -p sz-orm-es --features real` |
| AC-AR-2（313 pub API 文档） | M2-T3.1~T3.5 | `cargo doc --workspace --no-deps` 无警告 |
| AC-AR-3（README 更新） | M2-T4.1, M2-T4.2 | README 内容审查 |
| AC-AR-4（async trait 评估） | M2-T5.1~T5.3 | 评估文档审查 |
| AC-AR-5（query-builder 指南） | M2-T6.1~T6.3 | 指南文档审查 |
| AC-AR-6（doctest 通过） | M2-T7.1 | `cargo test --workspace --doc` |
| AC-PF-1（SmallString ≥ 1.15x） | M3-T2.3 | criterion 基准证据 |
| AC-PF-2（enum dispatch ≤ 0.7x） | M3-T3.3 | criterion 基准证据 |
| AC-PF-3（zero-copy L2 ≤ 0.5x） | M3-T4.3 | 分配计数证据 |
| AC-PF-4（result_map 评估） | M3-T6.2 | 评估文档审查 |
| AC-PF-5（Box<str> 内存减少） | M3-T5.2 | size_of 证据 |
| AC-PF-6（6 组基准） | M3-T7.1, M3-T7.2 | criterion 基准证据 |
| AC-PF-7（差分测试一致） | M3-T8.1 | `cargo test --workspace --all-features` |
| AC-TS-1（列名常量生成） | M4-T2.1, M4-T2.2 | trybuild 编译测试 |
| AC-TS-2（Column<T> 跨表引用） | M4-T3.1~T3.4 | trybuild 编译测试 |
| AC-TS-3（DSL 类型安全） | M4-T4.1~T4.4 | 差分测试 |
| AC-TS-4（零运行时开销） | M4-T5.1 | criterion 基准证据 |
| AC-TS-5（参数化绑定） | M4-T5.2 | SQL 注入扫描 |
| AC-DOC-1（313 API 文档） | M5-T2.1 | `cargo doc --workspace --no-deps` 无警告 |
| AC-DOC-2（README 更新） | M5-T2.1 | README 内容审查 |
| AC-DOC-3（三份迁移指南） | M5-T3.1, M5-T4.1, M5-T5.1 | 指南文档审查 + doctest |
| AC-DOC-4（doctest + CI 检查） | M5-T6.1 | `cargo test --workspace --doc` |
| AC-PC-1（案例脱敏 + 可编译） | M6-T1.2, M6-T1.4 | `cargo build --bin sz_pay_pattern` |
| AC-PC-2（README 生产证据） | M6-T2.1 | README 内容审查 |
| AC-PC-3（生产运行数据，可选） | M6-T2.1 | 数据脱敏审查 |
| AC-PC-4（密钥扫描） | M6-T1.3 | `grep -r "password\|secret\|key" examples/` |
| AC-ALL-1（无 Breaking Change） | M6-T8.2 | API 兼容性验证 |
| AC-ALL-2（cargo test 全通过） | M6-T7.1 | `cargo test --workspace` |
| AC-ALL-3（clippy 零警告） | M6-T7.1 | `cargo clippy --workspace --all-targets -- -D warnings` |
| AC-ALL-4（feature 隔离） | M6-T3.1 | `cargo check --all-features` |
| AC-ALL-5（下游零回归） | M6-T5.1 | sz-pay `cargo test` |
| AC-ALL-6（性能不回退） | M6-T6.1 | 性能基准对比 |
| AC-ALL-7（五方言一致） | M6-T4.1 | `cargo test --workspace -- --ignored` |
| AC-ALL-8（10 门禁全通过） | M6-T7.1 | gate.ps1 |
| AC-ALL-9（审计合规铁律） | M6-T7.1 | 每结论附 file:line 证据 |
| AC-ALL-10（31 REQ 全满足） | M6-T8.2 | 验收报告 |

---

# 10. 风险登记与缓解措施

| 编号 | 风险 | 等级 | 关联任务 | 缓解措施 |
|------|------|------|---------|---------|
| R-01 | 18 扩展包补齐测试工作量巨大 | 高 | M1-T2.1~T2.21 | 分 4 批补齐，每批附进度跟踪；可分配多人并行（18 包独立） |
| R-02 | 真实 ES/Consul/Nacos 环境不可用 | 中 | M1-T4.1, M1-T5.3, M1-T5.4 | 集成测试标注 `#[ignore]`，默认不运行；本机 MySQL/PG/Oracle 可用 |
| R-03 | MySQL INSERT IGNORE 修复引入回归 | 中 | M1-T3.1 | 修复后运行全 workspace 测试验证；仅修改测试表 DDL |
| R-04 | 313 pub API 文档补齐工作量巨大 | 高 | M2-T3.1~T3.5 | 分 3 批补齐（公开 > 内部 > 测试），每批 `cargo doc` 验证 |
| R-05 | async trait 迁移影响过大 | 中 | M2-T5.1~T5.3 | v3.4.0 仅评估文档，不强制迁移；HRTB 技术原因文档说明 |
| R-06 | SmallString 对长字符串无收益 | 低 | M3-T2.3 | 基准对比区分短/长字符串场景，短字符串场景收益证据 |
| R-07 | enum dispatch 五方言行为差异 | 中 | M3-T3.2 | 五方言集成测试覆盖，行为差分测试验证一致 |
| R-08 | zero-copy L2 与 Redis 不兼容 | 中 | M3-T4.2 | 兼容性测试覆盖，序列化格式不变 |
| R-09 | 性能优化破坏正确性 | 高 | M3-T8.1 | 差分测试 + 既有 6,327+ 测试套件覆盖 |
| R-10 | 性能基准结果波动 | 中 | M3-T7.1 | 基准多次运行取中位数 + 置信区间，CI 环境标准化 |
| R-11 | derive(Schema) 复杂类型支持不足 | 中 | M4-T2.3 | 跳过不支持字段并告警，用户可手动标注 |
| R-12 | Column<T> 与 &str 混用困惑 | 低 | M4-T3.2 | 支持混用（Deref），文档建议统一使用 Column<T> |
| R-13 | DSL 与 QueryBuilder 不一致 | 中 | M4-T4.4 | 差分测试覆盖，不一致即修复 |
| R-14 | 迁移指南版本不匹配 | 低 | M5-T3.1~T5.2 | 指南标注基于的竞品版本，定期更新 |
| R-15 | sz-pay 生产环境不可访问 | 中 | M6-T5.1 | 跳过生产运行数据收集（可选），标注"未收集" |
| R-16 | 案例脱敏遗漏 | 高 | M6-T1.3 | 脱敏审查 + 密钥扫描，遗漏即修复 |
| R-17 | feature 组合矩阵膨胀 | 低 | M6-T3.1 | 纳入既有门禁 10 Feature 全组合编译，feature 正交性设计 |
| R-18 | 下游 sz-pay 升级回归 | 中 | M6-T5.1 | feature gate 默认关闭确保零行为变更；实际回归验证 |
| R-19 | 五方言行为差异 | 中 | M6-T4.1 | 五方言集成测试覆盖；优化在 core/macros 层统一抽象 |
| R-20 | 文档与代码不符 | 中 | M2-T7.1, M5-T6.1 | cargo doc + doctest + CI 定期检查 |

---

# 11. 工程化审查规范（沿用 AGENTS.md）

## 11.1 10 道门禁（提交前必过）

| # | 门禁 | 命令 | 关联任务 |
|---|------|------|---------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` | 各里程碑门禁任务 |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` | 各里程碑门禁任务 |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` | 各里程碑门禁任务 |
| 4 | test 单元/集成测试 | `cargo test --workspace` | 各里程碑门禁任务 |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` | M2-T7.1, M5-T6.1, M6-T7.1 |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` | M6-T7.1 |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` | M6-T4.1 |
| 8 | 禁止占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` | 各里程碑门禁任务 |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` | M6-T7.1 |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` | M6-T3.1 |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD`（ADR-0001） | M6-T7.1 |

## 11.2 五维审查（每次 PR 必做）

正确性 → 可读性 → 架构 → 安全性 → 性能

## 11.3 审计合规铁律

- 每个缺陷修复必须附 `file:line` 证据 + 测试验证
- 禁止未验证即标记 ✅
- 多项修复必须逐项验证，禁止批量声称"全部通过"
- 修复后必须运行 `cargo test` 并附输出

## 11.4 AI 辅助开发 10 条硬约束

1. 禁止占位实现（todo!/unimplemented!/unreachable!）
2. 强制参数化查询（禁止 SQL 字符串拼接）
3. API 兼容性（签名变更必须同步更新所有调用方和测试）
4. 五维审查必过
5. unsafe 零容忍（必须有 // SAFETY: 注释）
6. 禁止 mock 逃逸
7. 门禁前置（主动运行 gate.ps1）
8. 跨平台意识
9. Feature 隔离
10. 教训记忆（阅读防御追溯表）