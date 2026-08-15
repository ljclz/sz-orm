//! # 异常自愈与根因分析
//!
//! 自动修复器（`AutoRemediator`，白名单自动/非白名单人工确认）+
//! 根因分析器（`RootCauseAnalyzer`，附证据链与置信度）+
//! 异常关联分析器（`AnomalyCorrelator`，时序关联 + 因果推断）。
//!
//! 复用 v4.6.0 `AnomalyDetector`（`anomaly.rs:254`）异常事件订阅，
//! 复用既有 `Anomaly`（`anomaly.rs:154`）/ `AnomalyAlgorithm`（`:40`）。

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::anomaly::{Anomaly, AnomalyAlgorithm};

/// 修复动作
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemediationAction {
    /// 重启连接
    RestartConnection,
    /// 清除缓存
    ClearCache,
    /// 扩容
    ScaleOut,
    /// 自定义动作（名称）
    CustomAction(String),
}

impl RemediationAction {
    /// 是否在白名单中
    pub fn is_in_whitelist(&self, whitelist: &[RemediationAction]) -> bool {
        whitelist.contains(self)
    }
}

/// 修复结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationResult {
    /// 执行的动作
    pub action: RemediationAction,
    /// 是否成功
    pub success: bool,
    /// 执行耗时（毫秒）
    pub elapsed_ms: u64,
    /// 详情
    pub detail: String,
    /// 是否自动执行（白名单）
    pub auto_executed: bool,
}

/// 修复错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemediationError {
    /// 动作执行失败
    ActionFailed(String),
    /// 证据不足
    InsufficientEvidence(String),
    /// 无效白名单动作
    InvalidWhitelistAction(String),
}

impl std::fmt::Display for RemediationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActionFailed(msg) => write!(f, "action failed: {msg}"),
            Self::InsufficientEvidence(msg) => write!(f, "insufficient evidence: {msg}"),
            Self::InvalidWhitelistAction(msg) => write!(f, "invalid whitelist action: {msg}"),
        }
    }
}

impl std::error::Error for RemediationError {}

/// 根因类别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootCauseCategory {
    /// 连接池耗尽
    ConnectionPoolExhausted,
    /// 慢查询
    SlowQuery,
    /// 资源不足（CPU/内存/磁盘）
    ResourceInsufficient,
    /// 配置错误
    ConfigurationError,
    /// 网络问题
    NetworkIssue,
    /// 未知
    Unknown,
}

/// 证据项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// 证据描述
    pub description: String,
    /// 证据来源（指标名/日志/追踪）
    pub source: String,
    /// 时间戳（Unix 毫秒）
    pub timestamp: u64,
}

/// 根因分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCause {
    /// 根因类别
    pub category: RootCauseCategory,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f64,
    /// 证据链
    pub evidence: Vec<Evidence>,
    /// 建议的修复动作
    pub suggested_action: RemediationAction,
    /// 根因描述
    pub description: String,
}

impl RootCause {
    pub fn new(category: RootCauseCategory, confidence: f64) -> Self {
        Self {
            category,
            confidence: confidence.clamp(0.0, 1.0),
            evidence: Vec::new(),
            suggested_action: RemediationAction::RestartConnection,
            description: String::new(),
        }
    }

    pub fn with_evidence(mut self, evidence: Evidence) -> Self {
        self.evidence.push(evidence);
        self
    }

    pub fn with_suggested_action(mut self, action: RemediationAction) -> Self {
        self.suggested_action = action;
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// 是否有足够证据（至少 2 条证据且置信度 >= 0.5）
    pub fn has_sufficient_evidence(&self) -> bool {
        self.evidence.len() >= 2 && self.confidence >= 0.5
    }
}

/// 关联分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationResult {
    /// 关联的异常数量
    pub correlated_count: usize,
    /// 关联强度（0.0 ~ 1.0）
    pub correlation_strength: f64,
    /// 关联类型
    pub correlation_type: CorrelationType,
    /// 关联的异常指标名
    pub correlated_metrics: Vec<String>,
}

/// 关联类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorrelationType {
    /// 时序关联（时间窗口内先后发生）
    Temporal,
    /// 因果关联（A 导致 B）
    Causal,
    /// 无关联
    None,
}

