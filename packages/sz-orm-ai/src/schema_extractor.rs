//! Schema 自动提取模块
//!
//! 提供从数据库 `information_schema` 自动提取表结构的能力，
//! 用于 NL2SQL 自动获取 SchemaContext，无需用户手动传入。
//!
//! 启用 `ai-schema-extract` feature 后可用（需传入 Connection）。
//! 未启用时零编译开销。

use async_trait::async_trait;
use thiserror::Error;

use sz_orm_core::Connection;
use sz_orm_core::DbType;
use sz_orm_core::Value;

use crate::nl2sql::{ColumnInfo, SchemaContext, TableInfo};

/// Schema 提取错误
#[derive(Debug, Error)]
pub enum SchemaExtractError {
    #[error("Database error: {0}")]
    DbError(String),
    #[error("Unsupported dialect: {0:?}")]
    UnsupportedDialect(DbType),
    #[error("Schema extraction failed: {0}")]
    ExtractionFailed(String),
}

/// Schema 提取器 trait
///
/// 所有方言的 Schema 提取器必须实现此 trait。
/// 提取结果为 [`SchemaContext`]，可直接传入 [`crate::nl2sql::Nl2SqlEngine::generate`]。
#[async_trait]
pub trait SchemaExtractor: Send + Sync {
    /// 从数据库连接提取完整 Schema（表 + 列 + 主键 + 外键）
    async fn extract_schema(
        &self,
        conn: &mut dyn Connection,
    ) -> Result<SchemaContext, SchemaExtractError>;
}

/// 按方言创建对应的 Schema 提取器
pub fn create_extractor(dialect: DbType) -> Box<dyn SchemaExtractor> {
    match dialect {
        DbType::MySQL | DbType::MariaDB | DbType::OceanBase | DbType::TiDB => {
            Box::new(MySqlSchemaExtractor)
        }
        DbType::PostgreSQL
        | DbType::Kingbase
        | DbType::PolarDB
        | DbType::GaussDB
        | DbType::CockroachDB
        | DbType::YugabyteDB => Box::new(PgSchemaExtractor),
        DbType::Sqlite | DbType::DuckDB => Box::new(SqliteSchemaExtractor),
        DbType::Oracle | DbType::Dameng => Box::new(OracleSchemaExtractor),
        DbType::SqlServer | DbType::Sybase => Box::new(MssqlSchemaExtractor),
        _ => Box::new(GenericSchemaExtractor),
    }
}

/// 从 Value 提取字符串
fn value_to_string(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// 从 Value 提取布尔值（"YES"/"NO" 或 "1"/"0"）
fn value_to_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::I32(i) => *i != 0,
        Value::I64(i) => *i != 0,
        _ => {
            let s = value_to_string(v).to_uppercase();
            s == "YES" || s == "1" || s == "TRUE"
        }
    }
}

// ==================== MySQL 提取器 ====================

/// MySQL Schema 提取器（兼容 MariaDB / OceanBase / TiDB）
pub struct MySqlSchemaExtractor;

#[async_trait]
impl SchemaExtractor for MySqlSchemaExtractor {
    async fn extract_schema(
        &self,
        conn: &mut dyn Connection,
    ) -> Result<SchemaContext, SchemaExtractError> {
        let tables_sql = "SELECT TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME";
        let table_rows = conn
            .query(tables_sql)
            .await
            .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

        let mut tables = Vec::with_capacity(table_rows.len());
        for row in &table_rows {
            let table_name = row
                .get("TABLE_NAME")
                .or_else(|| row.get("table_name"))
                .map(value_to_string)
                .unwrap_or_default();
            if table_name.is_empty() {
                continue;
            }
            let columns = extract_mysql_columns(conn, &table_name).await?;
            tables.push(TableInfo {
                name: table_name,
                columns,
            });
        }
        Ok(SchemaContext { tables })
    }
}

