//! 演进审计时间线：收集发布全过程事件 → 按时间排序 → 关联节点

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::SafetyNetError;

/// 时间线事件类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimelineEventType {
    ShadowVerify,
    GrayAdvance,
    HealthJudge,
    RollbackDryRun,
    FullRelease,
}

/// 时间线节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineNode {
    pub event_type: TimelineEventType,
    pub timestamp: SystemTime,
    pub description: String,
    pub success: bool,
}

/// 审计时间线
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTimeline {
    pub release_id: String,
    pub nodes: Vec<TimelineNode>,
}

/// 演进审计时间线构建器
pub struct EvolutionAuditTimeline {
    events: parking_lot::RwLock<std::collections::HashMap<String, Vec<TimelineNode>>>,
}

impl EvolutionAuditTimeline {
    pub fn new() -> Self {
        Self {
            events: parking_lot::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// 记录事件
    pub fn record_event(&self, release_id: &str, node: TimelineNode) {
        let mut events = self.events.write();
        events.entry(release_id.to_string()).or_default().push(node);
    }

    /// 构建时间线
    pub fn build_timeline(&self, release_id: &str) -> Result<AuditTimeline, SafetyNetError> {
        let events = self.events.read();
        let nodes = events
            .get(release_id)
            .ok_or_else(|| SafetyNetError::ReleaseNotFound(release_id.to_string()))?;
        let mut sorted_nodes: Vec<TimelineNode> = nodes.clone();
        sorted_nodes.sort_by_key(|n| n.timestamp);
        Ok(AuditTimeline {
            release_id: release_id.to_string(),
            nodes: sorted_nodes,
        })
    }
}

impl Default for EvolutionAuditTimeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_node(event_type: TimelineEventType, offset_secs: u64) -> TimelineNode {
        TimelineNode {
            event_type,
            timestamp: SystemTime::now() + Duration::from_secs(offset_secs),
            description: "test event".to_string(),
            success: true,
        }
    }

    #[test]
    fn test_build_timeline_complete() {
        let timeline = EvolutionAuditTimeline::new();
        timeline.record_event("release_001", make_node(TimelineEventType::ShadowVerify, 0));
        timeline.record_event("release_001", make_node(TimelineEventType::GrayAdvance, 10));
        timeline.record_event("release_001", make_node(TimelineEventType::FullRelease, 20));
        let result = timeline.build_timeline("release_001").unwrap();
        assert_eq!(result.nodes.len(), 3);
        assert_eq!(result.nodes[0].event_type, TimelineEventType::ShadowVerify);
        assert_eq!(result.nodes[1].event_type, TimelineEventType::GrayAdvance);
        assert_eq!(result.nodes[2].event_type, TimelineEventType::FullRelease);
    }

    #[test]
    fn test_build_timeline_not_found() {
        let timeline = EvolutionAuditTimeline::new();
        let result = timeline.build_timeline("nonexistent");
        assert!(matches!(result, Err(SafetyNetError::ReleaseNotFound(_))));
    }

    #[test]
    fn test_build_timeline_sorted() {
        let timeline = EvolutionAuditTimeline::new();
        timeline.record_event("release_002", make_node(TimelineEventType::FullRelease, 30));
        timeline.record_event("release_002", make_node(TimelineEventType::ShadowVerify, 0));
        timeline.record_event("release_002", make_node(TimelineEventType::GrayAdvance, 10));
        let result = timeline.build_timeline("release_002").unwrap();
        assert_eq!(result.nodes[0].event_type, TimelineEventType::ShadowVerify);
        assert_eq!(result.nodes[1].event_type, TimelineEventType::GrayAdvance);
        assert_eq!(result.nodes[2].event_type, TimelineEventType::FullRelease);
    }
}
