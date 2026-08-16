//! Model — 通用模型包装器

use napi_derive::napi;
use std::collections::HashMap;
use sz_orm_core::Value;

#[napi]
pub struct Model {
    table_name: String,
    pk_name: String,
    fields: HashMap<String, Value>,
}

#[napi]
impl Model {
    #[napi(constructor)]
    pub fn new(table_name: String, pk_name: Option<String>) -> Self {
        Self {
            table_name,
            pk_name: pk_name.unwrap_or_else(|| "id".to_string()),
            fields: HashMap::new(),
        }
    }

    #[napi(getter)]
    pub fn table_name(&self) -> String {
        self.table_name.clone()
    }

    #[napi(getter)]
    pub fn pk_name(&self) -> String {
        self.pk_name.clone()
    }

    #[napi]
    pub fn set_str(&mut self, key: String, value: String) {
        self.fields.insert(key, Value::String(value));
    }

    #[napi]
    pub fn set_i64(&mut self, key: String, value: i64) {
        self.fields.insert(key, Value::I64(value));
    }

    #[napi]
    pub fn set_f64(&mut self, key: String, value: f64) {
        self.fields.insert(key, Value::F64(value));
    }

    #[napi]
    pub fn set_bool(&mut self, key: String, value: bool) {
        self.fields.insert(key, Value::Bool(value));
    }

    #[napi]
    pub fn set_null(&mut self, key: String) {
        self.fields.insert(key, Value::Null);
    }

    #[napi]
    pub fn to_json_string(&self) -> String {
        let obj: HashMap<String, Value> = self.fields.clone();
        serde_json::to_string(&crate::types::value_to_json(&Value::Object(obj)))
            .unwrap_or_else(|_| "{}".to_string())
    }

    #[napi]
    pub fn status(&self) -> String {
        format!(
            "Model(table={}, pk={}, fields={})",
            self.table_name,
            self.pk_name,
            self.fields.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_default_pk() {
        let m = Model::new("users".to_string(), None);
        assert_eq!(m.table_name(), "users");
        assert_eq!(m.pk_name(), "id");
    }

    #[test]
    fn model_custom_pk() {
        let m = Model::new("orders".to_string(), Some("order_id".to_string()));
        assert_eq!(m.pk_name(), "order_id");
    }

    #[test]
    fn model_set_and_json() {
        let mut m = Model::new("users".to_string(), None);
        m.set_str("name".to_string(), "Alice".to_string());
        m.set_i64("age".to_string(), 30);
        m.set_bool("vip".to_string(), true);
        let json = m.to_json_string();
        assert!(json.contains("Alice"), "json should contain name: {json}");
        assert!(json.contains("30"), "json should contain age: {json}");
        assert!(json.contains("true"), "json should contain vip: {json}");
    }

    #[test]
    fn model_set_null() {
        let mut m = Model::new("users".to_string(), None);
        m.set_null("deleted_at".to_string());
        let json = m.to_json_string();
        assert!(json.contains("null"), "null field should serialize: {json}");
    }

    #[test]
    fn model_status_counts_fields() {
        let mut m = Model::new("users".to_string(), None);
        m.set_str("name".to_string(), "Bob".to_string());
        let status = m.status();
        assert!(
            status.contains("users"),
            "status should mention table: {status}"
        );
        assert!(
            status.contains("fields=1"),
            "status should count fields: {status}"
        );
    }

    #[test]
    fn model_empty_json_object() {
        let m = Model::new("users".to_string(), None);
        assert_eq!(m.to_json_string(), "{}");
    }

    #[test]
    fn model_set_f64() {
        let mut m = Model::new("products".to_string(), None);
        m.set_f64("price".to_string(), 19.99);
        let json = m.to_json_string();
        assert!(json.contains("19.99"), "json should contain price: {json}");
    }

    #[test]
    fn model_set_multiple_types() {
        let mut m = Model::new("users".to_string(), None);
        m.set_str("name".to_string(), "Alice".to_string());
        m.set_i64("age".to_string(), 30);
        m.set_f64("score".to_string(), 95.5);
        m.set_bool("active".to_string(), true);
        m.set_null("deleted_at".to_string());
        let json = m.to_json_string();
        assert!(json.contains("Alice"));
        assert!(json.contains("30"));
        assert!(json.contains("95.5"));
        assert!(json.contains("true"));
        assert!(json.contains("null"));
    }

    #[test]
    fn model_overwrite_field() {
        let mut m = Model::new("users".to_string(), None);
        m.set_str("name".to_string(), "Alice".to_string());
        m.set_str("name".to_string(), "Bob".to_string());
        let json = m.to_json_string();
        assert!(json.contains("Bob"));
        assert!(!json.contains("Alice"));
    }

    #[test]
    fn model_status_empty_fields() {
        let m = Model::new("users".to_string(), None);
        let status = m.status();
        assert!(status.contains("fields=0"));
    }
}