async fn extract_mysql_columns(
    conn: &mut dyn Connection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, SchemaExtractError> {
    let sql = format!(
        "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_KEY \
         FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{}' \
         ORDER BY ORDINAL_POSITION",
        table_name
    );
    let rows = conn
        .query(&sql)
        .await
        .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

    let mut columns = Vec::with_capacity(rows.len());
    for row in &rows {
        let name = row
            .get("COLUMN_NAME")
            .or_else(|| row.get("column_name"))
            .map(value_to_string)
            .unwrap_or_default();
        let data_type = row
            .get("DATA_TYPE")
            .or_else(|| row.get("data_type"))
            .map(value_to_string)
            .unwrap_or_else(|| "TEXT".into());
        let nullable = row
            .get("IS_NULLABLE")
            .or_else(|| row.get("is_nullable"))
            .map(value_to_bool)
            .unwrap_or(true);
        let is_pk = row
            .get("COLUMN_KEY")
            .or_else(|| row.get("column_key"))
            .map(|v| value_to_string(v).to_uppercase() == "PRI")
            .unwrap_or(false);
        if !name.is_empty() {
            columns.push(ColumnInfo {
                name,
                data_type,
                nullable,
                is_primary_key: is_pk,
            });
        }
    }
    Ok(columns)
}

// ==================== PostgreSQL 提取器 ====================

/// PostgreSQL Schema 提取器（兼容 Kingbase / PolarDB / GaussDB / CockroachDB / YugabyteDB）
pub struct PgSchemaExtractor;

#[async_trait]
impl SchemaExtractor for PgSchemaExtractor {
    async fn extract_schema(
        &self,
        conn: &mut dyn Connection,
    ) -> Result<SchemaContext, SchemaExtractError> {
        let tables_sql =
            "SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename";
        let table_rows = conn
            .query(tables_sql)
            .await
            .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

        let mut tables = Vec::with_capacity(table_rows.len());
        for row in &table_rows {
            let table_name = row
                .get("tablename")
                .map(value_to_string)
                .unwrap_or_default();
            if table_name.is_empty() {
                continue;
            }
            let columns = extract_pg_columns(conn, &table_name).await?;
            tables.push(TableInfo {
                name: table_name,
                columns,
            });
        }
        Ok(SchemaContext { tables })
    }
}

async fn extract_pg_columns(
    conn: &mut dyn Connection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, SchemaExtractError> {
    let sql = format!(
        "SELECT column_name, data_type, is_nullable, \
         (SELECT COUNT(*) FROM information_schema.key_column_usage kcu \
          JOIN information_schema.table_constraints tc ON kcu.constraint_name = tc.constraint_name \
          WHERE kcu.table_name = '{}' AND tc.constraint_type = 'PRIMARY KEY' AND kcu.column_name = c.column_name) as is_pk \
         FROM information_schema.columns c \
         WHERE table_schema = 'public' AND table_name = '{}' \
         ORDER BY ordinal_position",
        table_name, table_name
    );
    let rows = conn
        .query(&sql)
        .await
        .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

    let mut columns = Vec::with_capacity(rows.len());
    for row in &rows {
        let name = row
            .get("column_name")
            .map(value_to_string)
            .unwrap_or_default();
        let data_type = row
            .get("data_type")
            .map(value_to_string)
            .unwrap_or_else(|| "text".into());
        let nullable = row.get("is_nullable").map(value_to_bool).unwrap_or(true);
        let is_pk = row
            .get("is_pk")
            .map(|v| v.as_i64().unwrap_or(0) > 0)
            .unwrap_or(false);
        if !name.is_empty() {
            columns.push(ColumnInfo {
                name,
                data_type,
                nullable,
                is_primary_key: is_pk,
            });
        }
    }
    Ok(columns)
}

// ==================== SQLite 提取器 ====================

/// SQLite Schema 提取器（兼容 DuckDB）
pub struct SqliteSchemaExtractor;

#[async_trait]
impl SchemaExtractor for SqliteSchemaExtractor {
    async fn extract_schema(
        &self,
        conn: &mut dyn Connection,
    ) -> Result<SchemaContext, SchemaExtractError> {
        let tables_sql = "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name";
        let table_rows = conn
            .query(tables_sql)
            .await
            .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

        let mut tables = Vec::with_capacity(table_rows.len());
        for row in &table_rows {
            let table_name = row.get("name").map(value_to_string).unwrap_or_default();
            if table_name.is_empty() {
                continue;
            }
            let columns = extract_sqlite_columns(conn, &table_name).await?;
            tables.push(TableInfo {
                name: table_name,
                columns,
            });
        }
        Ok(SchemaContext { tables })
    }
}

async fn extract_sqlite_columns(
    conn: &mut dyn Connection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, SchemaExtractError> {
    let sql = format!("PRAGMA table_info('{}')", table_name);
    let rows = conn
        .query(&sql)
        .await
        .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

    let mut columns = Vec::with_capacity(rows.len());
    for row in &rows {
        let name = row.get("name").map(value_to_string).unwrap_or_default();
        let data_type = row
            .get("type")
            .map(value_to_string)
            .unwrap_or_else(|| "TEXT".into());
        let notnull = row
            .get("notnull")
            .map(|v| v.as_i64().unwrap_or(0) != 0)
            .unwrap_or(false);
        let is_pk = row
            .get("pk")
            .map(|v| v.as_i64().unwrap_or(0) != 0)
            .unwrap_or(false);
        if !name.is_empty() {
            columns.push(ColumnInfo {
                name,
                data_type,
                nullable: !notnull,
                is_primary_key: is_pk,
            });
        }
    }
    Ok(columns)
}

// ==================== Oracle 提取器 ====================

/// Oracle Schema 提取器（兼容 Dameng）
pub struct OracleSchemaExtractor;

#[async_trait]
impl SchemaExtractor for OracleSchemaExtractor {
    async fn extract_schema(
        &self,
        conn: &mut dyn Connection,
    ) -> Result<SchemaContext, SchemaExtractError> {
        let tables_sql = "SELECT TABLE_NAME FROM USER_TABLES ORDER BY TABLE_NAME";
        let table_rows = conn
            .query(tables_sql)
            .await
            .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

        let mut tables = Vec::with_capacity(table_rows.len());
        for row in &table_rows {
            let table_name = row
                .get("TABLE_NAME")
                .or_else(|| row.get("table_name"))
                .map(value_to_string)
                .unwrap_or_default();
            if table_name.is_empty() {
                continue;
            }
            let columns = extract_oracle_columns(conn, &table_name).await?;
            tables.push(TableInfo {
                name: table_name,
                columns,
            });
        }
        Ok(SchemaContext { tables })
    }
}

async fn extract_oracle_columns(
    conn: &mut dyn Connection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, SchemaExtractError> {
    let sql = format!(
        "SELECT COLUMN_NAME, DATA_TYPE, NULLABLE, \
         (SELECT COUNT(*) FROM USER_CONSTRAINTS uc \
          JOIN USER_CONS_COLUMNS ucc ON uc.CONSTRAINT_NAME = ucc.CONSTRAINT_NAME \
          WHERE uc.TABLE_NAME = '{}' AND uc.CONSTRAINT_TYPE = 'P' AND ucc.COLUMN_NAME = c.COLUMN_NAME) as IS_PK \
         FROM USER_TAB_COLUMNS c \
         WHERE TABLE_NAME = '{}' \
         ORDER BY COLUMN_ID",
        table_name, table_name
    );
    let rows = conn
        .query(&sql)
        .await
        .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

    let mut columns = Vec::with_capacity(rows.len());
    for row in &rows {
        let name = row
            .get("COLUMN_NAME")
            .or_else(|| row.get("column_name"))
            .map(value_to_string)
            .unwrap_or_default();
        let data_type = row
            .get("DATA_TYPE")
            .or_else(|| row.get("data_type"))
            .map(value_to_string)
            .unwrap_or_else(|| "VARCHAR2".into());
        let nullable = row
            .get("NULLABLE")
            .or_else(|| row.get("nullable"))
            .map(|v| value_to_string(v).to_uppercase() == "Y")
            .unwrap_or(true);
        let is_pk = row
            .get("IS_PK")
            .or_else(|| row.get("is_pk"))
            .map(|v| v.as_i64().unwrap_or(0) > 0)
            .unwrap_or(false);
        if !name.is_empty() {
            columns.push(ColumnInfo {
                name,
                data_type,
                nullable,
                is_primary_key: is_pk,
            });
        }
    }
    Ok(columns)
}

// ==================== MSSQL 提取器 ====================

/// SQL Server Schema 提取器（兼容 Sybase）
pub struct MssqlSchemaExtractor;

#[async_trait]
impl SchemaExtractor for MssqlSchemaExtractor {
    async fn extract_schema(
        &self,
        conn: &mut dyn Connection,
    ) -> Result<SchemaContext, SchemaExtractError> {
        let tables_sql = "SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_NAME";
        let table_rows = conn
            .query(tables_sql)
            .await
            .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

        let mut tables = Vec::with_capacity(table_rows.len());
        for row in &table_rows {
            let table_name = row
                .get("TABLE_NAME")
                .or_else(|| row.get("table_name"))
                .map(value_to_string)
                .unwrap_or_default();
            if table_name.is_empty() {
                continue;
            }
            let columns = extract_mssql_columns(conn, &table_name).await?;
            tables.push(TableInfo {
                name: table_name,
                columns,
            });
        }
        Ok(SchemaContext { tables })
    }
}

async fn extract_mssql_columns(
    conn: &mut dyn Connection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, SchemaExtractError> {
    let sql = format!(
        "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, \
         (SELECT COUNT(*) FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu \
          JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc ON kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
          WHERE kcu.TABLE_NAME = '{}' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' AND kcu.COLUMN_NAME = c.COLUMN_NAME) as IS_PK \
         FROM INFORMATION_SCHEMA.COLUMNS c \
         WHERE TABLE_NAME = '{}' \
         ORDER BY ORDINAL_POSITION",
        table_name, table_name
    );
    let rows = conn
        .query(&sql)
        .await
        .map_err(|e| SchemaExtractError::DbError(e.to_string()))?;

    let mut columns = Vec::with_capacity(rows.len());
    for row in &rows {
        let name = row
            .get("COLUMN_NAME")
            .or_else(|| row.get("column_name"))
            .map(value_to_string)
            .unwrap_or_default();
        let data_type = row
            .get("DATA_TYPE")
            .or_else(|| row.get("data_type"))
            .map(value_to_string)
            .unwrap_or_else(|| "nvarchar".into());
        let nullable = row
            .get("IS_NULLABLE")
            .or_else(|| row.get("is_nullable"))
            .map(value_to_bool)
            .unwrap_or(true);
        let is_pk = row
            .get("IS_PK")
            .or_else(|| row.get("is_pk"))
            .map(|v| v.as_i64().unwrap_or(0) > 0)
            .unwrap_or(false);
        if !name.is_empty() {
            columns.push(ColumnInfo {
                name,
                data_type,
                nullable,
                is_primary_key: is_pk,
            });
        }
    }
    Ok(columns)
}

// ==================== 通用提取器（不支持方言的 fallback） ====================

/// 通用 Schema 提取器（用于不支持的方言，返回空 Schema）
pub struct GenericSchemaExtractor;

#[async_trait]
impl SchemaExtractor for GenericSchemaExtractor {
    async fn extract_schema(
        &self,
        _conn: &mut dyn Connection,
    ) -> Result<SchemaContext, SchemaExtractError> {
        Ok(SchemaContext::default())
    }
}
