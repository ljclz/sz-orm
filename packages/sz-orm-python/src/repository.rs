//! PyRepository — 仓储模式包装器

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use sz_orm_core::Value;

use crate::types::value_to_py;

/// 分页结果
#[pyclass(name = "PageResult")]
#[derive(Clone)]
pub struct PyPageResult {
    items: Vec<HashMap<String, Value>>,
    total: u64,
    page: u64,
    page_size: u64,
}

#[pymethods]
impl PyPageResult {
    #[getter]
    fn total(&self) -> u64 {
        self.total
    }

    #[getter]
    fn page(&self) -> u64 {
        self.page
    }

    #[getter]
    fn page_size(&self) -> u64 {
        self.page_size
    }

    fn total_pages(&self) -> u64 {
        if self.page_size == 0 {
            return 0;
        }
        self.total.div_ceil(self.page_size)
    }

    fn has_next(&self) -> bool {
        self.page < self.total_pages()
    }

    fn has_prev(&self) -> bool {
        self.page > 1
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn items(&self, py: Python) -> PyObject {
        let list = pyo3::types::PyList::empty(py);
        for row in &self.items {
            let dict = PyDict::new(py);
            for (k, v) in row {
                dict.set_item(k, value_to_py(py, v)).unwrap();
            }
            list.append(dict).unwrap();
        }
        list.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "PageResult(total={}, page={}, page_size={}, items={})",
            self.total,
            self.page,
            self.page_size,
            self.items.len()
        )
    }
}

/// Python 仓储包装器
#[pyclass(name = "Repository")]
#[derive(Clone)]
pub struct PyRepository {
    table_name: String,
    pk_name: String,
}

#[pymethods]
impl PyRepository {
    #[new]
    #[pyo3(signature = (table_name, pk_name="id".to_string()))]
    fn new(table_name: String, pk_name: Option<String>) -> Self {
        Self {
            table_name,
            pk_name: pk_name.unwrap_or_else(|| "id".to_string()),
        }
    }

    #[getter]
    fn table_name(&self) -> &str {
        &self.table_name
    }

    /// 构建分页结果（从 Python 列表转换）
    fn paginate(
        &self,
        py: Python,
        items: &pyo3::types::PyList,
        total: u64,
        page: u64,
        page_size: u64,
    ) -> PyResult<PyPageResult> {
        let mut rows = Vec::new();
        for item in items.iter() {
            let dict = item.downcast::<pyo3::types::PyDict>()?;
            let mut row = HashMap::new();
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                let val = crate::types::py_to_value(py, v.into())?;
                row.insert(key, val);
            }
            rows.push(row);
        }
        Ok(PyPageResult {
            items: rows,
            total,
            page,
            page_size,
        })
    }

    fn __repr__(&self) -> String {
        format!("Repository(table={}, pk={})", self.table_name, self.pk_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_new() {
        let r = PyRepository::new("users".to_string(), None);
        assert_eq!(r.table_name, "users");
    }

    #[test]
    fn test_page_result_total_pages() {
        let r = PyPageResult {
            items: vec![],
            total: 25,
            page: 1,
            page_size: 10,
        };
        assert_eq!(r.total_pages(), 3);
        assert!(r.has_next());
        assert!(!r.has_prev());
    }

    #[test]
    fn test_page_result_empty() {
        let r = PyPageResult {
            items: vec![],
            total: 0,
            page: 1,
            page_size: 10,
        };
        assert!(r.is_empty());
        assert_eq!(r.total_pages(), 0);
    }
}
