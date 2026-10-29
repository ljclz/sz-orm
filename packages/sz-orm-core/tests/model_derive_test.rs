//! 集成测试：`#[derive(Model)]` 派生宏（R5, 2026-10-27）
//!
//! 验证派生宏生成的 `impl Model + ModelExt` 可编译且行为正确，
//! 覆盖：表名/主键/列清单/fillable/get_column_value/from_value/时间戳/软删除。

use sz_orm_core::{Model, ModelExt, Value};

#[derive(Debug, Clone, Default, sz_orm_core::Model)]
#[table(name = "users")]
struct User {
    #[column(primary_key)]
    id: i64,
    name: String,
    email: Option<String>,
}

#[derive(Debug, Clone, Default, sz_orm_core::Model)]
struct Article {
    id: i64,
    title: String,
    created_at: Option<String>,
    updated_at: Option<String>,
    deleted_at: Option<String>,
}

#[test]
fn test_derive_model_basic() {
    assert_eq!(User::table_name(), "users");
    assert_eq!(User::pk_name(), "id");
    assert_eq!(User::columns(), vec!["id", "name", "email"]);
    assert_eq!(User::fillable(), vec!["name", "email"]);

    let u = User {
        id: 1,
        name: "Alice".to_string(),
        email: None,
    };
    assert_eq!(u.pk(), 1);

    // get_column_value
    let v = u.get_column_value("name");
    assert!(matches!(v, Some(Value::String(s)) if s == "Alice"));
    assert!(matches!(u.get_column_value("id"), Some(Value::I64(1))));
    assert!(u.get_column_value("email").is_none());
    assert!(u.get_column_value("not_exist").is_none());

    // from_value
    let mut copy = User::default();
    let mut map = std::collections::HashMap::new();
    map.insert("name".to_string(), Value::String("Bob".to_string()));
    map.insert("email".to_string(), Value::Null);
    copy.from_value(map);
    assert_eq!(copy.name, "Bob");
    assert!(copy.email.is_none());
}

#[test]
fn test_derive_model_timestamp_soft_delete() {
    // 含 created_at/updated_at/deleted_at 时自动生成时间戳与软删除
    assert!(Article::timestamp_fields().is_some());
    assert_eq!(Article::soft_delete_field(), Some("deleted_at"));
    assert_eq!(Article::table_name(), "article"); // 默认 snake_case
    assert_eq!(Article::pk_name(), "id");
}

#[test]
fn test_derive_model_fill() {
    let mut u = User::default();
    let mut map = std::collections::HashMap::new();
    map.insert("id".to_string(), Value::I64(999)); // guarded（主键）
    map.insert("name".to_string(), Value::String("Carol".to_string()));
    u.fill(map);
    assert_eq!(u.id, 0, "主键受 guarded 保护，不应被批量赋值");
    assert_eq!(u.name, "Carol");
}
