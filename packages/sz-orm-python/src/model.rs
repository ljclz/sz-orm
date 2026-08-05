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
