//! 运维统一视图类型（v10.3.0）
//!
//! 一次调用聚合五大方向状态摘要 + 告警聚合，返回可序列化快照。

use serde::{Deserialize, Serialize};

use crate::auto_wire::AutoWireStatus;

/// 运维统一快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsSnapshot {
    /// 自动接线状态摘要
    pub auto_wire_status_summary: AutoWireStatus,
    /// 收益量化摘要
    pub profit_summary: ProfitSummary,
    /// 自愈摘要
    pub self_heal_summary: SelfHealSummary,
    /// 原子更新摘要
    pub atomic_update_summary: AtomicUpdateSummary,
    /// 闭环健康度摘要
    pub loop_health_summary: LoopHealthSummary,
    /// 告警聚合
    pub alert_aggregation: Vec<OpsAlert>,
    /// 快照时间（UNIX 毫秒）
    pub snapshot_time: u64,
}

/// 收益量化摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProfitSummary {
    /// 持续采集模式
    ContinuousCollect {
        hit_count: u64,
        skip_count: u64,
        hit_ratio: f64,
    },
    /// A/B 基准模式
    BenchmarkBaseline {
        throughput_uplift_pct: f64,
        latency_reduction_pct: f64,
    },
    /// 未启用
    Disabled,
}

/// 自愈摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealSummary {
    /// 事件总数
    pub total_events: usize,
    /// 最近事件类型
    pub last_event_type: Option<String>,
    /// 最近事件时间
    pub last_event_time: Option<u64>,
    /// 最近事件结果
    pub last_event_result: Option<String>,
    /// AutoHealer 是否启用（连接级自愈）
    pub auto_healer_enabled: bool,
    /// SelfHealEngine 是否启用（DB 级自愈）
    pub self_heal_engine_enabled: bool,
}

/// 原子更新摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicUpdateSummary {
    /// 事件总数
    pub total_events: usize,
    /// 最近事件类型
    pub last_event_type: Option<String>,
    /// 最近事件时间
    pub last_event_time: Option<u64>,
    /// 最近事件结果
    pub last_event_result: Option<String>,
}

/// 闭环健康度摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopHealthSummary {
    /// 健康评分 [0.0, 100.0]
    pub health_score: Option<f64>,
    /// 积压深度
    pub backlog_depth: Option<u32>,
    /// 降级次数
    pub degradation_count: Option<u64>,
    /// 是否启用
    pub enabled: bool,
}

/// 运维告警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsAlert {
    /// 告警来源
    pub source: AlertSource,
    /// 严重程度
    pub severity: AlertSeverity,
    /// 告警消息
    pub message: String,
    /// 时间戳（UNIX 毫秒）
    pub timestamp: u64,
}

/// 告警来源
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertSource {
    AutoWire,
    ProfitQuant,
    SelfHeal,
    AtomicUpdate,
    LoopHealth,
}

/// 告警严重程度
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertSeverity {
    Warn,
    Critical,
}
