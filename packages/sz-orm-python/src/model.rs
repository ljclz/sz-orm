//! PyModel — 通用模型包装器

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use sz_orm_core::Value;

use crate::types::{py_to_value, value_to_py};

#[pyclass(name = "Model")]
#[derive(Clone)]
pub struct PyModel {
    pub table_name: String,
    pub pk_name: String,
    pub fields: HashMap<String, Value>,
}

#[pymethods]
impl PyModel {
    #[new]
    #[pyo3(signature = (table_name, pk_name="id".to_string()))]
    fn new(table_name: String, pk_name: Option<String>) -> Self {
        Self {
            table_name,
            pk_name: pk_name.unwrap_or_else(|| "id".to_string()),
            fields: HashMap::new(),
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

    fn set(&mut self, py: Python, key: &str, value: PyObject) -> PyResult<()> {
        let v = py_to_value(py, value)?;
        self.fields.insert(key.to_string(), v);
        Ok(())
    }

    fn get(&self, py: Python, key: &str) -> PyObject {
        self.fields
            .get(key)
            .map(|v| value_to_py(py, v))
            .unwrap_or_else(|| py.None())
    }

    fn to_dict(&self, py: Python) -> PyObject {
        let dict = PyDict::new(py);
        for (k, v) in &self.fields {
            dict.set_item(k, value_to_py(py, v)).unwrap();
        }
        dict.into()
    }

    fn pk(&self, py: Python) -> PyObject {
        self.get(py, &self.pk_name)
    }

    fn __repr__(&self) -> String {
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
    fn model_default_fields() {
        let m = PyModel {
            table_name: "users".to_string(),
            pk_name: "id".to_string(),
            fields: HashMap::new(),
        };
        assert_eq!(m.table_name, "users");
        assert_eq!(m.pk_name, "id");
        assert_eq!(m.fields.len(), 0);
    }

    #[test]
    fn model_repr_contains_table_and_pk() {
        let m = PyModel {
            table_name: "orders".to_string(),
            pk_name: "order_id".to_string(),
            fields: HashMap::new(),
        };
        let repr = m.__repr__();
        assert!(repr.contains("orders"));
        assert!(repr.contains("order_id"));
        assert!(repr.contains("fields=0"));
    }
}
