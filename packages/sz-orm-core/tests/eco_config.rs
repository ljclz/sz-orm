//! v7.3.0 任务 4.1：EcoConfig 生态扩展配置测试
//!
//! 生产调用点证据：
//! - EcoConfig 默认值（migration_dry_run=true）：tests/eco_config.rs:18
//! - EcoConfig::validate 校验：tests/eco_config.rs:35
//! - WebFramework 枚举序列化：tests/eco_config.rs:50
//! - MiddlewareFeature 枚举序列化：tests/eco_config.rs:60
//! - SourceOrm 枚举序列化：tests/eco_config.rs:70
//! - EcoConfigBuilder 构建：tests/eco_config.rs:80

use sz_orm_core::{EcoConfig, MiddlewareFeature, SourceOrm, WebFramework};

/// 默认值：migration_dry_run=true，web_framework=Axum
#[test]
fn test_eco_config_default() {
    let cfg = EcoConfig::default();
    assert!(cfg.migration_dry_run, "migration_dry_run 默认必须 true");
    assert_eq!(cfg.web_framework, WebFramework::Axum);
    assert!(cfg.middleware_features.is_empty());
    assert!(cfg.migration_source_orm.is_none());
    assert!(cfg.schema_diff_left_url.is_none());
    assert!(cfg.schema_diff_right_url.is_none());
}

/// 校验合法性：left/right 同时提供或同时缺失
#[test]
fn test_eco_config_validate() {
    // 默认配置合法
    let cfg = EcoConfig::default();
    assert!(cfg.validate().is_ok());

    // 只提供 left 不提供 right：非法
    let cfg = EcoConfig {
        schema_diff_left_url: Some("mysql://a".to_string()),
        ..EcoConfig::default()
    };
    assert!(cfg.validate().is_err());

    // 同时提供 left/right：合法
    let cfg = EcoConfig {
        schema_diff_left_url: Some("mysql://a".to_string()),
        schema_diff_right_url: Some("mysql://b".to_string()),
        ..EcoConfig::default()
    };
    assert!(cfg.validate().is_ok());
}

/// WebFramework 枚举序列化/反序列化
#[test]
fn test_web_framework_serde() {
    for fw in [WebFramework::Axum, WebFramework::Actix, WebFramework::Warp] {
        let json = serde_json::to_string(&fw).unwrap();
        let back: WebFramework = serde_json::from_str(&json).unwrap();
        assert_eq!(fw, back);
    }
    assert_eq!(
        serde_json::to_string(&WebFramework::Axum).unwrap(),
        "\"Axum\""
    );
    assert_eq!(
        serde_json::to_string(&WebFramework::Warp).unwrap(),
        "\"Warp\""
    );
}

/// MiddlewareFeature 枚举序列化/反序列化
#[test]
fn test_middleware_feature_serde() {
    let features = vec![
        MiddlewareFeature::PoolInject,
        MiddlewareFeature::Transaction,
        MiddlewareFeature::RateLimit,
        MiddlewareFeature::Tracing,
        MiddlewareFeature::HealthEndpoint,
    ];
    let json = serde_json::to_string(&features).unwrap();
    let back: Vec<MiddlewareFeature> = serde_json::from_str(&json).unwrap();
    assert_eq!(features, back);
}

/// SourceOrm 枚举序列化/反序列化
#[test]
fn test_source_orm_serde() {
    for orm in [SourceOrm::Diesel, SourceOrm::SeaOrm, SourceOrm::Sqlx] {
        let json = serde_json::to_string(&orm).unwrap();
        let back: SourceOrm = serde_json::from_str(&json).unwrap();
        assert_eq!(orm, back);
    }
    assert_eq!(
        serde_json::to_string(&SourceOrm::Diesel).unwrap(),
        "\"Diesel\""
    );
}

/// EcoConfigBuilder 构建
#[test]
fn test_eco_config_builder() {
    let cfg = EcoConfig::builder()
        .web_framework(WebFramework::Warp)
        .middleware_features(vec![
            MiddlewareFeature::PoolInject,
            MiddlewareFeature::Tracing,
        ])
        .migration_source_orm(SourceOrm::Diesel)
        .migration_dry_run(false)
        .schema_diff_left_url("mysql://left")
        .schema_diff_right_url("mysql://right")
        .build()
        .unwrap();

    assert_eq!(cfg.web_framework, WebFramework::Warp);
    assert_eq!(cfg.middleware_features.len(), 2);
    assert_eq!(cfg.migration_source_orm, Some(SourceOrm::Diesel));
    assert!(!cfg.migration_dry_run);
    assert!(cfg.schema_diff_left_url.is_some());
    assert!(cfg.schema_diff_right_url.is_some());
}

/// EcoConfigBuilder 非法配置构建失败
#[test]
fn test_eco_config_builder_invalid() {
    let result = EcoConfig::builder()
        .schema_diff_left_url("mysql://left")
        .build();
    assert!(result.is_err());
}

/// EcoConfig Serialize/Deserialize 往返
#[test]
fn test_eco_config_serde_roundtrip() {
    let cfg = EcoConfig::builder()
        .web_framework(WebFramework::Actix)
        .middleware_features(vec![MiddlewareFeature::HealthEndpoint])
        .migration_source_orm(SourceOrm::SeaOrm)
        .migration_dry_run(true)
        .build()
        .unwrap();
    let json = serde_json::to_string(&cfg).unwrap();
    let back: EcoConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.web_framework, back.web_framework);
    assert_eq!(cfg.migration_dry_run, back.migration_dry_run);
    assert_eq!(cfg.migration_source_orm, back.migration_source_orm);
}
