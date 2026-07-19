//! Model abstraction layer
//!
//! Provides the core Model trait and related types

use crate::async_trait;
use crate::value::Value;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

/// Model is the core trait that all ORM models must implement
pub trait Model: Send + Sync + Sized + 'static {
    /// The primary key type for this model
    type PrimaryKey: Send + Sync + fmt::Debug + fmt::Display + Clone + Default;

    /// Get the table name for this model
    fn table_name() -> &'static str;

    /// Get the primary key column name
    fn pk_name() -> &'static str {
        "id"
    }

    /// Get the primary key value
    fn pk(&self) -> Self::PrimaryKey;

    /// Set the primary key value
    fn set_pk(&mut self, pk: Self::PrimaryKey);

    /// Get the foreign key name for a relation
    fn foreign_key(relation: &str) -> String {
        format!("{}_id", relation.to_lowercase())
    }

    /// Get the auto-timestamp fields
    fn timestamp_fields() -> Option<TimestampFields> {
        None
    }

    /// Get the soft delete field
    fn soft_delete_field() -> Option<&'static str> {
        None
    }
}

/// Timestamp field configuration
#[derive(Debug, Clone, Default)]
pub struct TimestampFields {
    /// Field name for created_at
    pub created_at: Option<&'static str>,
    /// Field name for updated_at
    pub updated_at: Option<&'static str>,
    /// Whether to auto-set timestamps on insert
    pub auto_now_insert: bool,
    /// Whether to auto-update timestamps on save
    pub auto_now_update: bool,
}

impl TimestampFields {
    pub fn new(created_at: Option<&'static str>, updated_at: Option<&'static str>) -> Self {
        Self {
            created_at,
            updated_at,
            auto_now_insert: created_at.is_some(),
            auto_now_update: updated_at.is_some(),
        }
    }

    pub fn with_both(created_at: &'static str, updated_at: &'static str) -> Self {
        Self {
            created_at: Some(created_at),
            updated_at: Some(updated_at),
            auto_now_insert: true,
            auto_now_update: true,
        }
    }
}

/// Represents a model's relationship to another model
#[derive(Debug, Clone)]
pub enum Relation {
    /// 多对一关系（如 Order 属于 User）
    BelongsTo(BelongsTo),
    /// 一对多关系（如 User 有多个 Order）
    HasMany(HasMany),
    /// 一对一关系（如 User 有一个 Profile）
    HasOne(HasOne),
    /// 多对多关系（通过中间表，如 User 与 Role）
    BelongsToMany(BelongsToMany),
}

/// Configuration for belongs-to relationship
#[derive(Debug, Clone)]
pub struct BelongsTo {
    pub foreign_key: String,
    pub parent_model: String,
    pub parent_pk: String,
}

/// Configuration for has-many relationship
#[derive(Debug, Clone)]
pub struct HasMany {
    pub foreign_key: String,
    pub child_model: String,
    pub child_pk: String,
}

/// Configuration for has-one relationship
#[derive(Debug, Clone)]
pub struct HasOne {
    pub foreign_key: String,
    pub child_model: String,
    pub child_pk: String,
}

/// Configuration for many-to-many relationship
#[derive(Debug, Clone)]
pub struct BelongsToMany {
    pub junction_table: String,
    pub foreign_key: String,
    pub other_key: String,
    pub target_model: String,
}

/// Trait for models that support relationship loading (ActiveRecord pattern)
#[async_trait]
pub trait ActiveRecord: Model + ModelExt + RelationLoader + Clone + Send + Sync {
    /// Eager load specified relationships
    /// Usage: user.with("orders").with("profile").load(&mut conn).await
    fn with(self, relation: &str) -> WithRelation<Self> {
        WithRelation {
            model: self,
            relations: vec![relation.to_string()],
        }
    }

