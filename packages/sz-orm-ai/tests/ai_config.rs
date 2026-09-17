//! 任务 3.1 集成测试：AiConfig 配置聚合与 AiDecision 决策结果聚合
//!
//! 从外部 crate 视角验证公共 API 行为：
//! - 默认值全 false（不改变 v7.2.0 既有行为）
//! - 校验失败场景
//! - Send + Sync 约束
//! - 序列化/反序列化

use sz_orm_ai::{AiConfig, AiDecision, AiDecisionType, AiAnnIndexType, RewritePath};

/// 验证默认配置所有开关为 false（不改变 v7.2.0 既有行为）
#[test]
fn test_default_config_all_disabled() {
    let config = AiConfig::default();
    assert!(!config.rewrite_enabled, "rewrite_enabled 默认应为 false");
    assert!(
        !config.index_advisor_enabled,
        "index_advisor_enabled 默认应为 false"
    );
    assert!(!config.nl2sql_enabled, "nl2sql_enabled 默认应为 false");
    assert!(
        !config.nl2sql_multi_turn,
        "nl2sql_multi_turn 默认应为 false"
    );
    assert_eq!(config.rewrite_path, RewritePath::Rule);
    assert_eq!(config.vector_ann_index, AiAnnIndexType::Hnsw);
    assert!(!config.any_enabled(), "默认配置不应启用任一 AI 能力");
}

/// 验证校验失败：召回率阈值越界
#[test]
fn test_validate_fail_recall_threshold_out_of_range() {
    let config = AiConfig {
        vector_recall_threshold: -0.1,
        ..AiConfig::default()
    };
    assert!(config.validate().is_err());

    let config = AiConfig {
        vector_recall_threshold: 1.1,
        ..AiConfig::default()
    };
    assert!(config.validate().is_err());
}

/// 验证校验失败：多轮对话启用但 NL2SQL 未启用
#[test]
fn test_validate_fail_multi_turn_without_nl2sql() {
    let config = AiConfig {
        nl2sql_enabled: false,
        nl2sql_multi_turn: true,
        ..AiConfig::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.contains("nl2sql_multi_turn"));
}

/// 验证校验成功：全启用且参数合法
#[test]
fn test_validate_success_all_enabled() {
    let config = AiConfig {
        rewrite_enabled: true,
        rewrite_path: RewritePath::Hybrid,
        index_advisor_enabled: true,
        nl2sql_enabled: true,
        nl2sql_multi_turn: true,
        vector_ann_index: AiAnnIndexType::Quantized,
        vector_recall_threshold: 0.95,
    };
    assert!(config.validate().is_ok());
    assert!(config.any_enabled());
}

/// 验证 Send + Sync 约束（跨线程传递与共享引用）
#[test]
fn test_send_sync_bounds() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<AiConfig>();
    assert_sync::<AiConfig>();
    assert_send::<AiDecision>();
    assert_sync::<AiDecision>();
    assert_send::<RewritePath>();
    assert_sync::<RewritePath>();
    assert_send::<AiAnnIndexType>();
    assert_sync::<AiAnnIndexType>();
    assert_send::<AiDecisionType>();
    assert_sync::<AiDecisionType>();
}

/// 验证序列化/反序列化往返
#[test]
fn test_serde_roundtrip() {
    let config = AiConfig {
        rewrite_enabled: true,
        rewrite_path: RewritePath::Llm,
        index_advisor_enabled: false,
        nl2sql_enabled: true,
        nl2sql_multi_turn: false,
        vector_ann_index: AiAnnIndexType::Ivf,
        vector_recall_threshold: 0.85,
    };
    let json = serde_json::to_string(&config).unwrap();
    let de: AiConfig = serde_json::from_str(&json).unwrap();
    assert!(de.rewrite_enabled);
    assert_eq!(de.rewrite_path, RewritePath::Llm);
    assert_eq!(de.vector_ann_index, AiAnnIndexType::Ivf);
    assert!((de.vector_recall_threshold - 0.85).abs() < 1e-9);
}

/// 验证 AiDecision 构造与字段
#[test]
fn test_ai_decision_construction() {
    let decision = AiDecision::new(
        AiDecisionType::VectorSearch,
        "查询向量 [1.0, 0.0, 0.0]",
        "top-10 结果，召回率 0.92",
        "HNSW 索引",
        0.92,
        5,
    );
    assert_eq!(decision.decision_type, AiDecisionType::VectorSearch);
    assert_eq!(decision.decision_type.as_str(), "VectorSearch");
    assert!((decision.confidence - 0.92).abs() < 1e-9);
    assert_eq!(decision.latency_ms, 5);
}

/// 验证枚举 as_str 全覆盖
#[test]
fn test_enum_as_str_coverage() {
    assert_eq!(RewritePath::Rule.as_str(), "Rule");
    assert_eq!(RewritePath::Llm.as_str(), "Llm");
    assert_eq!(RewritePath::Hybrid.as_str(), "Hybrid");

    assert_eq!(AiAnnIndexType::Hnsw.as_str(), "HNSW");
    assert_eq!(AiAnnIndexType::Ivf.as_str(), "IVF");
    assert_eq!(AiAnnIndexType::Quantized.as_str(), "Quantized");

    assert_eq!(AiDecisionType::Rewrite.as_str(), "Rewrite");
    assert_eq!(AiDecisionType::IndexRecommend.as_str(), "IndexRecommend");
    assert_eq!(AiDecisionType::Nl2sql.as_str(), "Nl2sql");
    assert_eq!(AiDecisionType::VectorSearch.as_str(), "VectorSearch");
}