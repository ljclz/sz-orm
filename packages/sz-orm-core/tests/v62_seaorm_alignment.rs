//! v6.2 对标验证：池配置 + 池指标 + streaming 模块可用性

#[test]
fn test_pool_config_available() {
    use sz_orm_core::PoolConfig;
    let config = PoolConfig::default();
    assert!(config.max_size > 0, "池配置应有最大连接数");
}

#[test]
fn test_pool_metrics_available() {
    use sz_orm_core::PoolMetrics;
    let metrics = PoolMetrics {
        acquire_count: 100,
        acquire_failed_count: 5,
        acquire_wait_time: std::time::Duration::from_millis(200),
        release_count: 95,
        connection_created_count: 10,
        connection_closed_count: 3,
    };
    assert_eq!(metrics.acquire_count, 100);
    let avg = metrics.average_acquire_wait_time();
    assert!(avg > std::time::Duration::ZERO, "平均等待时间应大于 0");
}

#[test]
fn test_pool_status_available() {
    use sz_orm_core::PoolStatus;
    let _ = std::marker::PhantomData::<PoolStatus>;
}
