//! Model 模块契约测试 — 对应 `docs/api-contracts.md` §7
//!
//! 锁定 Model trait 必需方法、ModelExt trait、foreign_key 默认推导、关联关系类型。

use std::collections::HashMap;
use sz_orm_core::Value;
use sz_orm_core::{Model, ModelExt};

// ===== 测试用模型 =====

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
    email: String,
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

impl ModelExt for User {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "email"]
    }

    fn fillable() -> Vec<&'static str> {
        vec!["name", "email"]
    }

    fn hidden() -> Vec<&'static str> {
        vec!["email"]
    }

    fn get_column_value(&self, column: &str) -> Option<Value> {
        match column {
            "id" => Some(Value::I64(self.id)),
            "name" => Some(Value::String(self.name.clone())),
            "email" => Some(Value::String(self.email.clone())),
            _ => None,
        }
    }

    fn from_value(&mut self, map: std::collections::HashMap<String, Value>) {
        if let Some(Value::I64(id)) = map.get("id") {
            self.id = *id;
        }
        if let Some(Value::String(name)) = map.get("name") {
            self.name = name.clone();
        }
        if let Some(Value::String(email)) = map.get("email") {
            self.email = email.clone();
        }
    }
}

// ===== §7.1 Model trait 必需方法契约 =====

#[test]
fn test_model_table_name_contract() {
    assert_eq!(User::table_name(), "users");
}

#[test]
fn test_model_pk_name_default_contract() {
    // 默认主键列名为 "id"
    assert_eq!(User::pk_name(), "id");
}

#[test]
fn test_model_pk_get_set_contract() {
    let mut u = User::default();
    assert_eq!(u.pk(), 0); // Default::default() 的 i64 是 0

    u.set_pk(42);
    assert_eq!(u.pk(), 42);
}

#[test]
fn test_model_primary_key_default_contract() {
    // PrimaryKey 必须实现 Default
    let pk = <User as Model>::PrimaryKey::default();
    let _ = pk; // 编译时验证
}

// ===== §7.1 foreign_key 默认推导契约 =====

#[test]
fn test_model_foreign_key_default_contract() {
    // 默认 foreign_key(relation) = "{relation}_id"（小写）
    assert_eq!(User::foreign_key("post"), "post_id");
    assert_eq!(User::foreign_key("Order"), "order_id"); // 转小写
    assert_eq!(User::foreign_key("USER"), "user_id");
}

#[test]
fn test_model_timestamp_fields_default_contract() {
    // 默认无时间戳字段
    assert!(User::timestamp_fields().is_none());
}

#[test]
fn test_model_soft_delete_field_default_contract() {
    // 默认无软删除字段
    assert!(User::soft_delete_field().is_none());
}

// ===== §7.2 ModelExt trait 契约 =====

#[test]
fn test_model_ext_columns_contract() {
    let cols = User::columns();
    assert_eq!(cols, vec!["id", "name", "email"]);
}

#[test]
fn test_model_ext_fillable_contract() {
    let fillable = User::fillable();
    assert_eq!(fillable, vec!["name", "email"]);
}

#[test]
fn test_model_ext_hidden_contract() {
    let hidden = User::hidden();
    assert_eq!(hidden, vec!["email"]);
}

#[test]
fn test_model_ext_guarded_includes_pk_by_default_contract() {
    // guarded() 默认含主键
    let guarded = User::guarded();
    assert!(
        guarded.contains(&"id"),
        "guarded 默认应包含主键列，实际: {:?}",
        guarded
    );
}

#[test]
fn test_model_ext_relations_default_empty_contract() {
    let relations = User::relations();
    assert!(relations.is_empty(), "默认 relations 应为空");
}

// ===== §7.2 fill 契约 =====

#[test]
fn test_model_ext_fill_skips_guarded_contract() {
    let mut u = User {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };

    let mut data = HashMap::new();
    // 尝试 fill 主键（应被跳过，因 id 在 guarded 中）
    data.insert("id".to_string(), Value::I64(999));
    data.insert("name".to_string(), Value::String("Bob".to_string()));

    u.fill(data);

    // id 不应被修改（guarded）
    assert_eq!(u.id, 1);
    // name 应被修改（fillable）
    assert_eq!(u.name, "Bob");
}

// ===== §7.2 to_json 契约 =====

#[test]
fn test_model_ext_to_json_contract() {
    let u = User {
        id: 42,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };

    let json = u.to_json();
    assert!(json.is_object());
    // hidden 字段不应出现在 JSON 中
    assert!(
        json.get("email").is_none(),
        "hidden 字段不应出现在 to_json 输出中"
    );
}

// ===== §7.1 PrimaryKey 约束契约（编译时验证） =====

#[test]
fn test_model_primary_key_trait_bounds_contract() {
    // PrimaryKey 必须满足 Send + Sync + Debug + Display + Clone + Default
    fn assert_pk_bounds<T>()
    where
        T: Send + Sync + std::fmt::Debug + std::fmt::Display + Clone + Default,
    {
    }
    assert_pk_bounds::<<User as Model>::PrimaryKey>();
}

#[test]
fn test_model_send_sync_sized_static_bounds_contract() {
    // Model: Send + Sync + Sized + 'static
    fn assert_model_bounds<T: Model>() {}
    assert_model_bounds::<User>();
}