    /// Eager load multiple relationships at once
    fn with_all(self, relations: Vec<&str>) -> WithRelation<Self> {
        WithRelation {
            model: self,
            relations: relations.into_iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Builder for eager loading relationships
pub struct WithRelation<M: Model + ModelExt + RelationLoader> {
    model: M,
    relations: Vec<String>,
}

impl<M: Model + ModelExt + RelationLoader> WithRelation<M> {
    /// Add another relationship to load
    pub fn with(mut self, relation: &str) -> Self {
        self.relations.push(relation.to_string());
        self
    }

    /// Load the model with specified relations
    /// The loaded relations will be attached to the model via set_relation_data
    pub async fn load<C>(self, conn: &mut C) -> Result<M, RelationError>
    where
        C: crate::pool::Connection + ?Sized,
    {
        let mut model = self.model;
        let relations_map = M::relations();

        for rel_name in &self.relations {
            let relation = relations_map
                .get(rel_name.as_str())
                .ok_or_else(|| RelationError::RelationNotFound(rel_name.clone()))?;

            match relation {
                Relation::HasMany(config) => {
                    let pk = model.pk();
                    let pk_str = format!("{}", pk);
                    let sql = format!(
                        "SELECT * FROM {} WHERE {} = '{}'",
                        config.child_model,
                        config.foreign_key,
                        pk_str
                    );
                    let rows = conn.query(&sql).await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
                Relation::HasOne(config) => {
                    let pk = model.pk();
                    let pk_str = format!("{}", pk);
                    let sql = format!(
                        "SELECT * FROM {} WHERE {} = '{}'",
                        config.child_model,
                        config.foreign_key,
                        pk_str
                    );
                    let rows = conn.query(&sql).await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
                Relation::BelongsTo(config) => {
                    let fk_value = model.get_relation_fk_value(&config.foreign_key);
                    let sql = format!(
                        "SELECT * FROM {} WHERE {} = '{}'",
                        config.parent_model,
                        config.parent_pk,
                        fk_value
                    );
                    let rows = conn.query(&sql).await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
                Relation::BelongsToMany(config) => {
                    let pk = model.pk();
                    let pk_str = format!("{}", pk);
                    let sql = format!(
                        "SELECT t.* FROM {} t INNER JOIN {} j ON t.{} = j.{} WHERE j.{} = '{}'",
                        config.target_model,
                        config.junction_table,
                        config.other_key,
                        config.other_key,
                        config.foreign_key,
                        pk_str
                    );
                    let rows = conn.query(&sql).await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
            }
        }

        Ok(model)
    }
}

/// Convert query rows to Vec<HashMap<String, Value>> for relation storage
pub fn rows_to_values(rows: Vec<HashMap<String, Value>>) -> Value {
    if rows.is_empty() {
        return Value::Array(vec![]);
    }
    let items: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let mut map = HashMap::new();
            for (k, v) in row {
                map.insert(k, v);
            }
            Value::from_map(map)
        })
        .collect();
    Value::Array(items)
}

/// Error type for relationship operations
#[derive(Error, Debug, Clone)]
pub enum RelationError {
    #[error("Relation '{0}' not found in model relations")]
    RelationNotFound(String),

    #[error("Query error during relation loading: {0}")]
    QueryError(String),

    #[error("Relation data not loaded. Call .with(\"{0}\") before accessing.")]
    NotLoaded(String),
}

/// Trait for models that can store loaded relation data
pub trait RelationLoader: Model {
    /// Get loaded relation data
    fn get_relation(&self, name: &str) -> Option<&Value>;

    /// Set loaded relation data
    fn set_relation_data(&mut self, name: &str, data: Value);

    /// Get foreign key value for a relation
    fn get_relation_fk_value(&self, fk_name: &str) -> String;
}

/// Extension methods on ModelExt for relation access
pub trait RelationAccess: ModelExt {
    /// Get a has-many relation (must be loaded first)
    fn get_has_many(&self, name: &str) -> Result<Vec<HashMap<String, Value>>, RelationError>
    where
        Self: RelationLoader,
    {
        let data = self
            .get_relation(name)
            .ok_or_else(|| RelationError::NotLoaded(name.to_string()))?;
        match data {
            Value::Array(items) => {
                let result: Vec<HashMap<String, Value>> = items
                    .iter()
                    .filter_map(|v| match v {
                        Value::Object(map) => Some(map.clone()),
                        _ => None,
                    })
                    .collect();
                Ok(result)
            }
            _ => Ok(vec![]),
        }
    }

    /// Get a has-one or belongs-to relation (must be loaded first, returns single)
    fn get_has_one(&self, name: &str) -> Result<Option<HashMap<String, Value>>, RelationError>
    where
        Self: RelationLoader,
    {
        let data = self
            .get_relation(name)
            .ok_or_else(|| RelationError::NotLoaded(name.to_string()))?;
        match data {
            Value::Array(items) => {
                if items.is_empty() {
                    Ok(None)
                } else {
                    match &items[0] {
                        Value::Object(map) => Ok(Some(map.clone())),
                        _ => Ok(None),
                    }
                }
            }
            _ => Ok(None),
        }
    }

    /// Get a belongs-to-many relation (must be loaded first)
    fn get_belongs_to_many(&self, name: &str) -> Result<Vec<HashMap<String, Value>>, RelationError>
    where
        Self: RelationLoader,
    {
        self.get_has_many(name)
    }
}

/// Scope for filtering query results
pub trait Scope: Send + Sync {
    /// Apply the scope to a query
    fn apply<M: Model>(&self, query: &mut QueryBuilderWrapper<M>);
}

/// Wrapper for query builder to add scope functionality
pub struct QueryBuilderWrapper<'a, M: Model> {
    pub builder: &'a mut dyn QueryBuilderExt<Model = M>,
}

pub trait QueryBuilderExt: Send + Sync {
    type Model: Model;

    fn and_where(&mut self, condition: &str);
    fn or_where(&mut self, condition: &str);
}

/// Model extension trait for additional functionality
pub trait ModelExt: Model {
    /// Get all columns for SELECT
    fn columns() -> Vec<&'static str>;

    /// Get fillable columns (for INSERT/UPDATE)
    fn fillable() -> Vec<&'static str>;

    /// Get guarded columns (not mass-assignable)
    fn guarded() -> Vec<&'static str> {
        vec![Self::pk_name()]
    }

    /// Get hidden columns (not serialized)
    fn hidden() -> Vec<&'static str> {
        vec![]
    }

    /// Get visible columns (serialized)
    fn visible() -> Vec<&'static str> {
        vec![]
    }

    /// Get casts (column -> type mapping)
    fn casts() -> std::collections::HashMap<&'static str, &'static str> {
        std::collections::HashMap::new()
    }

    /// Get dates (columns treated as dates)
    fn dates() -> Vec<&'static str> {
        vec![]
    }

