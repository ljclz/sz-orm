use std::collections::HashMap;
use std::pin::Pin;
use sz_orm_core::{
    ActiveRecord, Connection, HasMany, Model, ModelExt, Relation, RelationAccess, RelationLoader,
    Value,
};

#[derive(Clone)]
struct User {
    id: i64,
    name: String,
    relations: HashMap<String, Value>,
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
        vec!["id", "name"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name"]
    }
    fn relations() -> HashMap<&'static str, Relation> {
        let mut m = HashMap::new();
        m.insert(
            "orders",
            Relation::HasMany(HasMany {
                foreign_key: "user_id".to_string(),
                child_model: "orders".to_string(),
                child_pk: "id".to_string(),
            }),
        );
        m
    }
    fn get_column_value(&self, col: &str) -> Option<Value> {
        match col {
            "id" => Some(Value::I64(self.id)),
            "name" => Some(Value::String(self.name.clone())),
            _ => None,
        }
    }
}

impl RelationLoader for User {
    fn get_relation(&self, name: &str) -> Option<&Value> {
        self.relations.get(name)
    }
    fn set_relation_data(&mut self, name: &str, data: Value) {
        self.relations.insert(name.to_string(), data);
    }
    fn get_relation_fk_value(&self, fk: &str) -> String {
        match fk {
            "user_id" => self.id.to_string(),
            _ => "0".to_string(),
        }
    }
}

impl ActiveRecord for User {}
impl RelationAccess for User {}

struct MockConn;
impl Connection for MockConn {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(1) })
    }
    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Vec<HashMap<String, Value>>, sz_orm_core::DbError>,
                > + Send
                + 'a,
        >,
    > {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("user_id".to_string(), Value::I64(1));
        row.insert("total".to_string(), Value::String("100".to_string()));
        Box::pin(async move { Ok(vec![row]) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn test_relation_loader_set_and_get() {
    let mut u = User {
        id: 1,
        name: "Alice".into(),
        relations: HashMap::new(),
    };
    assert!(u.get_relation("orders").is_none());
    u.set_relation_data("orders", Value::Array(vec![]));
    assert!(u.get_relation("orders").is_some());
    assert!(matches!(u.get_relation("orders").unwrap(), Value::Array(_)));
}

#[test]
fn test_relation_loader_fk_value() {
    let u = User {
        id: 42,
        name: "Bob".into(),
        relations: HashMap::new(),
    };
    assert_eq!(u.get_relation_fk_value("user_id"), "42");
    assert_eq!(u.get_relation_fk_value("unknown"), "0");
}

#[tokio::test]
async fn test_active_record_with_load() {
    let u = User {
        id: 1,
        name: "Alice".into(),
        relations: HashMap::new(),
    };
    let mut conn = MockConn;
    let loaded = u.with("orders").load(&mut conn).await.unwrap();
    let orders = loaded
        .get_relation("orders")
        .expect("orders should be loaded");
    match orders {
        Value::Array(items) => assert_eq!(items.len(), 1),
        other => panic!("expected Array, got {:?}", other),
    }
}
