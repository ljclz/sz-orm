//! v10.3.0 自动接线零配置便利 e2e 测试（5 个）
//!
//! 验证 `default_auto_wire_factory()` 便利函数的生产接线行为。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;

use sz_orm_core::auto_wire::{AutoWireConfig, AutoWireFactory};
use sz_orm_core::pool::{Connection, ConnectionFactory, Pool, PoolConfigBuilder, QueryRows};
use sz_orm_core::DbError;

use sz_orm_observability::default_auto_wire_factory;
use sz_orm_observability::ObservabilityAutoWireFactory;

struct MockConnection;

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(1) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(Vec::new()) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

struct MockFac;

#[async_trait]
impl ConnectionFactory for MockFac {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConnection))
    }
}

#[test]
fn test_default_auto_wire_factory_returns_preconfigured_e2e() {
    let factory = default_auto_wire_factory();
    let callback = factory.create_event_callback(&Default::default());
    assert!(callback.is_some(), "便利函数返回的工厂必须能创建回调");
}

#[test]
fn test_default_auto_wire_factory_one_line_enable_e2e() {
    let mut config = PoolConfigBuilder::new()
        .auto_wire_with(Arc::new(default_auto_wire_factory()))
        .auto_wire(AutoWireConfig {
            auto_wire_enabled: true,
            ..Default::default()
        })
        .build()
        .unwrap();
    config.observability_loop.enabled = true;
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let status = pool.auto_wire_status();
    assert!(
        !status.wired_components.is_empty(),
        "一行代码启用自动接线后应有已接线组件"
    );
}

#[test]
fn test_default_auto_wire_factory_equivalent_to_manual_e2e() {
    let mut convenience_config = PoolConfigBuilder::new()
        .auto_wire_with(Arc::new(default_auto_wire_factory()))
        .auto_wire(AutoWireConfig {
            auto_wire_enabled: true,
            ..Default::default()
        })
        .build()
        .unwrap();
    convenience_config.observability_loop.enabled = true;
    let mut manual_config = PoolConfigBuilder::new()
        .auto_wire_with(Arc::new(ObservabilityAutoWireFactory::new(
            Default::default(),
        )))
        .auto_wire(AutoWireConfig {
            auto_wire_enabled: true,
            ..Default::default()
        })
        .build()
        .unwrap();
    manual_config.observability_loop.enabled = true;
    let pool1 = Pool::new(convenience_config, Arc::new(MockFac)).unwrap();
    let pool2 = Pool::new(manual_config, Arc::new(MockFac)).unwrap();
    let status1 = pool1.auto_wire_status();
    let status2 = pool2.auto_wire_status();
    assert_eq!(status1.wired_components, status2.wired_components);
}

#[test]
fn test_default_auto_wire_factory_exported_in_lib_e2e() {
    let _factory = sz_orm_observability::default_auto_wire_factory();
    let _callback = _factory.create_event_callback(&Default::default());
    assert!(_callback.is_some());
}

#[test]
fn test_core_does_not_depend_on_observability_e2e() {
    let core_cargo_toml = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sz-orm-core")
        .join("Cargo.toml");
    let content = std::fs::read_to_string(&core_cargo_toml).unwrap();
    assert!(
        !content.contains("sz-orm-observability"),
        "sz-orm-core 不能依赖 sz-orm-observability（避免循环依赖）"
    );
}
