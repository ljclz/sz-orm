//! PyActiveModel — 活跃模型包装器，支持三态字段更新

use pyo3::prelude::*;
use std::collections::HashMap;
use sz_orm_core::Value;

use crate::types::{py_to_value, value_to_py};

/// 字段三态：Set / Unchanged / NotSet
#[derive(Debug, Clone, PartialEq, Default)]
enum PyActiveValue {
    Set(Value),
    Unchanged,
    #[default]
    NotSet,
}

/// Python 活跃模型，支持 dirty tracking
#[pyclass(name = "ActiveModel")]
#[derive(Clone)]
pub struct PyActiveModel {
    table_name: String,
    pk_name: String,
    fields: HashMap<String, PyActiveValue>,
    is_new_record: bool,
}

#[pymethods]
impl PyActiveModel {
    #[new]
    #[pyo3(signature = (table_name, pk_name="id".to_string(), is_new=true))]
    fn new(table_name: String, pk_name: Option<String>, is_new: Option<bool>) -> Self {
        Self {
            table_name,
            pk_name: pk_name.unwrap_or_else(|| "id".to_string()),
            fields: HashMap::new(),
            is_new_record: is_new.unwrap_or(true),
        }
    }

    #[getter]
    fn table_name(&self) -> &str {
        &self.table_name
    }

    #[getter]
    fn pk_name(&self) -> &str {
        &self.pk_name
    }

    /// 设置字段值（标记为 Set）
    fn set(&mut self, py: Python, key: &str, value: PyObject) -> PyResult<()> {
        let v = py_to_value(py, value)?;
        self.fields.insert(key.to_string(), PyActiveValue::Set(v));
        Ok(())
    }

    /// 获取字段值
    fn get(&self, py: Python, key: &str) -> PyObject {
        match self.fields.get(key) {
            Some(PyActiveValue::Set(v)) => value_to_py(py, v),
            Some(PyActiveValue::Unchanged) => py.None(),
            Some(PyActiveValue::NotSet) | None => py.None(),
        }
    }

    /// 标记字段为未修改
    fn mark_unchanged(&mut self, key: &str) {
        self.fields
            .insert(key.to_string(), PyActiveValue::Unchanged);
    }

    /// 判断是否为新记录
    fn is_new(&self) -> bool {
        self.is_new_record
    }

    /// 判断是否有已修改字段
    fn is_modified(&self) -> bool {
        self.fields
            .values()
            .any(|v| matches!(v, PyActiveValue::Set(_)))
    }

    /// 返回已修改字段名列表
    fn changed_fields(&self) -> Vec<String> {
        self.fields
            .iter()
            .filter_map(|(k, v)| {
                if matches!(v, PyActiveValue::Set(_)) {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// 返回所有字段字典
    fn to_dict(&self, py: Python) -> PyObject {
        let dict = pyo3::types::PyDict::new(py);
        for (k, v) in &self.fields {
            if let PyActiveValue::Set(val) = v {
                dict.set_item(k, value_to_py(py, val)).unwrap();
            }
        }
        dict.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "ActiveModel(table={}, pk={}, modified={}, is_new={})",
            self.table_name,
            self.pk_name,
            self.is_modified(),
            self.is_new_record,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_model_new() {
        let m = PyActiveModel::new("users".to_string(), None, None);
        assert_eq!(m.table_name, "users");
        assert_eq!(m.pk_name, "id");
        assert!(m.is_new());
        assert!(!m.is_modified());
    }

    #[test]
    fn test_active_model_modified() {
        let mut m = PyActiveModel::new("users".to_string(), None, None);
        m.fields.insert(
            "name".to_string(),
            PyActiveValue::Set(Value::String("Alice".into())),
        );
        assert!(m.is_modified());
        assert_eq!(m.changed_fields(), vec!["name".to_string()]);
    }

    #[test]
    fn test_active_model_unchanged() {
        let mut m = PyActiveModel::new("users".to_string(), None, None);
        m.fields
            .insert("name".to_string(), PyActiveValue::Unchanged);
        assert!(!m.is_modified());
        assert!(m.changed_fields().is_empty());
    }
}
