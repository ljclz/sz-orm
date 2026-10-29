//! # 查询路径指标钩子（2026-10-28 架构评测 P1-3 可观测性接线）
//!
//! core 不能依赖 sz-orm-observability（依赖方向：observability → core），
//! 因此在 core 侧提供进程级钩子槽位：
//!
//! - 驱动层（sz-orm-sqlx 的连接包装）在语句执行完成后调用 [`record_query`]；
//! - sz-orm-observability 通过 [`install_query_metrics_hook`] 把
//!   `AlertMetricHandles`（13 个告警指标）桥接进该槽位。
//!
//! 未安装时开销为一次 `OnceLock::get` 原子读（无分支预测影响可忽略）。
//!
//! ## 诚实声明
//!
//! 安装动作由调用方显式执行（`sz_orm_observability::install_query_metrics()`），
//! 本模块**不会**自动启用任何采集——未安装时 [`record_query`] 是空操作。

use std::sync::{Arc, OnceLock};

/// 查询指标钩子：每次语句执行完成（成功或失败）回调一次
pub trait QueryMetricsHook: Send + Sync {
    /// `latency_us`：语句执行耗时（微秒）；`is_error`：是否以 Err 结束
    fn record(&self, latency_us: u64, is_error: bool);
}

static QUERY_METRICS_HOOK: OnceLock<Arc<dyn QueryMetricsHook>> = OnceLock::new();

/// 安装进程级查询指标钩子（幂等：重复安装返回 false，首个安装者生效）
pub fn install_query_metrics_hook(hook: Arc<dyn QueryMetricsHook>) -> bool {
    QUERY_METRICS_HOOK.set(hook).is_ok()
}

/// 查询钩子是否已安装（测试/诊断用）
pub fn query_metrics_hook_installed() -> bool {
    QUERY_METRICS_HOOK.get().is_some()
}

/// 驱动层热路径回调：语句执行完成后调用（未安装钩子时为空操作）
pub fn record_query(latency_us: u64, is_error: bool) {
    if let Some(hook) = QUERY_METRICS_HOOK.get() {
        hook.record(latency_us, is_error);
    }
}