    /// Get date format for a field
    fn date_format(_field: &str) -> Option<&'static str> {
        None
    }

    /// Get relationships
    fn relations() -> std::collections::HashMap<&'static str, Relation> {
        std::collections::HashMap::new()
    }

    /// Convert model to a value map
    fn to_value(&self) -> std::collections::HashMap<String, Value> {
        let mut map = std::collections::HashMap::new();
        for col in Self::columns() {
            if let Some(val) = Self::get_column_value(self, col) {
                // 跳过 hidden 字段
                if !Self::hidden().contains(&col) {
                    map.insert(col.to_string(), val);
                }
            }
        }
        map
    }

    /// Get a specific column value (must be overridden by implementation)
    fn get_column_value(&self, _column: &str) -> Option<Value> {
        None
    }

    /// Convert model from a value map (must be overridden by implementation)
    #[allow(clippy::wrong_self_convention)]
    fn from_value(&mut self, _map: std::collections::HashMap<String, Value>) {
        // Default: no-op. Implementations must override this.
    }

    /// 批量赋值：只填充 fillable 字段（过滤掉 guarded 字段）
    fn fill(&mut self, mut map: std::collections::HashMap<String, Value>) {
        let guarded = Self::guarded();
        let fillable = Self::fillable();
        // 移除 guarded 字段
        for g in &guarded {
            map.remove(*g);
        }
        // 如果 fillable 非空，只保留 fillable 字段
        if !fillable.is_empty() {
            map.retain(|k, _| fillable.contains(&k.as_str()));
        }
        self.from_value(map);
    }

    /// 序列化为 JSON
    fn to_json(&self) -> serde_json::Value {
        let map = self.to_value();
        let mut obj = serde_json::Map::new();
        for (k, v) in map {
            obj.insert(k, value_to_json(v));
        }
        serde_json::Value::Object(obj)
    }
}

