//! 可观测性自动闭环（v10.1.0）
//!
//! 慢查询→AI 分析→索引建议 + CRITICAL 告警→诊断报告，异步执行不阻塞主路径。
//! 默认 disabled，向后兼容 v10.0.0。
//! 配置类型定义在 sz-orm-core::performance_linkage 中，此处仅实现引擎逻辑。

use std::sync::{Arc, Mutex};
use sz_orm_core::pool::{PoolEvent, PoolEventCallback};
use sz_orm_core::ObservabilityLoopConfig;

/// 闭环事件
#[derive(Debug, Clone)]
pub enum LoopEvent {
    SlowQuery {
        sql: String,
        elapsed_ms: u64,
        plan: Option<String>,
    },
    PressureAlert {
        severity: AlertSeverity,
        detail: String,
    },
    ComplianceViolation {
        module: String,
        detail: String,
    },
}

/// 告警级别（闭环内部用，避免跨包依赖）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertSeverity {
    Warn,
    Critical,
}

/// 闭环指标
#[derive(Debug, Clone, Default)]
pub struct LoopMetrics {
    pub trigger_count: u64,
    pub analysis_count: u64,
    pub suggestion_count: u64,
    pub degradation_count: u64,
    pub avg_latency_ms: u64,
}

impl LoopMetrics {
    pub fn record_trigger(&mut self) {
        self.trigger_count += 1;
    }

    pub fn record_analysis(&mut self) {
        self.analysis_count += 1;
    }

    pub fn record_suggestion(&mut self) {
        self.suggestion_count += 1;
    }

    pub fn record_degradation(&mut self) {
        self.degradation_count += 1;
    }
}

/// 可观测性闭环引擎
pub struct ObservabilityLoop {
    config: ObservabilityLoopConfig,
    metrics: Mutex<LoopMetrics>,
    /// v10.1.0：事件通道发送端（None 时不支持 push_event，仅支持 handle_event）
    event_tx: Option<tokio::sync::mpsc::Sender<LoopEvent>>,
}

impl ObservabilityLoop {
    pub fn new(config: ObservabilityLoopConfig) -> Self {
        Self {
            config,
            metrics: Mutex::new(LoopMetrics::default()),
            event_tx: None,
        }
    }

    /// v10.1.0：创建带事件通道的闭环引擎
    ///
    /// 返回 (引擎, 事件接收端)。push_event 将事件推入通道，由消费端异步处理。
    /// 通道容量取 `config.event_queue_capacity`。
    pub fn new_with_channel(
        config: ObservabilityLoopConfig,
    ) -> (Self, tokio::sync::mpsc::Receiver<LoopEvent>) {
        let (tx, rx) = tokio::sync::mpsc::channel(config.event_queue_capacity as usize);
        let loop_engine = Self {
            config,
            metrics: Mutex::new(LoopMetrics::default()),
            event_tx: Some(tx),
        };
        (loop_engine, rx)
    }

    pub fn config(&self) -> &ObservabilityLoopConfig {
        &self.config
    }

    pub fn metrics(&self) -> LoopMetrics {
        self.metrics.lock().expect("metrics lock poisoned").clone()
    }

    /// 处理事件（同步入口，实际 AI 分析在 M3.2 异步消费中实现）
    pub fn handle_event(&self, event: &LoopEvent) {
        let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
        metrics.record_trigger();
        match event {
            LoopEvent::SlowQuery { .. } => {
                if self.config.slow_query_to_ai {
                    metrics.record_analysis();
                }
            }
            LoopEvent::PressureAlert { severity, .. } => {
                if *severity == AlertSeverity::Critical && self.config.alert_to_diagnosis {
                    metrics.record_analysis();
                }
            }
            LoopEvent::ComplianceViolation { .. } => {}
        }
    }

