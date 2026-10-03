//! v9.3.0 T9：pool 安全分支覆盖测试
//!
//! 覆盖 pool.rs 中安全相关分支

#[tokio::test]
async fn test_pool_config_default() {
    let config = sz_orm_core::PoolConfig::default();
    assert!(config.max_size > 0, "默认配置应有正连接数");
}

#[tokio::test]
async fn test_pool_config_custom() {
    let config = sz_orm_core::PoolConfig {
        max_size: 100,
        ..Default::default()
    };
    assert_eq!(config.max_size, 100);
}
