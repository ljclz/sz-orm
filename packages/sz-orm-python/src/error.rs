//! DbError → PyErr 映射

use pyo3::exceptions::{PyConnectionError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use sz_orm_core::DbError;

#[pyclass(name = "DbError")]
#[derive(Clone)]
pub struct PyDbError {
    pub message: String,
    pub code: String,
}

#[pymethods]
impl PyDbError {
    #[new]
    fn new(message: String) -> Self {
        Self {
            message,
            code: "DB000".to_string(),
        }
    }

    #[getter]
    fn message(&self) -> &str {
        &self.message
    }

    #[getter]
    fn code(&self) -> &str {
        &self.code
    }

    fn __str__(&self) -> String {
        format!("[{}] {}", self.code, self.message)
    }
}

#[allow(dead_code)]
pub fn db_error_to_pyerr(err: DbError) -> PyErr {
    let code = err.error_code().to_string();
    let msg = err.to_string();
    match &err {
        DbError::ConnectionRefused(_) | DbError::ConnectionError(_) => {
            PyConnectionError::new_err(format!("[{}] {}", code, msg))
        }
        DbError::InvalidInput(_) | DbError::Validation(_) => {
            PyValueError::new_err(format!("[{}] {}", code, msg))
        }
        _ => PyRuntimeError::new_err(format!("[{}] {}", code, msg)),
    }
}
