//! Dialect 模块契约测试 — 对应 `docs/api-contracts.md` §4
//!
//! 锁定各数据库方言的标识符引用、字符串转义、JSON 提取、自增关键字等契约。

use sz_orm_core::{get_dialect, DbType};

// ===== §4.1 标识符引用契约 =====

#[test]
fn test_mysql_dialect_quotes_with_backticks_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    // MySQL 用反引号
    assert_eq!(d.quote("users"), "`users`");
}

#[test]
fn test_postgres_dialect_quotes_with_double_quotes_contract() {
    let d = get_dialect(DbType::PostgreSQL).unwrap();
    assert_eq!(d.quote("users"), "\"users\"");
}

#[test]
fn test_sqlite_dialect_quotes_with_double_quotes_contract() {
    let d = get_dialect(DbType::Sqlite).unwrap();
    assert_eq!(d.quote("users"), "\"users\"");
}

#[test]
fn test_oracle_dialect_quotes_with_double_quotes_contract() {
    let d = get_dialect(DbType::Oracle).unwrap();
    assert_eq!(d.quote("users"), "\"users\"");
}

// ===== §4.1 字符串转义契约 =====

#[test]
fn test_dialect_escape_string_contract() {
    // MySQL 使用反斜杠转义：' -> \'
    let mysql_d = get_dialect(DbType::MySQL).unwrap();
    let mysql_escaped = mysql_d.escape_string("it's a test");
    assert!(
        mysql_escaped.contains("it\\'s a test"),
        "MySQL escape_string failed: {}",
        mysql_escaped
    );

    // PG/SQLite/Oracle/SQL Server 使用双单引号转义：' -> ''
    for db in [
        DbType::PostgreSQL,
        DbType::Sqlite,
        DbType::Oracle,
        DbType::SqlServer,
    ] {
        let d = get_dialect(db).unwrap();
        let escaped = d.escape_string("it's a test");
        assert!(
            escaped.contains("it''s a test"),
            "{:?} escape_string failed: {}",
            db,
            escaped
        );
    }
}

// ===== §4.1 自增关键字契约 =====

#[test]
fn test_auto_increment_keyword_contract() {
    assert_eq!(
        get_dialect(DbType::MySQL).unwrap().auto_increment_keyword(),
        "AUTO_INCREMENT"
    );
    // PG/Oracle/SQLite 各有自己的关键字
    let pg_kw = get_dialect(DbType::PostgreSQL)
        .unwrap()
        .auto_increment_keyword();
    assert!(!pg_kw.is_empty());
    let oracle_kw = get_dialect(DbType::Oracle)
        .unwrap()
        .auto_increment_keyword();
    assert!(!oracle_kw.is_empty());
}

// ===== §4.1 JSON 提取契约 =====

#[test]
fn test_json_extract_mysql_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = d.json_extract("data", "$.name");
    assert!(
        sql.contains("JSON_EXTRACT"),
        "MySQL 应使用 JSON_EXTRACT, got: {}",
        sql
    );
    assert!(sql.contains("data"));
    assert!(sql.contains("$.name"));
}

#[test]
fn test_json_extract_sqlite_contract() {
    let d = get_dialect(DbType::Sqlite).unwrap();
    let sql = d.json_extract("data", "$.name");
    assert!(
        sql.contains("json_extract"),
        "SQLite 应使用 json_extract, got: {}",
        sql
    );
}

#[test]
fn test_json_extract_oracle_contract() {
    let d = get_dialect(DbType::Oracle).unwrap();
    let sql = d.json_extract("data", "$.name");
    assert!(
        sql.contains("JSON_VALUE"),
        "Oracle 应使用 JSON_VALUE, got: {}",
        sql
    );
}

// ===== §4.2 get_dialect 契约 =====

#[test]
fn test_get_dialect_returns_ok_for_sql_dbs_contract() {
    for db in [
        DbType::MySQL,
        DbType::PostgreSQL,
        DbType::Sqlite,
        DbType::Oracle,
        DbType::SqlServer,
    ] {
        assert!(get_dialect(db).is_ok(), "{:?} 应返回 Ok", db);
    }
}

#[test]
fn test_get_dialect_returns_err_for_nosql_dbs_contract() {
    // NoSQL 数据库返回 Err(DbError::UnsupportedDialect) 或类似错误
    // 注：ClickHouse 现已有独立方言实现，不再是 NoSQL Unsupported
    for db in [
        DbType::Redis,
        DbType::MongoDB,
        DbType::VectorDb,
        DbType::PureJsDb,
    ] {
        let result = get_dialect(db);
        match result {
            Err(_e) => { /* 契约满足：NoSQL 无方言 */ }
            Ok(_) => panic!("{:?} 应返回 Err（无方言）", db),
        }
    }
}

#[test]
fn test_get_dialect_clickhouse_now_supported_contract() {
    // ClickHouse 现已支持独立方言（使用 MergeTree 引擎、Int64/UInt8 类型系统等）
    let d = get_dialect(DbType::ClickHouse).unwrap();
    assert_eq!(d.db_type(), DbType::ClickHouse);
    // ClickHouse 使用 backquote 标识符（与 MySQL 一致）
    assert_eq!(d.quote("users"), "`users`");
    // ClickHouse 不支持 RETURNING
    assert!(!d.supports_returning());
}

#[test]
fn test_get_dialect_new_chinese_dbs_supported_contract() {
    // 国产数据库 & 兼容方言现均已支持
    let supported_dbs = [
        DbType::Dameng,
        DbType::Kingbase,
        DbType::Db2,
        DbType::MariaDB,
        DbType::TiDB,
        DbType::PolarDB,
        DbType::GaussDB,
        DbType::GBase,
        DbType::Sybase,
    ];
    for db in supported_dbs {
        let result = get_dialect(db);
        assert!(result.is_ok(), "{:?} 应支持方言", db);
        let d = result.unwrap();
        assert_eq!(d.db_type(), db, "方言 db_type() 应与请求的 DbType 一致");
    }
}

#[test]
fn test_get_dialect_oceanbase_returns_mysql_contract() {
    // OceanBase 兼容 MySQL 协议，应返回 MySQL 方言
    let d = get_dialect(DbType::OceanBase).unwrap();
    assert_eq!(d.db_type(), DbType::MySQL);
    // 应使用反引号引用
    assert_eq!(d.quote("users"), "`users`");
}

#[test]
fn test_dialect_db_type_returns_correct_type_contract() {
    assert_eq!(get_dialect(DbType::MySQL).unwrap().db_type(), DbType::MySQL);
    assert_eq!(
        get_dialect(DbType::PostgreSQL).unwrap().db_type(),
        DbType::PostgreSQL
    );
    assert_eq!(
        get_dialect(DbType::Sqlite).unwrap().db_type(),
        DbType::Sqlite
    );
    assert_eq!(
        get_dialect(DbType::Oracle).unwrap().db_type(),
        DbType::Oracle
    );
}
