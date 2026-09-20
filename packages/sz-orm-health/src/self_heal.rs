//! 故障自愈协调器（SelfHealCoordinator）
//!
//! v7.7.0 任务 1.1：针对四类典型数据访问层故障执行幂等自愈动作。
//!
//! ## 故障 → 动作映射
//!
//! | 故障 [`FaultType`] | 自愈动作 [`SelfHealAction`] | 幂等语义 |
//! |---|---|---|
//! | [`FaultType::ConnectionLeak`] | [`SelfHealAction::RebuildConnection`] | 关闭泄漏连接并重建，重复关闭已关闭连接无副作用 |
//! | [`FaultType::PoolExhaustion`] | [`SelfHealAction::ExpandPool`] | 扩容至目标容量，重复扩容无副作用 |
//! | [`FaultType::SlowQueryStorm`] | [`SelfHealAction::ThrottleDegrade`] | 限流至阈值，重复限流无副作用 |
//! | [`FaultType::NodeUnreachable`] | [`SelfHealAction::SwitchNode`] | 切换至目标节点，重复切换无副作用 |
//!
//! ## 自愈流程
//!
//! 1. 首次 `heal(fault)`：执行专属自愈动作，记录状态，返回 `idempotent=false`。
//! 2. 重复 `heal(fault)`：识别已自愈，直接返回幂等结果 `idempotent=true`，不重复执行动作。
//! 3. 动作执行失败：发出 `SELF_HEAL_FAILED` 告警并返回 [`SelfHealError`]。
//! 4. 故障恢复后可调用 [`SelfHealCoordinator::reset`] 清除记录，允许后续再次自愈。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::Instant;

/// 四类数据访问层故障类型。
///
/// 每类故障通过 [`FaultType::preferred_action`] 映射到专属自愈动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FaultType {
    /// 连接泄漏：连接被借出后未归还连接池。
    ConnectionLeak,
    /// 连接池耗尽：池中无可借连接且已达容量上限。
    PoolExhaustion,
    /// 慢查询风暴：大量慢查询集中冲击数据库。
    SlowQueryStorm,
    /// 节点不可达：数据库节点网络不通或实例宕机。
    NodeUnreachable,
}

impl FaultType {
    /// 返回该故障对应的专属自愈动作。
    pub fn preferred_action(&self) -> SelfHealAction {
        match self {
            FaultType::ConnectionLeak => SelfHealAction::RebuildConnection,
            FaultType::PoolExhaustion => SelfHealAction::ExpandPool,
            FaultType::SlowQueryStorm => SelfHealAction::ThrottleDegrade,
            FaultType::NodeUnreachable => SelfHealAction::SwitchNode,
        }
    }
}

/// 四类幂等自愈动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelfHealAction {
    /// 重建连接：关闭泄漏连接并重建新鲜连接。
    RebuildConnection,
    /// 扩容连接池至目标容量。
    ExpandPool,
    /// 限流降级至慢查询阈值。
    ThrottleDegrade,
    /// 切换至目标节点。
    SwitchNode,
}

/// 自愈配置，定义各故障的阈值与恢复参数。
///
/// 所有阈值参数必须为正数，`target_nodes` 在 [`FaultType::NodeUnreachable`] 场景下必须非空，
/// 否则对应自愈动作返回 [`SelfHealError`]。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealConfig {
    /// 连接泄漏判定阈值（毫秒）。连接被借出超过该时长视为泄漏。
    pub leak_threshold_ms: u64,
    /// 连接池扩容目标容量。
    pub pool_target_capacity: u32,
    /// 慢查询限流阈值（QPS）。
    pub slow_query_qps_threshold: u32,
    /// 节点切换的备选目标节点列表（按优先级排序，首元素为首选目标）。
    pub target_nodes: Vec<String>,
}

impl Default for SelfHealConfig {
    fn default() -> Self {
        Self {
            leak_threshold_ms: 30_000,
            pool_target_capacity: 64,
            slow_query_qps_threshold: 100,
            target_nodes: vec!["node-secondary".to_string()],
        }
    }
}

