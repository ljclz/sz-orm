//! DistributedDbCompatReport 分布式 DB 兼容性报告测试（v7.5.0 组6.3）
//!
//! 验证兼容性报告的生成、SQL 兼容性结果、事务行为结果、基准对比等功能。

use sz_orm_sqlx::{
    BenchmarkComparison, DistributedDbCompatReport, DistributedDbType, SqlCompatResult,
    TxBehaviorResult,
};

#[test]
fn test_distributed_db_compat_report_creation() {
    let report = DistributedDbCompatReport::new(DistributedDbType::CockroachDb);
    assert_eq!(report.db_type, DistributedDbType::CockroachDb);
    assert!(report.sql_compat_results.is_empty());
    assert!(report.distributed_tx_behavior.is_empty());
    assert!(report.benchmark_vs_pg.is_empty());
}

#[test]
fn test_sql_compat_result_compatible() {
    let result = SqlCompatResult {
        sql: "SELECT * FROM users WHERE id = $1".to_string(),
        is_compatible: true,
        reason: None,
        alternative_sql: None,
    };
    assert!(result.is_compatible);
    assert!(result.reason.is_none());
    assert!(result.alternative_sql.is_none());
}

#[test]
fn test_sql_compat_result_incompatible() {
    let result = SqlCompatResult {
        sql: "SELECT * FROM users LIMIT 10 OFFSET 5".to_string(),
        is_compatible: false,
        reason: Some("CockroachDB 不支持 OFFSET without LIMIT in certain contexts".to_string()),
        alternative_sql: Some("SELECT * FROM users LIMIT 10 OFFSET 5".to_string()),
    };
    assert!(!result.is_compatible);
    assert!(result.reason.is_some());
    assert!(result.alternative_sql.is_some());
}

#[test]
fn test_tx_behavior_result() {
    let result = TxBehaviorResult {
        isolation_level: "SERIALIZABLE".to_string(),
        timeout: "30s".to_string(),
        retry_behavior: "automatic retry up to 5 times".to_string(),
        consistency_with_pg: true,
    };
    assert_eq!(result.isolation_level, "SERIALIZABLE");
    assert!(result.consistency_with_pg);
}

#[test]
fn test_benchmark_comparison() {
    let comparison = BenchmarkComparison {
        pg_baseline: 10000.0,
        distributed_result: 8500.0,
        throughput_ratio: 0.85,
        latency_ratio: 1.18,
    };
    assert!((comparison.throughput_ratio - 0.85).abs() < 1e-6);
    assert!((comparison.latency_ratio - 1.18).abs() < 1e-6);
}

#[test]
fn test_report_add_sql_compat() {
    let mut report = DistributedDbCompatReport::new(DistributedDbType::YugabyteDb);
    report.add_sql_compat(SqlCompatResult {
        sql: "SELECT 1".to_string(),
        is_compatible: true,
        reason: None,
        alternative_sql: None,
    });
    report.add_sql_compat(SqlCompatResult {
        sql: "SELECT * FROM t LIMIT 10".to_string(),
        is_compatible: true,
        reason: None,
        alternative_sql: None,
    });
    report.add_sql_compat(SqlCompatResult {
        sql: "SELECT * FROM t OFFSET 5".to_string(),
        is_compatible: false,
        reason: Some("not supported".to_string()),
        alternative_sql: Some("SELECT * FROM t LIMIT 1000000 OFFSET 5".to_string()),
    });
    assert_eq!(report.sql_compat_results.len(), 3);
    let rate = report.overall_sql_compat_rate();
    assert!((rate - 2.0 / 3.0).abs() < 1e-6);
}

#[test]
fn test_report_add_tx_behavior() {
    let mut report = DistributedDbCompatReport::new(DistributedDbType::CockroachDb);
    report.add_tx_behavior(TxBehaviorResult {
        isolation_level: "SERIALIZABLE".to_string(),
        timeout: "30s".to_string(),
        retry_behavior: "auto retry".to_string(),
        consistency_with_pg: true,
    });
    report.add_tx_behavior(TxBehaviorResult {
        isolation_level: "READ COMMITTED".to_string(),
        timeout: "30s".to_string(),
        retry_behavior: "no retry".to_string(),
        consistency_with_pg: false,
    });
    assert_eq!(report.distributed_tx_behavior.len(), 2);
    assert!(report.distributed_tx_behavior[0].consistency_with_pg);
    assert!(!report.distributed_tx_behavior[1].consistency_with_pg);
}

#[test]
fn test_report_add_benchmark() {
    let mut report = DistributedDbCompatReport::new(DistributedDbType::PostgreSql);
    report.add_benchmark(BenchmarkComparison {
        pg_baseline: 10000.0,
        distributed_result: 10000.0,
        throughput_ratio: 1.0,
        latency_ratio: 1.0,
    });
    assert_eq!(report.benchmark_vs_pg.len(), 1);
    assert!((report.benchmark_vs_pg[0].throughput_ratio - 1.0).abs() < 1e-6);
}

#[test]
fn test_report_to_json() {
    let mut report = DistributedDbCompatReport::new(DistributedDbType::CockroachDb);
    report.add_sql_compat(SqlCompatResult {
        sql: "SELECT 1".to_string(),
        is_compatible: true,
        reason: None,
        alternative_sql: None,
    });
    let json = report.to_json().unwrap();
    assert!(json.contains("CockroachDb"));
    assert!(json.contains("SELECT 1"));
    assert!(json.contains("is_compatible"));
    assert!(json.contains("true"));
}

#[test]
fn test_overall_sql_compat_rate_empty() {
    let report = DistributedDbCompatReport::new(DistributedDbType::YugabyteDb);
    assert_eq!(report.overall_sql_compat_rate(), 0.0);
}

#[test]
fn test_all_db_types() {
    let crdb = DistributedDbCompatReport::new(DistributedDbType::CockroachDb);
    let yb = DistributedDbCompatReport::new(DistributedDbType::YugabyteDb);
    let pg = DistributedDbCompatReport::new(DistributedDbType::PostgreSql);
    assert_eq!(crdb.db_type, DistributedDbType::CockroachDb);
    assert_eq!(yb.db_type, DistributedDbType::YugabyteDb);
    assert_eq!(pg.db_type, DistributedDbType::PostgreSql);
    assert_ne!(crdb.db_type, yb.db_type);
    assert_ne!(yb.db_type, pg.db_type);
}
