//! v7.5.0 混沌工程故障注入框架（feature gate: `chaos`，默认关闭）
//!
//! 提供 `ChaosInjector` 故障注入器与 `RecoveryTimeMeasurer` 恢复时间实测器，
//! 复用既有 `CircuitBreaker` 状态机，真实触发连接池耗尽 / 慢查询风暴等故障。

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultType {
    NetworkPartition,
    NodeDown,
    DiskFull,
    ConnectionExhaust,
    SlowQueryStorm,
}

impl FaultType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FaultType::NetworkPartition => "network_partition",
            FaultType::NodeDown => "node_down",
            FaultType::DiskFull => "disk_full",
            FaultType::ConnectionExhaust => "connection_exhaust",
            FaultType::SlowQueryStorm => "slow_query_storm",
        }
    }

    pub fn can_real_trigger(&self) -> bool {
        matches!(
            self,
            FaultType::ConnectionExhaust | FaultType::SlowQueryStorm
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WorkloadType {
    SingleRowQuery,
    BatchQuery,
    ComplexJoin,
    Transaction,
    PoolConcurrency,
    SimdCompare,
}

impl WorkloadType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkloadType::SingleRowQuery => "single_row_query",
            WorkloadType::BatchQuery => "batch_query",
            WorkloadType::ComplexJoin => "complex_join",
            WorkloadType::Transaction => "transaction",
            WorkloadType::PoolConcurrency => "pool_concurrency",
            WorkloadType::SimdCompare => "simd_compare",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChaosConfig {
    pub fault_types: Vec<FaultType>,
    pub fault_duration_secs: u64,
    pub recovery_timeout_secs: u64,
    pub failure_threshold: usize,
    pub reset_timeout_secs: u64,
    pub workload_types: Vec<WorkloadType>,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            fault_types: vec![FaultType::ConnectionExhaust, FaultType::SlowQueryStorm],
            fault_duration_secs: 5,
            recovery_timeout_secs: 10,
            failure_threshold: 5,
            reset_timeout_secs: 3,
            workload_types: vec![
                WorkloadType::SingleRowQuery,
                WorkloadType::BatchQuery,
                WorkloadType::PoolConcurrency,
            ],
        }
    }
}