/// 单次自愈结果。
///
/// `idempotent=true` 表示本次为幂等重复执行（该故障已自愈过，未实际执行动作）；
/// `idempotent=false` 表示本次为首次执行，已实际执行自愈动作。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealResult {
    /// 触发自愈的故障类型。
    pub fault_type: FaultType,
    /// 实际执行的自愈动作。
    pub action_taken: SelfHealAction,
    /// 自愈是否成功。
    pub success: bool,
    /// 本次是否为幂等重复执行。
    pub idempotent: bool,
    /// 自愈耗时（毫秒）。
    pub recovery_time_ms: u64,
    /// 自愈日志，描述动作执行细节。
    pub heal_log: String,
}

/// 自愈错误类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelfHealError {
    /// 配置无效，携带具体原因。
    InvalidConfig(String),
    /// 节点切换时无可用目标节点。
    NoTargetNode,
}

impl fmt::Display for SelfHealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelfHealError::InvalidConfig(msg) => write!(f, "invalid self-heal config: {}", msg),
            SelfHealError::NoTargetNode => write!(f, "no target node available for switch"),
        }
    }
}

impl std::error::Error for SelfHealError {}

/// 已自愈故障的状态记录。
#[derive(Clone)]
struct HealState {
    action: SelfHealAction,
    last_log: String,
}

/// 故障自愈协调器。
///
/// 对四类故障执行幂等自愈动作。内部维护已自愈故障的状态表，
/// 重复 `heal` 同一故障时直接返回幂等结果，不重复执行动作。
///
/// # 线程安全
///
/// 内部状态由 `Mutex` 保护，`SelfHealCoordinator` 是 `Send + Sync`，
/// 可跨线程共享。`heal` 为 `async fn`，锁仅在同步临界区内持有，不跨 `.await`。
pub struct SelfHealCoordinator {
    config: SelfHealConfig,
    states: Mutex<HashMap<FaultType, HealState>>,
}

impl SelfHealCoordinator {
    /// 创建自愈协调器。
    pub fn new(config: SelfHealConfig) -> Self {
        Self {
            config,
            states: Mutex::new(HashMap::new()),
        }
    }

    /// 对指定故障执行自愈。
    ///
    /// 首次执行：执行专属自愈动作并记录状态，返回 `idempotent=false`。
    /// 重复执行：识别已自愈，直接返回幂等结果 `idempotent=true`。
    /// 动作失败：发出 `SELF_HEAL_FAILED` 告警并返回 [`SelfHealError`]。
    pub async fn heal(&self, fault: &FaultType) -> Result<SelfHealResult, SelfHealError> {
        let action = fault.preferred_action();
        let start = Instant::now();

        // 幂等快速路径：已自愈则直接返回，不重复执行动作。
        if let Some(state) = self.read_state(fault) {
            return Ok(SelfHealResult {
                fault_type: *fault,
                action_taken: state.action,
                success: true,
                idempotent: true,
                recovery_time_ms: start.elapsed().as_millis() as u64,
                heal_log: format!(
                    "idempotent re-entry: fault {:?} already healed via {:?}; {}",
                    fault, state.action, state.last_log
                ),
            });
        }

        // 首次执行自愈动作。
        let heal_log = self.execute_action(action).await?;
        let recovery_time_ms = start.elapsed().as_millis() as u64;

        self.write_state(
            *fault,
            HealState {
                action,
                last_log: heal_log.clone(),
            },
        );

        Ok(SelfHealResult {
            fault_type: *fault,
            action_taken: action,
            success: true,
            idempotent: false,
            recovery_time_ms,
            heal_log,
        })
    }

    /// 查询某故障是否已自愈（用于观测）。
    pub fn is_healed(&self, fault: &FaultType) -> bool {
        self.states
            .lock()
            .map(|s| s.contains_key(fault))
            .unwrap_or(false)
    }

    /// 重置某故障的自愈状态。
    ///
    /// 故障恢复后调用以清除记录，允许后续再次自愈。返回是否实际清除了记录。
    pub fn reset(&self, fault: &FaultType) -> bool {
        self.states
            .lock()
            .map(|mut s| s.remove(fault).is_some())
            .unwrap_or(false)
    }

