//! BatchSizeAdvisor 边界测试（v8.8.0）
//!
//! 覆盖 BatchSizeAdvisor 的边界/极端场景：
//! - 0 列、0 行、usize::MAX 行
//! - param_limit 边界值（MySQL 65535/65536、PostgreSQL 32767/32768、Oracle 1000/1001、SQLite 999/1000、SQLServer 2100/2101）
//! - 约束验证：batch_size * columns <= param_limit、batch_size <= total_rows

use sz_orm_core::batch_advisor::BatchSizeAdvisor;
use sz_orm_core::DbType;

// ── 零值边界 ───────────────────────────────────────────────────────

#[test]
fn zero_columns_returns_one() {
    for db in [
        DbType::MySQL,
        DbType::PostgreSQL,
        DbType::Oracle,
        DbType::Sqlite,
        DbType::SqlServer,
    ] {
        let batch = BatchSizeAdvisor::advise(db, 0, 100);
        assert_eq!(batch, 1, "zero columns should return 1 for {db:?}");
    }
}

#[test]
fn zero_rows_returns_one() {
    for db in [
        DbType::MySQL,
        DbType::PostgreSQL,
        DbType::Oracle,
        DbType::Sqlite,
        DbType::SqlServer,
    ] {
        let batch = BatchSizeAdvisor::advise(db, 5, 0);
        assert_eq!(batch, 1, "zero rows should return 1 for {db:?}");
    }
}

#[test]
fn zero_columns_and_zero_rows_returns_one() {
    let batch = BatchSizeAdvisor::advise(DbType::MySQL, 0, 0);
    assert_eq!(batch, 1);
}

// ── 极端值边界 ─────────────────────────────────────────────────────

#[test]
fn max_rows_does_not_panic() {
    let batch = BatchSizeAdvisor::advise(DbType::MySQL, 3, usize::MAX);
    assert!(batch >= 1, "batch must be >= 1 even with usize::MAX rows");
    assert!(batch <= 500, "batch should be capped at ideal 500");
}

#[test]
fn max_rows_with_many_columns_does_not_panic() {
    let batch = BatchSizeAdvisor::advise(DbType::Oracle, 100, usize::MAX);
    assert!(batch >= 1);
    assert!(
        batch * 100 <= 1000,
        "batch * columns must not exceed Oracle param_limit 1000"
    );
}

// ── param_limit 边界 ──────────────────────────────────────────────

#[test]
fn mysql_param_limit_boundary() {
    let batch_at_limit = BatchSizeAdvisor::advise(DbType::MySQL, 65535, 10000);
    assert_eq!(batch_at_limit, 1, "columns == param_limit → batch = 1");
    let batch_over_limit = BatchSizeAdvisor::advise(DbType::MySQL, 65536, 10000);
    assert_eq!(
        batch_over_limit, 1,
        "columns > param_limit → max_by_params = 0, batch = 1"
    );
}

#[test]
fn postgres_param_limit_boundary() {
    let batch_at_limit = BatchSizeAdvisor::advise(DbType::PostgreSQL, 32767, 10000);
    assert_eq!(batch_at_limit, 1);
    let batch_over_limit = BatchSizeAdvisor::advise(DbType::PostgreSQL, 32768, 10000);
    assert_eq!(batch_over_limit, 1);
}

#[test]
fn oracle_param_limit_boundary() {
    let batch_at_limit = BatchSizeAdvisor::advise(DbType::Oracle, 1000, 10000);
    assert_eq!(batch_at_limit, 1);
    let batch_over_limit = BatchSizeAdvisor::advise(DbType::Oracle, 1001, 10000);
    assert_eq!(batch_over_limit, 1);
}

#[test]
fn sqlite_param_limit_boundary() {
    let batch_at_limit = BatchSizeAdvisor::advise(DbType::Sqlite, 999, 10000);
    assert_eq!(batch_at_limit, 1);
    let batch_over_limit = BatchSizeAdvisor::advise(DbType::Sqlite, 1000, 10000);
    assert_eq!(batch_over_limit, 1);
}

#[test]
fn sqlserver_param_limit_boundary() {
    let batch_at_limit = BatchSizeAdvisor::advise(DbType::SqlServer, 2100, 10000);
    assert_eq!(batch_at_limit, 1);
    let batch_over_limit = BatchSizeAdvisor::advise(DbType::SqlServer, 2101, 10000);
    assert_eq!(batch_over_limit, 1);
}

// ── 约束验证 ───────────────────────────────────────────────────────

#[test]
fn constraint_batch_times_columns_le_param_limit() {
    let cases = [
        (DbType::MySQL, 10, 10000),
        (DbType::PostgreSQL, 50, 100000),
        (DbType::Oracle, 3, 5000),
        (DbType::Sqlite, 7, 3000),
        (DbType::SqlServer, 20, 8000),
    ];
    for (db, cols, rows) in cases {
        let batch = BatchSizeAdvisor::advise(db, cols, rows);
        let limit = match db {
            DbType::MySQL => 65535,
            DbType::PostgreSQL => 32767,
            DbType::Oracle => 1000,
            DbType::Sqlite => 999,
            DbType::SqlServer => 2100,
            _ => 1000,
        };
        assert!(
            batch * cols <= limit,
            "{db:?}: batch({batch}) * cols({cols}) = {} > limit({limit})",
            batch * cols
        );
    }
}

#[test]
fn constraint_batch_le_total_rows() {
    for (db, cols, rows) in [
        (DbType::MySQL, 2, 100),
        (DbType::PostgreSQL, 5, 50),
        (DbType::Oracle, 10, 30),
        (DbType::Sqlite, 3, 7),
        (DbType::SqlServer, 4, 15),
    ] {
        let batch = BatchSizeAdvisor::advise(db, cols, rows);
        assert!(batch <= rows, "{db:?}: batch({batch}) > rows({rows})");
    }
}

// ── batch_ranges 边界 ─────────────────────────────────────────────

#[test]
fn batch_ranges_zero_rows_returns_empty() {
    let ranges = BatchSizeAdvisor::batch_ranges(DbType::MySQL, 5, 0);
    assert!(ranges.is_empty(), "zero rows → empty ranges");
}

#[test]
fn batch_ranges_cover_all_rows_exactly() {
    for (db, cols, rows) in [
        (DbType::MySQL, 2, 1000),
        (DbType::Oracle, 10, 999),
        (DbType::Sqlite, 1, 1),
        (DbType::SqlServer, 3, 100),
    ] {
        let ranges = BatchSizeAdvisor::batch_ranges(db, cols, rows);
        let covered: usize = ranges.iter().map(|(s, e)| e - s).sum();
        assert_eq!(covered, rows, "{db:?}: ranges cover {covered} != {rows}");
        for (s, e) in &ranges {
            assert!(s < e, "{db:?}: range ({s}, {e}) has s >= e");
        }
    }
}

#[test]
fn batch_ranges_no_overlap_no_gap() {
    let ranges = BatchSizeAdvisor::batch_ranges(DbType::PostgreSQL, 3, 100);
    for i in 1..ranges.len() {
        assert_eq!(ranges[i].0, ranges[i - 1].1, "ranges must be contiguous");
    }
}
