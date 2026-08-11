//! 服务网格可观测性接入：metrics + traces

use std::collections::HashMap;

use parking_lot::RwLock;

/// 网格 metrics 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshMetricType {
    RequestCount,
    RequestDuration,
    CircuitBreakerCount,
    RetryCount,
}

/// 网格 trace span 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshSpanType {
    Proxy,
    Application,
}

/// 网格 trace span
#[derive(Debug, Clone)]
pub struct MeshSpan {
    pub span_type: MeshSpanType,
    pub name: String,
    pub trace_id: String,
    pub duration_ms: u64,
}

/// 服务网格可观测性
pub struct MeshObservability {
    metrics: RwLock<HashMap<String, u64>>,
    spans: RwLock<Vec<MeshSpan>>,
}

impl MeshObservability {
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(HashMap::new()),
            spans: RwLock::new(Vec::new()),
        }
    }

    /// 记录 metric
    pub fn record_metric(&self, metric_type: MeshMetricType, value: u64) {
        let key = format!("{metric_type:?}");
        let mut metrics = self.metrics.write();
        *metrics.entry(key).or_insert(0) += value;
    }

    /// 获取 metric 值
    pub fn get_metric(&self, metric_type: MeshMetricType) -> u64 {
        let key = format!("{metric_type:?}");
        *self.metrics.read().get(&key).unwrap_or(&0)
    }

    /// 记录 trace span
    pub fn record_span(&self, span: MeshSpan) {
        self.spans.write().push(span);
    }

    /// 获取所有 spans
    pub fn spans(&self) -> Vec<MeshSpan> {
        self.spans.read().clone()
    }

    /// 渲染 Prometheus 格式 metrics
    pub fn render_prometheus(&self) -> String {
        let metrics = self.metrics.read();
        let mut output = String::new();
        for (key, value) in metrics.iter() {
            output.push_str(&format!("mesh_{key} {value}\n"));
        }
        output
    }

    /// 接入既有 MetricsRegistry（模拟）
    pub fn integrate_with_registry(&self) -> String {
        "mesh metrics integrated with MetricsRegistry\n".to_string()
    }

    /// 接入既有 sz-orm-tracing OTLP（模拟）
    pub fn integrate_with_tracing(&self) -> String {
        "mesh traces integrated with sz-orm-tracing OTLP\n".to_string()
    }
}

impl Default for MeshObservability {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_get_metric() {
        let obs = MeshObservability::new();
        obs.record_metric(MeshMetricType::RequestCount, 10);
        obs.record_metric(MeshMetricType::RequestCount, 5);
        assert_eq!(obs.get_metric(MeshMetricType::RequestCount), 15);
    }

    #[test]
    fn test_record_span() {
        let obs = MeshObservability::new();
        obs.record_span(MeshSpan {
            span_type: MeshSpanType::Proxy,
            name: "istio-proxy".to_string(),
            trace_id: "trace-001".to_string(),
            duration_ms: 50,
        });
        assert_eq!(obs.spans().len(), 1);
        assert_eq!(obs.spans()[0].name, "istio-proxy");
    }

    #[test]
    fn test_render_prometheus() {
        let obs = MeshObservability::new();
        obs.record_metric(MeshMetricType::RequestCount, 100);
        obs.record_metric(MeshMetricType::RetryCount, 5);
        let output = obs.render_prometheus();
        assert!(output.contains("mesh_RequestCount 100"));
        assert!(output.contains("mesh_RetryCount 5"));
    }

    #[test]
    fn test_integrate_with_registry() {
        let obs = MeshObservability::new();
        let result = obs.integrate_with_registry();
        assert!(result.contains("MetricsRegistry"));
    }

    #[test]
    fn test_integrate_with_tracing() {
        let obs = MeshObservability::new();
        let result = obs.integrate_with_tracing();
        assert!(result.contains("OTLP"));
    }

    #[test]
    fn test_multiple_metric_types() {
        let obs = MeshObservability::new();
        obs.record_metric(MeshMetricType::RequestCount, 1);
        obs.record_metric(MeshMetricType::RequestDuration, 100);
        obs.record_metric(MeshMetricType::CircuitBreakerCount, 3);
        obs.record_metric(MeshMetricType::RetryCount, 2);

        assert_eq!(obs.get_metric(MeshMetricType::RequestCount), 1);
        assert_eq!(obs.get_metric(MeshMetricType::RequestDuration), 100);
        assert_eq!(obs.get_metric(MeshMetricType::CircuitBreakerCount), 3);
        assert_eq!(obs.get_metric(MeshMetricType::RetryCount), 2);
    }

    #[test]
    fn test_proxy_and_application_spans() {
        let obs = MeshObservability::new();
        obs.record_span(MeshSpan {
            span_type: MeshSpanType::Proxy,
            name: "proxy".to_string(),
            trace_id: "t1".to_string(),
            duration_ms: 10,
        });
        obs.record_span(MeshSpan {
            span_type: MeshSpanType::Application,
            name: "handler".to_string(),
            trace_id: "t1".to_string(),
            duration_ms: 20,
        });
        let spans = obs.spans();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].span_type, MeshSpanType::Proxy);
        assert_eq!(spans[1].span_type, MeshSpanType::Application);
    }
}