    /// v10.1.0：推送事件到异步队列（不阻塞主路径）
    ///
    /// 队列满时丢弃事件并递增 `degradation_count`（背压保护）。
    /// 未配置通道时（event_tx 为 None）直接调用 handle_event 同步处理。
    pub fn push_event(&self, event: LoopEvent) {
        if let Some(ref tx) = self.event_tx {
            match tx.try_send(event) {
                Ok(()) => {
                    let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
                    metrics.record_trigger();
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
                    metrics.record_degradation();
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
                    metrics.record_degradation();
                }
            }
        } else {
            self.handle_event(&event);
        }
    }

    /// 记算平均延迟（生产消费点：基于触发次数计算真实平均延迟）
    pub fn update_avg_latency(&self, total_latency_ms: u64) {
        let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
        if let Some(avg) = total_latency_ms.checked_div(metrics.trigger_count) {
            metrics.avg_latency_ms = avg;
        }
    }

    /// 记算建议率（生产消费点：建议数/触发数，衡量闭环有效性）
    pub fn suggestion_rate(&self) -> f64 {
        let metrics = self.metrics.lock().expect("metrics lock poisoned");
        if metrics.trigger_count == 0 {
            0.0
        } else {
            metrics.suggestion_count as f64 / metrics.trigger_count as f64
        }
    }

    /// 记算降级率（生产消费点：降级数/触发数，衡量 LLM 可用性）
    pub fn degradation_rate(&self) -> f64 {
        let metrics = self.metrics.lock().expect("metrics lock poisoned");
        if metrics.trigger_count == 0 {
            0.0
        } else {
            metrics.degradation_count as f64 / metrics.trigger_count as f64
        }
    }

    /// 记录降级（LLM 不可用时调用）
    pub fn record_degradation(&self) {
        let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
        metrics.record_degradation();
    }

    /// 记录建议生成
    pub fn record_suggestion(&self) {
        let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
        metrics.record_suggestion();
    }
}

/// v10.1.0：将 PoolEvent 映射为 LoopEvent
///
/// 映射规则：
/// - `BackpressureWarning` / `PoolExhausted` → `PressureAlert { Critical }`
/// - `AcquireTimeout` → `PressureAlert { Warn }`
/// - 其他事件不映射（返回 None）
pub fn pool_event_to_loop_event(event: &PoolEvent) -> Option<LoopEvent> {
    match event {
        PoolEvent::BackpressureWarning {
            queue_len,
            threshold,
            ts_ms,
        } => Some(LoopEvent::PressureAlert {
            severity: AlertSeverity::Critical,
            detail: format!(
                "backpressure: queue={}/{}, ts={}",
                queue_len, threshold, ts_ms
            ),
        }),
        PoolEvent::PoolExhausted { strategy, ts_ms } => Some(LoopEvent::PressureAlert {
            severity: AlertSeverity::Critical,
            detail: format!("pool exhausted: strategy={}, ts={}", strategy, ts_ms),
        }),
        PoolEvent::AcquireTimeout => Some(LoopEvent::PressureAlert {
            severity: AlertSeverity::Warn,
            detail: "acquire timeout".to_string(),
        }),
        _ => None,
    }
}

