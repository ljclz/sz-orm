//! PyQueryBuilder — SQL 构建器
//!
//! 使用 `&mut self` 方法（非链式返回 Self），因为 PyO3 pyclass 对象是共享的。
//! Python 侧用法：qb = QueryBuilder(); qb.table("users"); qb.where_eq("id", 1)

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, Value};

use crate::types::{py_to_value, value_to_py};

#[pyclass(name = "QueryBuilder")]
pub struct PyQueryBuilder {
    db_type: DbType,
    table: Option<String>,
    select_columns: Vec<String>,
    where_clauses: Vec<(String, Value, bool)>,
    order_by: Vec<(String, bool)>,
    limit_val: Option<usize>,
    offset_val: Option<usize>,
}

fn dialect_or_err(db_type: DbType) -> PyResult<Box<dyn sz_orm_core::dialect::Dialect>> {
    get_dialect(db_type).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[pymethods]
impl PyQueryBuilder {
    #[new]
    #[pyo3(signature = (db_type="mysql".to_string()))]
    fn new(db_type: Option<String>) -> PyResult<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        let db_type = DbType::from_str(&dt).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown DbType: {}", dt))
        })?;
        Ok(Self {
            db_type,
            table: None,
            select_columns: vec![],
            where_clauses: vec![],
            order_by: vec![],
            limit_val: None,
            offset_val: None,
        })
    }

    fn set_table(&mut self, table: &str) {
        self.table = Some(table.to_string());
    }

    fn set_select(&mut self, columns: Vec<String>) {
        self.select_columns = columns;
    }

    fn where_eq(&mut self, py: Python, field: &str, value: PyObject) -> PyResult<()> {
        let v = py_to_value(py, value)?;
        self.where_clauses.push((field.to_string(), v, false));
        Ok(())
    }

    fn or_where_eq(&mut self, py: Python, field: &str, value: PyObject) -> PyResult<()> {
        let v = py_to_value(py, value)?;
        self.where_clauses.push((field.to_string(), v, true));
        Ok(())
    }

    fn add_order_by(&mut self, field: &str) {
        self.order_by.push((field.to_string(), false));
    }

    fn add_order_desc(&mut self, field: &str) {
        self.order_by.push((field.to_string(), true));
    }

    fn set_limit(&mut self, limit: usize) {
        self.limit_val = Some(limit);
    }

    fn set_offset(&mut self, offset: usize) {
        self.offset_val = Some(offset);
    }

    fn build_select(&self, py: Python) -> PyResult<PyObject> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = self
            .table
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("table not set"))?;

        let cols = if self.select_columns.is_empty() {
            "*".to_string()
        } else {
            self.select_columns
                .iter()
                .map(|c| dialect.quote(c))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let mut sql = format!("SELECT {} FROM {}", cols, dialect.quote(table));
        let mut params: Vec<PyObject> = vec![];

        if !self.where_clauses.is_empty() {
            let mut clauses = vec![];
            for (i, (field, value, is_or)) in self.where_clauses.iter().enumerate() {
                let connector = if *is_or {
                    " OR "
                } else if i == 0 {
                    ""
                } else {
                    " AND "
                };
                clauses.push(format!("{}{} = ?", connector, dialect.quote(field)));
                params.push(value_to_py(py, value));
            }
            sql.push_str(&format!(" WHERE {}", clauses.join("")));
        }

        if !self.order_by.is_empty() {
            let orders: Vec<String> = self
                .order_by
                .iter()
                .map(|(f, desc)| {
                    if *desc {
                        format!("{} DESC", dialect.quote(f))
                    } else {
                        dialect.quote(f)
                    }
                })
                .collect();
            sql.push_str(&format!(" ORDER BY {}", orders.join(", ")));
        }

        if let Some(limit) = self.limit_val {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset_val {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let sql_obj: PyObject = sql.into_py(py);
        let params_list = PyList::new(py, &params);
        Ok(PyTuple::new(py, &[sql_obj, params_list.into()]).into())
    }

    fn build_insert(&self, py: Python, data: &PyDict) -> PyResult<PyObject> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = self
            .table
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("table not set"))?;

        let mut cols = vec![];
        let mut placeholders = vec![];
        let mut params: Vec<PyObject> = vec![];

        for (key, value) in data.iter() {
            let col_name: String = key.extract()?;
            cols.push(dialect.quote(&col_name));
            placeholders.push("?".to_string());
            let v = py_to_value(py, value.into_py(py))?;
            params.push(value_to_py(py, &v));
        }

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            dialect.quote(table),
            cols.join(", "),
            placeholders.join(", ")
        );

        let sql_obj: PyObject = sql.into_py(py);
        let params_list = PyList::new(py, &params);
        Ok(PyTuple::new(py, &[sql_obj, params_list.into()]).into())
    }

    fn build_update(&self, py: Python, data: &PyDict) -> PyResult<PyObject> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = self
            .table
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("table not set"))?;

        let mut sets = vec![];
        let mut params: Vec<PyObject> = vec![];

        for (key, value) in data.iter() {
            let col_name: String = key.extract()?;
            sets.push(format!("{} = ?", dialect.quote(&col_name)));
            let v = py_to_value(py, value.into_py(py))?;
            params.push(value_to_py(py, &v));
        }

        let mut sql = format!("UPDATE {} SET {}", dialect.quote(table), sets.join(", "));

        if !self.where_clauses.is_empty() {
            let mut clauses = vec![];
            for (i, (field, value, is_or)) in self.where_clauses.iter().enumerate() {
                let connector = if *is_or {
                    " OR "
                } else if i == 0 {
                    ""
                } else {
                    " AND "
                };
                clauses.push(format!("{}{} = ?", connector, dialect.quote(field)));
                params.push(value_to_py(py, value));
            }
            sql.push_str(&format!(" WHERE {}", clauses.join("")));
        }

        let sql_obj: PyObject = sql.into_py(py);
        let params_list = PyList::new(py, &params);
        Ok(PyTuple::new(py, &[sql_obj, params_list.into()]).into())
    }

    fn build_delete(&self, py: Python) -> PyResult<PyObject> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = self
            .table
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("table not set"))?;

        let mut sql = format!("DELETE FROM {}", dialect.quote(table));
        let mut params: Vec<PyObject> = vec![];

        if !self.where_clauses.is_empty() {
            let mut clauses = vec![];
            for (i, (field, value, is_or)) in self.where_clauses.iter().enumerate() {
                let connector = if *is_or {
                    " OR "
                } else if i == 0 {
                    ""
                } else {
                    " AND "
                };
                clauses.push(format!("{}{} = ?", connector, dialect.quote(field)));
                params.push(value_to_py(py, value));
            }
            sql.push_str(&format!(" WHERE {}", clauses.join("")));
        }

        let sql_obj: PyObject = sql.into_py(py);
        let params_list = PyList::new(py, &params);
        Ok(PyTuple::new(py, &[sql_obj, params_list.into()]).into())
    }
}
