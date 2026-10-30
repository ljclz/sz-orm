//! v10.11.0 T4.* 零信任合规自动化端到端接线测试
//!
//! 验证生产调用点可达：
//! - T4.1 零信任访问验证：ZeroTrustVerifier + DeviceFingerprint + DynamicPermission
//! - T4.2 持续合规监控：ContinuousComplianceMonitor + EventDrivenScanner + AutoRemediator
//! - T4.3 数据生命周期自动化（后续任务追加）
//! - T4.4 零知识审计（后续任务追加）
//! - T4.5 供应链安全深化（后续任务追加）

#![cfg(feature = "compliance-auto-scan")]

use sz_orm_audit::compliance_auto_checker::{
    AutoRemediator, ComplianceAutoCheckConfig, ContinuousComplianceMonitor, DataAccessEvent,
    DataAccessEventType, MonitorResult, RemediationAction, RemediationActionType, RemediationError,
    ViolationSeverity,
};
use sz_orm_audit::gdpr_ccpa::{
    LifecycleAutoManager, LifecycleError, LifecycleResult, LifecycleStatus, RetentionConfig,
};
use sz_orm_audit::hash_chain_enhanced::{
    AuditOpType, AuditResult, EnhancedAuditEntry, MerkleVerifier, ZeroKnowledgeAuditor,
    ZkAuditResult,
};
use sz_orm_audit::{
    AccessContext, AccessRequest, BehaviorProfile, ContextVerifier, DenyReason, DeviceFingerprint,
    ErrorCode, VerifyResult, ZeroTrustVerifier,
};
use sz_orm_audit::{
    ReproducibleBuildVerifier, SbomFormat, SbomGenerator, Vulnerability, VulnerabilityMonitor,
    VulnerabilitySeverity,
};

/// 构造合法的访问请求
fn make_valid_request() -> AccessRequest {
    AccessRequest {
        identity: "user1".to_string(),
        device_fingerprint: DeviceFingerprint::new("device1", 0.8, 1000),
        context: AccessContext::new("127.0.0.1", 1000),
        requested_permission: "read".to_string(),
    }
}

/// 构造已注册的验证器
fn make_verifier_with_registrations() -> ZeroTrustVerifier {
    let verifier = ZeroTrustVerifier::new();
    verifier.register_device(DeviceFingerprint::new("device1", 0.8, 1000));
    verifier.grant_permission("user1", vec!["read".to_string()]);
    verifier
}

/// T4.1 测试：身份验证
///
/// 验证行为副作用：空身份返回 Deny(IdentityFailed)。
#[test]
fn test_zero_trust_identity_verify() {
    let verifier = make_verifier_with_registrations();
    let mut request = make_valid_request();
    request.identity = "".to_string();

    let result = verifier.verify_access(&request).unwrap();
    assert!(result.is_denied());
    if let VerifyResult::Deny { reason, error_code } = result {
        assert_eq!(reason, DenyReason::IdentityFailed);
        assert_eq!(error_code, ErrorCode::E401IdentityFailed);
    }
}

/// T4.1 测试：设备指纹验证
///
/// 验证行为副作用：未注册设备返回 Deny(DeviceUnknown)。
#[test]
fn test_zero_trust_device_fingerprint() {
    let verifier = make_verifier_with_registrations();
    let mut request = make_valid_request();
    request.device_fingerprint = DeviceFingerprint::new("unknown_device", 0.8, 1000);

    let result = verifier.verify_access(&request).unwrap();
    assert!(result.is_denied());
    if let VerifyResult::Deny { reason, error_code } = result {
        assert_eq!(reason, DenyReason::DeviceUnknown);
        assert_eq!(error_code, ErrorCode::E403DeviceUnknown);
    }
}

/// T4.1 测试：低信任评分设备被拒绝
///
/// 验证行为副作用：信任评分低于阈值的设备返回 Deny(DeviceUnknown)。
#[test]
fn test_zero_trust_device_low_trust_score() {
    let verifier = make_verifier_with_registrations();
    verifier.register_device(DeviceFingerprint::new("low_trust", 0.3, 1000));

    let mut request = make_valid_request();
    request.device_fingerprint = DeviceFingerprint::new("low_trust", 0.3, 1000);

    let result = verifier.verify_access(&request).unwrap();
    assert!(result.is_denied());
    if let VerifyResult::Deny { reason, .. } = result {
        assert_eq!(reason, DenyReason::DeviceUnknown);
    }
}

/// T4.1 测试：上下文验证
///
/// 验证行为副作用：异常请求频率返回 Deny(ContextAbnormal)。
#[test]
fn test_zero_trust_context_verify() {
    let verifier = make_verifier_with_registrations();
    let mut request = make_valid_request();
    request.context.behavior_profile.avg_request_rate = 2000.0;

    let result = verifier.verify_access(&request).unwrap();
    assert!(result.is_denied());
    if let VerifyResult::Deny { reason, error_code } = result {
        assert_eq!(reason, DenyReason::ContextAbnormal);
        assert_eq!(error_code, ErrorCode::E403ContextAbnormal);
    }
}

/// T4.1 测试：IP 白名单验证
///
/// 验证行为副作用：非白名单 IP 返回 Deny(ContextAbnormal)。
#[test]
fn test_zero_trust_context_ip_whitelist() {
    let verifier = ZeroTrustVerifier::new().with_allowed_ip("192.168.1.1");
    verifier.register_device(DeviceFingerprint::new("device1", 0.8, 1000));
    verifier.grant_permission("user1", vec!["read".to_string()]);

    let request = AccessRequest {
        identity: "user1".to_string(),
        device_fingerprint: DeviceFingerprint::new("device1", 0.8, 1000),
        context: AccessContext::new("10.0.0.1", 1000),
        requested_permission: "read".to_string(),
    };

    let result = verifier.verify_access(&request).unwrap();
    assert!(result.is_denied());
    if let VerifyResult::Deny { reason, .. } = result {
        assert_eq!(reason, DenyReason::ContextAbnormal);
    }
}

