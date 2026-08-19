//! SAP HANA 真实驱动桥接（v4.9.0 TASK-003）
//!
//! 基于 hdbconnect_async v0.32.0（纯 Rust async + bb8 连接池 + tokio），
//! 提供 connect/query/execute/事务桥接，实现 sz-orm-core 的 Connection trait。
//! 通过 feature `dialect-saphana-driver` 启用，默认不启用。
//!
//! # 示例
//!
//! ```no_run
//! use sz_orm_sqlx::saphana_adapter::SapHanaConnection;
//! use sz_orm_core::pool::Connection;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut conn = SapHanaConnection::connect("hdbsql://user:pass@host:30015").await?;
//! let rows = conn.query("SELECT 'hello' as msg FROM DUMMY").await?;
//! assert_eq!(rows.len(), 1);
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use hdbconnect_async::{Connection as HanaConn, HdbError};

use sz_orm_core::{Connection, DbError, QueryRows, Value};

/// SAP HANA 连接桥接，包装 hdbconnect_async::Connection。
pub struct SapHanaConnection {
    conn: HanaConn,
}

fn map_err(e: HdbError) -> DbError {
    DbError::QueryError(format!("SAP HANA: {e}"))
}

impl SapHanaConnection {
    /// 连接 SAP HANA，url 格式：`hdbsql://user:pass@host:port`
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        let conn = HanaConn::new(url).await.map_err(map_err)?;
        Ok(Self { conn })
    }
}

impl Connection for SapHanaConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let n = self.conn.dml(sql).await.map_err(map_err)?;
            Ok(n as u64)
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let rs = self.conn.query(sql).await.map_err(map_err)?;
            let rows: Vec<HashMap<String, String>> = rs.try_into().await.map_err(map_err)?;
            Ok(rows
                .into_iter()
                .map(|m| m.into_iter().map(|(k, v)| (k, Value::String(v))).collect())
                .collect())
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.conn.set_auto_commit(false).await;
            Ok(())
        })
    }

    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.conn.commit().await.map_err(map_err)?;
            self.conn.set_auto_commit(true).await;
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.conn.rollback().await.map_err(map_err)?;
            self.conn.set_auto_commit(true).await;
            Ok(())
        })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { !self.conn.is_broken().await })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            // hdbconnect_async Connection 在 Drop 时自动关闭
            Ok(())
        })
    }
}