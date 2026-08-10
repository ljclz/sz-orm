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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pydb_error_new() {
        let err = PyDbError::new("test error".to_string());
        assert_eq!(err.message, "test error");
        assert_eq!(err.code, "DB000");
    }

    #[test]
    fn test_pydb_error_getters() {
        let err = PyDbError::new("connection failed".to_string());
        assert_eq!(err.message(), "connection failed");
        assert_eq!(err.code(), "DB000");
    }

    #[test]
    fn test_pydb_error_str() {
        let err = PyDbError::new("not found".to_string());
        assert_eq!(err.__str__(), "[DB000] not found");
    }

    #[test]
    fn test_pydb_error_clone() {
        let err = PyDbError::new("original".to_string());
        let cloned = err.clone();
        assert_eq!(cloned.message, "original");
        assert_eq!(cloned.code, "DB000");
    }
}
