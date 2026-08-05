//! QueryBuilder — SQL 构建器

use napi_derive::napi;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, Value};

type Result<T> = napi::bindgen_prelude::Result<T>;

#[napi]
pub struct QueryBuilder {
    db_type: DbType,
    table: Option<String>,
    select_columns: Vec<String>,
    where_clauses: Vec<(String, Value, bool)>,
    order_by: Vec<(String, bool)>,
    limit_val: Option<u32>,
    offset_val: Option<u32>,
}

#[napi(object)]
pub struct SqlWithParams {
    pub sql: String,
    pub params: Vec<String>,
}

fn parse_db_type(s: &str) -> Result<DbType> {
    DbType::from_str(s).ok_or_else(|| napi::Error::from_reason(format!("unknown DbType: {}", s)))
}

fn dialect_or_err(db_type: DbType) -> Result<Box<dyn sz_orm_core::dialect::Dialect>> {
    get_dialect(db_type).map_err(|e| napi::Error::from_reason(e.to_string()))
}

#[napi]
impl QueryBuilder {
    #[napi(constructor)]
    pub fn new(db_type: Option<String>) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        let db_type = parse_db_type(&dt)?;
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

    #[napi]
    pub fn set_table(&mut self, table: String) {
        self.table = Some(table);
    }

    #[napi]
    pub fn set_select(&mut self, columns: Vec<String>) {
        self.select_columns = columns;
    }

    #[napi]
    pub fn where_eq_str(&mut self, field: String, value: String) {
        self.where_clauses
            .push((field, Value::String(value), false));
    }

    #[napi]
    pub fn where_eq_i64(&mut self, field: String, value: i64) {
        self.where_clauses.push((field, Value::I64(value), false));
    }

    #[napi]
    pub fn where_eq_f64(&mut self, field: String, value: f64) {
        self.where_clauses.push((field, Value::F64(value), false));
    }

    #[napi]
    pub fn where_eq_bool(&mut self, field: String, value: bool) {
        self.where_clauses.push((field, Value::Bool(value), false));
    }

    #[napi]
    pub fn or_where_eq_str(&mut self, field: String, value: String) {
        self.where_clauses.push((field, Value::String(value), true));
    }

    #[napi]
    pub fn or_where_eq_i64(&mut self, field: String, value: i64) {
        self.where_clauses.push((field, Value::I64(value), true));
    }

    #[napi]
    pub fn add_order_by(&mut self, field: String) {
        self.order_by.push((field, false));
    }

    #[napi]
    pub fn add_order_desc(&mut self, field: String) {
        self.order_by.push((field, true));
    }

    #[napi]
    pub fn set_limit(&mut self, limit: u32) {
        self.limit_val = Some(limit);
    }

    #[napi]
    pub fn set_offset(&mut self, offset: u32) {
        self.offset_val = Some(offset);
    }

    fn build_where_clause(
        &self,
        dialect: &dyn sz_orm_core::dialect::Dialect,
    ) -> (String, Vec<String>) {
        let mut sql = String::new();
        let mut params: Vec<String> = vec![];
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
                params.push(crate::types::value_to_json_string(value));
            }
            sql.push_str(&format!(" WHERE {}", clauses.join("")));
        }
        (sql, params)
    }

    #[napi]
    pub fn build_select(&self) -> Result<SqlWithParams> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = self
            .table
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("table not set"))?;

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
        let (where_sql, params) = self.build_where_clause(&*dialect);
        sql.push_str(&where_sql);

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

        Ok(SqlWithParams { sql, params })
    }

    #[napi]
    pub fn build_delete(&self) -> Result<SqlWithParams> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = self
            .table
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("table not set"))?;

        let mut sql = format!("DELETE FROM {}", dialect.quote(table));
        let (where_sql, params) = self.build_where_clause(&*dialect);
        sql.push_str(&where_sql);

        Ok(SqlWithParams { sql, params })
    }
}
