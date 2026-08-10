use serde_json::json;
use std::time::Duration;
use sz_orm_wasm::*;

// ============================================================================
// WasmQuery
// ============================================================================

#[test]
fn test_wasm_query_new() {
    let q = WasmQuery::new("SELECT * FROM users");
    assert_eq!(q.sql, "SELECT * FROM users");
    assert!(q.params.is_empty());
}

#[test]
fn test_wasm_query_with_params() {
    let q = WasmQuery::with_params("SELECT * FROM users WHERE id = ?", vec![json!(1)]);
    assert_eq!(q.params.len(), 1);
    assert_eq!(q.params[0], json!(1));
}

// ============================================================================
// WasmDatabase - CREATE TABLE
// ============================================================================

#[test]
fn test_create_table() {
    let db = WasmDatabase::new();
    let result = db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"));
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
    assert!(db.table_names().contains(&"users".to_string()));
}

#[test]
fn test_table_names_sorted() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE zebra (id INTEGER)"))
        .unwrap();
    db.execute(WasmQuery::new("CREATE TABLE apple (id INTEGER)"))
        .unwrap();
    let names = db.table_names();
    assert_eq!(names[0], "apple");
    assert_eq!(names[1], "zebra");
}

// ============================================================================
// WasmDatabase - INSERT
// ============================================================================

#[test]
fn test_insert_single() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
        .unwrap();
    let result = db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(1), json!("Alice")],
    ));
    assert_eq!(result.unwrap(), 1);
    assert_eq!(db.table_row_count("users"), 1);
}

#[test]
fn test_insert_multiple() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
        .unwrap();
    let result = db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?), (?, ?)",
        vec![json!(1), json!("Alice"), json!(2), json!("Bob")],
    ));
    assert_eq!(result.unwrap(), 2);
    assert_eq!(db.table_row_count("users"), 2);
}

// ============================================================================
// WasmDatabase - SELECT
// ============================================================================

#[test]
fn test_select_all() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
        .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(1), json!("Alice")],
    ))
    .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(2), json!("Bob")],
    ))
    .unwrap();

    let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_select_with_where() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
        .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(1), json!("Alice")],
    ))
    .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(2), json!("Bob")],
    ))
    .unwrap();

    let rows = db
        .query(WasmQuery::with_params(
            "SELECT * FROM users WHERE id = ?",
            vec![json!(1)],
        ))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], json!("Alice"));
}

#[test]
fn test_select_empty_table() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER)"))
        .unwrap();
    let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn test_select_nonexistent_table() {
    let db = WasmDatabase::new();
    let rows = db
        .query(WasmQuery::new("SELECT * FROM nonexistent"))
        .unwrap();
    assert!(rows.is_empty());
}

// ============================================================================
// WasmDatabase - UPDATE
// ============================================================================

#[test]
fn test_update_with_where() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
        .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(1), json!("Alice")],
    ))
    .unwrap();

    let updated = db
        .execute(WasmQuery::with_params(
            "UPDATE users SET name = ? WHERE id = ?",
            vec![json!("AliceUpdated"), json!(1)],
        ))
        .unwrap();
    assert_eq!(updated, 1);

    let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
    assert_eq!(rows[0]["name"], json!("AliceUpdated"));
}

#[test]
fn test_update_all() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
        .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(1), json!("Alice")],
    ))
    .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(2), json!("Bob")],
    ))
    .unwrap();

    let updated = db
        .execute(WasmQuery::with_params(
            "UPDATE users SET name = ?",
            vec![json!("Generic")],
        ))
        .unwrap();
    assert_eq!(updated, 2);
}

// ============================================================================
// WasmDatabase - DELETE
// ============================================================================

#[test]
fn test_delete_with_where() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
        .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(1), json!("Alice")],
    ))
    .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(2), json!("Bob")],
    ))
    .unwrap();

    let deleted = db
        .execute(WasmQuery::with_params(
            "DELETE FROM users WHERE id = ?",
            vec![json!(1)],
        ))
        .unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(db.table_row_count("users"), 1);
}

#[test]
fn test_delete_all() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER)"))
        .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id) VALUES (?)",
        vec![json!(1)],
    ))
    .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id) VALUES (?)",
        vec![json!(2)],
    ))
    .unwrap();

    let deleted = db.execute(WasmQuery::new("DELETE FROM users")).unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(db.table_row_count("users"), 0);
}

// ============================================================================
// WasmDatabase - Error cases
// ============================================================================

#[test]
fn test_query_non_select_error() {
    let db = WasmDatabase::new();
    let result = db.query(WasmQuery::new("INSERT INTO users VALUES (1)"));
    assert!(result.is_err());
}

#[test]
fn test_select_missing_from_error() {
    let db = WasmDatabase::new();
    let result = db.query(WasmQuery::new("SELECT 1"));
    assert!(result.is_err());
}

// ============================================================================
// WasmDatabase - table_rows
// ============================================================================

#[test]
fn test_table_rows() {
    let db = WasmDatabase::new();
    db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
        .unwrap();
    db.execute(WasmQuery::with_params(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        vec![json!(1), json!("Alice")],
    ))
    .unwrap();

    let rows = db.table_rows("users");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], json!(1));

    let empty = db.table_rows("nonexistent");
    assert!(empty.is_empty());
}

// ============================================================================
// WasmDatabase - advanced module
// ============================================================================

#[test]
fn test_memory_config() {
    let unlimited = MemoryConfig::unlimited();
    assert!(unlimited.max_tables.is_none());
    let strict = MemoryConfig::strict();
    assert!(strict.max_tables.is_some());
}

#[test]
fn test_memory_config_builder() {
    let config = MemoryConfig::unlimited()
        .with_max_tables(10)
        .with_max_rows_per_table(1000);
    assert_eq!(config.max_tables, Some(10));
    assert_eq!(config.max_rows_per_table, Some(1000));
}

#[test]
fn test_sandbox_config() {
    let deny_all = SandboxConfig::deny_all();
    assert!(!deny_all.allow_symlinks);
    assert!(deny_all.rules.is_empty());
}

#[test]
fn test_sandbox_config_allow_rw() {
    let config = SandboxConfig::allow_rw("/tmp");
    assert_eq!(config.rules.len(), 1);
}

#[test]
fn test_sandbox_config_allow_ro() {
    let config = SandboxConfig::allow_ro("/data");
    assert_eq!(config.rules.len(), 1);
}

#[test]
fn test_module_cache() {
    let cache = ModuleCache::new(100, 1024 * 1024, Duration::from_secs(60));
    let stats = cache.stats();
    let _ = stats;
}

#[test]
fn test_async_task_scheduler() {
    let db = WasmDatabase::new();
    let scheduler = AsyncTaskScheduler::new(db, 100);
    let _ = scheduler;
}