/// 将 Value 转换为 serde_json::Value（递归处理 Array）
pub fn value_to_json(v: Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::I8(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::I16(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::I32(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::I64(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::U8(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::U16(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::U32(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::U64(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::F32(n) => serde_json::Number::from_f64(n as f64)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::F64(n) => serde_json::Number::from_f64(n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s),
        Value::Bytes(b) => {
            serde_json::Value::String(b.iter().map(|byte| format!("{:02x}", byte)).collect())
        }
        Value::Uuid(s) | Value::Date(s) | Value::DateTime(s) | Value::Time(s) | Value::Json(s) => {
            serde_json::Value::String(s)
        }
        Value::Array(arr) => serde_json::Value::Array(arr.into_iter().map(value_to_json).collect()),
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k, value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_fields() {
        let ts = TimestampFields::new(Some("created_at"), Some("updated_at"));
        assert!(ts.created_at.is_some());
        assert!(ts.updated_at.is_some());

        let ts2 = TimestampFields::with_both("created_at", "updated_at");
        assert!(ts2.auto_now_insert);
        assert!(ts2.auto_now_update);
    }

    #[test]
    fn test_foreign_key() {
        struct TestModel;
        impl Model for TestModel {
            type PrimaryKey = i64;

            fn table_name() -> &'static str {
                "test_models"
            }

            fn pk(&self) -> Self::PrimaryKey {
                1
            }

            fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
        }

        let fk = TestModel::foreign_key("user");
        assert_eq!(fk, "user_id");

        let fk = TestModel::foreign_key("Role");
        assert_eq!(fk, "role_id");
    }

    #[test]
    fn test_relation_documentation() {
        // 验证 Relation 枚举的语义
        let belongs_to = Relation::BelongsTo(BelongsTo {
            foreign_key: "user_id".to_string(),
            parent_model: "User".to_string(),
            parent_pk: "id".to_string(),
        });
        if let Relation::BelongsTo(ref bt) = belongs_to {
            assert_eq!(bt.parent_model, "User");
        }

        let has_one = Relation::HasOne(HasOne {
            foreign_key: "user_id".to_string(),
            child_model: "Profile".to_string(),
            child_pk: "id".to_string(),
        });
        if let Relation::HasOne(ref ho) = has_one {
            assert_eq!(ho.child_model, "Profile");
        }

        let has_many = Relation::HasMany(HasMany {
            foreign_key: "user_id".to_string(),
            child_model: "Order".to_string(),
            child_pk: "id".to_string(),
        });
        if let Relation::HasMany(ref hm) = has_many {
            assert_eq!(hm.child_model, "Order");
        }

        let many_to_many = Relation::BelongsToMany(BelongsToMany {
            junction_table: "user_role".to_string(),
            foreign_key: "user_id".to_string(),
            other_key: "role_id".to_string(),
            target_model: "Role".to_string(),
        });
        if let Relation::BelongsToMany(ref mtm) = many_to_many {
            assert_eq!(mtm.junction_table, "user_role");
        }
    }

    #[test]
    fn test_model_ext_implementation() {
        /// 测试用的完整 ModelExt 实现
        struct UserModel {
            id: i64,
            name: String,
            email: String,
            password: String, // hidden
        }

        impl Model for UserModel {
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

        impl ModelExt for UserModel {
            fn columns() -> Vec<&'static str> {
                vec!["id", "name", "email", "password"]
            }

            fn fillable() -> Vec<&'static str> {
                vec!["name", "email", "password"]
            }

            fn hidden() -> Vec<&'static str> {
                vec!["password"]
            }

            fn get_column_value(&self, column: &str) -> Option<Value> {
                match column {
                    "id" => Some(Value::I64(self.id)),
                    "name" => Some(Value::String(self.name.clone())),
                    "email" => Some(Value::String(self.email.clone())),
                    "password" => Some(Value::String(self.password.clone())),
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
                if let Some(Value::String(password)) = map.get("password") {
                    self.password = password.clone();
                }
            }
        }

        let user = UserModel {
            id: 1,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            password: "secret".to_string(),
        };

        // 测试 to_value（应该跳过 hidden 字段）
        let values = user.to_value();
        assert!(values.contains_key("name"));
        assert!(values.contains_key("email"));
        // password 是 hidden，不应出现在 to_value 结果中
        assert!(!values.contains_key("password"));

        // 测试 to_json
        let json = user.to_json();
        assert!(json.is_object());
        assert!(json.get("name").is_some());
        assert!(json.get("password").is_none());

        // 测试 fill（应该过滤 guarded 字段）
        let mut user2 = UserModel {
            id: 0,
            name: String::new(),
            email: String::new(),
            password: String::new(),
        };
        let mut fill_data = std::collections::HashMap::new();
        fill_data.insert("id".to_string(), Value::I64(999)); // guarded, 应被过滤
        fill_data.insert("name".to_string(), Value::String("Bob".to_string()));
        fill_data.insert(
            "email".to_string(),
            Value::String("bob@example.com".to_string()),
        );
        fill_data.insert("password".to_string(), Value::String("hashed".to_string()));

        user2.fill(fill_data);
        // id 应保持 0（被过滤）
        assert_eq!(user2.id, 0);
        assert_eq!(user2.name, "Bob");
        assert_eq!(user2.email, "bob@example.com");
    }

    // ============= ActiveRecord 测试 =============

    use crate::pool::Connection;
    use std::pin::Pin;

    /// 模拟数据库连接，用于测试关系加载
    struct MockConnection {
        query_results: HashMap<String, Vec<HashMap<String, Value>>>,
    }

    impl Connection for MockConnection {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<u64, crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(1) })
        }

        fn query<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Vec<HashMap<String, Value>>, crate::DbError>,
                    > + Send
                    + 'a,
            >,
        > {
            let result = self.query_results.get(sql).cloned().unwrap_or_default();
            Box::pin(async move { Ok(result) })
        }

        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }

        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }

        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }

        fn is_connected(&self) -> bool {
            true
        }

        fn ping<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }

        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    /// 测试用的 UserModel（带关系支持）
    #[derive(Clone)]
    #[allow(dead_code)]
    struct UserModel {
        id: i64,
        name: String,
        email: String,
        password: String,
        team_id: i64,
        relations: HashMap<String, Value>,
    }

    impl Model for UserModel {
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

    impl ModelExt for UserModel {
        fn columns() -> Vec<&'static str> {
            vec!["id", "name", "email", "team_id"]
        }
        fn fillable() -> Vec<&'static str> {
            vec!["name", "email"]
        }
        fn hidden() -> Vec<&'static str> {
            vec!["password"]
        }
        fn relations() -> HashMap<&'static str, Relation> {
            let mut map = HashMap::new();
            map.insert(
                "orders",
                Relation::HasMany(HasMany {
                    foreign_key: "user_id".to_string(),
                    child_model: "orders".to_string(),
                    child_pk: "id".to_string(),
                }),
            );
            map.insert(
                "profile",
                Relation::HasOne(HasOne {
                    foreign_key: "user_id".to_string(),
                    child_model: "profiles".to_string(),
                    child_pk: "id".to_string(),
                }),
            );
            map.insert(
                "team",
                Relation::BelongsTo(BelongsTo {
                    foreign_key: "team_id".to_string(),
                    parent_model: "teams".to_string(),
                    parent_pk: "id".to_string(),
                }),
            );
            map.insert(
                "roles",
                Relation::BelongsToMany(BelongsToMany {
                    junction_table: "user_roles".to_string(),
                    foreign_key: "user_id".to_string(),
                    other_key: "role_id".to_string(),
                    target_model: "roles".to_string(),
                }),
            );
            map
        }
        fn get_column_value(&self, column: &str) -> Option<Value> {
            match column {
                "id" => Some(Value::I64(self.id)),
                "name" => Some(Value::String(self.name.clone())),
                "email" => Some(Value::String(self.email.clone())),
                "team_id" => Some(Value::I64(self.team_id)),
                _ => None,
            }
        }
        fn from_value(&mut self, map: HashMap<String, Value>) {
            if let Some(Value::I64(id)) = map.get("id") {
                self.id = *id;
            }
            if let Some(Value::String(name)) = map.get("name") {
                self.name = name.clone();
            }
            if let Some(Value::String(email)) = map.get("email") {
                self.email = email.clone();
            }
            if let Some(Value::I64(tid)) = map.get("team_id") {
                self.team_id = *tid;
            }
        }
    }

    impl RelationLoader for UserModel {
        fn get_relation(&self, name: &str) -> Option<&Value> {
            self.relations.get(name)
        }
        fn set_relation_data(&mut self, name: &str, data: Value) {
            self.relations.insert(name.to_string(), data);
        }
        fn get_relation_fk_value(&self, fk_name: &str) -> String {
            match fk_name {
                "user_id" => format!("{}", self.id),
                "team_id" => format!("{}", self.team_id),
                _ => "0".to_string(),
            }
        }
    }

    impl ActiveRecord for UserModel {}
    impl RelationAccess for UserModel {}

    fn make_user() -> UserModel {
        UserModel {
            id: 1,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            password: "secret".to_string(),
            team_id: 10,
            relations: HashMap::new(),
        }
    }

    fn make_order_row(id: i64, user_id: i64, total: &str) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(id));
        row.insert("user_id".to_string(), Value::I64(user_id));
        row.insert("total".to_string(), Value::String(total.to_string()));
        row
    }

    fn make_profile_row(user_id: i64, bio: &str) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(100));
        row.insert("user_id".to_string(), Value::I64(user_id));
        row.insert("bio".to_string(), Value::String(bio.to_string()));
        row
    }

    fn make_team_row(id: i64, name: &str) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(id));
        row.insert("name".to_string(), Value::String(name.to_string()));
        row
    }

    fn make_role_row(id: i64, name: &str) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(id));
        row.insert("name".to_string(), Value::String(name.to_string()));
        row
    }

    #[tokio::test]
    async fn test_active_record_with_has_many() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM orders WHERE user_id = '1'".to_string(),
                    vec![
                        make_order_row(1, 1, "99.99"),
                        make_order_row(2, 1, "149.50"),
                    ],
                );
                m
            },
        };

        let user = user.with("orders").load(&mut conn).await.unwrap();
        let data = user.get_relation("orders");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected Array");
        }
    }

    #[tokio::test]
    async fn test_active_record_with_has_one() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM profiles WHERE user_id = '1'".to_string(),
                    vec![make_profile_row(1, "Hello world")],
                );
                m
            },
        };

        let user = user.with("profile").load(&mut conn).await.unwrap();
        let data = user.get_relation("profile");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_active_record_with_belongs_to() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM teams WHERE id = '10'".to_string(),
                    vec![make_team_row(10, "Engineering")],
                );
                m
            },
        };

        let user = user.with("team").load(&mut conn).await.unwrap();
        let data = user.get_relation("team");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_active_record_with_belongs_to_many() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT t.* FROM roles t INNER JOIN user_roles j ON t.role_id = j.role_id WHERE j.user_id = '1'".to_string(),
                    vec![
                        make_role_row(1, "admin"),
                        make_role_row(2, "editor"),
                    ],
                );
                m
            },
        };

        let user = user.with("roles").load(&mut conn).await.unwrap();
        let data = user.get_relation("roles");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 2);
        }
    }

    #[tokio::test]
    async fn test_active_record_with_all() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM orders WHERE user_id = '1'".to_string(),
                    vec![make_order_row(1, 1, "99.99")],
                );
                m.insert(
                    "SELECT * FROM profiles WHERE user_id = '1'".to_string(),
                    vec![make_profile_row(1, "Bio")],
                );
                m
            },
        };

        let user = user
            .with_all(vec!["orders", "profile"])
            .load(&mut conn)
            .await
            .unwrap();

        assert!(user.get_relation("orders").is_some());
        assert!(user.get_relation("profile").is_some());
    }

    #[tokio::test]
    async fn test_active_record_relation_not_found() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: HashMap::new(),
        };

        let result = user.with("nonexistent").load(&mut conn).await;
        assert!(result.is_err());
        match result {
            Err(RelationError::RelationNotFound(name)) => {
                assert_eq!(name, "nonexistent");
            }
            _ => panic!("Expected RelationNotFound"),
        }
    }

    #[test]
    fn test_active_record_not_loaded() {
        let user = make_user();
        let result = user.get_has_many("orders");
        assert!(result.is_err());
        match result {
            Err(RelationError::NotLoaded(name)) => {
                assert_eq!(name, "orders");
            }
            _ => panic!("Expected NotLoaded"),
        }
    }

    #[test]
    fn test_rows_to_values_empty() {
        let rows: Vec<HashMap<String, Value>> = vec![];
        let result = rows_to_values(rows);
        assert_eq!(result, Value::Array(vec![]));
    }

    #[test]
    fn test_rows_to_values_with_data() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("test".to_string()));
        let rows = vec![row];
        let result = rows_to_values(rows);

        match &result {
            Value::Array(items) => {
                assert_eq!(items.len(), 1);
                assert!(items[0].is_object());
            }
            _ => panic!("Expected Array"),
        }
    }

    #[tokio::test]
    async fn test_relation_access_has_many() {
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM orders WHERE user_id = '1'".to_string(),
                    vec![
                        make_order_row(1, 1, "99.99"),
                        make_order_row(2, 1, "149.50"),
                    ],
                );
                m
            },
        };

        let user = make_user().with("orders").load(&mut conn).await.unwrap();
        let orders = user.get_has_many("orders").unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].get("total").unwrap(), &Value::String("99.99".to_string()));
    }

    #[tokio::test]
    async fn test_relation_access_has_one() {
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM profiles WHERE user_id = '1'".to_string(),
                    vec![make_profile_row(1, "My bio")],
                );
                m
            },
        };

        let user = make_user().with("profile").load(&mut conn).await.unwrap();
        let profile = user.get_has_one("profile").unwrap();
        assert!(profile.is_some());
        assert_eq!(
            profile.unwrap().get("bio").unwrap(),
            &Value::String("My bio".to_string())
        );
    }

    #[test]
    fn test_value_object() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), Value::String("value".to_string()));
        let obj = Value::from_map(map);
        assert!(obj.is_object());

        if let Value::Object(m) = &obj {
            assert_eq!(m.get("key").unwrap(), &Value::String("value".to_string()));
        } else {
            panic!("Expected Object");
        }
    }
}
