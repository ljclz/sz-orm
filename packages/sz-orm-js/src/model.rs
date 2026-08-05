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
