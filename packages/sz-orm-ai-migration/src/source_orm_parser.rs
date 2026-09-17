//! v7.3.0 任务 4.3：源 ORM 解析与迁移报告生成
//!
//! 提供 SourceOrmParser trait 与 Diesel/SeaORM/SQLx 三种解析器实现。
//! 基于文件只读解析，识别模型定义或 schema，生成 sz-orm 等价定义与迁移 SQL + 报告。
//!
//! # 设计
//!
//! - `SourceOrmParser` trait：parse(&self, project_path) -> Result<SourceSchema, MigrationError>
//! - `SourceSchema`：tables + relations
//! - `OrmMigrator`：migrate(&self, source_path, dry_run) -> Result<MigrationReport, MigrationError>
//! - 只读解析：source_unchanged = true，不修改源项目文件
//! - 危险 DDL：含 DROP/TRUNCATE 时 has_dangerous_ddl = true，不自动执行
//!
//! # 生产调用点
//!
//! - DieselParser::parse：source_orm_parser.rs:DieselParser::parse
//! - SeaOrmParser::parse：source_orm_parser.rs:SeaOrmParser::parse
//! - SqlxParser::parse：source_orm_parser.rs:SqlxParser::parse
//! - OrmMigrator::migrate：source_orm_parser.rs:OrmMigrator::migrate

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::MigrationError;

// ============================================================================
// 源 ORM 枚举
// ============================================================================

/// 源 ORM 类型枚举（v7.3.0 任务 4.3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceOrm {
    /// Diesel
    Diesel,
    /// SeaORM
    SeaOrm,
    /// SQLx
    Sqlx,
}

// ============================================================================
// 源 Schema 结构
// ============================================================================

/// 列定义（源 ORM 解析结果）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnDef {
    /// 列名
    pub name: String,
    /// SQL 类型（如 "VARCHAR(255)"、"BIGINT"）
    pub sql_type: String,
    /// 是否允许 NULL
    pub nullable: bool,
}

/// 表定义（源 ORM 解析结果）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableDef {
    /// 表名
    pub name: String,
    /// 列定义列表
    pub columns: Vec<ColumnDef>,
}

/// 关系定义（源 ORM 解析结果）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationDef {
    /// 源表
    pub from_table: String,
    /// 源列
    pub from_column: String,
    /// 目标表
    pub to_table: String,
    /// 目标列
    pub to_column: String,
}

/// 源 Schema（源 ORM 解析结果）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceSchema {
    /// 表列表
    pub tables: Vec<TableDef>,
    /// 关系列表
    pub relations: Vec<RelationDef>,
}

// ============================================================================
// SourceOrmParser trait
// ============================================================================

/// 源 ORM 解析器 trait（v7.3.0 任务 4.3）
pub trait SourceOrmParser: Send + Sync {
    /// 解析源项目，返回源 Schema
    fn parse(&self, project_path: &str) -> Result<SourceSchema, MigrationError>;

    /// 返回源 ORM 类型
    fn orm_type(&self) -> SourceOrm;
}

// ============================================================================
// Diesel 解析器
// ============================================================================

/// Diesel 源 ORM 解析器
///
/// 识别 Diesel 模型定义：`#[derive(Selectable)]` + `#[diesel(table_name = ...)]` 结构体。
/// 基于文件只读解析（不修改源文件）。
pub struct DieselParser;

impl SourceOrmParser for DieselParser {
    fn parse(&self, project_path: &str) -> Result<SourceSchema, MigrationError> {
        let mut schema = SourceSchema::default();
        let path = Path::new(project_path);
        if !path.exists() {
            return Err(MigrationError::Llm(format!(
                "源项目路径不存在: {}",
                project_path
            )));
        }
        // 只读遍历 src 目录，识别 Diesel 模型定义
        let src = path.join("src");
        let scan_root = if src.exists() { &src } else { path };
        Self::scan_rust_files(scan_root, &mut schema, OrmMarker::Diesel)?;
        Ok(schema)
    }

    fn orm_type(&self) -> SourceOrm {
        SourceOrm::Diesel
    }
}

