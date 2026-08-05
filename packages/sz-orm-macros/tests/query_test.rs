//! query! 宏编译测试

use sz_orm_macros::FromQueryResult;

#[derive(FromQueryResult)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
}

#[test]
fn test_query_macro_returns_query_object() {
    let q = sz_orm_core::query!("SELECT id, name FROM users WHERE id = 1");
    assert_eq!(q.sql(), "SELECT id, name FROM users WHERE id = 1");
}

#[test]
fn test_query_macro_with_params() {
    let q = sz_orm_core::query!("SELECT * FROM users WHERE id = ? AND name = ?");
    assert!(q.sql().contains("SELECT *"));
    assert!(q.sql().contains("WHERE id = ?"));
}

#[test]
fn test_query_macro_simple_select() {
    let q = sz_orm_core::query!("SELECT id FROM users");
    assert_eq!(q.sql(), "SELECT id FROM users");
}

// ---- P0-1：query! 支持可选类型参数 query!(T, "SQL") ----

#[test]
fn test_query_macro_with_type_param() {
    // P0-1：query!(User, "SQL") 返回 QueryAs<User>，与 query_as!(User, "SQL") 等价
    let q = sz_orm_core::query!(User, "SELECT id, name FROM users WHERE id = 1");
    assert_eq!(q.sql(), "SELECT id, name FROM users WHERE id = 1");
}

#[test]
fn test_query_macro_with_path_type_param() {
    // P0-1：类型参数支持路径（如 crate::User）
    let q = sz_orm_core::query!(crate::User, "SELECT id, name FROM users");
    assert!(q.sql().contains("SELECT"));
}
