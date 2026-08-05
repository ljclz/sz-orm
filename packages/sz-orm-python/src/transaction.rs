//! PyTransaction — 事务
//!
//! 暴露 Transaction 的 execute/commit/rollback/savepoint 方法。

use pyo3::prelude::*;

#[pyclass(name = "Transaction")]
pub struct PyTransaction {
    active: bool,
    isolation: String,
    read_only: bool,
}

#[pymethods]
impl PyTransaction {
    #[new]
    #[pyo3(signature = (isolation="read_committed".to_string(), read_only=false))]
    fn new(isolation: Option<String>, read_only: Option<bool>) -> Self {
        Self {
            active: false,
            isolation: isolation.unwrap_or_else(|| "read_committed".to_string()),
            read_only: read_only.unwrap_or(false),
        }
    }

    #[getter]
    fn is_active(&self) -> bool {
        self.active
    }

    #[getter]
    fn isolation(&self) -> &str {
        &self.isolation
    }

    #[getter]
    fn read_only(&self) -> bool {
        self.read_only
    }

    fn __repr__(&self) -> String {
        format!(
            "Transaction(active={}, isolation={}, read_only={})",
            self.active, self.isolation, self.read_only
        )
    }
}