/// T4.1 测试：动态权限验证
///
/// 验证行为副作用：无权限返回 Deny(PermissionDenied)。
#[test]
fn test_zero_trust_dynamic_permission() {
    let verifier = make_verifier_with_registrations();
    let mut request = make_valid_request();
    request.requested_permission = "admin".to_string();

    let result = verifier.verify_access(&request).unwrap();
    assert!(result.is_denied());
    if let VerifyResult::Deny { reason, error_code } = result {
        assert_eq!(reason, DenyReason::PermissionDenied);
        assert_eq!(error_code, ErrorCode::E403PermissionDenied);
    }
}

/// T4.1 测试：验证失败返回明确错误码
///
/// 验证行为副作用：每种拒绝原因对应明确的错误码（非静默拒绝）。
#[test]
fn test_zero_trust_deny_with_error_code() {
    let verifier = make_verifier_with_registrations();

    // 身份失败 → E401
    let req1 = AccessRequest {
        identity: "".to_string(),
        device_fingerprint: DeviceFingerprint::new("device1", 0.8, 1000),
        context: AccessContext::new("127.0.0.1", 1000),
        requested_permission: "read".to_string(),
    };
    if let VerifyResult::Deny { error_code, .. } = verifier.verify_access(&req1).unwrap() {
        assert_eq!(error_code.code(), "E401_IDENTITY_FAILED");
    }

    // 设备未知 → E403
    let req2 = AccessRequest {
        identity: "user1".to_string(),
        device_fingerprint: DeviceFingerprint::new("unknown", 0.8, 1000),
        context: AccessContext::new("127.0.0.1", 1000),
        requested_permission: "read".to_string(),
    };
    if let VerifyResult::Deny { error_code, .. } = verifier.verify_access(&req2).unwrap() {
        assert_eq!(error_code.code(), "E403_DEVICE_UNKNOWN");
    }

    // 权限不足 → E403
    let req3 = AccessRequest {
        identity: "user1".to_string(),
        device_fingerprint: DeviceFingerprint::new("device1", 0.8, 1000),
        context: AccessContext::new("127.0.0.1", 1000),
        requested_permission: "admin".to_string(),
    };
    if let VerifyResult::Deny { error_code, .. } = verifier.verify_access(&req3).unwrap() {
        assert_eq!(error_code.code(), "E403_PERMISSION_DENIED");
    }
}

/// T4.1 端到端测试：零信任访问全流程
///
/// 验证行为副作用：身份→设备→上下文→权限全流程通过后放行。
#[test]
fn test_zero_trust_access_integration() {
    let verifier = make_verifier_with_registrations();
    let request = make_valid_request();

    let result = verifier.verify_access(&request).unwrap();
    assert!(result.is_allowed(), "合法请求应放行");
}

/// T4.1 端到端测试：验证失败返回明确错误码（非静默拒绝）
///
/// 验证行为副作用：拒绝时返回明确错误码和原因，用户可申诉。
#[test]
fn e2e_zero_trust_access_deny_with_error_code() {
    let verifier = make_verifier_with_registrations();

    // 无权限请求
    let request = AccessRequest {
        identity: "user1".to_string(),
        device_fingerprint: DeviceFingerprint::new("device1", 0.8, 1000),
        context: AccessContext::new("127.0.0.1", 1000),
        requested_permission: "delete".to_string(),
    };

    let result = verifier.verify_access(&request).unwrap();
    assert!(result.is_denied(), "无权限请求应拒绝");

    if let VerifyResult::Deny { reason, error_code } = result {
        // 明确错误码
        assert_eq!(error_code.code(), "E403_PERMISSION_DENIED");
        assert_eq!(reason, DenyReason::PermissionDenied);

        // 申诉机制：申诉通过后加入白名单
        verifier.appeal_approve("user1", vec!["delete".to_string()]);

        // 申诉后再次请求应放行
        let result_after = verifier.verify_access(&request).unwrap();
        assert!(result_after.is_allowed(), "申诉通过后应放行");
    }
}

/// T4.1 测试：行为画像默认值 + 行为副作用
///
/// 验证行为副作用：
/// - Default 值合理（avg_request_rate > 0, common_hours 非空）
/// - 默认请求频率（10.0）低于阈值（1000.0）→ ContextVerifier 不拒绝
/// - 异常请求频率（2000.0）超过阈值 → ContextVerifier 返回 ContextAbnormal
#[test]
fn test_behavior_profile_default() {
    let profile = BehaviorProfile::default();
    assert!(profile.avg_request_rate > 0.0, "默认请求频率应 > 0");
    assert!(!profile.common_hours.is_empty(), "默认时间段应非空");

    let verifier = ContextVerifier::new();
    let ctx_normal = AccessContext::new("127.0.0.1", 3600 * 10);
    assert!(
        verifier.verify(&ctx_normal).is_none(),
        "默认频率 10.0 < 阈值 1000.0，应通过"
    );

    let mut ctx_abnormal = AccessContext::new("127.0.0.1", 3600 * 10);
    ctx_abnormal.behavior_profile.avg_request_rate = 2000.0;
    assert_eq!(
        verifier.verify(&ctx_abnormal),
        Some(DenyReason::ContextAbnormal),
        "频率 2000.0 > 阈值 1000.0，应拒绝"
    );
}

/// T4.1 测试：访问上下文请求小时
///
/// 验证行为副作用：request_hour 正确计算。
#[test]
fn test_access_context_request_hour() {
    // timestamp = 3600 * 10 = 10:00 AM
    let ctx = AccessContext::new("127.0.0.1", 3600 * 10);
    assert_eq!(ctx.request_hour(), 10, "10:00 AM 应返回 10");
}
// ==================== T4.2: 持续合规监控测试 ====================