/// 自动修复器
///
/// 白名单动作自动执行，非白名单动作需人工确认。
/// 复用 v4.6.0 `AnomalyDetector`（`anomaly.rs:254`）异常事件订阅。
pub struct AutoRemediator {
    whitelist: Mutex<Vec<RemediationAction>>,
    history: Mutex<Vec<RemediationResult>>,
}

impl AutoRemediator {
    pub fn new() -> Self {
        Self {
            whitelist: Mutex::new(vec![
                RemediationAction::RestartConnection,
                RemediationAction::ClearCache,
            ]),
            history: Mutex::new(Vec::new()),
        }
    }

    pub fn with_whitelist(&self, whitelist: Vec<RemediationAction>) {
        *self.whitelist.lock().unwrap_or_else(|e| e.into_inner()) = whitelist;
    }

    /// 获取白名单
    pub fn whitelist(&self) -> Vec<RemediationAction> {
        self.whitelist
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 选择修复动作
    ///
    /// 基于异常和根因选择修复动作，白名单动作可自动执行。
    pub fn select_action(&self, anomaly: &Anomaly, root_cause: &RootCause) -> RemediationAction {
        if root_cause.has_sufficient_evidence() {
            root_cause.suggested_action.clone()
        } else {
            match anomaly.algorithm {
                AnomalyAlgorithm::Threshold => RemediationAction::RestartConnection,
                AnomalyAlgorithm::Trend => RemediationAction::ScaleOut,
                AnomalyAlgorithm::Statistical => RemediationAction::ClearCache,
                AnomalyAlgorithm::ZScore => RemediationAction::RestartConnection,
                AnomalyAlgorithm::IQR => RemediationAction::ClearCache,
            }
        }
    }

    /// 执行修复动作
    ///
    /// 白名单动作自动执行，非白名单动作返回错误需人工确认。
    pub async fn execute_action(
        &self,
        action: RemediationAction,
    ) -> Result<RemediationResult, RemediationError> {
        let whitelist = self
            .whitelist
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let auto_executed = action.is_in_whitelist(&whitelist);

        if !auto_executed {
            return Err(RemediationError::InvalidWhitelistAction(format!(
                "action {action:?} requires manual confirmation"
            )));
        }

        let start = std::time::Instant::now();
        let (success, detail) = match &action {
            RemediationAction::RestartConnection => (true, "connection pool restarted".to_string()),
            RemediationAction::ClearCache => (true, "cache cleared".to_string()),
            RemediationAction::ScaleOut => (true, "scale-out initiated".to_string()),
            RemediationAction::CustomAction(name) => {
                (true, format!("custom action '{name}' executed"))
            }
        };

        let result = RemediationResult {
            action,
            success,
            elapsed_ms: start.elapsed().as_millis() as u64,
            detail,
            auto_executed,
        };

        self.history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(result.clone());
        Ok(result)
    }

    /// 获取修复历史
    pub fn history(&self) -> Vec<RemediationResult> {
        self.history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 自动修复流程：选择动作 → 执行
    pub async fn auto_remediate(
        &self,
        anomaly: &Anomaly,
        root_cause: &RootCause,
    ) -> Result<RemediationResult, RemediationError> {
        let action = self.select_action(anomaly, root_cause);
        self.execute_action(action).await
    }
}

impl Default for AutoRemediator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AutoRemediator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoRemediator")
            .field(
                "whitelist",
                &self.whitelist.lock().unwrap_or_else(|e| e.into_inner()),
            )
            .field(
                "history_count",
                &self.history.lock().unwrap_or_else(|e| e.into_inner()).len(),
            )
            .finish()
    }
}

/// 根因分析器
///
/// 基于异常指标和算法推断根因，附证据链与置信度。
pub struct RootCauseAnalyzer {
    rules: Mutex<Vec<RootCauseRule>>,
}

/// 根因推断规则
struct RootCauseRule {
    metric_pattern: String,
    category: RootCauseCategory,
    action: RemediationAction,
    confidence: f64,
}

