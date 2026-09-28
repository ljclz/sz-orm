//! v9.2.0 M6-T35：PoolConfigBuilder/build/validate + 非法配置（4 tests）

use std::time::Duration;
use sz_orm_core::{PoolConfig, PoolConfigBuilder, PoolError, TlsConfig, TlsVersion};

#[test]
fn test_pool_config_builder_build_valid() {
    let config = PoolConfigBuilder::new()
        .max_size(20)
        .min_idle(5)
        .acquire_timeout(10)
        .idle_timeout(120)
        .max_lifetime(1800)
        .build()
        .unwrap();
    assert_eq!(config.max_size, 20);
    assert_eq!(config.min_idle, 5);
    assert_eq!(config.acquire_timeout, Duration::from_secs(10));
    assert_eq!(config.idle_timeout, Duration::from_secs(120));
    assert_eq!(config.max_lifetime, Duration::from_secs(1800));
}

#[test]
fn test_pool_config_builder_default_matches_pool_config_default() {
    let built = PoolConfigBuilder::new().build().unwrap();
    let default = PoolConfig::default();
    assert_eq!(built.max_size, default.max_size);
    assert_eq!(built.min_idle, default.min_idle);
    assert_eq!(built.acquire_timeout, default.acquire_timeout);
}

#[test]
fn test_pool_config_validate_rejects_zero_max_size() {
    let config = PoolConfig {
        max_size: 0,
        ..PoolConfig::default()
    };
    let err = config.validate().unwrap_err();
    assert!(
        matches!(err, PoolError::InvalidConfig(ref s) if s.contains("max_size")),
        "应拒绝 max_size=0，实际: {:?}",
        err
    );
}

#[test]
fn test_pool_config_validate_rejects_min_idle_exceeds_max() {
    let config = PoolConfig {
        max_size: 5,
        min_idle: 10,
        ..PoolConfig::default()
    };
    let err = config.validate().unwrap_err();
    assert!(
        matches!(err, PoolError::InvalidConfig(ref s) if s.contains("min_idle")),
        "应拒绝 min_idle > max_size，实际: {:?}",
        err
    );
    let _ = TlsConfig {
        enabled: true,
        ca_cert_path: None,
        client_cert_path: None,
        client_key_path: None,
        min_version: TlsVersion::Tls12,
    };
}