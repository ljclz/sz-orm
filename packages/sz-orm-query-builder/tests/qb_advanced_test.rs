use sz_orm_core::DbType;
use sz_orm_query_builder::SelectQuery;

#[test]
fn test_with_cte() {
    let sql = SelectQuery::new()
        .column("id")
        .from("active_users")
        .with_cte("active_users", "SELECT id FROM users WHERE active = 1")
        .build(DbType::PostgreSQL);
    assert!(sql.contains("WITH active_users AS"));
    assert!(sql.contains("SELECT id FROM users WHERE active = 1"));
}

#[test]
fn test_with_recursive_cte() {
    let sql = SelectQuery::new()
        .column("id")
        .from("tree")
        .with_recursive_cte(
            "tree",
            "SELECT 1 UNION ALL SELECT n+1 FROM tree WHERE n < 10",
        )
        .build(DbType::PostgreSQL);
    assert!(sql.contains("WITH RECURSIVE tree AS"));
}

#[test]
fn test_window_function() {
    let sql = SelectQuery::new()
        .column("id")
        .from("orders")
        .window_function("SUM(amount) OVER (PARTITION BY user_id)")
        .build(DbType::MySQL);
    assert!(sql.contains("SUM(amount) OVER (PARTITION BY user_id)"));
}

#[test]
fn test_row_number() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .row_number("dept", "salary DESC", "rn")
        .build(DbType::MySQL);
    assert!(sql.contains("ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC) AS rn"));
}

#[test]
fn test_rank() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .rank("dept", "score DESC", "rk")
        .build(DbType::MySQL);
    assert!(sql.contains("RANK() OVER (PARTITION BY dept ORDER BY score DESC) AS rk"));
}

#[test]
fn test_dense_rank() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .dense_rank("dept", "score DESC", "drk")
        .build(DbType::MySQL);
    assert!(sql.contains("DENSE_RANK() OVER (PARTITION BY dept ORDER BY score DESC) AS drk"));
}
