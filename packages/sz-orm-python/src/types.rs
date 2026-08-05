//! Value ↔ Python 类型双向映射

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString};
use sz_orm_core::Value;

#[pyclass(name = "DbType")]
#[derive(Clone)]
pub struct PyDbType {
    pub db_type: sz_orm_core::DbType,
}

#[pymethods]
impl PyDbType {
    #[new]
    fn new(s: &str) -> PyResult<Self> {
        let db_type = sz_orm_core::DbType::from_str(s).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown DbType: {}", s))
        })?;
        Ok(Self { db_type })
    }

    fn __str__(&self) -> &str {
        self.db_type.as_str()
    }
}

pub fn value_to_py(py: Python, v: &Value) -> PyObject {
    match v {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_py(py),
        Value::I8(n) => (*n as i64).into_py(py),
        Value::I16(n) => (*n as i64).into_py(py),
        Value::I32(n) => (*n as i64).into_py(py),
        Value::I64(n) => n.into_py(py),
        Value::U8(n) => (*n as u64).into_py(py),
        Value::U16(n) => (*n as u64).into_py(py),
        Value::U32(n) => (*n as u64).into_py(py),
        Value::U64(n) => n.into_py(py),
        Value::F32(f) => (*f as f64).into_py(py),
        Value::F64(f) => f.into_py(py),
        Value::Decimal(s) => PyString::new(py, s).into(),
        Value::String(s) => PyString::new(py, s).into(),
        Value::Bytes(b) => PyBytes::new(py, b).into(),
        Value::Uuid(s) | Value::Date(s) | Value::DateTime(s) | Value::Time(s) | Value::Json(s) => {
            PyString::new(py, s).into()
        }
        Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(value_to_py(py, item)).unwrap();
            }
            list.into()
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, value_to_py(py, v)).unwrap();
            }
            dict.into()
        }
        _ => py.None(),
    }
}

pub fn py_to_value(py: Python, obj: PyObject) -> PyResult<Value> {
    if obj.is_none(py) {
        return Ok(Value::Null);
    }
    if let Ok(b) = obj.extract::<bool>(py) {
        return Ok(Value::Bool(b));
    }
    if let Ok(n) = obj.extract::<i64>(py) {
        return Ok(Value::I64(n));
    }
    if let Ok(n) = obj.extract::<u64>(py) {
        return Ok(Value::U64(n));
    }
    if let Ok(f) = obj.extract::<f64>(py) {
        return Ok(Value::F64(f));
    }
    if let Ok(s) = obj.extract::<String>(py) {
        return Ok(Value::String(s));
    }
    if let Ok(b) = obj.extract::<Vec<u8>>(py) {
        return Ok(Value::Bytes(b));
    }
    if let Ok(list) = obj.extract::<Vec<PyObject>>(py) {
        let arr: Vec<Value> = list
            .iter()
            .map(|o| py_to_value(py, o.clone()).unwrap_or(Value::Null))
            .collect();
        return Ok(Value::Array(arr));
    }
    Err(pyo3::exceptions::PyValueError::new_err("不支持的类型"))
}
