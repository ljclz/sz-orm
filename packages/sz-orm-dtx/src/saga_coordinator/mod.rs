//! v8.0.0 Saga 跨服务编排模块（`dtx-saga-coordinator` feature gate）
//!
//! 提供 `SagaCoordinator` 跨服务编排 + 鉴权 + 并行步骤 + 条件分支 + 补偿链 + OpenTelemetry 追踪。
//! 复用 v7.9.0 `Saga`/`SagaManager`（`packages/sz-orm-dtx/src/saga.rs:377`）、
//! `IdempotencyChecker`（`saga.rs:2055`）、`AutoCompensationGenerator`（`saga.rs:1955`）。

pub mod coordinator;

pub use coordinator::{
    AuthContext, CrossServiceSagaDef, DistError, ParallelGroup, SagaCoordConfig, SagaCoordResult,
    SagaCoordinator, SagaStepDef, TraceSpan,
};