/// 构造持续合规监控器（敏感字段：ssn, password, credit_card）
fn make_monitor() -> ContinuousComplianceMonitor {
    ContinuousComplianceMonitor::new(
        ComplianceAutoCheckConfig::default(),
        vec![
            "ssn".to_string(),
            "password".to_string(),
            "credit_card".to_string(),
        ],
    )
}

/// T4.2 测试：事件驱动扫描检测违规
///
/// 验证行为副作用：敏感字段未脱敏的事件被扫描器检测为违规。
#[test]
fn test_continuous_compliance_event_driven() {
    let monitor = make_monitor();
    let event = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "users",
        vec!["ssn".to_string()],
        "SELECT ssn FROM users WHERE ssn='123-45-6789'",
        1000,
    );

    let result = monitor.on_data_access_event(&event).unwrap();
    match result {
        MonitorResult::AutoRemediated {
            violation,
            remediation,
            ..
        } => {
            assert!(violation.description.contains("ssn"), "违规应涉及 ssn 字段");
            assert!(remediation.success, "自动处置应成功");
            assert_ne!(
                remediation.before_value, remediation.after_value,
                "处置前后值应不同"
            );
            assert!(
                remediation.after_value.contains("******"),
                "处置后应包含脱敏标记"
            );
        }
        other => panic!("低风险敏感字段违规应自动处置，got {:?}", other),
    }

    assert_eq!(monitor.total_events(), 1, "应处理 1 个事件");
    assert_eq!(monitor.violation_count(), 1, "应检测到 1 个违规");
    assert_eq!(monitor.auto_remediated_count(), 1, "应自动处置 1 个");
}

/// T4.2 测试：合规事件返回 Compliant
///
/// 验证行为副作用：无敏感字段的事件返回 Compliant。
#[test]
fn test_continuous_compliance_compliant_event() {
    let monitor = make_monitor();
    let event = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "orders",
        vec!["order_id".to_string(), "amount".to_string()],
        "SELECT order_id, amount FROM orders",
        2000,
    );

    let result = monitor.on_data_access_event(&event).unwrap();
    assert!(
        matches!(result, MonitorResult::Compliant),
        "合规事件应返回 Compliant"
    );
    assert_eq!(monitor.compliant_count(), 1);
    assert_eq!(monitor.violation_count(), 0);
}

/// T4.2 测试：低风险违规自动处置
///
/// 验证行为副作用：Medium 级别违规自动脱敏，before != after。
#[test]
fn test_continuous_compliance_auto_remediate() {
    let monitor = make_monitor();
    // 模拟敏感 SQL（占位数据，仅用于验证脱敏/处置副作用）
    let simulated_sql = format!("SELECT * FROM users WHERE password='{}'", "secret123");
    let event = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "users",
        vec!["password".to_string()],
        &simulated_sql,
        3000,
    );

    let result = monitor.on_data_access_event(&event).unwrap();
    match result {
        MonitorResult::AutoRemediated { remediation, .. } => {
            assert!(remediation.success, "自动脱敏应成功");
            assert!(
                remediation.before_value.contains("secret123"),
                "处置前应包含明文"
            );
            assert!(
                remediation.after_value.contains("******"),
                "处置后应包含脱敏标记"
            );
            assert!(
                !remediation.after_value.contains("secret123"),
                "处置后不应包含明文"
            );
        }
        other => panic!("Medium 违规应自动处置，got {:?}", other),
    }
}

/// T4.2 测试：高风险违规告警人工介入
///
/// 验证行为副作用：High 级别违规（Schema 变更）返回 Violation 告警。
#[test]
fn test_continuous_compliance_high_risk_alert() {
    let monitor = make_monitor();
    let event = DataAccessEvent::new(
        DataAccessEventType::SchemaChange,
        "admin1",
        "users",
        vec!["id".to_string()],
        "ALTER TABLE users ADD COLUMN ssn VARCHAR(11)",
        4000,
    );

    let result = monitor.on_data_access_event(&event).unwrap();
    match result {
        MonitorResult::Violation {
            violation, alert, ..
        } => {
            assert_eq!(
                violation.severity,
                ViolationSeverity::High,
                "Schema 变更应为 High 风险"
            );
            assert!(
                alert.message.contains("需人工介入"),
                "高风险告警应提示人工介入"
            );
        }
        other => panic!("High 违规应告警人工介入，got {:?}", other),
    }
    assert_eq!(monitor.manual_alerted_count(), 1, "应告警 1 个高风险");
}

/// T4.2 测试：阻断表访问检测
///
/// 验证行为副作用：访问阻断表返回 High 级别违规告警。
#[test]
fn test_continuous_compliance_blocked_table() {
    let monitor = make_monitor().with_blocked_tables(vec!["secret_table".to_string()]);
    let event = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "secret_table",
        vec!["data".to_string()],
        "SELECT data FROM secret_table",
        5000,
    );

    let result = monitor.on_data_access_event(&event).unwrap();
    match result {
        MonitorResult::Violation { violation, .. } => {
            assert_eq!(violation.severity, ViolationSeverity::High);
            assert!(violation.description.contains("secret_table"));
        }
        other => panic!("阻断表访问应告警，got {:?}", other),
    }
}

/// T4.2 测试：自定义处置策略
///
/// 验证行为副作用：注册自定义 RemediationAction 后，自动处置使用自定义策略。
#[test]
fn test_continuous_compliance_custom_action() {
    let monitor = make_monitor();
    let custom_action = RemediationAction {
        action_id: "CUSTOM-ENCRYPT-ssn".to_string(),
        action_type: RemediationActionType::AutoEncrypt,
        description: "自定义加密 ssn 字段".to_string(),
        target_field: Some("ssn".to_string()),
    };
    monitor.register_custom_action(custom_action);

    let event = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "users",
        vec!["ssn".to_string()],
        "SELECT ssn FROM users WHERE ssn='111-22-3333'",
        6000,
    );

    let result = monitor.on_data_access_event(&event).unwrap();
    match result {
        MonitorResult::AutoRemediated { remediation, .. } => {
            assert_eq!(
                remediation.action.action_type,
                RemediationActionType::AutoEncrypt,
                "应使用自定义加密策略"
            );
            assert!(remediation.success, "自定义处置应成功");
        }
        other => panic!("应使用自定义策略自动处置，got {:?}", other),
    }
}

