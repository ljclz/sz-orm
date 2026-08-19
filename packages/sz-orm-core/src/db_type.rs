//! 数据库类型定义
//!
//! 定义所有支持的数据库类型

use serde::{Deserialize, Serialize};
use std::fmt;

/// 支持的数据库类型
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
    /// ClickHouse（列式 OLAP 数据库）
    ClickHouse,
    /// Oracle
    Oracle,
    /// OceanBase（阿里云分布式数据库，MySQL 兼容）
    OceanBase,
    /// SQL Server
    SqlServer,
    /// Vector Database (Milvus, Qdrant, etc.)
    VectorDb,
    /// PureJS Database
    PureJsDb,
    /// 达梦数据库 DM8（Oracle 兼容方言，国产数据库）
    Dameng,
    /// 人大金仓 KingbaseES（PostgreSQL 兼容方言，国产数据库）
    Kingbase,
    /// IBM DB2 LUW
    Db2,
    /// MariaDB（MySQL 兼容方言）
    MariaDB,
    /// TiDB（MySQL 兼容分布式数据库）
    TiDB,
    /// PolarDB（阿里云 PG/MySQL 兼容云数据库）
    PolarDB,
    /// GaussDB（华为云 PG 兼容分布式数据库）
    GaussDB,
    /// GBase 8s（南大通用，Informix 兼容方言）
    GBase,
    /// Sybase ASE
    Sybase,
    /// DuckDB（嵌入式 OLAP 数据库）
    DuckDB,
    /// CockroachDB（PostgreSQL 兼容分布式数据库，v3.5.0 新增）
    #[cfg(feature = "dialect-cockroachdb")]
    CockroachDB,
    /// YugabyteDB（PostgreSQL 兼容分布式数据库，v3.5.0 新增）
    #[cfg(feature = "dialect-yugabytedb")]
    YugabyteDB,
    /// Snowflake（云数仓数据库，v3.6.0 新增，支持 VARIANT/OBJECT/ARRAY 半结构化类型）
    #[cfg(feature = "dialect-snowflake")]
    Snowflake,
    /// Redshift（AWS 云数仓数据库，v3.6.0 新增，PostgreSQL 兼容 + COPY/UNLOAD 特性）
    #[cfg(feature = "dialect-redshift")]
    Redshift,
    /// Informix（IBM Informix 数据库，v3.7.0 新增，SQL generation only: 仅 SQL 生成，无真实驱动连接，SERIAL/ROW 类型）
    #[cfg(feature = "dialect-informix")]
    Informix,
    /// SAP HANA（SAP HANA 内存数据库，v3.7.0 新增；v4.9.0 TASK-003 已集成真实驱动 hdbconnect_async v0.32.0，feature dialect-saphana-driver，计算列 + CE 函数）
    #[cfg(feature = "dialect-saphana")]
    SapHana,
    /// Firebird（Firebird6 数据库，v3.7.0 新增，SQL generation only: 仅 SQL 生成，无真实驱动连接，GENERATOR/SEQUENCE + EXECUTE BLOCK）
    #[cfg(feature = "dialect-firebird")]
    Firebird,
}

