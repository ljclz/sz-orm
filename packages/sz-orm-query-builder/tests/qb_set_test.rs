use sz_orm_core::DbType;
use sz_orm_query_builder::SelectQuery;

#[test]
fn test_union() {
    let q1 = SelectQuery::new().column("id").from("active_users");
    let q2 = SelectQuery::new().column("id").from("pending_users");
    let sql = q1.union(q2).build(DbType::MySQL);
    assert!(sql.contains("UNION"));
}

#[test]
fn test_union_all() {
    let q1 = SelectQuery::new().column("id").from("a");
    let q2 = SelectQuery::new().column("id").from("b");
    let sql = q1.union_all(q2).build(DbType::MySQL);
    assert!(sql.contains("UNION ALL"));
}

#[test]
fn test_intersect() {
    let q1 = SelectQuery::new().column("id").from("a");
    let q2 = SelectQuery::new().column("id").from("b");
    let sql = q1.intersect(q2).build(DbType::MySQL);
    assert!(sql.contains("INTERSECT"));
}

#[test]
fn test_except() {
    let q1 = SelectQuery::new().column("id").from("a");
    let q2 = SelectQuery::new().column("id").from("b");
    let sql = q1.except(q2).build(DbType::MySQL);
    assert!(sql.contains("EXCEPT"));
}

#[test]
fn test_set_query_build_chained() {
    let q1 = SelectQuery::new().column("id").from("a");
    let q2 = SelectQuery::new().column("id").from("b");
    let q3 = SelectQuery::new().column("id").from("c");
    let sql = q1.union(q2).union_all(q3).build(DbType::MySQL);
    assert!(sql.contains("UNION"));
    assert!(sql.contains("UNION ALL"));
}