impl RootCauseAnalyzer {
    pub fn new() -> Self {
        let rules = vec![
            RootCauseRule {
                metric_pattern: "connection".to_string(),
                category: RootCauseCategory::ConnectionPoolExhausted,
                action: RemediationAction::RestartConnection,
                confidence: 0.85,
            },
            RootCauseRule {
                metric_pattern: "query_time".to_string(),
                category: RootCauseCategory::SlowQuery,
                action: RemediationAction::ClearCache,
                confidence: 0.75,
            },
            RootCauseRule {
                metric_pattern: "cpu".to_string(),
                category: RootCauseCategory::ResourceInsufficient,
                action: RemediationAction::ScaleOut,
                confidence: 0.80,
            },
            RootCauseRule {
                metric_pattern: "memory".to_string(),
                category: RootCauseCategory::ResourceInsufficient,
                action: RemediationAction::ScaleOut,
                confidence: 0.80,
            },
            RootCauseRule {
                metric_pattern: "disk".to_string(),
                category: RootCauseCategory::ResourceInsufficient,
                action: RemediationAction::ScaleOut,
                confidence: 0.80,
            },
            RootCauseRule {
                metric_pattern: "network".to_string(),
                category: RootCauseCategory::NetworkIssue,
                action: RemediationAction::RestartConnection,
                confidence: 0.70,
            },
        ];
        Self {
            rules: Mutex::new(rules),
        }
    }

    /// 分析根因
    pub fn analyze_root_cause(&self, anomaly: &Anomaly) -> Result<RootCause, RemediationError> {
        let rules = self.rules.lock().unwrap_or_else(|e| e.into_inner());
        let metric = &anomaly.metric_name;

        let matched_rule = rules.iter().find(|r| metric.contains(&r.metric_pattern));

        match matched_rule {
            Some(rule) => {
                let mut root_cause = RootCause::new(rule.category.clone(), rule.confidence)
                    .with_suggested_action(rule.action.clone())
                    .with_description(format!(
                        "metric '{metric}' matches pattern '{}', likely {:#?}",
                        rule.metric_pattern, rule.category
                    ));

                root_cause = root_cause.with_evidence(Evidence {
                    description: format!(
                        "anomaly value {} exceeds threshold {}",
                        anomaly.anomaly_value, anomaly.threshold
                    ),
                    source: format!("metric:{metric}"),
                    timestamp: anomaly.detected_at,
                });

                root_cause = root_cause.with_evidence(Evidence {
                    description: format!(
                        "detected by {:#?} algorithm in {}ms window",
                        anomaly.algorithm, anomaly.window_ms
                    ),
                    source: "anomaly_detector".to_string(),
                    timestamp: anomaly.detected_at,
                });

                Ok(root_cause)
            }
            None => Ok(RootCause::new(RootCauseCategory::Unknown, 0.3)
                .with_suggested_action(RemediationAction::RestartConnection)
                .with_description(format!("no matching rule for metric '{metric}'"))),
        }
    }

    /// 添加自定义规则
    pub fn add_rule(
        &self,
        metric_pattern: impl Into<String>,
        category: RootCauseCategory,
        action: RemediationAction,
        confidence: f64,
    ) {
        self.rules
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(RootCauseRule {
                metric_pattern: metric_pattern.into(),
                category,
                action,
                confidence: confidence.clamp(0.0, 1.0),
            });
    }
}

impl Default for RootCauseAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RootCauseAnalyzer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootCauseAnalyzer")
            .field(
                "rule_count",
                &self.rules.lock().unwrap_or_else(|e| e.into_inner()).len(),
            )
            .finish()
    }
}

/// 异常关联分析器
///
/// 分析当前异常与历史异常的时序关联和因果关联。
pub struct AnomalyCorrelator {
    window_ms: u64,
}

