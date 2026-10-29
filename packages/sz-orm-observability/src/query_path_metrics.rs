//! 查询路径指标接入器（v10.13.0，2026-10-28 可观测性遗留项落地）
//!
//! [`AlertMetricHandles`] 提供延迟/操作/错误等写入句柄，但需要调用方自行管理
//! 计时与结果分类。本模块提供 [`QueryPathMetrics`]：封装三个最常用的写入动作
//! （延迟直方图 / 操作计数 / 错误计数），让应用层查询路径**一行代码**完成埋点。
//!
//! **接入方式（需手动接入）**：
//! ```no_run
//! use sz_orm_observability::{MetricsRegistry, QueryPathMetrics, register_alert_metrics};
//! use std::time::Instant;
//!
//! let registry = MetricsRegistry::new();
//! let handles = register_alert_metrics(&registry);
//! let metrics = QueryPathMetrics::new(handles);
//!
//! let start = Instant::now();
//! // ... 执行真实查询 ...
//! metrics.observe_query(start, Ok(()));   // 成功：延迟 + ops
//! metrics.observe_query(start, Err(()));  // 失败：延迟 + ops + errors
//! ```
//!
//! 完整可运行示例见 `examples/query_path_metrics.rs`。
//! 未接入时所有告警指标渲染为 0（注册 ≠ 有数据，见 [`crate::alert_metrics`]）。

use std::time::Instant;

use crate::AlertMetricHandles;

/// 查询路径指标接入器：封装延迟直方图 / 操作计数 / 错误计数写入。
///
/// 通过 [`AlertMetricHandles`] 创建，查询路径只需在请求完成时调用
/// [`observe_query`](Self::observe_query) 或 [`record`](Self::record)。
#[derive(Clone)]
pub struct QueryPathMetrics {
    handles: AlertMetricHandles,
}

impl QueryPathMetrics {
    /// 从告警指标句柄创建接入器。
    pub fn new(handles: AlertMetricHandles) -> Self {
        Self { handles }
    }

    /// 返回底层告警指标句柄（需要直接写入 SLO/soak 指标时使用）。
    pub fn handles(&self) -> &AlertMetricHandles {
        &self.handles
    }

    /// 开始一次查询计时（查询开始处调用）。
    #[inline]
    pub fn start() -> Instant {
        Instant::now()
    }

    /// 记录一次查询结果：按耗时写入延迟直方图，按结果分类写入操作/错误计数。
    ///
    /// - 成功：延迟 + `ops_total` +1
    /// - 失败：延迟 + `ops_total` +1 + `errors_total` +1
    ///
    /// `start` 应在查询开始处由 [`Self::start`] 取得。
    pub fn observe_query(&self, start: Instant, result: Result<(), ()>) {
        let latency_us = start.elapsed().as_micros() as f64;
        self.handles.record_latency_us(latency_us);
        self.handles.inc_ops(1.0);
        if result.is_err() {
            self.handles.inc_errors(1.0);
        }
    }

    /// 记录一次查询：显式传入耗时（µs）与是否失败。
    ///
    /// 与 [`observe_query`](Self::observe_query) 等价，但允许调用方自行计时
    /// （例如批量执行时统一计算耗时）。
    pub fn record(&self, latency_us: f64, is_error: bool) {
        self.handles.record_latency_us(latency_us);
        self.handles.inc_ops(1.0);
        if is_error {
            self.handles.inc_errors(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricsRegistry;

    fn setup() -> (MetricsRegistry, QueryPathMetrics) {
        let registry = MetricsRegistry::new();
        let handles = crate::register_alert_metrics(&registry);
        let metrics = QueryPathMetrics::new(handles);
        (registry, metrics)
    }

    #[test]
    fn test_observe_ok_records_latency_and_ops() {
        let (registry, metrics) = setup();
        let start = QueryPathMetrics::start();
        // 人为制造 ~1ms 耗时，确保直方图记录非零延迟
        std::thread::sleep(std::time::Duration::from_millis(1));
        metrics.observe_query(start, Ok(()));

        let text = registry.render();
        assert!(
            text.contains("sz_orm_ops_total 1"),
            "成功查询应写入 ops_total=1，实际输出: {text}"
        );
        assert!(
            text.contains("sz_orm_errors_total 0"),
            "成功查询不应增加错误计数（errors_total=0），实际输出: {text}"
        );
        assert!(
            text.contains("sz_orm_latency_us_bucket"),
            "成功查询应写入延迟直方图 bucket，实际输出: {text}"
        );
    }

    #[test]
    fn test_observe_err_records_errors() {
        let (registry, metrics) = setup();
        let start = QueryPathMetrics::start();
        metrics.observe_query(start, Err(()));

        let text = registry.render();
        assert!(
            text.contains("sz_orm_errors_total 1"),
            "失败查询应写入 errors_total=1，实际输出: {text}"
        );
        assert!(
            text.contains("sz_orm_ops_total 1"),
            "失败查询仍应写入 ops_total=1，实际输出: {text}"
        );
    }

    #[test]
    fn test_record_explicit_latency_and_error_flag() {
        let (registry, metrics) = setup();
        metrics.record(42.0, true);
        metrics.record(7.0, false);

        let text = registry.render();
        assert!(
            text.contains("sz_orm_errors_total 1"),
            "显式 is_error=true 应写入 errors_total=1，实际输出: {text}"
        );
        assert!(
            text.contains("sz_orm_ops_total 2"),
            "两次 record 应写入 ops_total=2，实际输出: {text}"
        );
    }

    #[test]
    fn test_handles_returns_underlying_handles() {
        let (registry, metrics) = setup();
        // 通过返回的句柄直接写入 SLO 指标，验证句柄引用可用
        metrics.handles().set_slo(0.1, 0.05, 0.8, false);
        let h = metrics.handles();
        h.inc_ops(1.0);
        // 行为断言：注册表渲染输出必须反映写入（ops_total ≥1、SLO gauge 可见）
        let text = registry.render();
        assert!(
            text.contains("sz_orm_ops_total"),
            "句柄写入 ops_total 后渲染应包含该指标: {text}"
        );
        assert!(
            text.contains("sz_orm_slo_error_budget_remaining"),
            "set_slo 写入后渲染应包含 SLO gauge: {text}"
        );
    }
}
