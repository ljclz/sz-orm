//! Database type definitions
//!
//! Defines all supported database types

use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported database types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum DbType {
    /// MySQL
    MySQL,
    /// PostgreSQL
    #[default]
    PostgreSQL,
    /// SQLite
    Sqlite,
    /// Redis
    Redis,
    /// MongoDB
    MongoDB,
    /// ClickHouse
    ClickHouse,
    /// Oracle
    Oracle,
    /// OceanBase
    OceanBase,
    /// SQL Server
    SqlServer,
    /// Vector Database (Milvus, Qdrant, etc.)
    VectorDb,
    /// PureJS Database
    PureJsDb,
}

impl DbType {
    /// Get the database type name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            DbType::MySQL => "mysql",
            DbType::PostgreSQL => "postgres",
            DbType::Sqlite => "sqlite",
            DbType::Redis => "redis",
            DbType::MongoDB => "mongodb",
            DbType::ClickHouse => "clickhouse",
            DbType::Oracle => "oracle",
            DbType::OceanBase => "oceanbase",
            DbType::SqlServer => "mssql",
            DbType::VectorDb => "vectordb",
            DbType::PureJsDb => "purejsdb",
        }
    }

    /// Parse from string
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mysql" => Some(DbType::MySQL),
            "postgres" | "postgresql" => Some(DbType::PostgreSQL),
            "sqlite" | "sqlite3" => Some(DbType::Sqlite),
            "redis" => Some(DbType::Redis),
            "mongodb" | "mongo" => Some(DbType::MongoDB),
            "clickhouse" => Some(DbType::ClickHouse),
            "oracle" => Some(DbType::Oracle),
            "oceanbase" => Some(DbType::OceanBase),
            "mssql" | "sqlserver" => Some(DbType::SqlServer),
            "vectordb" | "vector" => Some(DbType::VectorDb),
            "purejsdb" | "purejs" => Some(DbType::PureJsDb),
            _ => None,
        }
    }

    /// Check if the database supports schema (tables, columns)
    pub fn supports_schema(&self) -> bool {
        !matches!(self, DbType::Redis | DbType::VectorDb)
    }

    /// Check if the database supports transactions
    pub fn supports_transaction(&self) -> bool {
        !matches!(self, DbType::Redis | DbType::VectorDb | DbType::MongoDB)
    }

    /// Check if the database supports foreign keys
    pub fn supports_foreign_key(&self) -> bool {
        !matches!(
            self,
            DbType::Redis | DbType::VectorDb | DbType::MongoDB | DbType::Sqlite
        )
    }

    /// Check if the database supports stored procedures
    pub fn supports_stored_procedure(&self) -> bool {
        matches!(
            self,
            DbType::MySQL
                | DbType::PostgreSQL
                | DbType::Oracle
                | DbType::OceanBase
                | DbType::SqlServer
        )
    }

    /// Get the default port for this database type
    pub fn default_port(&self) -> u16 {
        match self {
            DbType::MySQL => 3306,
            DbType::PostgreSQL => 5432,
            DbType::Sqlite => 0,
            DbType::Redis => 6379,
            DbType::MongoDB => 27017,
            DbType::ClickHouse => 8123,
            DbType::Oracle => 1521,
            DbType::OceanBase => 2881,
            DbType::SqlServer => 1433,
            DbType::VectorDb => 19530,
            DbType::PureJsDb => 0,
        }
    }
}

impl fmt::Display for DbType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_type_as_str() {
        assert_eq!(DbType::MySQL.as_str(), "mysql");
        assert_eq!(DbType::PostgreSQL.as_str(), "postgres");
    }

    #[test]
    fn test_db_type_from_str() {
        assert_eq!(DbType::from_str("mysql"), Some(DbType::MySQL));
        assert_eq!(DbType::from_str("postgres"), Some(DbType::PostgreSQL));
        assert_eq!(DbType::from_str("unknown"), None);
    }

    #[test]
    fn test_db_type_supports_schema() {
        assert!(DbType::MySQL.supports_schema());
        assert!(!DbType::Redis.supports_schema());
    }

    #[test]
    fn test_db_type_default_port() {
        assert_eq!(DbType::MySQL.default_port(), 3306);
        assert_eq!(DbType::PostgreSQL.default_port(), 5432);
        assert_eq!(DbType::Sqlite.default_port(), 0);
    }
}