/// v10.1.0：创建 Pool 事件回调，将 PoolEvent 映射为 LoopEvent 并推入闭环引擎
///
/// 生产调用点：上层应用创建 ObservabilityLoop 后调用此函数获取回调，
/// 通过 `PoolConfigBuilder::on_event` 设置到 Pool。
pub fn create_pool_event_callback(loop_engine: Arc<ObservabilityLoop>) -> PoolEventCallback {
    Arc::new(move |event: PoolEvent| {
        if let Some(loop_event) = pool_event_to_loop_event(&event) {
            loop_engine.push_event(loop_event);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_config_default_disabled() {
        let config = ObservabilityLoopConfig::default();
        assert!(!config.enabled);
        assert!(!config.slow_query_to_ai);
        assert!(!config.alert_to_diagnosis);

        let loop_engine = ObservabilityLoop::new(config);
        loop_engine.push_event(LoopEvent::PressureAlert {
            severity: AlertSeverity::Critical,
            detail: "test".to_string(),
        });
        assert_eq!(
            loop_engine.metrics().trigger_count,
            1,
            "push_event should increment trigger_count"
        );
    }

    #[test]
    fn test_loop_metrics_increments() {
        let config = ObservabilityLoopConfig {
            enabled: true,
            slow_query_to_ai: true,
            alert_to_diagnosis: true,
            ..Default::default()
        };
        let loop_engine = ObservabilityLoop::new(config);

        loop_engine.handle_event(&LoopEvent::SlowQuery {
            sql: "SELECT * FROM t".to_string(),
            elapsed_ms: 5000,
            plan: None,
        });
        loop_engine.handle_event(&LoopEvent::PressureAlert {
            severity: AlertSeverity::Critical,
            detail: "pool exhausted".to_string(),
        });

        let metrics = loop_engine.metrics();
        assert_eq!(metrics.trigger_count, 2);
        assert_eq!(metrics.analysis_count, 2);
    }

    #[test]
    fn test_loop_degradation() {
        let loop_engine = ObservabilityLoop::new(ObservabilityLoopConfig::default());
        loop_engine.record_degradation();
        loop_engine.record_degradation();
        let metrics = loop_engine.metrics();
        assert_eq!(metrics.degradation_count, 2);
    }

    #[test]
    fn test_loop_disabled_no_analysis() {
        let config = ObservabilityLoopConfig {
            enabled: false,
            slow_query_to_ai: false,
            ..Default::default()
        };
        let loop_engine = ObservabilityLoop::new(config);
        loop_engine.handle_event(&LoopEvent::SlowQuery {
            sql: "SELECT 1".to_string(),
            elapsed_ms: 100,
            plan: None,
        });
        let metrics = loop_engine.metrics();
        assert_eq!(metrics.trigger_count, 1);
        assert_eq!(metrics.analysis_count, 0);
    }

    #[test]
    fn test_suggestion_rate_calculation() {
        let loop_engine = ObservabilityLoop::new(ObservabilityLoopConfig::default());
        loop_engine.handle_event(&LoopEvent::SlowQuery {
            sql: "SELECT 1".to_string(),
            elapsed_ms: 100,
            plan: None,
        });
        loop_engine.record_suggestion();
        loop_engine.record_suggestion();
        assert_eq!(loop_engine.suggestion_rate(), 2.0);
    }

    #[test]
    fn test_degradation_rate_calculation() {
        let loop_engine = ObservabilityLoop::new(ObservabilityLoopConfig::default());
        loop_engine.handle_event(&LoopEvent::SlowQuery {
            sql: "SELECT 1".to_string(),
            elapsed_ms: 100,
            plan: None,
        });
        loop_engine.record_degradation();
        assert_eq!(loop_engine.degradation_rate(), 1.0);
    }

    #[test]
    fn test_avg_latency_update() {
        let loop_engine = ObservabilityLoop::new(ObservabilityLoopConfig::default());
        loop_engine.handle_event(&LoopEvent::SlowQuery {
            sql: "SELECT 1".to_string(),
            elapsed_ms: 100,
            plan: None,
        });
        loop_engine.handle_event(&LoopEvent::SlowQuery {
            sql: "SELECT 2".to_string(),
            elapsed_ms: 200,
            plan: None,
        });
        loop_engine.update_avg_latency(300);
        assert_eq!(loop_engine.metrics().avg_latency_ms, 150);
    }

    #[test]
    fn test_rates_zero_when_no_triggers() {
        let loop_engine = ObservabilityLoop::new(ObservabilityLoopConfig::default());
        assert_eq!(loop_engine.suggestion_rate(), 0.0);
        assert_eq!(loop_engine.degradation_rate(), 0.0);
    }

    #[tokio::test]
    async fn test_push_event_with_channel() {
        let config = ObservabilityLoopConfig {
            enabled: true,
            event_queue_capacity: 10,
            ..Default::default()
        };
        let (loop_engine, mut rx) = ObservabilityLoop::new_with_channel(config);

        loop_engine.push_event(LoopEvent::SlowQuery {
            sql: "SELECT 1".to_string(),
            elapsed_ms: 100,
            plan: None,
        });

        let event = rx.recv().await;
        assert!(event.is_some());
        let metrics = loop_engine.metrics();
        assert_eq!(metrics.trigger_count, 1);
    }

    #[tokio::test]
    async fn test_push_event_queue_full_degrades() {
        let config = ObservabilityLoopConfig {
            enabled: true,
            event_queue_capacity: 1,
            ..Default::default()
        };
        let (loop_engine, _rx) = ObservabilityLoop::new_with_channel(config);

        loop_engine.push_event(LoopEvent::SlowQuery {
            sql: "SELECT 1".to_string(),
            elapsed_ms: 100,
            plan: None,
        });
        loop_engine.push_event(LoopEvent::SlowQuery {
            sql: "SELECT 2".to_string(),
            elapsed_ms: 200,
            plan: None,
        });

        let metrics = loop_engine.metrics();
        assert_eq!(metrics.trigger_count, 1);
        assert_eq!(metrics.degradation_count, 1);
    }

    #[test]
    fn test_push_event_without_channel_falls_back_to_handle() {
        let config = ObservabilityLoopConfig {
            enabled: true,
            slow_query_to_ai: true,
            ..Default::default()
        };
        let loop_engine = ObservabilityLoop::new(config);

        loop_engine.push_event(LoopEvent::SlowQuery {
            sql: "SELECT 1".to_string(),
            elapsed_ms: 100,
            plan: None,
        });

        let metrics = loop_engine.metrics();
        assert_eq!(metrics.trigger_count, 1);
        assert_eq!(metrics.analysis_count, 1);
    }

    #[test]
    fn test_pool_event_to_loop_event_backpressure() {
        let event = PoolEvent::BackpressureWarning {
            queue_len: 10,
            threshold: 8,
            ts_ms: 1000,
        };
        let loop_event = pool_event_to_loop_event(&event);
        assert!(loop_event.is_some());
        if let Some(LoopEvent::PressureAlert { severity, detail }) = loop_event {
            assert_eq!(severity, AlertSeverity::Critical);
            assert!(detail.contains("backpressure"));
        } else {
            panic!("should be PressureAlert");
        }
    }

    #[test]
    fn test_pool_event_to_loop_event_exhausted() {
        let event = PoolEvent::PoolExhausted {
            strategy: "fail_fast",
            ts_ms: 2000,
        };
        let loop_event = pool_event_to_loop_event(&event);
        assert!(loop_event.is_some());
        if let Some(LoopEvent::PressureAlert { severity, detail }) = loop_event {
            assert_eq!(severity, AlertSeverity::Critical);
            assert!(detail.contains("exhausted"));
        } else {
            panic!("should be PressureAlert");
        }
    }

    #[test]
    fn test_pool_event_to_loop_event_timeout() {
        let event = PoolEvent::AcquireTimeout;
        let loop_event = pool_event_to_loop_event(&event);
        assert!(loop_event.is_some());
        if let Some(LoopEvent::PressureAlert { severity, .. }) = loop_event {
            assert_eq!(severity, AlertSeverity::Warn);
        } else {
            panic!("should be PressureAlert");
        }
    }

    #[test]
    fn test_pool_event_to_loop_event_connection_created_none() {
        let event = PoolEvent::ConnectionCreated;
        let loop_event = pool_event_to_loop_event(&event);
        assert!(loop_event.is_none());
    }

    #[tokio::test]
    async fn test_create_pool_event_callback_pushes_events() {
        let config = ObservabilityLoopConfig {
            enabled: true,
            event_queue_capacity: 10,
            ..Default::default()
        };
        let (loop_engine, mut rx) = ObservabilityLoop::new_with_channel(config);
        let callback = create_pool_event_callback(Arc::new(loop_engine));

        callback(PoolEvent::BackpressureWarning {
            queue_len: 5,
            threshold: 3,
            ts_ms: 100,
        });

        let event = rx.recv().await;
        assert!(event.is_some());
    }
}