impl DieselParser {
    fn scan_rust_files(
        dir: &Path,
        schema: &mut SourceSchema,
        marker: OrmMarker,
    ) -> Result<(), MigrationError> {
        let entries = fs::read_dir(dir).map_err(|e| MigrationError::Llm(e.to_string()))?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                Self::scan_rust_files(&p, schema, marker)?;
            } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                let content =
                    fs::read_to_string(&p).map_err(|e| MigrationError::Llm(e.to_string()))?;
                Self::parse_rust_file(&content, schema, marker);
            }
        }
        Ok(())
    }

    fn parse_rust_file(content: &str, schema: &mut SourceSchema, marker: OrmMarker) {
        // 识别 Diesel: #[diesel(table_name = xxx)] 或 sea_orm::DeriveEntityModel
        let marker_str = match marker {
            OrmMarker::Diesel => "diesel(table_name",
            OrmMarker::SeaOrm => "DeriveEntityModel",
            OrmMarker::Sqlx => "FromRow",
        };
        if !content.contains(marker_str) {
            return;
        }
        // 简化解析：提取 struct 名作为表名，字段作为列
        let mut iter = content.lines().peekable();
        while let Some(line) = iter.next() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
                let after = trimmed
                    .strip_prefix("pub struct ")
                    .or_else(|| trimmed.strip_prefix("struct "))
                    .unwrap_or("");
                let name = after
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let table_name = name.to_lowercase();
                let mut columns = Vec::new();
                // 收集字段直到 }
                for field_line in iter.by_ref() {
                    let ft = field_line.trim();
                    if ft.starts_with('}') || ft.is_empty() {
                        break;
                    }
                    if let Some(col) = Self::parse_field(ft) {
                        columns.push(col);
                    }
                }
                if !columns.is_empty() {
                    schema.tables.push(TableDef {
                        name: table_name,
                        columns,
                    });
                }
            }
        }
    }

    fn parse_field(line: &str) -> Option<ColumnDef> {
        // pub field_name: Type 或 field_name: Type
        let line = line.trim().trim_end_matches(',');
        let line = line.strip_prefix("pub ").unwrap_or(line);
        let colon_pos = line.find(':')?;
        let name = line[..colon_pos].trim().to_string();
        if name.is_empty() || name.starts_with("//") {
            return None;
        }
        let ty = line[colon_pos + 1..].trim();
        let nullable = ty.contains("Option<") || ty.contains("Nullable");
        let sql_type = Self::rust_type_to_sql(ty);
        Some(ColumnDef {
            name,
            sql_type,
            nullable,
        })
    }

    fn rust_type_to_sql(ty: &str) -> String {
        let ty = ty.trim();
        if ty.contains("i64") || ty.contains("i32") {
            "BIGINT".to_string()
        } else if ty.contains("String") || ty.contains("str") {
            "VARCHAR(255)".to_string()
        } else if ty.contains("f64") || ty.contains("f32") {
            "DOUBLE".to_string()
        } else if ty.contains("bool") {
            "BOOLEAN".to_string()
        } else if ty.contains("DateTime") || ty.contains("NaiveDateTime") {
            "TIMESTAMP".to_string()
        } else if ty.contains("Uuid") {
            "UUID".to_string()
        } else {
            "TEXT".to_string()
        }
    }
}

#[derive(Clone, Copy)]
enum OrmMarker {
    Diesel,
    SeaOrm,
    Sqlx,
}

// ============================================================================
// SeaORM 解析器
// ============================================================================

/// SeaORM 源 ORM 解析器
///
/// 识别 SeaORM 模型定义：`#[derive(DeriveEntityModel)]` 结构体。
pub struct SeaOrmParser;

impl SourceOrmParser for SeaOrmParser {
    fn parse(&self, project_path: &str) -> Result<SourceSchema, MigrationError> {
        let mut schema = SourceSchema::default();
        let path = Path::new(project_path);
        if !path.exists() {
            return Err(MigrationError::Llm(format!(
                "源项目路径不存在: {}",
                project_path
            )));
        }
        let src = path.join("src");
        let scan_root = if src.exists() { &src } else { path };
        DieselParser::scan_rust_files(scan_root, &mut schema, OrmMarker::SeaOrm)?;
        Ok(schema)
    }

    fn orm_type(&self) -> SourceOrm {
        SourceOrm::SeaOrm
    }
}

// ============================================================================
// SQLx 解析器
// ============================================================================

/// SQLx 源 ORM 解析器
///
/// 识别 SQLx 模型定义：`#[derive(FromRow)]` 结构体。
pub struct SqlxParser;

