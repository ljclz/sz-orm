//! v8.8.0 黑帽测试 — 恶意输入 / SQL 注入向量
//!
//! 验证 v8.8.0 优化在恶意输入下的行为：
//! - 恶意超大行数/列数不 OOM、不 panic
//! - SQL 注入向量被参数化查询拦截
//! - param_limit 溢出保护

use sz_orm_core::batch_advisor::BatchSizeAdvisor;
use sz_orm_core::{get_dialect, AggExpr, DbType, HavingOp, Model, QueryBuilder, Value};

// ── 恶意超大输入 ──────────────────────────────────────────────────

#[test]
fn malicious_huge_row_count_no_oom() {
    let batch = BatchSizeAdvisor::advise(DbType::MySQL, 3, usize::MAX);
    assert!(batch >= 1, "must not OOM, must return >= 1");
    assert!(batch <= 500, "should be capped at ideal 500");
}

#[test]
fn malicious_huge_column_count_no_panic() {
    let batch = BatchSizeAdvisor::advise(DbType::Oracle, usize::MAX, 10000);
    assert_eq!(
        batch, 1,
        "columns = usize::MAX → max_by_params = 0, batch = 1"
    );
}

#[test]
fn malicious_zero_columns_no_panic() {
    let batch = BatchSizeAdvisor::advise(DbType::Sqlite, 0, usize::MAX);
    assert_eq!(batch, 1, "zero columns → 1");
}

#[test]
fn malicious_param_limit_overflow_protection() {
    let batch = BatchSizeAdvisor::advise(DbType::SqlServer, 2101, 100000);
    assert_eq!(batch, 1, "columns > param_limit → batch = 1, no overflow");
    let batch2 = BatchSizeAdvisor::advise(DbType::Oracle, 1001, 100000);
    assert_eq!(batch2, 1);
}

// ── SQL 注入向量 ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct User {
    id: i64,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

fn mysql_builder() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

#[test]
fn sql_injection_drop_table_in_having_rejected() {
    let qb = mysql_builder().table("users").group_by("user_id");
    let result = qb.having(
        AggExpr::Sum("total`; DROP TABLE users; --".to_string()),
        HavingOp::Gt,
        Value::I64(5),
    );
    assert!(result.is_err(), "DROP TABLE injection must be rejected");
}

#[test]
fn sql_injection_union_in_having_rejected() {
    let qb = mysql_builder().table("users").group_by("user_id");
    let result = qb.having(
        AggExpr::Sum("total UNION SELECT password FROM users --".to_string()),
        HavingOp::Gt,
        Value::I64(5),
    );
    assert!(result.is_err(), "UNION injection must be rejected");
}

#[test]
fn sql_injection_value_bound_as_param_not_inlined() {
    let evil_value = "5 OR 1=1; DROP TABLE users; --";
    let qb = mysql_builder()
        .table("users")
        .group_by("user_id")
        .having(
            AggExpr::CountStar,
            HavingOp::Gt,
            Value::String(evil_value.to_string()),
        )
        .expect("valid aggregate");
    let (sql, params) = qb.build_select_with_params();
    assert!(sql.contains('?'), "value must be parameterized");
    assert!(
        !sql.contains("DROP TABLE"),
        "injection must not appear in SQL text"
    );
    assert!(
        !sql.contains("OR 1=1"),
        "injection must not appear in SQL text"
    );
    assert_eq!(params, vec![Value::String(evil_value.to_string())]);
}

#[test]
fn sql_injection_where_eq_parameterized() {
    let evil = "1' OR '1'='1";
    let qb = mysql_builder()
        .table("users")
        .where_eq("name", Value::String(evil.to_string()));
    let (sql, params) = qb.build_select_with_params();
    assert!(
        !sql.contains("OR '1'='1"),
        "injection must not appear in SQL text"
    );
    assert!(sql.contains('?'), "must use parameter placeholder");
    assert_eq!(params, vec![Value::String(evil.to_string())]);
}

#[test]
fn sql_injection_comment_injection_rejected() {
    let qb = mysql_builder().table("users").group_by("user_id");
    let result = qb.having(
        AggExpr::Sum("total -- ".to_string()),
        HavingOp::Gt,
        Value::I64(5),
    );
    assert!(result.is_err(), "comment injection must be rejected");
}
