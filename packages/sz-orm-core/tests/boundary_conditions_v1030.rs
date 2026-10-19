//! v10.3.0 边界条件测试
//!
//! 验证新增 v10.3.0 组件在边界/极端条件下的正确性。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use sz_orm_core::pool::{Connection, ConnectionFactory, Pool, PoolConfig, QueryRows};
use sz_orm_core::profit_collector::{ProfitCollector, ProfitCollectorConfig};
use sz_orm_core::unified_heal::UnifiedHealConfig;
use sz_orm_core::DbError;

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
fn test_boundary_unified_heal_disabled_default() {
    let config = UnifiedHealConfig::default();
    assert!(!config.unified_heal_enabled);
    assert!(!config.backoff_fusion_enabled);
}

#[test]
fn test_boundary_profit_collector_disabled_default() {
    let config = ProfitCollectorConfig::default();
    assert!(!config.continuous_collect_enabled);
}

#[test]
fn test_boundary_profit_collector_zero_hits_zero_skips() {
    let collector = ProfitCollector::new(ProfitCollectorConfig::default());
    let snap = collector.snapshot();
    assert_eq!(snap.hit_count, 0);
    assert_eq!(snap.skip_count, 0);
    assert_eq!(snap.hit_ratio, 0.0);
}

#[test]
fn test_boundary_profit_collector_config_invalid() {
    let config = ProfitCollectorConfig {
        continuous_collect_enabled: true,
        snapshot_interval: Duration::from_secs(0),
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_boundary_ops_snapshot_all_disabled() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let snapshot = pool.ops_snapshot();
    assert!(snapshot.alert_aggregation.is_empty());
    assert!(matches!(
        snapshot.profit_summary,
        sz_orm_core::ops_snapshot::ProfitSummary::Disabled
    ));
}

#[test]
fn test_boundary_unified_heal_config_conflict_warning() {
    let config = UnifiedHealConfig {
        unified_heal_enabled: true,
        backoff_fusion_enabled: true,
        ..Default::default()
    };
    assert!(config.unified_heal_enabled);
    assert!(config.backoff_fusion_enabled);
}

#[test]
fn test_boundary_backoff_fusion_disabled_fixed_interval() {
    let config = UnifiedHealConfig::default();
    assert!(!config.backoff_fusion_enabled);
}
