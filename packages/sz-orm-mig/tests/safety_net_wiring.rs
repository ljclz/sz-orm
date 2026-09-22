//! v7.9.0 演进安全网端到端接线测试

use std::time::{Duration, SystemTime};

use sz_orm_mig::gray_release::TrafficSwitchMethod;
use sz_orm_mig::gray_traffic_router::GrayTrafficRouter;
use sz_orm_mig::safety_net::impact_analyzer::ChangeObject;
use sz_orm_mig::safety_net::{
    EvolutionAuditTimeline, FreezeCheckResult, FreezeConfig, FreezeWindow, ImpactAnalyzer,
    ImpactConfig, RollbackSandbox, SandboxConfig, ShadowConfig, ShadowTrafficVerifier,
    TimelineEventType, TimelineNode,
};

/// 测试 1：FreezeWindow 冻结期拦截变更全链路
#[tokio::test]
async fn test_safety_net_freeze_window_chain() {
    let now = SystemTime::now();
    let config = FreezeConfig {
        window_start: now,
        window_end: now + Duration::from_secs(3600),
        exceptions: vec!["critical_hotfix".to_string()].into_iter().collect(),
        max_exemption_per_hour: 5,
    };
    let fw = FreezeWindow::new(config);

    assert_eq!(fw.check("regular_change"), FreezeCheckResult::Frozen);
    assert_eq!(fw.check("critical_hotfix"), FreezeCheckResult::Exempted);

    let exemption = fw.request_exemption("emergency_fix").unwrap();
    assert!(exemption.approved);
}

/// 测试 2：ShadowTrafficVerifier 影子流量验证阻断缺陷发布
#[tokio::test]
async fn test_safety_net_shadow_verifier_blocks_defects() {
    let router = GrayTrafficRouter::new(TrafficSwitchMethod::Percentage, 20);
    let verifier = ShadowTrafficVerifier::new(router, ShadowConfig::default());

    let clean_result = verifier.verify(vec![], 100).unwrap();
    assert!(!clean_result.defect_detected);
    assert_eq!(clean_result.shadow_requests, 20);

    let defects = vec!["result_mismatch".to_string(), "timeout_error".to_string()];
    let result = verifier.verify(defects, 100);
    assert!(result.is_err());
}

/// 测试 3：RollbackSandbox 沙箱预演回滚正确性验证
#[tokio::test]
async fn test_safety_net_rollback_sandbox_dry_run() {
    let sandbox = RollbackSandbox::new(SandboxConfig::default());
    let rollback_op = sz_orm_mig::safety_net::rollback_sandbox::RollbackOp {
        rollback_sql: "ALTER TABLE orders DROP COLUMN new_col".to_string(),
        target_version: "v1.0".to_string(),
    };
    let snapshot = sz_orm_mig::safety_net::rollback_sandbox::DataSnapshot {
        snapshot_id: "snap_20260921".to_string(),
        tables: vec!["orders".to_string(), "users".to_string()],
        row_count: 5000,
    };
    let result = sandbox.dry_run(&rollback_op, &snapshot).unwrap();
    assert!(result.dry_run_passed);
    assert!(result.correctness_verified);
    assert!(!result.diff.is_empty());
}

/// 测试 4：EvolutionAuditTimeline 完整发布时间线构建
#[tokio::test]
async fn test_safety_net_audit_timeline_full() {
    let timeline = EvolutionAuditTimeline::new();
    let base = SystemTime::now();
    timeline.record_event(
        "release_v790",
        TimelineNode {
            event_type: TimelineEventType::ShadowVerify,
            timestamp: base,
            description: "影子验证通过".to_string(),
            success: true,
        },
    );
    timeline.record_event(
        "release_v790",
        TimelineNode {
            event_type: TimelineEventType::GrayAdvance,
            timestamp: base + Duration::from_secs(60),
            description: "灰度推进 10%".to_string(),
            success: true,
        },
    );
    timeline.record_event(
        "release_v790",
        TimelineNode {
            event_type: TimelineEventType::FullRelease,
            timestamp: base + Duration::from_secs(600),
            description: "全量发布".to_string(),
            success: true,
        },
    );
    let result = timeline.build_timeline("release_v790").unwrap();
    assert_eq!(result.nodes.len(), 3);
    assert_eq!(result.nodes[0].event_type, TimelineEventType::ShadowVerify);
    assert_eq!(result.nodes[2].event_type, TimelineEventType::FullRelease);
}
/// 测试 5：ImpactAnalyzer 影响面分析全链路——变更前评估受影响查询/应用/下游消费者
#[tokio::test]
async fn test_safety_net_impact_analyzer_chain() {
    let analyzer = ImpactAnalyzer::new(ImpactConfig::default());

    let change = ChangeObject {
        target_table: "orders".to_string(),
        change_type: "ALTER ADD COLUMN".to_string(),
    };
    let report = analyzer.analyze(&change).unwrap();
    assert!(!report.timeout);
    assert_eq!(report.affected_queries.len(), 2);
    assert!(report.affected_queries[0].contains("orders"));
    assert_eq!(report.affected_apps.len(), 2);
    assert_eq!(report.downstream_consumers.len(), 1);
    assert!(report.downstream_consumers[0].contains("orders"));

    let timeout_result = analyzer.analyze_with_timeout_flag(&change, true);
    assert!(matches!(
        timeout_result,
        Err(sz_orm_mig::safety_net::SafetyNetError::ImpactAnalysisTimeout(_))
    ));
}
