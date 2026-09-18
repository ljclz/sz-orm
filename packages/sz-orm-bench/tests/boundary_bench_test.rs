//! v7.4.0 任务 4.1：BenchConfig/DbBackend 边界测试

use sz_orm_bench::{BenchConfig, DbBackend};

#[test]
fn test_bench_config_default_values() {
    let config = BenchConfig::default();
    assert!(config.measure_rounds >= 1);
}

#[test]
fn test_bench_config_validate_min_rounds() {
    let mut config = BenchConfig::default();
    config.measure_rounds = 3;
    assert!(config.validate().is_err(), "measure_rounds < 5 应校验失败");
}

#[test]
fn test_bench_config_validate_valid() {
    let mut config = BenchConfig::default();
    config.measure_rounds = 10;
    assert!(config.validate().is_ok());
}

#[test]
fn test_db_backend_sqlite_display() {
    let backend = DbBackend::Sqlite;
    let s = format!("{:?}", backend);
    assert!(s.contains("Sqlite"));
}

#[test]
fn test_db_backend_mysql_display() {
    let backend = DbBackend::Mysql;
    let s = format!("{:?}", backend);
    assert!(s.contains("Mysql"));
}

#[test]
fn test_db_backend_postgres_display() {
    let backend = DbBackend::Postgres;
    let s = format!("{:?}", backend);
    assert!(s.contains("Postgres"));
}

#[test]
fn test_bench_config_zero_rounds() {
    let mut config = BenchConfig::default();
    config.measure_rounds = 0;
    assert!(config.validate().is_err());
}

#[test]
fn test_bench_config_max_rounds() {
    let mut config = BenchConfig::default();
    config.measure_rounds = 10000;
    assert!(config.validate().is_ok());
}
