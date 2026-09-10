//! Db2 独立方言集成测试
//!
//! Db2Dialect 是独立方言实现，使用双引号 quote、
//! OFFSET ... FETCH FIRST ... ROWS ONLY 分页、IDENTITY 列、SMALLINT 替代 BOOLEAN。
//!
//! 受限说明：Db2 真实 DB 集成测试需要 ODBC 驱动或专用 `ibm-db2` crate。
//! 受项目"不引入新 Rust 依赖"约束，当前仅提供方言断言测试（10 个）。
//!
//! 后续扩展路径：
//! - 当项目引入 `ibm-db2` 或 ODBC 驱动后，补充真实 DB 测试
//! - 使用 `"user"` 双引号 quote、`?` 占位符、`FETCH FIRST n ROWS ONLY` 分页验证
//! - Db2 Docker 容器部署：`docker run -d ibmcom/db2`（需接受许可证协议）
//! - 环境变量：SZ_ORM_DB2_URL 配置连接参数
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_db2

use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

fn test_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ]
}

// ==================== 方言断言测试 ====================

#[test]
fn test_db2_dialect_quote() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    assert_eq!(dialect.quote("user"), "\"user\"");
}

#[test]
fn test_db2_dialect_db_type() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    assert_eq!(dialect.db_type(), DbType::Db2);
}

#[test]
fn test_db2_dialect_pagination() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let sql = dialect.build_pagination("SELECT 1", 10, 20);
    assert!(
        sql.contains("FETCH FIRST") || sql.contains("OFFSET"),
        "Db2 pagination must use OFFSET/FETCH FIRST: got {}",
        sql
    );
}

#[test]
fn test_db2_dialect_create_table() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let sql = dialect.build_create_table("test_table", &test_columns());
    assert!(sql.contains("CREATE TABLE"), "Must contain CREATE TABLE");
    assert!(
        sql.contains("\"test_table\""),
        "Must quote table name with double quotes"
    );
}

#[test]
fn test_db2_dialect_no_returning() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    assert!(
        !dialect.supports_returning(),
        "Db2 does not support RETURNING"
    );
}

#[test]
fn test_db2_dialect_supports_lock() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    assert!(
        dialect.supports_lock_for_update(),
        "Db2 supports FOR UPDATE"
    );
}

#[test]
fn test_db2_dialect_type_mapping_bigint() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "BIGINT".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(sql.contains("BIGINT"), "BIGINT should stay BIGINT in Db2");
}

#[test]
fn test_db2_dialect_type_mapping_int() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "INT".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: true,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(sql.contains("INTEGER"), "INT should map to INTEGER in Db2");
}

#[test]
fn test_db2_dialect_type_mapping_boolean() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "flag".to_string(),
        sql_type: "BOOLEAN".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: false,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(
        sql.contains("SMALLINT"),
        "BOOLEAN should map to SMALLINT in Db2"
    );
}

#[test]
fn test_db2_dialect_type_mapping_text() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "content".to_string(),
        sql_type: "TEXT".to_string(),
        nullable: true,
        default: None,
        auto_increment: false,
        primary_key: false,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(sql.contains("CLOB"), "TEXT should map to CLOB in Db2");
}

#[test]
fn test_db2_dialect_type_mapping_datetime() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "ts".to_string(),
        sql_type: "DATETIME".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: false,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(
        sql.contains("TIMESTAMP"),
        "DATETIME should map to TIMESTAMP in Db2"
    );
}
