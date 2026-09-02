//! 感知信号采集器：聚合多源诊断信号

use crate::types::{AgentError, PerceptionSnapshot};
use chrono::Utc;
use std::collections::HashMap;

/// 感知信号采集器
///
/// 并行采集 6 源信号：
/// - sz-orm-diagnosis SlowQueryDiagnoser（慢查询）
/// - sz-orm-diagnosis ConnectionPoolDiagnoser（连接池）
/// - sz-orm-diagnosis DeadlockDetector（死锁）
/// - sz-orm-anomaly AnomalyDetector（异常）
/// - sz-orm-diagnosis FailurePredictor（故障预测）
pub struct PerceptionCollector {
    /// 是否启用慢查询采集
    pub enable_slow_query: bool,
    /// 是否启用连接池采集
    pub enable_pool: bool,
    /// 是否启用死锁采集
    pub enable_deadlock: bool,
    /// 是否启用异常采集
    pub enable_anomaly: bool,
    /// 是否启用故障预测采集
    pub enable_failure_prediction: bool,
}

impl PerceptionCollector {
    pub fn new() -> Self {
        Self {
            enable_slow_query: true,
            enable_pool: true,
            enable_deadlock: true,
            enable_anomaly: true,
            enable_failure_prediction: true,
        }
    }

    /// 采集感知快照
    ///
    /// 在实际集成中，此方法会并行调用各诊断源。
    /// 当前实现提供信号聚合框架，各诊断源通过 `set_*` 方法注入。
    pub async fn collect(
        &self,
        slow_queries: Vec<String>,
        pool_metrics: HashMap<String, f64>,
        deadlocks: Vec<String>,
        anomalies: Vec<String>,
        failure_predictions: Vec<String>,
    ) -> Result<PerceptionSnapshot, AgentError> {
        let health_score = self.compute_health_score(
            &slow_queries,
            &pool_metrics,
            &deadlocks,
            &anomalies,
            &failure_predictions,
        );

        Ok(PerceptionSnapshot {
            timestamp: Utc::now(),
            slow_queries: if self.enable_slow_query {
                slow_queries
            } else {
                Vec::new()
            },
            pool_metrics: if self.enable_pool {
                pool_metrics
            } else {
                HashMap::new()
            },
            deadlocks: if self.enable_deadlock {
                deadlocks
            } else {
                Vec::new()
            },
            anomalies: if self.enable_anomaly {
                anomalies
            } else {
                Vec::new()
            },
            failure_predictions: if self.enable_failure_prediction {
                failure_predictions
            } else {
                Vec::new()
            },
            health_score,
        })
    }

    /// 计算综合健康评分 [0, 1]
    fn compute_health_score(
        &self,
        slow_queries: &[String],
        pool_metrics: &HashMap<String, f64>,
        deadlocks: &[String],
        anomalies: &[String],
        failure_predictions: &[String],
    ) -> f64 {
        let mut penalty = 0.0;
        penalty += slow_queries.len() as f64 * 0.05;
        penalty += deadlocks.len() as f64 * 0.15;
        penalty += anomalies.len() as f64 * 0.10;
        penalty += failure_predictions.len() as f64 * 0.20;

        if let Some(&utilization) = pool_metrics.get("utilization") {
            if utilization > 0.8 {
                penalty += (utilization - 0.8) * 0.5;
            }
        }

        (1.0 - penalty).max(0.0)
    }
}

impl Default for PerceptionCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_collect_empty_signals() {
        let collector = PerceptionCollector::new();
        let snapshot = collector
            .collect(
                Vec::new(),
                HashMap::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
            .await
            .unwrap();
        assert_eq!(snapshot.health_score, 1.0);
        assert!(snapshot.slow_queries.is_empty());
    }

    #[tokio::test]
    async fn test_collect_with_signals() {
        let collector = PerceptionCollector::new();
        let snapshot = collector
            .collect(
                vec!["SELECT * FROM big_table".to_string()],
                HashMap::from([("utilization".to_string(), 0.9)]),
                vec!["deadlock-1".to_string()],
                vec!["anomaly-1".to_string()],
                vec!["failure-1".to_string()],
            )
            .await
            .unwrap();
        assert!(snapshot.health_score < 1.0);
        assert_eq!(snapshot.slow_queries.len(), 1);
        assert_eq!(snapshot.deadlocks.len(), 1);
    }

    #[tokio::test]
    async fn test_health_score_never_negative() {
        let collector = PerceptionCollector::new();
        let snapshot = collector
            .collect(
                vec!["q1".into(); 100],
                HashMap::new(),
                vec!["d1".into(); 100],
                vec!["a1".into(); 100],
                vec!["f1".into(); 100],
            )
            .await
            .unwrap();
        assert!(snapshot.health_score >= 0.0);
    }
}
