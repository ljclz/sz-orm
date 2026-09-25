//! M3: shadow.rs 补充测试 — 覆盖 ShadowComparison/ShadowStats/ShadowConfig/ShadowConnection

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::time::Duration;

use sz_orm_core::shadow::{
    MismatchAction, ShadowComparison, ShadowConfig, ShadowConnection, ShadowStats,
};
use sz_orm_core::{Connection, DbError, Value};

// === Mock Connection ===

struct MockConnection {
    rows: Vec<HashMap<String, Value>>,
    connected: bool,
}

impl MockConnection {
    fn new(rows: Vec<HashMap<String, Value>>) -> Self {
        Self {
            rows,
            connected: true,
        }
    }

    fn empty() -> Self {
        Self {
            rows: vec![],
            connected: true,
        }
    }
}

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async { Ok(1) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<HashMap<String, Value>>, DbError>>
                + Send
                + 'a,
        >,
    > {
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), DbError>> + Send + 'a>> {
        self.connected = false;
        Box::pin(async { Ok(()) })
    }
}

fn make_row(id: i64, name: &str) -> HashMap<String, Value> {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(id));
    row.insert("name".to_string(), Value::String(name.to_string()));
    row
}

// === ShadowComparison::latency_ratio ===

#[test]
fn test_shadow_comparison_latency_ratio_normal() {
    let c = ShadowComparison {
        sql: "SELECT 1".to_string(),
        orm_duration: Duration::from_millis(20),
        raw_duration: Duration::from_millis(10),
        orm_rows: 1,
        raw_rows: 1,
        consistent: true,
        mismatch: None,
    };
    assert!((c.latency_ratio() - 2.0).abs() < 1e-9);
}

#[test]
fn test_shadow_comparison_latency_ratio_zero_raw() {
    let c = ShadowComparison {
        sql: "SELECT 1".to_string(),
        orm_duration: Duration::from_millis(10),
        raw_duration: Duration::ZERO,
        orm_rows: 1,
        raw_rows: 1,
        consistent: true,
        mismatch: None,
    };
    assert_eq!(c.latency_ratio(), 1.0);
}

#[test]
fn test_shadow_comparison_latency_ratio_equal() {
    let c = ShadowComparison {
        sql: "SELECT 1".to_string(),
        orm_duration: Duration::from_millis(50),
        raw_duration: Duration::from_millis(50),
        orm_rows: 1,
        raw_rows: 1,
        consistent: true,
        mismatch: None,
    };
    assert!((c.latency_ratio() - 1.0).abs() < 1e-9);
}

// === ShadowStats ===

#[test]
fn test_shadow_stats_avg_orm_us_empty() {
    let stats = ShadowStats::default();
    assert_eq!(stats.avg_orm_us(), 0);
}

#[test]
fn test_shadow_stats_avg_raw_us_empty() {
    let stats = ShadowStats::default();
    assert_eq!(stats.avg_raw_us(), 0);
}

#[test]
fn test_shadow_stats_avg_orm_us_with_data() {
    let stats = ShadowStats::default();
    stats.comparisons.store(4, Ordering::Relaxed);
    stats.orm_total_us.store(400, Ordering::Relaxed);
    assert_eq!(stats.avg_orm_us(), 100);
}

#[test]
fn test_shadow_stats_avg_raw_us_with_data() {
    let stats = ShadowStats::default();
    stats.comparisons.store(5, Ordering::Relaxed);
    stats.raw_total_us.store(250, Ordering::Relaxed);
    assert_eq!(stats.avg_raw_us(), 50);
}

#[test]
fn test_shadow_stats_mismatch_rate_zero() {
    let stats = ShadowStats::default();
    assert_eq!(stats.mismatch_rate(), 0.0);
}

#[test]
fn test_shadow_stats_mismatch_rate_full() {
    let stats = ShadowStats::default();
    stats.comparisons.store(10, Ordering::Relaxed);
    stats.mismatches.store(10, Ordering::Relaxed);
    assert!((stats.mismatch_rate() - 1.0).abs() < 1e-9);
}

// === ShadowConfig ===

#[test]
fn test_shadow_config_default() {
    let config = ShadowConfig::default();
    assert_eq!(config.timeout, Duration::from_secs(3));
    assert!(!config.row_count_only);
    assert_eq!(config.max_compare_rows, 10_000);
}