/// T4.2 端到端测试：违规事件 ≤ 1 秒内告警
///
/// 验证行为副作用：违规事件从扫描到告警的延迟 ≤ 1000ms。
#[test]
fn e2e_continuous_compliance_violation_alert_within_1s() {
    let monitor = make_monitor();
    let event = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "users",
        vec!["credit_card".to_string()],
        "SELECT credit_card FROM payments WHERE credit_card='4111111111111111'",
        7000,
    );

    let result = monitor.on_data_access_event(&event).unwrap();
    let latency = match &result {
        MonitorResult::AutoRemediated { latency_ms, .. } => *latency_ms,
        MonitorResult::Violation { latency_ms, .. } => *latency_ms,
        MonitorResult::Compliant => 0,
    };

    assert!(
        latency <= 1000,
        "违规告警延迟应 ≤ 1000ms，实际 {}ms",
        latency
    );
    assert!(
        monitor.max_alert_latency_ms() <= 1000,
        "最大告警延迟应 ≤ 1000ms"
    );
}

/// T4.2 端到端测试：持续合规监控全流程（违规→告警→处置）
///
/// 验证行为副作用：多个事件依次处理，统计正确。
#[test]
fn test_continuous_compliance_integration() {
    let monitor = make_monitor().with_blocked_tables(vec!["admin_logs".to_string()]);

    // 事件 1：合规查询
    let e1 = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "orders",
        vec!["order_id".to_string()],
        "SELECT order_id FROM orders",
        10000,
    );
    let r1 = monitor.on_data_access_event(&e1).unwrap();
    assert!(matches!(r1, MonitorResult::Compliant));

    // 事件 2：敏感字段未脱敏（Medium → 自动处置）
    let e2 = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "users",
        vec!["ssn".to_string()],
        "SELECT ssn FROM users WHERE ssn='999-88-7777'",
        10001,
    );
    let r2 = monitor.on_data_access_event(&e2).unwrap();
    assert!(matches!(r2, MonitorResult::AutoRemediated { .. }));

    // 事件 3：阻断表访问（High → 告警人工介入）
    let e3 = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "admin_logs",
        vec!["log_entry".to_string()],
        "SELECT log_entry FROM admin_logs",
        10002,
    );
    let r3 = monitor.on_data_access_event(&e3).unwrap();
    assert!(matches!(r3, MonitorResult::Violation { .. }));

    // 事件 4：Schema 变更（High → 告警人工介入）
    let e4 = DataAccessEvent::new(
        DataAccessEventType::SchemaChange,
        "admin1",
        "users",
        vec!["id".to_string()],
        "DROP TABLE users",
        10003,
    );
    let r4 = monitor.on_data_access_event(&e4).unwrap();
    assert!(matches!(r4, MonitorResult::Violation { .. }));

    // 统计验证
    assert_eq!(monitor.total_events(), 4, "应处理 4 个事件");
    assert_eq!(monitor.compliant_count(), 1, "1 个合规");
    assert_eq!(monitor.violation_count(), 3, "3 个违规");
    assert_eq!(monitor.auto_remediated_count(), 1, "1 个自动处置");
    assert_eq!(monitor.manual_alerted_count(), 2, "2 个人工告警");
}

/// T4.2 测试：AutoRemediator 独立使用
///
/// 验证行为副作用：AutoRemediator 可独立使用，高风险返回错误。
#[test]
fn test_auto_remediator_standalone() {
    let remediator = AutoRemediator::new();
    let event = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "users",
        vec!["ssn".to_string()],
        "SELECT ssn FROM users WHERE ssn='123-45-6789'",
        8000,
    );

    let low_violation = sz_orm_audit::compliance_auto_checker::Violation {
        id: "V-LOW-1".to_string(),
        description: "敏感字段 'ssn' 在数据访问中未脱敏".to_string(),
        severity: ViolationSeverity::Medium,
    };
    let result = remediator.auto_remediate(&low_violation, &event).unwrap();
    assert!(result.success, "低风险自动处置应成功");
    assert_eq!(remediator.auto_remediated_count(), 1);

    let high_violation = sz_orm_audit::compliance_auto_checker::Violation {
        id: "V-HIGH-1".to_string(),
        description: "Schema 变更".to_string(),
        severity: ViolationSeverity::High,
    };
    let err = remediator
        .auto_remediate(&high_violation, &event)
        .unwrap_err();
    assert_eq!(
        err,
        RemediationError::HighRiskRequiresManual,
        "高风险应拒绝自动处置"
    );
    assert_eq!(remediator.manual_alerted_count(), 1);
}

/// T4.2 测试：已脱敏事件不触发违规
///
/// 验证行为副作用：raw_sql 包含 ****** 的事件不触发敏感字段违规。
#[test]
fn test_continuous_compliance_already_desensitized() {
    let monitor = make_monitor();
    let event = DataAccessEvent::new(
        DataAccessEventType::Query,
        "user1",
        "users",
        vec!["ssn".to_string()],
        "SELECT ssn FROM users WHERE ssn='******'",
        9000,
    );

    let result = monitor.on_data_access_event(&event).unwrap();
    assert!(
        matches!(result, MonitorResult::Compliant),
        "已脱敏事件应合规"
    );
}
// ==================== T4.3: 数据生命周期自动化测试 ====================

/// 一天的秒数
const DAY_SECS: u64 = 86400;