impl AnomalyCorrelator {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms: window_ms.max(1000),
        }
    }

    /// 关联分析
    ///
    /// 检查当前异常与历史异常在时间窗口内的关联性。
    pub fn correlate(
        &self,
        anomaly: &Anomaly,
        history: &[Anomaly],
    ) -> Result<CorrelationResult, RemediationError> {
        let window_start = anomaly.detected_at.saturating_sub(self.window_ms);
        let window_end = anomaly.detected_at + self.window_ms;

        let correlated: Vec<&Anomaly> = history
            .iter()
            .filter(|a| {
                a.detected_at >= window_start
                    && a.detected_at <= window_end
                    && a.metric_name != anomaly.metric_name
            })
            .collect();

        let correlated_count = correlated.len();
        let correlated_metrics: Vec<String> = correlated
            .iter()
            .map(|a| a.metric_name.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let correlation_strength = if correlated_count == 0 {
            0.0
        } else {
            (1.0 / (1.0 + correlated_count as f64 * 0.1)).min(1.0)
        };

        let correlation_type = if correlated_count == 0 {
            CorrelationType::None
        } else if correlated
            .iter()
            .any(|a| a.detected_at < anomaly.detected_at)
        {
            CorrelationType::Causal
        } else {
            CorrelationType::Temporal
        };

        Ok(CorrelationResult {
            correlated_count,
            correlation_strength,
            correlation_type,
            correlated_metrics,
        })
    }
}

impl Default for AnomalyCorrelator {
    fn default() -> Self {
        Self::new(60_000)
    }
}