#[test]
fn test_shadow_config_custom() {
    let config = ShadowConfig {
        timeout: Duration::from_secs(5),
        row_count_only: true,
        max_compare_rows: 100,
    };
    assert_eq!(config.timeout, Duration::from_secs(5));
    assert!(config.row_count_only);
    assert_eq!(config.max_compare_rows, 100);
}

// === MismatchAction ===

#[test]
fn test_mismatch_action_equality() {
    assert_eq!(MismatchAction::Record, MismatchAction::Record);
    assert_eq!(MismatchAction::Error, MismatchAction::Error);
    assert_eq!(MismatchAction::Panic, MismatchAction::Panic);
    assert_ne!(MismatchAction::Record, MismatchAction::Error);
}

// === ShadowConnection ===

#[tokio::test]
async fn test_shadow_connection_new() {
    let orm = MockConnection::new(vec![make_row(1, "Alice")]);
    let raw = MockConnection::new(vec![make_row(1, "Alice")]);
    let shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());
    assert_eq!(shadow.stats().comparisons.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_shadow_connection_with_mismatch_action() {
    let orm = MockConnection::empty();
    let raw = MockConnection::empty();
    let shadow = ShadowConnection::new(orm, raw, ShadowConfig::default())
        .with_mismatch_action(MismatchAction::Error);
    assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_shadow_query_consistent() {
    let rows = vec![make_row(1, "Alice"), make_row(2, "Bob")];
    let orm = MockConnection::new(rows.clone());
    let raw = MockConnection::new(rows);
    let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());

    let result = shadow.query_shadow("SELECT * FROM users").await.unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(shadow.stats().comparisons.load(Ordering::Relaxed), 1);
    assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_shadow_query_row_count_mismatch() {
    let orm = MockConnection::new(vec![make_row(1, "Alice")]);
    let raw = MockConnection::new(vec![make_row(1, "Alice"), make_row(2, "Bob")]);
    let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());

    let result = shadow.query_shadow("SELECT * FROM users").await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(shadow.stats().comparisons.load(Ordering::Relaxed), 1);
    assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_shadow_query_value_mismatch() {
    let orm = MockConnection::new(vec![make_row(1, "Alice")]);
    let raw = MockConnection::new(vec![make_row(1, "Bob")]);
    let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());

    let result = shadow.query_shadow("SELECT * FROM users").await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_shadow_query_row_count_only_mode() {
    let orm = MockConnection::new(vec![make_row(1, "Alice")]);
    let raw = MockConnection::new(vec![make_row(1, "Bob")]);
    let config = ShadowConfig {
        row_count_only: true,
        ..ShadowConfig::default()
    };
    let mut shadow = ShadowConnection::new(orm, raw, config);

    shadow.query_shadow("SELECT * FROM users").await.unwrap();
    assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_shadow_query_panic_mode_returns_error() {
    let orm = MockConnection::new(vec![make_row(1, "Alice")]);
    let raw = MockConnection::new(vec![make_row(1, "Bob")]);
    let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default())
        .with_mismatch_action(MismatchAction::Panic);

    let result = shadow.query_shadow("SELECT * FROM users").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_shadow_query_empty_results_consistent() {
    let orm = MockConnection::empty();
    let raw = MockConnection::empty();
    let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());

    let result = shadow
        .query_shadow("SELECT * FROM empty_table")
        .await
        .unwrap();
    assert_eq!(result.len(), 0);
    assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_shadow_query_multiple_calls_accumulate_stats() {
    let rows = vec![make_row(1, "Alice")];
    let orm = MockConnection::new(rows.clone());
    let raw = MockConnection::new(rows);
    let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());

    shadow.query_shadow("SELECT 1").await.unwrap();
    shadow.query_shadow("SELECT 2").await.unwrap();
    shadow.query_shadow("SELECT 3").await.unwrap();

    assert_eq!(shadow.stats().comparisons.load(Ordering::Relaxed), 3);
    assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_shadow_stats_after_mismatch() {
    let orm = MockConnection::new(vec![make_row(1, "Alice")]);
    let raw = MockConnection::new(vec![make_row(1, "Bob")]);
    let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());

    shadow.query_shadow("SELECT * FROM users").await.unwrap();

    let stats = shadow.stats();
    assert_eq!(stats.comparisons.load(Ordering::Relaxed), 1);
    assert_eq!(stats.mismatches.load(Ordering::Relaxed), 1);
    assert!((stats.mismatch_rate() - 1.0).abs() < 1e-9);
}