/// T4.3 测试：保留期检查
///
/// 验证行为副作用：未到期数据返回 NotExpired，到期数据通过检查。
#[test]
fn test_lifecycle_retention_check() {
    let config = RetentionConfig {
        retention_period_days: 30,
        auto_archive: true,
        auto_destroy: false,
        destroy_audit: true,
    };
    let manager = LifecycleAutoManager::new(config);
    manager.register_data("data1", "content", 1000 * DAY_SECS);

    // 未到期（创建后 10 天，保留期 30 天）
    let result = manager.check_and_process("data1", 1010 * DAY_SECS).unwrap();
    match result {
        LifecycleResult::Retained { remaining_days } => {
            assert!(remaining_days > 0, "未到期应返回正剩余天数");
            assert_eq!(remaining_days, 20);
        }
        other => panic!("未到期应保留，got {:?}", other),
    }

    // 到期（创建后 30 天）
    let result = manager.check_and_process("data1", 1030 * DAY_SECS).unwrap();
    match result {
        LifecycleResult::Archived { archive_id } => {
            assert!(!archive_id.is_empty(), "归档 ID 应非空");
        }
        other => panic!("到期应归档，got {:?}", other),
    }
}

/// T4.3 测试：自动归档
///
/// 验证行为副作用：到期数据被归档，archive_count 增加。
#[test]
fn test_lifecycle_auto_archive() {
    let config = RetentionConfig {
        retention_period_days: 7,
        auto_archive: true,
        auto_destroy: false,
        destroy_audit: true,
    };
    let manager = LifecycleAutoManager::new(config);
    manager.register_data("log1", "log content", 0);

    assert_eq!(manager.archive_count(), 0, "初始无归档");
    let result = manager.check_and_process("log1", 7 * DAY_SECS).unwrap();
    match result {
        LifecycleResult::Archived { archive_id } => {
            assert!(
                manager.archiver().get_archive(&archive_id).is_some(),
                "归档应可查"
            );
        }
        other => panic!("应归档，got {:?}", other),
    }
    assert_eq!(manager.archive_count(), 1, "归档数应为 1");

    let state = manager.get_state("log1").unwrap();
    assert_eq!(state.status, LifecycleStatus::Archived, "状态应为 Archived");
}

/// T4.3 端到端测试：到期数据自动销毁 + 审计留痕
///
/// 验证行为副作用：数据被销毁后不可获取，审计留痕存在。
#[test]
fn e2e_lifecycle_auto_destroy_with_audit_trail() {
    let config = RetentionConfig {
        retention_period_days: 1,
        auto_archive: true,
        auto_destroy: true,
        destroy_audit: true,
    };
    let manager = LifecycleAutoManager::new(config);
    manager.register_data("temp1", "sensitive data", 0);

    assert!(manager.get_data("temp1").is_some(), "销毁前数据应存在");

    let now = 2 * DAY_SECS;
    let result = manager.check_and_process("temp1", now).unwrap();
    match result {
        LifecycleResult::Destroyed { destroy_audit_id } => {
            assert!(!destroy_audit_id.is_empty(), "销毁审计 ID 应非空");
        }
        other => panic!("应销毁，got {:?}", other),
    }

    assert!(manager.get_data("temp1").is_none(), "销毁后数据应不可获取");
    assert_eq!(manager.destroy_count(), 1, "销毁数应为 1");

    let trails = manager.destroy_audit_trails();
    assert_eq!(trails.len(), 1, "应有 1 条销毁审计留痕");
    assert_eq!(trails[0].data_id, "temp1", "审计留痕 data_id 应匹配");
    assert_eq!(trails[0].destroyed_at, now, "销毁时间应匹配");

    let state = manager.get_state("temp1").unwrap();
    assert_eq!(
        state.status,
        LifecycleStatus::Destroyed,
        "状态应为 Destroyed"
    );
}

/// T4.3 端到端测试：不误销毁未到期数据
///
/// 验证行为副作用：未到期数据返回 Retained，不被销毁。
#[test]
fn e2e_lifecycle_not_destroy_unexpired() {
    let config = RetentionConfig {
        retention_period_days: 30,
        auto_archive: true,
        auto_destroy: true,
        destroy_audit: true,
    };
    let manager = LifecycleAutoManager::new(config);
    manager.register_data("data2", "important data", 1000 * DAY_SECS);

    let result = manager.check_and_process("data2", 1010 * DAY_SECS).unwrap();
    match result {
        LifecycleResult::Retained { remaining_days } => {
            assert!(remaining_days > 0, "未到期应保留");
        }
        other => panic!("未到期不应销毁，got {:?}", other),
    }

    assert!(manager.get_data("data2").is_some(), "数据应仍存在");
    assert_eq!(manager.destroy_count(), 0, "不应有销毁");
}

/// T4.3 测试：业务持有阻止销毁
///
/// 验证行为副作用：有业务持有的到期数据返回 BusinessHold 错误。
#[test]
fn test_lifecycle_business_hold() {
    let config = RetentionConfig {
        retention_period_days: 1,
        auto_archive: true,
        auto_destroy: true,
        destroy_audit: true,
    };
    let manager = LifecycleAutoManager::new(config);
    manager.register_data("hold1", "data on hold", 0);
    manager.set_business_hold("hold1", true).unwrap();

    let result = manager.check_and_process("hold1", 10 * DAY_SECS);
    assert_eq!(
        result.unwrap_err(),
        LifecycleError::BusinessHold,
        "业务持有应阻止处理"
    );
    assert!(manager.get_data("hold1").is_some(), "数据应仍存在");

    // 释放持有后可处理
    manager.set_business_hold("hold1", false).unwrap();
    let result = manager.check_and_process("hold1", 10 * DAY_SECS).unwrap();
    assert!(
        matches!(result, LifecycleResult::Destroyed { .. }),
        "释放持有后应销毁"
    );
}

