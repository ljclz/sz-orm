//! PyPaginator — 分页器包装器

use pyo3::prelude::*;

/// Python 分页器
#[pyclass(name = "Paginator")]
#[derive(Clone)]
pub struct PyPaginator {
    total: u64,
    page: u64,
    per_page: u64,
}

#[pymethods]
impl PyPaginator {
    #[new]
    #[pyo3(signature = (total=0, page=1, per_page=10))]
    fn new(total: Option<u64>, page: Option<u64>, per_page: Option<u64>) -> Self {
        Self {
            total: total.unwrap_or(0),
            page: page.unwrap_or(1),
            per_page: per_page.unwrap_or(10),
        }
    }

    #[getter]
    fn total(&self) -> u64 {
        self.total
    }

    #[setter]
    fn set_total(&mut self, value: u64) {
        self.total = value;
    }

    #[getter]
    fn page(&self) -> u64 {
        self.page
    }

    #[getter]
    fn per_page(&self) -> u64 {
        self.per_page
    }

    /// 总页数
    fn total_pages(&self) -> u64 {
        if self.per_page == 0 {
            return 0;
        }
        self.total.div_ceil(self.per_page)
    }

    /// 是否有下一页
    fn has_next(&self) -> bool {
        self.page < self.total_pages()
    }

    /// 是否有上一页
    fn has_prev(&self) -> bool {
        self.page > 1
    }

    /// OFFSET 值
    fn offset(&self) -> u64 {
        (self.page.saturating_sub(1)) * self.per_page
    }

    /// LIMIT 值
    fn limit(&self) -> u64 {
        self.per_page
    }

    /// 从 Python 列表构建分页切片
    fn slice_data(&self, py: Python, data: &pyo3::types::PyList) -> PyObject {
        let start = self.offset() as usize;
        let per_page = self.per_page as usize;
        let end = (start + per_page).min(data.len());
        let list = pyo3::types::PyList::empty(py);
        if start < data.len() {
            for i in start..end {
                if let Ok(item) = data.get_item(i) {
                    list.append(item).unwrap();
                }
            }
        }
        list.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "Paginator(total={}, page={}, per_page={}, total_pages={})",
            self.total,
            self.page,
            self.per_page,
            self.total_pages()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paginator_new() {
        let p = PyPaginator::new(Some(100), Some(1), Some(10));
        assert_eq!(p.total, 100);
        assert_eq!(p.page, 1);
        assert_eq!(p.per_page, 10);
    }

    #[test]
    fn test_paginator_total_pages() {
        let p = PyPaginator::new(Some(25), Some(1), Some(10));
        assert_eq!(p.total_pages(), 3);
        assert!(p.has_next());
        assert!(!p.has_prev());
    }

    #[test]
    fn test_paginator_offset() {
        let p = PyPaginator::new(Some(100), Some(3), Some(10));
        assert_eq!(p.offset(), 20);
        assert_eq!(p.limit(), 10);
    }

    #[test]
    fn test_paginator_empty() {
        let p = PyPaginator::new(None, None, None);
        assert_eq!(p.total, 0);
        assert_eq!(p.total_pages(), 0);
    }
}