impl DbType {
    /// 返回数据库类型名称的字符串形式
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
            DbType::Dameng => "dameng",
            DbType::Kingbase => "kingbase",
            DbType::Db2 => "db2",
            DbType::MariaDB => "mariadb",
            DbType::TiDB => "tidb",
            DbType::PolarDB => "polardb",
            DbType::GaussDB => "gaussdb",
            DbType::GBase => "gbase",
            DbType::Sybase => "sybase",
            DbType::DuckDB => "duckdb",
            #[cfg(feature = "dialect-cockroachdb")]
            DbType::CockroachDB => "cockroachdb",
            #[cfg(feature = "dialect-yugabytedb")]
            DbType::YugabyteDB => "yugabytedb",
            #[cfg(feature = "dialect-snowflake")]
            DbType::Snowflake => "snowflake",
            #[cfg(feature = "dialect-redshift")]
            DbType::Redshift => "redshift",
            #[cfg(feature = "dialect-informix")]
            DbType::Informix => "informix",
            #[cfg(feature = "dialect-saphana")]
            DbType::SapHana => "saphana",
            #[cfg(feature = "dialect-firebird")]
            DbType::Firebird => "firebird",
        }
    }

    /// 从字符串解析数据库类型（不区分大小写）
    /// 支持 "mysql"、"postgres"/"postgresql"、"sqlite"/"sqlite3" 等常见别名
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
            "dameng" | "dm" | "dm7" | "dm8" => Some(DbType::Dameng),
            "kingbase" | "kingbasees" | "es" => Some(DbType::Kingbase),
            "db2" => Some(DbType::Db2),
            "mariadb" => Some(DbType::MariaDB),
            "tidb" => Some(DbType::TiDB),
            "polardb" => Some(DbType::PolarDB),
            "gaussdb" => Some(DbType::GaussDB),
            "gbase" | "gbase8s" => Some(DbType::GBase),
            "sybase" | "ase" => Some(DbType::Sybase),
            "duckdb" => Some(DbType::DuckDB),
            #[cfg(feature = "dialect-cockroachdb")]
            "cockroachdb" | "cockroach" => Some(DbType::CockroachDB),
            #[cfg(feature = "dialect-yugabytedb")]
            "yugabytedb" | "yugabyte" => Some(DbType::YugabyteDB),
            #[cfg(feature = "dialect-snowflake")]
            "snowflake" => Some(DbType::Snowflake),
            #[cfg(feature = "dialect-redshift")]
            "redshift" => Some(DbType::Redshift),
            #[cfg(feature = "dialect-informix")]
            "informix" => Some(DbType::Informix),
            #[cfg(feature = "dialect-saphana")]
            "saphana" | "hana" => Some(DbType::SapHana),
            #[cfg(feature = "dialect-firebird")]
            "firebird" | "firebirdsql" => Some(DbType::Firebird),
            _ => None,
        }
    }

    /// 判断该数据库是否支持 schema（表、列等结构定义）
    pub fn supports_schema(&self) -> bool {
        !matches!(self, DbType::Redis | DbType::VectorDb)
    }

    /// 判断该数据库是否支持事务
    pub fn supports_transaction(&self) -> bool {
        !matches!(self, DbType::Redis | DbType::VectorDb | DbType::MongoDB)
    }

    /// 判断该数据库是否支持外键约束
    pub fn supports_foreign_key(&self) -> bool {
        !matches!(
            self,
            DbType::Redis | DbType::VectorDb | DbType::MongoDB | DbType::Sqlite
        )
    }

    /// 判断该数据库是否支持存储过程
    pub fn supports_stored_procedure(&self) -> bool {
        matches!(
            self,
            DbType::MySQL
                | DbType::PostgreSQL
                | DbType::Oracle
                | DbType::OceanBase
                | DbType::SqlServer
                | DbType::Dameng
                | DbType::Kingbase
                | DbType::Db2
                | DbType::MariaDB
                | DbType::TiDB
                | DbType::PolarDB
                | DbType::GaussDB
                | DbType::GBase
                | DbType::Sybase
        )
    }

    /// 获取该数据库类型的默认端口
    /// SQLite/PureJsDb 返回 0（不使用网络端口）
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
            DbType::Dameng => 5236,
            DbType::Kingbase => 54321,
            DbType::Db2 => 50000,
            DbType::MariaDB => 3306,
            DbType::TiDB => 4000,
            DbType::PolarDB => 5432,
            DbType::GaussDB => 25308,
            DbType::GBase => 9088,
            DbType::Sybase => 5000,
            DbType::DuckDB => 0, // 嵌入式数据库，无网络端口
            #[cfg(feature = "dialect-cockroachdb")]
            DbType::CockroachDB => 26257, // CockroachDB 默认端口
            #[cfg(feature = "dialect-yugabytedb")]
            DbType::YugabyteDB => 5433, // YugabyteDB 默认端口（与 PG 5432 区分）
            #[cfg(feature = "dialect-snowflake")]
            DbType::Snowflake => 443, // Snowflake HTTPS 端口
            #[cfg(feature = "dialect-redshift")]
            DbType::Redshift => 5439, // Redshift 默认端口
            #[cfg(feature = "dialect-informix")]
            DbType::Informix => 9088, // Informix 默认端口（sqlexec）
            #[cfg(feature = "dialect-saphana")]
            DbType::SapHana => 39015, // SAP HANA 默认端口（systemdb）
            #[cfg(feature = "dialect-firebird")]
            DbType::Firebird => 3050, // Firebird 默认端口
        }
    }

    /// 判断该数据库是否为 MySQL 方言家族（MySQL/MariaDB/TiDB/OceanBase 等）
    pub fn is_mysql_family(&self) -> bool {
        matches!(
            self,
            DbType::MySQL | DbType::MariaDB | DbType::TiDB | DbType::OceanBase
        )
    }

    /// 判断该数据库是否为 PostgreSQL 方言家族（PostgreSQL/Kingbase/PolarDB-PG/GaussDB）
    pub fn is_postgres_family(&self) -> bool {
        match self {
            DbType::PostgreSQL | DbType::Kingbase | DbType::GaussDB | DbType::PolarDB => true,
            #[cfg(feature = "dialect-cockroachdb")]
            DbType::CockroachDB => true,
            #[cfg(feature = "dialect-yugabytedb")]
            DbType::YugabyteDB => true,
            #[cfg(feature = "dialect-redshift")]
            DbType::Redshift => true,
            _ => false,
        }
    }

    /// 判断该数据库是否为 Oracle 方言家族（Oracle/达梦）
    pub fn is_oracle_family(&self) -> bool {
        matches!(self, DbType::Oracle | DbType::Dameng)
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