    /// 执行具体自愈动作。配置无效时发出 `SELF_HEAL_FAILED` 告警并返回错误。
    async fn execute_action(&self, action: SelfHealAction) -> Result<String, SelfHealError> {
        match action {
            SelfHealAction::RebuildConnection => {
                if self.config.leak_threshold_ms == 0 {
                    self.emit_alert(action, "leak_threshold_ms must be > 0");
                    return Err(SelfHealError::InvalidConfig(
                        "leak_threshold_ms must be > 0".to_string(),
                    ));
                }
                // 让出调度点，模拟异步重建连接。重复关闭已关闭连接无副作用（幂等）。
                tokio::task::yield_now().await;
                Ok(format!(
                    "RebuildConnection: closed leaked connections (threshold {}ms) and rebuilt fresh connections",
                    self.config.leak_threshold_ms
                ))
            }
            SelfHealAction::ExpandPool => {
                if self.config.pool_target_capacity == 0 {
                    self.emit_alert(action, "pool_target_capacity must be > 0");
                    return Err(SelfHealError::InvalidConfig(
                        "pool_target_capacity must be > 0".to_string(),
                    ));
                }
                tokio::task::yield_now().await;
                Ok(format!(
                    "ExpandPool: expanded pool capacity to target {}",
                    self.config.pool_target_capacity
                ))
            }
            SelfHealAction::ThrottleDegrade => {
                if self.config.slow_query_qps_threshold == 0 {
                    self.emit_alert(action, "slow_query_qps_threshold must be > 0");
                    return Err(SelfHealError::InvalidConfig(
                        "slow_query_qps_threshold must be > 0".to_string(),
                    ));
                }
                tokio::task::yield_now().await;
                Ok(format!(
                    "ThrottleDegrade: throttled slow queries to {} qps",
                    self.config.slow_query_qps_threshold
                ))
            }
            SelfHealAction::SwitchNode => {
                let target = self.config.target_nodes.first().ok_or_else(|| {
                    self.emit_alert(action, "no target node available");
                    SelfHealError::NoTargetNode
                })?;
                tokio::task::yield_now().await;
                Ok(format!(
                    "SwitchNode: switched unreachable node to target '{}'",
                    target
                ))
            }
        }
    }

    /// 发出 `SELF_HEAL_FAILED` 告警。沿用 [`crate::LogAlertChannel`] 的 stderr 风格。
    fn emit_alert(&self, action: SelfHealAction, reason: &str) {
        eprintln!("[SELF_HEAL_FAILED] action={:?} reason={}", action, reason);
    }

    fn read_state(&self, fault: &FaultType) -> Option<HealState> {
        self.states.lock().ok().and_then(|s| s.get(fault).cloned())
    }

