# ADR-0011: 异步运行时仅支持 Tokio

- **状态**: Accepted
- **日期**: 2026-08-03
- **相关代码**: `packages/sz-orm-core/src/pool.rs`、`packages/sz-orm-core/src/query.rs`、`packages/sz-orm-macros/src/lib.rs`
- **决策**: sz-orm 仅支持 Tokio 异步运行时，不支持 async-std

## 背景

sz-orm 的核心连接池、查询执行、db-verify 编译期验证等组件均依赖 Tokio 提供的异步原语：

| Tokio API | 使用场景 | 出现次数 |
|-----------|---------|---------|
| `tokio::sync::Notify` | 连接池等待通知 | 多处 |
| `tokio::sync::broadcast` | 池状态广播 | 6 处 |
| `tokio::sync::Mutex/RwLock` | 共享状态保护 | 多处 |
| `tokio::time::timeout/sleep` | 超时控制、混沌测试 | 17 处 |
| `tokio::runtime::Runtime` | db-verify 编译期验证 | 2 处 |
| `tokio::task` | 任务调度 | 1 处 |

## 决策

**sz-orm 仅支持 Tokio 异步运行时。** 不支持 async-std，也不提供运行时抽象层。

## 理由

1. **Tokio 生态主导地位**：Rust 异步生态系统中 Tokio 是事实标准。sqlx、axum、actix-web、tonic 等核心库均以 Tokio 为首选运行时。

2. **sqlx 的 Tokio 优先设计**：sz-orm 的底层 DB 驱动 sqlx 虽然支持 async-std（通过 `sqlx-rt` feature 切换），但其高级功能（连接池、TLS、MySQL/PG 驱动）在 Tokio 下的测试覆盖率和社区支持远高于 async-std。

3. **连接池实现深度耦合**：自研连接池使用 `tokio::sync::Notify` 实现无锁等待队列，使用 `tokio::sync::broadcast` 实现池状态广播。抽象为运行时无关层需要引入 `async-lock`、`futures` 等额外依赖，且无法完全消除运行时差异（如 `Notify` 在 async-std 中无等价物）。

4. **db-verify 编译期验证**：`query!` 宏的 db-verify feature 在编译期创建 `tokio::runtime::Runtime` 执行 EXPLAIN。async-std 无等价的同步 runtime 创建 API（`async_std::task::block_on` 有嵌套调用限制）。

5. **维护成本**：支持双运行时意味着所有异步测试需运行两次、所有同步原语需抽象、CI 矩阵翻倍。对于 ORM 这一层，运行时选择应由应用层决定，而非框架层。

## 替代方案

| 方案 | 评估 |
|------|------|
| 提供 `async-runtime` feature 切换 | ❌ 维护成本过高，且 sqlx 的 async-std 支持本身就不如 Tokio 成熟 |
| 使用 `async-lock` 等跨运行时原语 | ❌ 引入额外依赖，且无法覆盖所有 Tokio 特有 API |
| 仅支持 Tokio（采纳） | ✅ 与生态一致，维护成本最低 |

## 用户迁移指南

若你的应用使用 async-std，可在 `main()` 入口处通过 `tokio::runtime::Runtime` 启动 Tokio，或在 `Cargo.toml` 中同时引入两个运行时（不推荐，会有命名冲突）。推荐方案是将应用迁移到 Tokio。

## 影响

- 评估报告中 `async-std` 行标记为 **不支持（设计决策）**，不计入技术债务。
- 未来如需支持其他运行时，必须通过 RFC 流程重新评估。