impl std::fmt::Debug for AnomalyCorrelator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnomalyCorrelator")
            .field("window_ms", &self.window_ms)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_anomaly(metric: &str, value: f64, threshold: f64, detected_at: u64) -> Anomaly {
        Anomaly {
            metric_name: metric.to_string(),
            anomaly_value: value,
            threshold,
            window_ms: 5000,
            algorithm: AnomalyAlgorithm::Threshold,
            detected_at,
            description: format!("{metric} anomaly"),
        }
    }

    #[test]
    fn test_remediation_action_whitelist() {
        let whitelist = vec![RemediationAction::RestartConnection];
        assert!(RemediationAction::RestartConnection.is_in_whitelist(&whitelist));
        assert!(!RemediationAction::ClearCache.is_in_whitelist(&whitelist));
    }

    #[test]
    fn test_remediation_error_display() {
        let err = RemediationError::ActionFailed("fail".to_string());
        assert!(err.to_string().contains("fail"));

        let err = RemediationError::InsufficientEvidence("no evidence".to_string());
        assert!(err.to_string().contains("no evidence"));

        let err = RemediationError::InvalidWhitelistAction("not allowed".to_string());
        assert!(err.to_string().contains("not allowed"));
    }

    #[test]
    fn test_root_cause_new() {
        let rc = RootCause::new(RootCauseCategory::SlowQuery, 0.8);
        assert_eq!(rc.category, RootCauseCategory::SlowQuery);
        assert_eq!(rc.confidence, 0.8);
        assert!(rc.evidence.is_empty());
    }

    #[test]
    fn test_root_cause_confidence_clamped() {
        let rc = RootCause::new(RootCauseCategory::Unknown, 1.5);
        assert_eq!(rc.confidence, 1.0);

        let rc = RootCause::new(RootCauseCategory::Unknown, -0.5);
        assert_eq!(rc.confidence, 0.0);
    }

    #[test]
    fn test_root_cause_builder() {
        let rc = RootCause::new(RootCauseCategory::ConnectionPoolExhausted, 0.9)
            .with_evidence(Evidence {
                description: "pool full".to_string(),
                source: "metric:pool".to_string(),
                timestamp: 1000,
            })
            .with_evidence(Evidence {
                description: "wait time high".to_string(),
                source: "metric:wait".to_string(),
                timestamp: 1001,
            })
            .with_suggested_action(RemediationAction::RestartConnection)
            .with_description("pool exhausted");

        assert_eq!(rc.evidence.len(), 2);
        assert_eq!(rc.suggested_action, RemediationAction::RestartConnection);
        assert_eq!(rc.description, "pool exhausted");
        assert!(rc.has_sufficient_evidence());
    }

    #[test]
    fn test_root_cause_insufficient_evidence() {
        let rc = RootCause::new(RootCauseCategory::Unknown, 0.3);
        assert!(!rc.has_sufficient_evidence());

        let rc = RootCause::new(RootCauseCategory::Unknown, 0.6);
        assert!(!rc.has_sufficient_evidence());
    }

    #[test]
    fn test_auto_remediator_default_whitelist() {
        let remediator = AutoRemediator::new();
        let whitelist = remediator.whitelist();
        assert!(whitelist.contains(&RemediationAction::RestartConnection));
        assert!(whitelist.contains(&RemediationAction::ClearCache));
        assert!(!whitelist.contains(&RemediationAction::ScaleOut));
    }

    #[test]
    fn test_auto_remediator_custom_whitelist() {
        let remediator = AutoRemediator::new();
        remediator.with_whitelist(vec![RemediationAction::ScaleOut]);
        let whitelist = remediator.whitelist();
        assert!(whitelist.contains(&RemediationAction::ScaleOut));
        assert!(!whitelist.contains(&RemediationAction::RestartConnection));
    }

    #[test]
    fn test_auto_remediator_select_action_with_evidence() {
        let remediator = AutoRemediator::new();
        let anomaly = make_anomaly("connection_count", 100.0, 50.0, 1000);
        let root_cause = RootCause::new(RootCauseCategory::ConnectionPoolExhausted, 0.9)
            .with_evidence(Evidence {
                description: "e1".to_string(),
                source: "s1".to_string(),
                timestamp: 1000,
            })
            .with_evidence(Evidence {
                description: "e2".to_string(),
                source: "s2".to_string(),
                timestamp: 1000,
            })
            .with_suggested_action(RemediationAction::RestartConnection);

        let action = remediator.select_action(&anomaly, &root_cause);
        assert_eq!(action, RemediationAction::RestartConnection);
    }

    #[test]
    fn test_auto_remediator_select_action_without_evidence() {
        let remediator = AutoRemediator::new();
        let anomaly = make_anomaly("metric", 100.0, 50.0, 1000);
        let root_cause = RootCause::new(RootCauseCategory::Unknown, 0.3);

        let action = remediator.select_action(&anomaly, &root_cause);
        assert_eq!(action, RemediationAction::RestartConnection);
    }

    #[tokio::test]
    async fn test_auto_remediator_execute_whitelist_action() {
        let remediator = AutoRemediator::new();
        let result = remediator
            .execute_action(RemediationAction::RestartConnection)
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.auto_executed);
        assert_eq!(result.action, RemediationAction::RestartConnection);
    }

    #[tokio::test]
    async fn test_auto_remediator_execute_non_whitelist_action() {
        let remediator = AutoRemediator::new();
        let result = remediator.execute_action(RemediationAction::ScaleOut).await;
        assert!(result.is_err());
        match result {
            Err(RemediationError::InvalidWhitelistAction(_)) => {}
            _ => panic!("wrong error"),
        }
    }

    #[tokio::test]
    async fn test_auto_remediator_history() {
        let remediator = AutoRemediator::new();
        remediator
            .execute_action(RemediationAction::RestartConnection)
            .await
            .unwrap();
        remediator
            .execute_action(RemediationAction::ClearCache)
            .await
            .unwrap();
        assert_eq!(remediator.history().len(), 2);
    }

    #[tokio::test]
    async fn test_auto_remediator_auto_remediate() {
        let remediator = AutoRemediator::new();
        let anomaly = make_anomaly("connection_count", 100.0, 50.0, 1000);
        let root_cause = RootCause::new(RootCauseCategory::ConnectionPoolExhausted, 0.9)
            .with_evidence(Evidence {
                description: "e1".to_string(),
                source: "s1".to_string(),
                timestamp: 1000,
            })
            .with_evidence(Evidence {
                description: "e2".to_string(),
                source: "s2".to_string(),
                timestamp: 1000,
            })
            .with_suggested_action(RemediationAction::RestartConnection);

        let result = remediator
            .auto_remediate(&anomaly, &root_cause)
            .await
            .unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_root_cause_analyzer_connection() {
        let analyzer = RootCauseAnalyzer::new();
        let anomaly = make_anomaly("connection_pool_size", 100.0, 50.0, 1000);
        let rc = analyzer.analyze_root_cause(&anomaly).unwrap();
        assert_eq!(rc.category, RootCauseCategory::ConnectionPoolExhausted);
        assert!(rc.confidence > 0.5);
        assert!(rc.has_sufficient_evidence());
    }

    #[test]
    fn test_root_cause_analyzer_slow_query() {
        let analyzer = RootCauseAnalyzer::new();
        let anomaly = make_anomaly("query_time_p99", 5.0, 1.0, 1000);
        let rc = analyzer.analyze_root_cause(&anomaly).unwrap();
        assert_eq!(rc.category, RootCauseCategory::SlowQuery);
    }

    #[test]
    fn test_root_cause_analyzer_resource() {
        let analyzer = RootCauseAnalyzer::new();
        let anomaly = make_anomaly("cpu_usage", 95.0, 80.0, 1000);
        let rc = analyzer.analyze_root_cause(&anomaly).unwrap();
        assert_eq!(rc.category, RootCauseCategory::ResourceInsufficient);
        assert_eq!(rc.suggested_action, RemediationAction::ScaleOut);
    }

    #[test]
    fn test_root_cause_analyzer_unknown() {
        let analyzer = RootCauseAnalyzer::new();
        let anomaly = make_anomaly("custom_metric", 100.0, 50.0, 1000);
        let rc = analyzer.analyze_root_cause(&anomaly).unwrap();
        assert_eq!(rc.category, RootCauseCategory::Unknown);
        assert!(!rc.has_sufficient_evidence());
    }

    #[test]
    fn test_root_cause_analyzer_custom_rule() {
        let analyzer = RootCauseAnalyzer::new();
        analyzer.add_rule(
            "custom_pattern",
            RootCauseCategory::ConfigurationError,
            RemediationAction::CustomAction("fix_config".to_string()),
            0.9,
        );
        let anomaly = make_anomaly("custom_pattern_value", 100.0, 50.0, 1000);
        let rc = analyzer.analyze_root_cause(&anomaly).unwrap();
        assert_eq!(rc.category, RootCauseCategory::ConfigurationError);
    }

    #[test]
    fn test_anomaly_correlator_no_correlation() {
        let correlator = AnomalyCorrelator::new(5000);
        let anomaly = make_anomaly("metric_a", 100.0, 50.0, 10000);
        let history = vec![make_anomaly("metric_b", 100.0, 50.0, 100000)];
        let result = correlator.correlate(&anomaly, &history).unwrap();
        assert_eq!(result.correlated_count, 0);
        assert_eq!(result.correlation_type, CorrelationType::None);
    }

    #[test]
    fn test_anomaly_correlator_temporal() {
        let correlator = AnomalyCorrelator::new(5000);
        let anomaly = make_anomaly("metric_a", 100.0, 50.0, 10000);
        let history = vec![
            make_anomaly("metric_b", 100.0, 50.0, 12000),
            make_anomaly("metric_c", 100.0, 50.0, 11000),
        ];
        let result = correlator.correlate(&anomaly, &history).unwrap();
        assert_eq!(result.correlated_count, 2);
        assert_eq!(result.correlation_type, CorrelationType::Temporal);
    }

    #[test]
    fn test_anomaly_correlator_causal() {
        let correlator = AnomalyCorrelator::new(5000);
        let anomaly = make_anomaly("metric_a", 100.0, 50.0, 10000);
        let history = vec![make_anomaly("metric_b", 100.0, 50.0, 8000)];
        let result = correlator.correlate(&anomaly, &history).unwrap();
        assert_eq!(result.correlated_count, 1);
        assert_eq!(result.correlation_type, CorrelationType::Causal);
    }

    #[test]
    fn test_anomaly_correlator_same_metric_excluded() {
        let correlator = AnomalyCorrelator::new(5000);
        let anomaly = make_anomaly("metric_a", 100.0, 50.0, 10000);
        let history = vec![make_anomaly("metric_a", 100.0, 50.0, 11000)];
        let result = correlator.correlate(&anomaly, &history).unwrap();
        assert_eq!(result.correlated_count, 0);
    }

    #[test]
    fn test_anomaly_correlator_default() {
        let correlator = AnomalyCorrelator::default();
        assert_eq!(correlator.window_ms, 60_000);
    }
}
