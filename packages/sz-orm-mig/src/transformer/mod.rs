use super::RowData;
use crate::error::MigError;
use std::collections::HashMap;

pub trait DataTransformer: Send + Sync {
    fn transform(&self, row: RowData) -> Result<RowData, MigError>;
    fn transform_batch(&self, rows: Vec<RowData>) -> Result<Vec<RowData>, MigError>;
}

pub struct TypeTransformer;

impl TypeTransformer {
    pub fn new() -> Self {
        Self
    }

    pub fn mysql_to_pg_value(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => {
                if s.eq_ignore_ascii_case("true") {
                    serde_json::Value::Bool(true)
                } else if s.eq_ignore_ascii_case("false") {
                    serde_json::Value::Bool(false)
                } else if let Ok(num) = s.parse::<i64>() {
                    serde_json::Value::Number(num.into())
                } else if let Ok(num) = s.parse::<f64>() {
                    serde_json::Number::from_f64(num)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::String(s))
                } else {
                    serde_json::Value::String(s)
                }
            }
            other => other,
        }
    }

    pub fn pg_to_mysql_value(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Bool(b) => {
                serde_json::Value::String(if b { "1".to_string() } else { "0".to_string() })
            }
            serde_json::Value::Number(n) => serde_json::Value::String(n.to_string()),
            serde_json::Value::Array(arr) => {
                serde_json::Value::String(serde_json::to_string(&arr).unwrap_or_default())
            }
            serde_json::Value::Object(obj) => {
                serde_json::Value::String(serde_json::to_string(&obj).unwrap_or_default())
            }
            other => other,
        }
    }

    pub fn sqlite_to_mysql_value(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Number(ref n) => {
                if let Some(i) = n.as_i64() {
                    serde_json::Value::String(i.to_string())
                } else if let Some(f) = n.as_f64() {
                    serde_json::Value::String(f.to_string())
                } else {
                    serde_json::Value::Number(n.clone())
                }
            }
            serde_json::Value::Bool(b) => {
                serde_json::Value::String(if b { "1".to_string() } else { "0".to_string() })
            }
            serde_json::Value::Null => serde_json::Value::String("NULL".to_string()),
            other => other,
        }
    }
}

impl Default for TypeTransformer {
    fn default() -> Self {
        Self::new()
    }
}

impl DataTransformer for TypeTransformer {
    fn transform(&self, row: RowData) -> Result<RowData, MigError> {
        let mut new_data = HashMap::new();
        for (key, value) in row.data {
            let transformed = self.mysql_to_pg_value(value);
            new_data.insert(key, transformed);
        }
        Ok(RowData { data: new_data })
    }

    fn transform_batch(&self, rows: Vec<RowData>) -> Result<Vec<RowData>, MigError> {
        rows.into_iter().map(|row| self.transform(row)).collect()
    }
}

pub struct ColumnMapper {
    mappings: HashMap<String, String>,
}

impl ColumnMapper {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn map(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.mappings.insert(from.into(), to.into());
        self
    }

    pub fn transform(&self, row: RowData) -> RowData {
        let mut new_data = HashMap::new();
        for (key, value) in row.data {
            let new_key = self.mappings.get(&key).cloned().unwrap_or(key);
            new_data.insert(new_key, value);
        }
        RowData { data: new_data }
    }
}

impl Default for ColumnMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl DataTransformer for ColumnMapper {
    fn transform(&self, row: RowData) -> Result<RowData, MigError> {
        Ok(self.transform(row))
    }

    fn transform_batch(&self, rows: Vec<RowData>) -> Result<Vec<RowData>, MigError> {
        Ok(rows.into_iter().map(|row| self.transform(row)).collect())
    }
}

pub struct ChainTransformer {
    transformers: Vec<Box<dyn DataTransformer>>,
}

impl ChainTransformer {
    pub fn new() -> Self {
        Self {
            transformers: Vec::new(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add<T: DataTransformer + 'static>(mut self, transformer: T) -> Self {
        self.transformers.push(Box::new(transformer));
        self
    }

    pub fn transform(&self, row: RowData) -> Result<RowData, MigError> {
        let mut result = row;
        for transformer in &self.transformers {
            result = transformer.transform(result)?;
        }
        Ok(result)
    }

    pub fn transform_batch(&self, rows: Vec<RowData>) -> Result<Vec<RowData>, MigError> {
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(self.transform(row)?);
        }
        Ok(results)
    }
}

impl Default for ChainTransformer {
    fn default() -> Self {
        Self::new()
    }
}

impl DataTransformer for ChainTransformer {
    fn transform(&self, row: RowData) -> Result<RowData, MigError> {
        self.transform(row)
    }

    fn transform_batch(&self, rows: Vec<RowData>) -> Result<Vec<RowData>, MigError> {
        self.transform_batch(rows)
    }
}

pub struct FilterTransformer {
    include_columns: Option<Vec<String>>,
    exclude_columns: Vec<String>,
}

impl FilterTransformer {
    pub fn new() -> Self {
        Self {
            include_columns: None,
            exclude_columns: Vec::new(),
        }
    }

    pub fn include(mut self, columns: Vec<String>) -> Self {
        self.include_columns = Some(columns);
        self
    }

    pub fn exclude(mut self, columns: Vec<String>) -> Self {
        self.exclude_columns = columns;
        self
    }

    fn should_include(&self, column: &str) -> bool {
        if let Some(ref includes) = self.include_columns {
            return includes.contains(&column.to_string());
        }
        !self.exclude_columns.contains(&column.to_string())
    }
}

impl Default for FilterTransformer {
    fn default() -> Self {
        Self::new()
    }
}

impl DataTransformer for FilterTransformer {
    fn transform(&self, row: RowData) -> Result<RowData, MigError> {
        let mut new_data = HashMap::new();
        for (key, value) in row.data {
            if self.should_include(&key) {
                new_data.insert(key, value);
            }
        }
        Ok(RowData { data: new_data })
    }

    fn transform_batch(&self, rows: Vec<RowData>) -> Result<Vec<RowData>, MigError> {
        rows.into_iter().map(|row| self.transform(row)).collect()
    }
}