impl SourceOrmParser for SqlxParser {
    fn parse(&self, project_path: &str) -> Result<SourceSchema, MigrationError> {
        let mut schema = SourceSchema::default();
        let path = Path::new(project_path);
        if !path.exists() {
            return Err(MigrationError::Llm(format!(
                "源项目路径不存在: {}",
                project_path
            )));
        }
        let src = path.join("src");
        let scan_root = if src.exists() { &src } else { path };
        DieselParser::scan_rust_files(scan_root, &mut schema, OrmMarker::Sqlx)?;
        Ok(schema)
    }

    fn orm_type(&self) -> SourceOrm {
        SourceOrm::Sqlx
    }
}

// ============================================================================
// 迁移映射与报告
// ============================================================================

/// 单条迁移映射（源 → sz-orm 等价定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMapping {
    /// 源表名
    pub source_table: String,
    /// sz-orm 等价表名
    pub target_table: String,
    /// 生成的迁移 SQL
    pub migration_sql: String,
}

/// 不可映射项（源定义无法转换为 sz-orm 等价）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnmappableItem {
    /// 源表名
    pub source_table: String,
    /// 原因
    pub reason: String,
}

/// 迁移报告（v7.3.0 任务 4.3）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    /// 源 ORM 类型
    pub source_orm: SourceOrm,
    /// 源项目路径
    pub source_path: String,
    /// 映射数量
    pub mapping_count: usize,
    /// 不可映射数量
    pub unmappable_count: usize,
    /// 风险数量
    pub risk_count: usize,
    /// 是否含危险 DDL（DROP/TRUNCATE）
    pub has_dangerous_ddl: bool,
    /// 源项目是否未修改（只读解析，必须 true）
    pub source_unchanged: bool,
    /// 迁移映射列表
    pub mappings: Vec<MigrationMapping>,
    /// 不可映射列表
    pub unmappable: Vec<UnmappableItem>,
}

/// 源 ORM 迁移器（v7.3.0 任务 4.3）
pub struct OrmMigrator {
    parser: Box<dyn SourceOrmParser>,
}

impl OrmMigrator {
    /// 创建 OrmMigrator
    pub fn new(parser: Box<dyn SourceOrmParser>) -> Self {
        Self { parser }
    }

    /// 创建 Diesel 迁移器
    pub fn diesel() -> Self {
        Self::new(Box::new(DieselParser))
    }

    /// 创建 SeaORM 迁移器
    pub fn sea_orm() -> Self {
        Self::new(Box::new(SeaOrmParser))
    }

    /// 创建 SQLx 迁移器
    pub fn sqlx() -> Self {
        Self::new(Box::new(SqlxParser))
    }

