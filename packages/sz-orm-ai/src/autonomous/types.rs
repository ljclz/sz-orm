//! AI 自治闭环核心数据结构

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

/// 自治错误类型
#[derive(Debug, thiserror::Error)]
pub enum AutonomousError {
    #[error("配置无效: {0}")]
    ConfigInvalid(String),
    #[error("策略未找到: {0}")]
    PolicyNotFound(String),
    #[error("边界超出: 动作 {action} 值 {value} 超出范围 [{min}, {max}]")]
    BoundaryExceeded {
        action: String,
        value: f64,
        min: f64,
        max: f64,
    },
    #[error("熔断器已断开: 策略 {0}")]
    CircuitBroken(String),
    #[error("审计不可用")]
    AuditUnavailable,
    #[error("动作执行失败: {0}")]
    ActionFailed(String),
    #[error("验证失败: {0}")]
    VerificationFailed(String),
    #[error("LLM 推理失败: {0}")]
    LlmFailed(String),
}

/// 异常事件（触发自治决策的输入）
#[derive(Debug, Clone)]
pub struct AnomalyEvent {
    pub event_type: String,
    pub timestamp: SystemTime,
    pub severity: Severity,
    pub context: HashMap<String, String>,
    pub event_hash: u64,
}

/// 严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// 自治策略
#[derive(Debug, Clone)]
pub struct AutonomousPolicy {
    pub name: String,
    pub version: String,
    pub trigger: TriggerCondition,
    pub action: AutonomousAction,
    pub boundary: ActionBoundary,
    pub circuit_breaker: CircuitBreakerConfig,
    pub enabled: bool,
}

/// 触发条件
#[derive(Debug, Clone)]
pub struct TriggerCondition {
    pub event_type: String,
    pub severity_threshold: Severity,
    pub context_match: HashMap<String, String>,
}

/// 自治动作类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousAction {
    AutoRemediation,
    AutoScaling,
    AutoTuning,
    SlowQueryGovernance,
    /// v7.9.0 降级非核心功能
    DegradeNonCore {
        features: Vec<String>,
    },
}

/// 动作边界
#[derive(Debug, Clone)]
pub struct ActionBoundary {
    pub min: f64,
    pub max: f64,
}

/// 熔断器配置
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub break_duration: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            break_duration: Duration::from_secs(1800),
        }
    }
}

/// 自治决策
#[derive(Debug, Clone)]
pub struct AutonomousDecision {
    pub policy_name: String,
    pub action: AutonomousAction,
    pub reasoning: String,
    pub dry_run: bool,
    pub timestamp: Instant,
}

/// 自治执行结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub message: String,
    pub duration: Duration,
}

/// 自治验证结果
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub verified: bool,
    pub health_status: String,
    pub metrics: HashMap<String, f64>,
}

/// 自治完整结果
#[derive(Debug, Clone)]
pub struct AutonomousOutcome {
    pub decision: AutonomousDecision,
    pub execution_result: ExecutionResult,
    pub verification_result: VerificationResult,
    pub audit_record_id: String,
}

/// 熔断器状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// 熔断器运行时状态
#[derive(Debug, Clone)]
pub struct CircuitRuntimeState {
    pub state: CircuitState,
    pub failure_count: u32,
    pub last_failure_time: Option<Instant>,
    pub opened_at: Option<Instant>,
}

impl Default for CircuitRuntimeState {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            last_failure_time: None,
            opened_at: None,
        }
    }
}

/// 幂等去重条目
#[derive(Debug, Clone)]
pub struct IdempotencyEntry {
    pub event_hash: u64,
    pub timestamp: Instant,
}

/// LLM 建议
#[derive(Debug, Clone)]
pub struct LlmSuggestion {
    pub action: AutonomousAction,
    pub reasoning: String,
    pub confidence: f64,
    pub degraded: bool,
}

/// 审计记录
#[derive(Debug, Clone)]
pub struct AutonomousAuditRecord {
    pub record_id: String,
    pub event: AnomalyEvent,
    pub decision: AutonomousDecision,
    pub execution_result: Option<ExecutionResult>,
    pub verification_result: Option<VerificationResult>,
    pub rollback_performed: bool,
    pub timestamp: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autonomous_error_display() {
        let err = AutonomousError::ConfigInvalid("missing field".to_string());
        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn test_circuit_breaker_default() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.break_duration, Duration::from_secs(1800));
    }

    #[test]
    fn test_circuit_runtime_state_default() {
        let state = CircuitRuntimeState::default();
        assert_eq!(state.state, CircuitState::Closed);
        assert_eq!(state.failure_count, 0);
    }

    #[test]
    fn test_anomaly_event_creation() {
        let event = AnomalyEvent {
            event_type: "high_latency".to_string(),
            timestamp: SystemTime::now(),
            severity: Severity::Warning,
            context: HashMap::new(),
            event_hash: 12345,
        };
        assert_eq!(event.event_type, "high_latency");
        assert_eq!(event.severity, Severity::Warning);
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Critical);
    }
}