/// T4.3 端到端测试：数据生命周期全流程（到期→归档→销毁→审计）
///
/// 验证行为副作用：auto_destroy=false 时仅归档，auto_destroy=true 时归档+销毁。
#[test]
fn test_lifecycle_auto_integration() {
    // 阶段 1：仅归档（auto_destroy=false）
    let config = RetentionConfig {
        retention_period_days: 1,
        auto_archive: true,
        auto_destroy: false,
        destroy_audit: true,
    };
    let manager = LifecycleAutoManager::new(config);
    manager.register_data("doc1", "document content", 0);

    let result = manager.check_and_process("doc1", 2 * DAY_SECS).unwrap();
    match result {
        LifecycleResult::Archived { archive_id } => {
            assert!(manager.archiver().get_archive(&archive_id).is_some());
        }
        other => panic!("应仅归档，got {:?}", other),
    }
    assert_eq!(manager.archive_count(), 1, "归档 1");
    assert_eq!(manager.destroy_count(), 0, "不销毁");
    assert_eq!(
        manager.get_state("doc1").unwrap().status,
        LifecycleStatus::Archived
    );

    // 阶段 2：归档 + 销毁（auto_destroy=true）
    let config2 = RetentionConfig {
        retention_period_days: 1,
        auto_archive: true,
        auto_destroy: true,
        destroy_audit: true,
    };
    let manager2 = LifecycleAutoManager::new(config2);
    manager2.register_data("doc2", "document content", 0);

    let result = manager2.check_and_process("doc2", 2 * DAY_SECS).unwrap();
    assert!(
        matches!(result, LifecycleResult::Destroyed { .. }),
        "应归档+销毁"
    );
    assert_eq!(manager2.archive_count(), 1, "归档 1");
    assert_eq!(manager2.destroy_count(), 1, "销毁 1");
    assert!(manager2.get_data("doc2").is_none(), "数据已销毁");

    let trails = manager2.destroy_audit_trails();
    assert_eq!(trails.len(), 1, "销毁审计留痕 1 条");
    assert_eq!(trails[0].data_id, "doc2");
}

/// T4.3 测试：未找到数据返回 NotFound
///
/// 验证行为副作用：不存在的 data_id 返回 NotFound 错误。
#[test]
fn test_lifecycle_not_found() {
    let manager = LifecycleAutoManager::new(RetentionConfig::default());
    let result = manager.check_and_process("nonexistent", 0);
    assert_eq!(result.unwrap_err(), LifecycleError::NotFound);
}
// ==================== T4.4: 零知识审计测试 ====================

/// 构造增强审计条目
fn make_audit_entry(sql: &str, user: &str, ip: &str, ts: i64) -> EnhancedAuditEntry {
    EnhancedAuditEntry {
        subject: user.to_string(),
        object: "users".to_string(),
        timestamp: ts,
        op_type: AuditOpType::from_sql(sql),
        result: AuditResult::Success,
        source_ip: ip.to_string(),
        sql: sql.to_string(),
    }
}

/// T4.4 测试：审计脱敏
///
/// 验证行为副作用：包含敏感关键词的 SQL 被脱敏，不包含敏感数据。
#[test]
fn test_zero_knowledge_audit_desensitize() {
    // 模拟敏感 SQL（占位数据，仅用于验证脱敏副作用）
    let simulated_sql = format!(
        "SELECT * FROM users WHERE password='{}' AND token='{}'",
        "secret123", "abc"
    );
    let entry = make_audit_entry(&simulated_sql, "admin", "10.0.0.1", 1000);

    let desensitized = sz_orm_audit::AuditDesensitizer::desensitize(&entry);
    assert!(
        !desensitized.desensitized_sql.contains("password"),
        "脱敏后不应包含 password"
    );
    assert!(
        !desensitized.desensitized_sql.contains("secret123"),
        "脱敏后不应包含明文密码"
    );
    assert!(
        !desensitized.desensitized_sql.contains("token"),
        "脱敏后不应包含 token"
    );
    assert!(
        desensitized.desensitized_sql.contains("******"),
        "脱敏后应包含 ******"
    );
    assert!(
        !desensitized.contains_sensitive(),
        "脱敏条目不应包含敏感关键词"
    );
}

/// T4.4 测试：默克尔证明验证
///
/// 验证行为副作用：合法证明验证通过，篡改证明验证失败。
#[test]
fn test_zero_knowledge_merkle_verify() {
    let auditor = ZeroKnowledgeAuditor::new();

    let entries = vec![
        make_audit_entry("SELECT * FROM orders", "user1", "10.0.0.1", 1000),
        make_audit_entry("INSERT INTO logs VALUES(1)", "user2", "10.0.0.2", 2000),
        make_audit_entry("UPDATE users SET name='x'", "user3", "10.0.0.3", 3000),
    ];

    let mut results = Vec::new();
    for e in entries {
        results.push(auditor.log_zero_knowledge(e).unwrap());
    }

    let root = auditor.merkle_root();
    assert!(!root.is_empty(), "默克尔根应非空");

    for result in &results {
        let valid = auditor.verify_integrity(&result.merkle_proof).unwrap();
        assert!(valid, "每个条目的默克尔证明应验证通过");
    }

    let tampered_proof = MerkleVerifier::verify(&results[0].merkle_proof, "invalid_root");
    assert!(!tampered_proof, "篡改根哈希应验证失败");
}

/// T4.4 端到端测试：零知识审计不泄露敏感数据 + 默克尔证明可验证
///
/// 验证行为副作用：审计日志脱敏后不泄露敏感数据，默克尔证明可验证完整性。
#[test]
fn e2e_zero_knowledge_audit_no_sensitive_leak() {
    let auditor = ZeroKnowledgeAuditor::new();

    let result = auditor
        .log_zero_knowledge(make_audit_entry(
            "SELECT * FROM users WHERE password='mypassword' AND ssn='123-45-6789'",
            "admin",
            "10.0.0.1",
            1000,
        ))
        .unwrap();

    assert!(
        !result
            .desensitized_entry
            .desensitized_sql
            .contains("mypassword"),
        "审计结果不应泄露明文密码"
    );
    assert!(
        !result
            .desensitized_entry
            .desensitized_sql
            .contains("123-45-6789"),
        "审计结果不应泄露 SSN"
    );
    assert!(
        !result.desensitized_entry.contains_sensitive(),
        "脱敏条目不应包含任何敏感关键词"
    );

    let valid = auditor.verify_integrity(&result.merkle_proof).unwrap();
    assert!(valid, "默克尔证明应验证通过");

    assert!(auditor.verify_chain(), "哈希链应完整");
}

