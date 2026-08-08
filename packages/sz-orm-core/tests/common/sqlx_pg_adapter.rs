//! sqlx PostgreSQL 到 sz_orm_core::Connection 的适配器
//!
//! 将 ? 占位符转换为 PostgreSQL 的 $N 格式。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use sqlx::postgres::{PgPool, PgRow};
use sqlx::{Column, Row};
use sz_orm_core::Connection;
use sz_orm_core::DbError;
use sz_orm_core::Value;

pub struct SqlxPgAdapter {
    pool: PgPool,
}

impl SqlxPgAdapter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn convert_placeholders(sql: &str) -> String {
        let mut result = String::with_capacity(sql.len() + 16);
        let mut n = 1;
        let mut in_quote = false;
        for c in sql.chars() {
            if c == '\'' {
                in_quote = !in_quote;
            }
            if c == '?' && !in_quote {
                result.push_str(&format!("${}", n));
                n += 1;
            } else {
                result.push(c);
            }
        }
        result
    }

    fn row_to_map(row: &PgRow) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        for (i, col) in row.columns().iter().enumerate() {
            let name = col.name().to_string();
            let val = Self::try_get_value(row, i);
            map.insert(name, val);
        }
        map
    }

    fn try_get_value(row: &PgRow, idx: usize) -> Value {
        if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
            return v.map(Value::I32).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
            return v.map(Value::I64).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
            return v.map(Value::F64).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
            return v.map(Value::String).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
            return v.map(Value::Bool).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
            return v.map(Value::Bytes).unwrap_or(Value::Null);
        }
        Value::Null
    }

    fn bind_value<'q>(
        query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
        v: &Value,
    ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
        match v {
            Value::Null => query.bind(None::<i64>),
            Value::Bool(b) => query.bind(*b),
            Value::I8(i) => query.bind(*i as i32),
            Value::I16(i) => query.bind(*i as i32),
            Value::I32(i) => query.bind(*i),
            Value::I64(i) => query.bind(*i),
            Value::U8(u) => query.bind(*u as i32),
            Value::U16(u) => query.bind(*u as i32),
            Value::U32(u) => query.bind(*u as i64),
            Value::U64(u) => query.bind(*u as i64),
            Value::F32(f) => query.bind(*f as f64),
            Value::F64(f) => query.bind(*f),
            Value::String(s) => query.bind(s.clone()),
            Value::Bytes(b) => query.bind(b.clone()),
            _ => query.bind(None::<i64>),
        }
    }
}

impl Connection for SqlxPgAdapter {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let pg_sql = Self::convert_placeholders(sql);
        Box::pin(async move {
            let result = sqlx::query(sqlx::AssertSqlSafe(pg_sql.as_str()))
                .execute(&self.pool)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(result.rows_affected())
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        let pg_sql = Self::convert_placeholders(sql);
        Box::pin(async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(pg_sql.as_str()))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(rows.iter().map(Self::row_to_map).collect())
        })
    }

    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        let pg_sql = Self::convert_placeholders(sql);
        Box::pin(async move {
            let mut query = sqlx::query(sqlx::AssertSqlSafe(pg_sql.as_str()));
            for p in params {
                query = Self::bind_value(query, p);
            }
            let rows = query
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(rows.iter().map(Self::row_to_map).collect())
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query("BEGIN")
                .execute(&self.pool)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query("COMMIT")
                .execute(&self.pool)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query("ROLLBACK")
                .execute(&self.pool)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(())
        })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { sqlx::query("SELECT 1").execute(&self.pool).await.is_ok() })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}
