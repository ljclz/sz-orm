//! query_as! 宏编译测试

use sz_orm_macros::FromQueryResult;

#[derive(FromQueryResult)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
}

#[test]
fn test_query_as_macro_expansion() {
    let q = sz_orm_core::query_as!(User, "SELECT id, name FROM users WHERE id = 1");
    assert_eq!(q.sql(), "SELECT id, name FROM users WHERE id = 1");
}

#[test]
fn test_query_as_macro_path_type() {
    let q = sz_orm_core::query_as!(crate::User, "SELECT id, name FROM users");
    assert!(q.sql().contains("SELECT"));
}

#[test]
fn test_from_query_result_row_desc() {
    // P0-2：derive 宏生成 row_desc() 列名元数据
    let desc = <User as sz_orm_core::FromQueryResult>::row_desc();
    assert_eq!(desc, vec!["id", "name"]);
}

#[test]
fn test_from_query_result_column_types() {
    // P0-2：derive 宏生成 column_types() 列类型元数据
    let types = <User as sz_orm_core::FromQueryResult>::column_types();
    assert_eq!(types, vec![("id", "BIGINT"), ("name", "TEXT")]);
}

#[test]
fn test_from_query_result_const_column_types() {
    // P0-2：derive 宏额外生成 const fn __sz_orm_column_types()，
    // 可在 const 上下文中调用（供 query_as! 编译期类型验证使用）
    const TYPES: &[(&str, &str)] = User::__sz_orm_column_types();
    assert_eq!(TYPES, vec![("id", "BIGINT"), ("name", "TEXT")]);
    // const fn 数据与 trait 方法 column_types() 完全一致
    assert_eq!(
        TYPES,
        <User as sz_orm_core::FromQueryResult>::column_types()
    );
}

#[derive(FromQueryResult)]
#[allow(dead_code)]
struct UserWithColumnRename {
    id: i64,
    #[column(name = "user_name")]
    name: String,
}

#[test]
fn test_from_query_result_row_desc_with_rename() {
    // P0-2：#[column(name)] 重命名应反映在 row_desc() 中
    let desc = <UserWithColumnRename as sz_orm_core::FromQueryResult>::row_desc();
    assert_eq!(desc, vec!["id", "user_name"]);
}

#[test]
fn test_from_query_result_column_types_with_rename() {
    // P0-2：#[column(name)] 重命名应反映在 column_types() 中
    let types = <UserWithColumnRename as sz_orm_core::FromQueryResult>::column_types();
    assert_eq!(types, vec![("id", "BIGINT"), ("user_name", "TEXT")]);
}

#[derive(FromQueryResult)]
#[allow(dead_code)]
struct UserWithOption {
    id: i64,
    name: Option<String>,
    age: Option<i32>,
}

#[test]
fn test_from_query_result_column_types_with_option() {
    // P0-2：Option<T> 字段的 SQL 类型应展开为内层类型
    let types = <UserWithOption as sz_orm_core::FromQueryResult>::column_types();
    assert_eq!(
        types,
        vec![("id", "BIGINT"), ("name", "TEXT"), ("age", "INTEGER")]
    );
}