/// T4.4 端到端测试：零知识审计全流程（脱敏→入链→默克尔证明→验证）
///
/// 验证行为副作用：多个条目依次入链，每个默克尔证明可验证，哈希链完整。
#[test]
fn test_zero_knowledge_audit_integration() {
    let auditor = ZeroKnowledgeAuditor::new();

    let test_cases = vec![
        ("SELECT * FROM orders", "user1", "10.0.0.1", 1000i64),
        (
            "SELECT * FROM users WHERE password='secret'",
            "admin",
            "10.0.0.2",
            2000,
        ),
        ("INSERT INTO logs VALUES(1)", "user2", "10.0.0.3", 3000),
        (
            "UPDATE users SET token='new_token' WHERE id=1",
            "admin",
            "10.0.0.4",
            4000,
        ),
        ("DELETE FROM temp WHERE id=1", "user3", "10.0.0.5", 5000),
    ];

    let mut results: Vec<ZkAuditResult> = Vec::new();
    for (sql, user, ip, ts) in test_cases {
        let entry = make_audit_entry(sql, user, ip, ts);
        let result = auditor.log_zero_knowledge(entry).unwrap();
        results.push(result);
    }

    assert_eq!(auditor.len(), 5, "应入链 5 个条目");

    for (i, result) in results.iter().enumerate() {
        assert!(
            !result.desensitized_entry.contains_sensitive(),
            "条目 {} 脱敏后不应包含敏感数据",
            i
        );
        let valid = auditor.verify_integrity(&result.merkle_proof).unwrap();
        assert!(valid, "条目 {} 默克尔证明应验证通过", i);
    }

    assert!(auditor.verify_chain(), "哈希链应完整");

    let root = auditor.merkle_root();
    assert_ne!(root, "0".repeat(64), "默克尔根不应为零哈希");

    let desensitized = auditor.get_desensitized_entries();
    for entry in &desensitized {
        assert!(
            !entry.desensitized_sql.contains("secret"),
            "脱敏条目不应包含明文 secret"
        );
        assert!(
            !entry.desensitized_sql.contains("new_token"),
            "脱敏条目不应包含明文 token"
        );
    }
}
// ==================== T4.5: 供应链安全深化测试 ====================

/// T4.5 测试：SBOM 生成
///
/// 验证行为副作用：SBOM 生成后包含正确的组件和格式。
#[test]
fn test_sbom_generation() {
    let gen = SbomGenerator::new(SbomFormat::CycloneDX);
    let sbom = gen
        .generate_from_deps(&[
            ("serde", "1.0.197"),
            ("tokio", "1.37.0"),
            ("sha2", "0.10.8"),
        ])
        .unwrap();

    assert_eq!(sbom.components.len(), 3, "应包含 3 个组件");
    assert_eq!(sbom.format, SbomFormat::CycloneDX);
    assert_eq!(sbom.spec_version, "1.4", "CycloneDX 规范版本 1.4");
    assert_eq!(gen.generate_count(), 1, "生成计数应为 1");

    assert_eq!(sbom.components[0].name, "serde");
    assert_eq!(sbom.components[0].version, "1.0.197");
    assert!(
        sbom.components[0].package_url.contains("serde"),
        "package_url 应包含组件名"
    );
}

/// T4.5 测试：SBOM SPDX 格式
///
/// 验证行为副作用：SPDX 格式 SBOM 使用正确的规范版本。
#[test]
fn test_sbom_spdx_format() {
    let gen = SbomGenerator::new(SbomFormat::Spdx);
    let sbom = gen.generate_from_deps(&[("serde", "1.0")]).unwrap();
    assert_eq!(sbom.spec_version, "SPDX-2.2");
}

/// T4.5 测试：漏洞监控
///
/// 验证行为副作用：注册的漏洞在 SBOM 中被检测到，高危漏洞阻断构建。
#[test]
fn test_vulnerability_monitoring() {
    let monitor = VulnerabilityMonitor::new();
    monitor.register_vulnerability(
        Vulnerability::new(
            "CVE-2024-0001",
            "serde",
            VulnerabilitySeverity::High,
            "RCE vulnerability in serde",
        )
        .with_fixed_version("1.0.198"),
    );
    monitor.register_vulnerability(
        Vulnerability::new(
            "CVE-2024-0002",
            "tokio",
            VulnerabilitySeverity::Critical,
            "Critical DoS in tokio",
        )
        .with_fixed_version("1.38.0"),
    );

    assert_eq!(monitor.known_vuln_count(), 2, "应注册 2 个漏洞");

    let gen = SbomGenerator::new(SbomFormat::CycloneDX);
    let sbom = gen
        .generate_from_deps(&[("serde", "1.0.197"), ("tokio", "1.37.0")])
        .unwrap();

    let report = monitor.monitor(&sbom).unwrap();
    assert_eq!(report.vulnerabilities.len(), 2, "应检测到 2 个漏洞");
    assert_eq!(report.high_count, 1, "1 个 High");
    assert_eq!(report.critical_count, 1, "1 个 Critical");
    assert!(report.should_block_build, "高危漏洞应阻断构建");
    assert_eq!(monitor.scan_count(), 1, "扫描计数应为 1");
    assert_eq!(monitor.total_vulns_found(), 2, "累计发现 2 个漏洞");
}

/// T4.5 测试：无漏洞时不阻断构建
///
/// 验证行为副作用：无漏洞时 should_block_build = false。
#[test]
fn test_vulnerability_monitor_no_vulns() {
    let monitor = VulnerabilityMonitor::new();
    let gen = SbomGenerator::new(SbomFormat::CycloneDX);
    let sbom = gen.generate_from_deps(&[("serde", "1.0")]).unwrap();

    let report = monitor.monitor(&sbom).unwrap();
    assert_eq!(report.vulnerabilities.len(), 0, "无已知漏洞");
    assert!(!report.should_block_build, "无漏洞不应阻断构建");
}

