use sz_orm_core::DbType;
use sz_orm_query_builder::InsertQuery;

fn base_insert() -> InsertQuery {
    InsertQuery::new()
        .into_table("users")
        .value("id", "1")
        .value("name", "'Alice'")
}

#[test]
fn test_on_conflict_do_nothing() {
    let sql = base_insert()
        .on_conflict_do_nothing(&["id"])
        .build_with_dialect(DbType::PostgreSQL);
    assert!(sql.contains("ON CONFLICT"));
    assert!(sql.contains("DO NOTHING"));
}

#[test]
fn test_on_conflict_do_update() {
    let sql = base_insert()
        .value("count", "1")
        .on_conflict_do_update(&["id"], &[("count", "users.count + 1")])
        .build_with_dialect(DbType::PostgreSQL);
    assert!(sql.contains("ON CONFLICT"));
    assert!(sql.contains("DO UPDATE SET"));
}

#[test]
fn test_on_duplicate_key_update() {
    let sql = base_insert()
        .value("count", "1")
        .on_duplicate_key_update(&[("count", "count + 1")])
        .build_with_dialect(DbType::MySQL);
    assert!(sql.contains("ON DUPLICATE KEY UPDATE"));
}

#[test]
fn test_replace() {
    let sql = base_insert().replace().build_with_dialect(DbType::MySQL);
    assert!(sql.contains("REPLACE INTO"));
}

#[test]
fn test_returning() {
    let sql = base_insert()
        .returning(&["id", "name"])
        .build_with_dialect(DbType::PostgreSQL);
    assert!(sql.contains("RETURNING"));
}

#[test]
fn test_returning_all() {
    let sql = base_insert()
        .returning_all()
        .build_with_dialect(DbType::PostgreSQL);
    assert!(sql.contains("RETURNING *"));
}