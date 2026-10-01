//! v9.2.0 M18: GenericKeyRepository::clear 覆盖（从内联测试迁出）

use sz_orm_core::repository::{EntityAttributes, GenericKeyRepository};
use sz_orm_core::Value;

#[derive(Debug, Clone, PartialEq)]
struct User {
    id: i64,
    name: String,
    age: i64,
    email: String,
}

impl User {
    fn new(id: i64, name: &str, age: i64, email: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            age,
            email: email.to_string(),
        }
    }
}

impl EntityAttributes for User {
    fn get_attribute(&self, field: &str) -> Option<Value> {
        match field {
            "id" => Some(Value::I64(self.id)),
            "name" => Some(Value::String(self.name.clone())),
            "age" => Some(Value::I64(self.age)),
            "email" => Some(Value::String(self.email.clone())),
            _ => None,
        }
    }
}

#[test]
fn test_generic_key_repo_clear() {
    let repo: GenericKeyRepository<User, i64> =
        GenericKeyRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
        ]);
    assert!(!repo.is_empty());
    repo.clear();
    assert!(repo.is_empty());
    assert_eq!(repo.len(), 0);
}