/// T4.5 测试：已修复漏洞不报告
///
/// 验证行为副作用：组件版本 >= fixed_version 时不报告漏洞。
#[test]
fn test_vulnerability_fixed_version() {
    let monitor = VulnerabilityMonitor::new();
    monitor.register_vulnerability(
        Vulnerability::new(
            "CVE-2024-0003",
            "serde",
            VulnerabilitySeverity::Medium,
            "Fixed in 1.0.198",
        )
        .with_fixed_version("1.0.198"),
    );

    let gen = SbomGenerator::new(SbomFormat::CycloneDX);
    let sbom = gen.generate_from_deps(&[("serde", "1.0.200")]).unwrap();

    let report = monitor.monitor(&sbom).unwrap();
    assert_eq!(report.vulnerabilities.len(), 0, "已修复版本不应报告漏洞");
}

/// T4.5 测试：可重现构建验证
///
/// 验证行为副作用：哈希一致时 is_reproducible = true。
#[test]
fn test_reproducible_build_verify() {
    let verifier = ReproducibleBuildVerifier::new();
    let data = b"lock file content";
    let hash = ReproducibleBuildVerifier::compute_hash(data);
    verifier.set_expected_hash(&hash);

    let result = verifier.verify(&hash).unwrap();
    assert!(result.is_reproducible, "哈希一致应可重现");
    assert_eq!(result.expected_hash, result.actual_hash);
    assert_eq!(verifier.verify_count(), 1);
    assert_eq!(verifier.mismatch_count(), 0);
}

/// T4.5 测试：不可重现构建检测
///
/// 验证行为副作用：哈希不一致时 is_reproducible = false，mismatch_count 增加。
#[test]
fn test_reproducible_build_mismatch() {
    let verifier = ReproducibleBuildVerifier::new();
    verifier.set_expected_hash("expected_hash_123");
    let result = verifier.verify("different_hash_456").unwrap();

    assert!(!result.is_reproducible, "哈希不一致应不可重现");
    assert_eq!(verifier.mismatch_count(), 1, "不匹配计数应为 1");
}

/// T4.5 端到端测试：供应链安全全流程（SBOM→漏洞监控→可重现构建）
///
/// 验证行为副作用：SBOM 生成 → 漏洞监控 → 可重现构建验证全流程。
#[test]
fn e2e_sbom_generation_and_vulnerability_monitor() {
    // 1. SBOM 生成
    let gen = SbomGenerator::new(SbomFormat::CycloneDX);
    let sbom = gen
        .generate_from_deps(&[
            ("serde", "1.0.197"),
            ("tokio", "1.37.0"),
            ("sha2", "0.10.8"),
        ])
        .unwrap();
    assert_eq!(sbom.components.len(), 3);

    // 2. 漏洞监控
    let monitor = VulnerabilityMonitor::new();
    monitor.register_vulnerability(
        Vulnerability::new(
            "CVE-2024-9999",
            "tokio",
            VulnerabilitySeverity::Critical,
            "Critical vulnerability",
        )
        .with_fixed_version("1.38.0"),
    );

    let report = monitor.monitor(&sbom).unwrap();
    assert!(report.should_block_build, "Critical 漏洞应阻断构建");
    assert_eq!(report.critical_count, 1);

    // 3. 可重现构建验证
    let verifier = ReproducibleBuildVerifier::new();
    let lock_content = b"Cargo.lock content hash test";
    let lock_hash = ReproducibleBuildVerifier::compute_hash(lock_content);
    verifier.set_expected_hash(&lock_hash);

    let result = verifier.verify(&lock_hash).unwrap();
    assert!(result.is_reproducible, "构建应可重现");

    // 4. 篡改锁定文件后不可重现
    let tampered = verifier.verify("tampered_hash").unwrap();
    assert!(!tampered.is_reproducible, "篡改后应不可重现");
    assert_eq!(verifier.mismatch_count(), 1);
}

/// T4.5 端到端测试：供应链安全集成
///
/// 验证行为副作用：多组件 SBOM + 多漏洞监控 + 多次验证。
#[test]
fn test_supply_chain_security_integration() {
    let gen = SbomGenerator::new(SbomFormat::Spdx);
    let sbom = gen
        .generate_from_deps(&[
            ("serde", "1.0"),
            ("serde_json", "1.0"),
            ("tokio", "1.0"),
            ("sqlx", "0.7"),
        ])
        .unwrap();
    assert_eq!(sbom.format, SbomFormat::Spdx);
    assert_eq!(sbom.components.len(), 4);

    let monitor = VulnerabilityMonitor::new();
    monitor.register_vulnerability(Vulnerability::new(
        "CVE-2024-A",
        "sqlx",
        VulnerabilitySeverity::High,
        "SQL injection in sqlx",
    ));
    monitor.register_vulnerability(Vulnerability::new(
        "CVE-2024-B",
        "tokio",
        VulnerabilitySeverity::Medium,
        "DoS in tokio",
    ));

    let report = monitor.monitor(&sbom).unwrap();
    assert_eq!(report.vulnerabilities.len(), 2);
    assert_eq!(report.high_count, 1);
    assert_eq!(report.medium_count, 1);
    assert!(report.should_block_build, "High 漏洞应阻断构建");

    let verifier = ReproducibleBuildVerifier::new();
    let hash1 = ReproducibleBuildVerifier::compute_hash(b"v1");
    let hash2 = ReproducibleBuildVerifier::compute_hash(b"v2");
    assert_ne!(hash1, hash2, "不同内容应产生不同哈希");

    verifier.set_expected_hash(&hash1);
    assert!(verifier.verify(&hash1).unwrap().is_reproducible);
    assert!(!verifier.verify(&hash2).unwrap().is_reproducible);
    assert_eq!(verifier.verify_count(), 2);
}