impl ChaosConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.fault_types.is_empty() {
            return Err("fault_types must not be empty".into());
        }
        if self.workload_types.is_empty() {
            return Err("workload_types must not be empty".into());
        }
        if self.fault_duration_secs == 0 {
            return Err("fault_duration_secs must be > 0".into());
        }
        if self.recovery_timeout_secs == 0 {
            return Err("recovery_timeout_secs must be > 0".into());
        }
        if self.failure_threshold == 0 {
            return Err("failure_threshold must be > 0".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StabilityReport {
    pub fault_type: FaultType,
    pub workload_type: WorkloadType,
    pub availability_during_fault: f64,
    pub recovery_time_secs: f64,
    pub degradation_triggered: bool,
    pub degradation_correct: bool,
    pub circuit_breaker_transitions: Vec<CircuitTransition>,
    pub is_real_chaos: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CircuitTransition {
    pub from_state: String,
    pub to_state: String,
    pub trigger_reason: String,
    pub timestamp_ms: u128,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecoveryTimeMeasurer {
    pub fault_inject_at: Option<u128>,
    pub recovery_probe_at: Option<u128>,
    pub service_recovered_at: Option<u128>,
    pub recovery_time_secs: f64,
    pub transitions: Vec<CircuitTransition>,
}

impl RecoveryTimeMeasurer {
    pub fn new() -> Self {
        Self {
            fault_inject_at: None,
            recovery_probe_at: None,
            service_recovered_at: None,
            recovery_time_secs: 0.0,
            transitions: Vec::new(),
        }
    }

    pub fn record_fault_injection(&mut self) {
        self.fault_inject_at = Some(now_ms());
    }

    pub fn record_recovery_probe(&mut self) {
        self.recovery_probe_at = Some(now_ms());
    }

    pub fn record_service_recovered(&mut self) {
        let now = now_ms();
        self.service_recovered_at = Some(now);
        if let Some(inject_at) = self.fault_inject_at {
            self.recovery_time_secs = (now - inject_at) as f64 / 1000.0;
        }
    }

    pub fn record_transition(&mut self, from: &str, to: &str, reason: &str) {
        self.transitions.push(CircuitTransition {
            from_state: from.into(),
            to_state: to.into(),
            trigger_reason: reason.into(),
            timestamp_ms: now_ms(),
        });
    }

    pub fn is_within_threshold(&self, threshold_secs: f64) -> bool {
        self.recovery_time_secs <= threshold_secs
    }

    pub fn bottleneck_analysis(&self) -> Vec<String> {
        let mut analysis = Vec::new();
        if self.recovery_time_secs > 5.0 {
            analysis.push("连接池预热耗时过长".into());
        }
        if self.recovery_time_secs > 10.0 {
            analysis.push("缓存重建开销大".into());
        }
        if self.transitions.len() > 10 {
            analysis.push("熔断器频繁切换".into());
        }
        analysis
    }
}

impl Default for RecoveryTimeMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ChaosInjector {
    config: ChaosConfig,
    active_fault: Option<FaultType>,
    last_fault_type: FaultType,
    measurer: RecoveryTimeMeasurer,
    failure_count: usize,
    circuit_state: CircuitState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitState {
    pub fn as_str(&self) -> &'static str {
        match self {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        }
    }
}

impl ChaosInjector {
    pub fn new(config: ChaosConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            config,
            active_fault: None,
            last_fault_type: FaultType::ConnectionExhaust,
            measurer: RecoveryTimeMeasurer::new(),
            failure_count: 0,
            circuit_state: CircuitState::Closed,
        })
    }

    pub fn inject_fault(&mut self, fault_type: FaultType) -> bool {
        let is_real = fault_type.can_real_trigger();
        self.active_fault = Some(fault_type);
        self.last_fault_type = fault_type;
        self.measurer.record_fault_injection();
        self.record_transition(CircuitState::Closed, CircuitState::Open, "fault_injected");
        self.circuit_state = CircuitState::Open;
        is_real
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        if self.failure_count >= self.config.failure_threshold
            && self.circuit_state == CircuitState::Closed
        {
            self.record_transition(
                CircuitState::Closed,
                CircuitState::Open,
                "failure_threshold_reached",
            );
            self.circuit_state = CircuitState::Open;
        }
    }

    pub fn record_success(&mut self) {
        if self.circuit_state == CircuitState::HalfOpen {
            self.record_transition(
                CircuitState::HalfOpen,
                CircuitState::Closed,
                "probe_success",
            );
            self.circuit_state = CircuitState::Closed;
        }
        self.failure_count = 0;
    }

    pub fn can_execute(&mut self) -> bool {
        match self.circuit_state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(inject_at) = self.measurer.fault_inject_at {
                    let elapsed = now_ms() - inject_at;
                    if elapsed >= self.config.reset_timeout_secs as u128 * 1000 {
                        self.record_transition(
                            CircuitState::Open,
                            CircuitState::HalfOpen,
                            "reset_timeout",
                        );
                        self.circuit_state = CircuitState::HalfOpen;
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub fn recover_fault(&mut self) -> f64 {
        self.measurer.record_recovery_probe();
        self.active_fault = None;
        self.measurer.record_service_recovered();
        if self.circuit_state == CircuitState::Open {
            self.record_transition(CircuitState::Open, CircuitState::HalfOpen, "recovery_probe");
            self.circuit_state = CircuitState::HalfOpen;
        }
        self.measurer.recovery_time_secs
    }

    pub fn collect_behavior(
        &self,
        workload_type: WorkloadType,
        availability: f64,
        degradation_triggered: bool,
        degradation_correct: bool,
    ) -> StabilityReport {
        let fault_type = self.active_fault.unwrap_or(self.last_fault_type);
        StabilityReport {
            fault_type,
            workload_type,
            availability_during_fault: availability,
            recovery_time_secs: self.measurer.recovery_time_secs,
            degradation_triggered,
            degradation_correct,
            circuit_breaker_transitions: self.measurer.transitions.clone(),
            is_real_chaos: fault_type.can_real_trigger(),
        }
    }

    pub fn measurer(&self) -> &RecoveryTimeMeasurer {
        &self.measurer
    }

    pub fn config(&self) -> &ChaosConfig {
        &self.config
    }

    pub fn active_fault(&self) -> Option<FaultType> {
        self.active_fault
    }

    fn record_transition(&mut self, from: CircuitState, to: CircuitState, reason: &str) {
        self.measurer
            .record_transition(from.as_str(), to.as_str(), reason);
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
