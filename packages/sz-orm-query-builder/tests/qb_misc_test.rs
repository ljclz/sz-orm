use sz_orm_core::DbType;
use sz_orm_query_builder::SelectQuery;

#[test]
fn test_from_subquery() {
    let inner = SelectQuery::new()
        .column("id")
        .from("orders")
        .build(DbType::MySQL);
    let sql = SelectQuery::new()
        .column("id")
        .from_subquery(&inner, "t")
        .build(DbType::MySQL);
    assert!(sql.contains("FROM ("));
    assert!(sql.contains(") AS `t`"));
}

#[test]
fn test_distinct() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .distinct()
        .build(DbType::MySQL);
    assert!(sql.contains("SELECT DISTINCT"));
}

#[test]
fn test_for_update() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .for_update()
        .build(DbType::MySQL);
    assert!(sql.contains("FOR UPDATE"));
}

#[test]
fn test_for_update_with_options() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .for_update_with_options("NOWAIT")
        .build(DbType::MySQL);
    assert!(sql.contains("FOR UPDATE NOWAIT"));
}