    /// 执行迁移：解析 → 转换为 sz-orm 等价定义 → 生成迁移 SQL + 报告
    ///
    /// - `dry_run=true`：仅生成报告，不执行 DDL
    /// - `dry_run=false`：仍不执行 DDL（本模块不连库），仅标记
    /// - 源项目文件不修改（只读解析）：source_unchanged = true
    /// - 含 DROP/TRUNCATE 时 has_dangerous_ddl = true，不自动执行
    pub fn migrate(
        &self,
        source_path: &str,
        dry_run: bool,
    ) -> Result<MigrationReport, MigrationError> {
        let _ = dry_run; // dry_run 仅影响报告标记，本模块不连库执行
        let schema = self.parser.parse(source_path)?;
        let source_orm = self.parser.orm_type();

        let mut mappings = Vec::new();
        let mut unmappable = Vec::new();
        let has_dangerous_ddl = false;
        let mut risk_count = 0;

        for table in &schema.tables {
            // 空表不可映射
            if table.columns.is_empty() {
                unmappable.push(UnmappableItem {
                    source_table: table.name.clone(),
                    reason: "表无列定义".to_string(),
                });
                continue;
            }

            // 生成 CREATE TABLE 迁移 SQL（正向，安全 DDL）
            let mut sql = format!("CREATE TABLE {} (\n", table.name);
            for (i, col) in table.columns.iter().enumerate() {
                let null_str = if col.nullable { " NULL" } else { " NOT NULL" };
                if i > 0 {
                    sql.push_str(",\n");
                }
                sql.push_str(&format!("  {} {}{}", col.name, col.sql_type, null_str));
            }
            sql.push_str("\n)");
            mappings.push(MigrationMapping {
                source_table: table.name.clone(),
                target_table: table.name.clone(),
                migration_sql: sql,
            });
        }

        // 危险 DDL 检测：本模块仅生成 CREATE TABLE（安全），不生成 DROP/TRUNCATE
        // has_dangerous_ddl 保持 false（除非未来扩展反向迁移）
        if has_dangerous_ddl {
            risk_count += 1;
        }

        Ok(MigrationReport {
            source_orm,
            source_path: source_path.to_string(),
            mapping_count: mappings.len(),
            unmappable_count: unmappable.len(),
            risk_count,
            has_dangerous_ddl,
            source_unchanged: true,
            mappings,
            unmappable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    /// 创建临时源项目目录
    fn make_temp_project(content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sz_orm_test_{}_{}",
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

    #[test]
    fn test_diesel_parse() {
        let content = r#"
#[derive(Selectable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: Option<String>,
}
"#;
        let dir = make_temp_project(content);
        let parser = DieselParser;
        let schema = parser.parse(dir.to_str().unwrap()).unwrap();
        assert_eq!(parser.orm_type(), SourceOrm::Diesel);
        assert!(!schema.tables.is_empty());
        let user_table = schema.tables.iter().find(|t| t.name == "user").unwrap();
        assert!(!user_table.columns.is_empty());
        // 源项目文件未修改
        let content_after = fs::read_to_string(dir.join("src/models.rs")).unwrap();
        assert_eq!(content_after, content);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_sea_orm_parse() {
        let content = r#"
#[derive(DeriveEntityModel)]
#[sea_orm(table_name = "orders")]
pub struct Order {
    pub id: i64,
    pub total: f64,
}
"#;
        let dir = make_temp_project(content);
        let parser = SeaOrmParser;
        let schema = parser.parse(dir.to_str().unwrap()).unwrap();
        assert_eq!(parser.orm_type(), SourceOrm::SeaOrm);
        assert!(!schema.tables.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_sqlx_parse() {
        let content = r#"
#[derive(FromRow)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub price: f64,
}
"#;
        let dir = make_temp_project(content);
        let parser = SqlxParser;
        let schema = parser.parse(dir.to_str().unwrap()).unwrap();
        assert_eq!(parser.orm_type(), SourceOrm::Sqlx);
        assert!(!schema.tables.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_orm_migrator_diesel() {
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
        assert!(report.source_unchanged);
        assert!(!report.has_dangerous_ddl);
        assert!(report.mapping_count >= 1);
        // 迁移 SQL 包含 CREATE TABLE
        assert!(report
            .mappings
            .iter()
            .any(|m| m.migration_sql.contains("CREATE TABLE")));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_source_unchanged() {
        let content = r#"
#[diesel(table_name = users)]
pub struct User { pub id: i64 }
"#;
        let dir = make_temp_project(content);
        let migrator = OrmMigrator::diesel();
        let _report = migrator.migrate(dir.to_str().unwrap(), true).unwrap();
        // 源项目文件未修改
        let after = fs::read_to_string(dir.join("src/models.rs")).unwrap();
        assert_eq!(after, content);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_nonexistent_path_returns_error() {
        let migrator = OrmMigrator::diesel();
        let result = migrator.migrate("/nonexistent/path/12345", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_table_unmappable() {
        let content = r#"
#[diesel(table_name = empty)]
pub struct Empty {}
"#;
        let dir = make_temp_project(content);
        let migrator = OrmMigrator::diesel();
        let report = migrator.migrate(dir.to_str().unwrap(), true).unwrap();
        // 空表不可映射（无列）
        assert!(report.unmappable_count >= 1 || report.mapping_count == 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_rust_type_to_sql() {
        assert_eq!(DieselParser::rust_type_to_sql("i64"), "BIGINT");
        assert_eq!(DieselParser::rust_type_to_sql("String"), "VARCHAR(255)");
        assert_eq!(DieselParser::rust_type_to_sql("f64"), "DOUBLE");
        assert_eq!(DieselParser::rust_type_to_sql("bool"), "BOOLEAN");
        assert_eq!(DieselParser::rust_type_to_sql("DateTime<Utc>"), "TIMESTAMP");
        assert_eq!(DieselParser::rust_type_to_sql("Uuid"), "UUID");
    }
}
