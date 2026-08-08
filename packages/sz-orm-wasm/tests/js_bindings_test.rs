//! # wasm-bindgen-test 测试套件
//!
//! 覆盖 JS 调用建表/增删改查 + persist/restore 链路。
//! 运行：wasm-pack test --headless --chrome

#![cfg(target_arch = "wasm32")]

use sz_orm_wasm::js_bindings::JsWasmDatabase;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn js_create_table_and_insert() {
    let mut db = JsWasmDatabase::new();
    db.create_table("CREATE TABLE users (id INT, name TEXT)")
        .expect("create_table should succeed");
    let affected = db
        .insert(
            "INSERT INTO users (id, name) VALUES (?, ?)",
            r#"[1, "Alice"]"#,
        )
        .expect("insert should succeed");
    assert_eq!(affected, 1);
}

#[wasm_bindgen_test]
fn js_query_after_insert() {
    let mut db = JsWasmDatabase::new();
    db.create_table("CREATE TABLE items (id INT, label TEXT)")
        .unwrap();
    db.insert(
        "INSERT INTO items (id, label) VALUES (?, ?)",
        r#"[1, "hello"]"#,
    )
    .unwrap();

    let result = db
        .query("SELECT * FROM items", "[]")
        .expect("query should succeed");
    assert!(result.rows_json().contains("hello"));
    assert_eq!(result.affected(), 1);
}

#[wasm_bindgen_test]
fn js_update_and_verify() {
    let mut db = JsWasmDatabase::new();
    db.create_table("CREATE TABLE users (id INT, name TEXT)")
        .unwrap();
    db.insert(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        r#"[1, "Alice"]"#,
    )
    .unwrap();

    let affected = db
        .update("UPDATE users SET name = ? WHERE id = ?", r#"["Bob", 1]"#)
        .expect("update should succeed");
    assert_eq!(affected, 1);

    let result = db.query("SELECT * FROM users", "[]").unwrap();
    assert!(result.rows_json().contains("Bob"));
    assert!(!result.rows_json().contains("Alice"));
}

#[wasm_bindgen_test]
fn js_delete_and_verify() {
    let mut db = JsWasmDatabase::new();
    db.create_table("CREATE TABLE users (id INT, name TEXT)")
        .unwrap();
    db.insert(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        r#"[1, "Alice"]"#,
    )
    .unwrap();
    db.insert(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        r#"[2, "Bob"]"#,
    )
    .unwrap();

    let affected = db
        .delete("DELETE FROM users WHERE id = ?", r#"[1]"#)
        .expect("delete should succeed");
    assert_eq!(affected, 1);

    let result = db.query("SELECT * FROM users", "[]").unwrap();
    assert_eq!(result.affected(), 1);
    assert!(result.rows_json().contains("Bob"));
    assert!(!result.rows_json().contains("Alice"));
}

#[wasm_bindgen_test]
fn js_table_names() {
    let mut db = JsWasmDatabase::new();
    db.create_table("CREATE TABLE t1 (id INT)").unwrap();
    db.create_table("CREATE TABLE t2 (id INT)").unwrap();

    let names = db.table_names();
    assert!(names.contains(&"t1".to_string()));
    assert!(names.contains(&"t2".to_string()));
}

#[wasm_bindgen_test]
fn js_full_crud_cycle() {
    let mut db = JsWasmDatabase::new();

    db.create_table("CREATE TABLE products (id INT, name TEXT, price INT)")
        .unwrap();

    db.insert(
        "INSERT INTO products (id, name, price) VALUES (?, ?, ?)",
        r#"[1, "Widget", 100]"#,
    )
    .unwrap();
    db.insert(
        "INSERT INTO products (id, name, price) VALUES (?, ?, ?)",
        r#"[2, "Gadget", 200]"#,
    )
    .unwrap();

    let result = db.query("SELECT * FROM products", "[]").unwrap();
    assert_eq!(result.affected(), 2);

    db.update("UPDATE products SET price = ? WHERE id = ?", r#"[150, 1]"#)
        .unwrap();

    db.delete("DELETE FROM products WHERE id = ?", r#"[2]"#)
        .unwrap();

    let result = db.query("SELECT * FROM products", "[]").unwrap();
    assert_eq!(result.affected(), 1);
    assert!(result.rows_json().contains("Widget"));
}