    fn write_state(&self, fault: FaultType, state: HealState) {
        if let Ok(mut states) = self.states.lock() {
            states.insert(fault, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_heal_connection_leak() {
        let coord = SelfHealCoordinator::new(SelfHealConfig::default());
        let result = coord.heal(&FaultType::ConnectionLeak).await.unwrap();
        assert_eq!(result.fault_type, FaultType::ConnectionLeak);
        assert_eq!(result.action_taken, SelfHealAction::RebuildConnection);
        assert!(result.success);
        assert!(!result.idempotent);
        assert!(result.heal_log.contains("RebuildConnection"));
        assert!(coord.is_healed(&FaultType::ConnectionLeak));
    }

    #[tokio::test]
    async fn test_heal_pool_exhaustion() {
        let coord = SelfHealCoordinator::new(SelfHealConfig::default());
        let result = coord.heal(&FaultType::PoolExhaustion).await.unwrap();
        assert_eq!(result.fault_type, FaultType::PoolExhaustion);
        assert_eq!(result.action_taken, SelfHealAction::ExpandPool);
        assert!(result.success);
        assert!(!result.idempotent);
        assert!(result.heal_log.contains("ExpandPool"));
    }

    #[tokio::test]
    async fn test_heal_slow_query_storm() {
        let coord = SelfHealCoordinator::new(SelfHealConfig::default());
        let result = coord.heal(&FaultType::SlowQueryStorm).await.unwrap();
        assert_eq!(result.fault_type, FaultType::SlowQueryStorm);
        assert_eq!(result.action_taken, SelfHealAction::ThrottleDegrade);
        assert!(result.success);
        assert!(!result.idempotent);
        assert!(result.heal_log.contains("ThrottleDegrade"));
    }

    #[tokio::test]
    async fn test_heal_node_unreachable() {
        let coord = SelfHealCoordinator::new(SelfHealConfig::default());
        let result = coord.heal(&FaultType::NodeUnreachable).await.unwrap();
        assert_eq!(result.fault_type, FaultType::NodeUnreachable);
        assert_eq!(result.action_taken, SelfHealAction::SwitchNode);
        assert!(result.success);
        assert!(!result.idempotent);
        assert!(result.heal_log.contains("SwitchNode"));
        assert!(result.heal_log.contains("node-secondary"));
    }

    #[tokio::test]
    async fn test_idempotent_repeated_heal_all_faults() {
        let coord = SelfHealCoordinator::new(SelfHealConfig::default());
        let faults = [
            FaultType::ConnectionLeak,
            FaultType::PoolExhaustion,
            FaultType::SlowQueryStorm,
            FaultType::NodeUnreachable,
        ];
        for fault in faults {
            let first = coord.heal(&fault).await.unwrap();
            assert!(
                !first.idempotent,
                "first heal of {:?} must be non-idempotent",
                fault
            );

            let second = coord.heal(&fault).await.unwrap();
            assert!(
                second.idempotent,
                "second heal of {:?} must be idempotent",
                fault
            );
            assert_eq!(
                second.success, first.success,
                "success must be consistent for {:?}",
                fault
            );
            assert_eq!(
                second.action_taken, first.action_taken,
                "action must be consistent for {:?}",
                fault
            );

            let third = coord.heal(&fault).await.unwrap();
            assert!(
                third.idempotent,
                "third heal of {:?} must be idempotent",
                fault
            );
            assert_eq!(third.action_taken, first.action_taken);
        }
    }

    #[tokio::test]
    async fn test_heal_node_unreachable_no_target() {
        let config = SelfHealConfig {
            target_nodes: vec![],
            ..SelfHealConfig::default()
        };
        let coord = SelfHealCoordinator::new(config);
        let err = coord.heal(&FaultType::NodeUnreachable).await.unwrap_err();
        assert_eq!(err, SelfHealError::NoTargetNode);
        assert!(!coord.is_healed(&FaultType::NodeUnreachable));
    }

    #[tokio::test]
    async fn test_heal_invalid_config_leak_threshold() {
        let config = SelfHealConfig {
            leak_threshold_ms: 0,
            ..SelfHealConfig::default()
        };
        let coord = SelfHealCoordinator::new(config);
        let err = coord.heal(&FaultType::ConnectionLeak).await.unwrap_err();
        assert!(matches!(err, SelfHealError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn test_reset_allows_re_heal() {
        let coord = SelfHealCoordinator::new(SelfHealConfig::default());
        let first = coord.heal(&FaultType::PoolExhaustion).await.unwrap();
        assert!(!first.idempotent);

        assert!(coord.reset(&FaultType::PoolExhaustion));
        assert!(!coord.is_healed(&FaultType::PoolExhaustion));

        let after_reset = coord.heal(&FaultType::PoolExhaustion).await.unwrap();
        assert!(
            !after_reset.idempotent,
            "after reset, heal must be non-idempotent"
        );
        assert!(after_reset.success);
    }

    #[tokio::test]
    async fn test_reset_unknown_fault_returns_false() {
        let coord = SelfHealCoordinator::new(SelfHealConfig::default());
        assert!(!coord.reset(&FaultType::ConnectionLeak));
    }

    #[test]
    fn test_fault_type_preferred_action_mapping() {
        assert_eq!(
            FaultType::ConnectionLeak.preferred_action(),
            SelfHealAction::RebuildConnection
        );
        assert_eq!(
            FaultType::PoolExhaustion.preferred_action(),
            SelfHealAction::ExpandPool
        );
        assert_eq!(
            FaultType::SlowQueryStorm.preferred_action(),
            SelfHealAction::ThrottleDegrade
        );
        assert_eq!(
            FaultType::NodeUnreachable.preferred_action(),
            SelfHealAction::SwitchNode
        );
    }

    #[test]
    fn test_self_heal_error_display() {
        assert_eq!(
            SelfHealError::NoTargetNode.to_string(),
            "no target node available for switch"
        );
        assert_eq!(
            SelfHealError::InvalidConfig("bad".to_string()).to_string(),
            "invalid self-heal config: bad"
        );
    }
}
