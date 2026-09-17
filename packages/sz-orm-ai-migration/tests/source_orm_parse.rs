//! v7.3.0 任务 4.3：源 ORM 解析与迁移报告生成测试
//!
//! 生产调用点证据：
//! - DieselParser::parse：tests/source_orm_parse.rs:Diesel 源项目解析
//! - SeaOrmParser::parse：tests/source_orm_parse.rs:SeaORM 源项目解析
//! - SqlxParser::parse：tests/source_orm_parse.rs:SQLx 源项目解析
//! - OrmMigrator::migrate：tests/source_orm_parse.rs:迁移报告
//! - 源项目文件未修改：tests/source_orm_parse.rs:只读保证
//! - 危险 DDL 不自动执行：tests/source_orm_parse.rs:危险 DDL
//! - 不可识别源 ORM 返回错误：tests/source_orm_parse.rs:错误返回

use std::fs;
use std::io::Write;
use sz_orm_ai_migration::{
    DieselParser, OrmMigrator, SeaOrmParser, SourceOrm, SourceOrmParser, SqlxParser,
};

/// 创建临时源项目目录
fn make_temp_project(content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sz_orm_aimig_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    let mut f = fs::File::create(src.join("models.rs")).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    dir
}

/// Diesel 源项目解析
#[test]
fn test_diesel_source_parse() {
    let content = r#"
#[derive(Selectable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: Option<String>,
    pub age: i32,
}
"#;
    let dir = make_temp_project(content);
    let parser = DieselParser;
    let schema = parser.parse(dir.to_str().unwrap()).unwrap();
    assert_eq!(parser.orm_type(), SourceOrm::Diesel);
    assert!(!schema.tables.is_empty());
    let user_table = schema
        .tables
        .iter()
        .find(|t| t.name == "user")
        .expect("应解析出 user 表");
    assert!(user_table.columns.len() >= 3);
    // email 字段 nullable
    let email = user_table
        .columns
        .iter()
        .find(|c| c.name == "email")
        .expect("应解析出 email 列");
    assert!(email.nullable);
    fs::remove_dir_all(&dir).ok();
}

/// SeaORM 源项目解析
#[test]
fn test_sea_orm_source_parse() {
    let content = r#"
#[derive(DeriveEntityModel)]
#[sea_orm(table_name = "orders")]
pub struct Order {
    pub id: i64,
    pub total: f64,
    pub status: String,
}
"#;
    let dir = make_temp_project(content);
    let parser = SeaOrmParser;
    let schema = parser.parse(dir.to_str().unwrap()).unwrap();
    assert_eq!(parser.orm_type(), SourceOrm::SeaOrm);
    assert!(!schema.tables.is_empty());
    fs::remove_dir_all(&dir).ok();
}

/// SQLx 源项目解析
#[test]
fn test_sqlx_source_parse() {
    let content = r#"
#[derive(FromRow)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub price: f64,
    pub active: bool,
}
"#;
    let dir = make_temp_project(content);
    let parser = SqlxParser;
    let schema = parser.parse(dir.to_str().unwrap()).unwrap();
    assert_eq!(parser.orm_type(), SourceOrm::Sqlx);
    assert!(!schema.tables.is_empty());
    fs::remove_dir_all(&dir).ok();
}

/// 迁移报告生成
#[test]
fn test_migration_report() {
    let content = r#"
#[diesel(table_name = users)]
pub struct User {
    pub id: i64,
    pub name: String,
}
"#;
    let dir = make_temp_project(content);
    let migrator = OrmMigrator::diesel();
    let report = migrator.migrate(dir.to_str().unwrap(), true).unwrap();
    assert_eq!(report.source_orm, SourceOrm::Diesel);
    assert!(report.mapping_count >= 1);
    assert!(report
        .mappings
        .iter()
        .any(|m| m.migration_sql.contains("CREATE TABLE")));
    fs::remove_dir_all(&dir).ok();
}

/// 源项目文件未修改（只读保证）
#[test]
fn test_source_unchanged() {
    let content = r#"
#[diesel(table_name = users)]
pub struct User { pub id: i64, pub name: String }
"#;
    let dir = make_temp_project(content);
    let migrator = OrmMigrator::diesel();
    let report = migrator.migrate(dir.to_str().unwrap(), true).unwrap();
    assert!(report.source_unchanged, "source_unchanged 必须为 true");
    // 文件内容未变
    let after = fs::read_to_string(dir.join("src/models.rs")).unwrap();
    assert_eq!(after, content);
    fs::remove_dir_all(&dir).ok();
}

/// 危险 DDL 不自动执行
#[test]
fn test_no_dangerous_ddl() {
    let content = r#"
#[diesel(table_name = users)]
pub struct User { pub id: i64 }
"#;
    let dir = make_temp_project(content);
    let migrator = OrmMigrator::diesel();
    let report = migrator.migrate(dir.to_str().unwrap(), true).unwrap();
    // 本模块仅生成 CREATE TABLE（安全 DDL），不生成 DROP/TRUNCATE
    assert!(!report.has_dangerous_ddl, "不应含危险 DDL");
    for m in &report.mappings {
        assert!(!m.migration_sql.contains("DROP"));
        assert!(!m.migration_sql.contains("TRUNCATE"));
    }
    fs::remove_dir_all(&dir).ok();
}

/// 不可识别源 ORM 返回错误（路径不存在）
#[test]
fn test_nonexistent_source_returns_error() {
    let migrator = OrmMigrator::diesel();
    let result = migrator.migrate("/nonexistent/path/zzz_12345", true);
    assert!(result.is_err());

    let migrator = OrmMigrator::sea_orm();
    let result = migrator.migrate("/nonexistent/path/zzz_12345", true);
    assert!(result.is_err());

    let migrator = OrmMigrator::sqlx();
    let result = migrator.migrate("/nonexistent/path/zzz_12345", true);
    assert!(result.is_err());
}

/// dry_run=false 仍不执行 DDL（本模块不连库）
#[test]
fn test_dry_run_false_still_safe() {
    let content = r#"
#[diesel(table_name = users)]
pub struct User { pub id: i64, pub name: String }
"#;
    let dir = make_temp_project(content);
    let migrator = OrmMigrator::diesel();
    let report = migrator.migrate(dir.to_str().unwrap(), false).unwrap();
    // dry_run=false 也不连库执行，source_unchanged 仍为 true
    assert!(report.source_unchanged);
    assert!(!report.has_dangerous_ddl);
    fs::remove_dir_all(&dir).ok();
}

/// 三种解析器 orm_type 正确
#[test]
fn test_parser_orm_types() {
    assert_eq!(DieselParser.orm_type(), SourceOrm::Diesel);
    assert_eq!(SeaOrmParser.orm_type(), SourceOrm::SeaOrm);
    assert_eq!(SqlxParser.orm_type(), SourceOrm::Sqlx);
}

/// 迁移报告序列化
#[test]
fn test_migration_report_serde() {
    let content = r#"
#[diesel(table_name = users)]
pub struct User { pub id: i64, pub name: String }
"#;
    let dir = make_temp_project(content);
    let migrator = OrmMigrator::diesel();
    let report = migrator.migrate(dir.to_str().unwrap(), true).unwrap();
    let json = serde_json::to_string(&report).unwrap();
    let back: sz_orm_ai_migration::MigrationReport = serde_json::from_str(&json).unwrap();
    assert_eq!(report.source_orm, back.source_orm);
    assert_eq!(report.mapping_count, back.mapping_count);
    fs::remove_dir_all(&dir).ok();
}
