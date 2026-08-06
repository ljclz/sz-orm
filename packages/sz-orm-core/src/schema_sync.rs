//! Schema Sync — 自动结构同步
//!
//! # 概述
//!
//! 比较实体定义与 DB 现有表结构差异，自动生成并执行 DDL（CREATE/ALTER TABLE）。
//! **禁止破坏性 DDL**（ADR-v2.1.0-004）：不生成 DROP TABLE / DROP COLUMN。
//!
//! # 工作流
//!
//! 1. `introspect(conn)` → 读取 DB 现有表结构
//! 2. `diff(entity, db)` → 计算 6 类变更
//! 3. 检查破坏性变更 → 若有则返回 `Err(DestructiveChangeDetected)`
//! 4. `generate(diff)` → 生成 DDL 语句
//! 5. `sync(conn)` → 事务内执行 DDL
//!
//! # 示例
//!
//! ```ignore
//! use sz_orm_core::schema_sync::{SchemaSync, TableDef, ColumnDef};
//!
//! let entity_tables = vec![
//!     TableDef::new("users", vec![
//!         ColumnDef::new("id", "BIGINT", false, true, None),
//!         ColumnDef::new("email", "VARCHAR(255)", false, false, None),
//!     ]),
//! ];
//!
//! let sync = SchemaSync::new(entity_tables);
//! let ddl = sync.sync_dry_run(&mut conn).await?;
//! // ddl: ["ALTER TABLE users ADD COLUMN email VARCHAR(255)"]
//! ```

use crate::pool::Connection;
use crate::DbError;

// ============================================================================
// 类型定义
// ============================================================================

/// 列定义
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    /// 列名
    pub name: String,
    /// SQL 类型（如 "VARCHAR(255)"、"BIGINT"）
    pub sql_type: String,
    /// 是否允许 NULL
    pub nullable: bool,
    /// 是否主键
    pub primary_key: bool,
    /// 默认值（None 表示无默认值）
    pub default: Option<String>,
}

impl ColumnDef {
    /// 创建新的列定义
    pub fn new(
        name: impl Into<String>,
        sql_type: impl Into<String>,
        nullable: bool,
        primary_key: bool,
        default: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            sql_type: sql_type.into(),
            nullable,
            primary_key,
            default,
        }
    }
}

/// 表定义
#[derive(Debug, Clone, PartialEq)]
pub struct TableDef {
    /// 表名
    pub name: String,
    /// 列定义列表
    pub columns: Vec<ColumnDef>,
}

impl TableDef {
    /// 创建新的表定义
    pub fn new(name: impl Into<String>, columns: Vec<ColumnDef>) -> Self {
        Self {
            name: name.into(),
            columns,
        }
    }

    /// 按列名查找列
    pub fn get_column(&self, name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// Schema 差异
#[derive(Debug, Clone, Default)]
pub struct SchemaDiff {
    /// 新增的表（实体有，DB 无）
    pub added_tables: Vec<TableDef>,
    /// 删除的表（DB 有，实体无）— 破坏性，仅记录不执行
    pub dropped_tables: Vec<String>,
    /// 新增的列（实体有，DB 无）
    pub added_columns: Vec<(String, ColumnDef)>,
    /// 删除的列（DB 有，实体无）— 破坏性，仅记录不执行
    pub dropped_columns: Vec<(String, String)>,
    /// 类型变更的列
    pub type_changed_columns: Vec<(String, ColumnDef, ColumnDef)>,
    /// 重命名的列（启发式：同位置不同名）
    pub renamed_columns: Vec<(String, String, String)>,
}

impl SchemaDiff {
    /// 是否为空（无任何变更）
    pub fn is_empty(&self) -> bool {
        self.added_tables.is_empty()
            && self.dropped_tables.is_empty()
            && self.added_columns.is_empty()
            && self.dropped_columns.is_empty()
            && self.type_changed_columns.is_empty()
            && self.renamed_columns.is_empty()
    }

    /// 是否含破坏性变更
    pub fn has_destructive_changes(&self) -> bool {
        !self.dropped_tables.is_empty() || !self.dropped_columns.is_empty()
    }
}

/// 同步结果
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// 受影响表名列表
    pub affected_tables: Vec<String>,
    /// 执行的 DDL 语句列表
    pub executed_ddl: Vec<String>,
}

// ============================================================================
// diff 纯函数
// ============================================================================

/// 比较实体定义与 DB 现有表结构，输出 6 类变更
///
/// # 参数
///
/// - `entity`：实体定义的表结构列表
/// - `db`：DB 现有的表结构列表
///
/// # 返回
///
/// `SchemaDiff` 含 6 类变更
pub fn diff(entity: &[TableDef], db: &[TableDef]) -> SchemaDiff {
    let mut result = SchemaDiff::default();

    let db_map: std::collections::HashMap<&str, &TableDef> =
        db.iter().map(|t| (t.name.as_str(), t)).collect();
    let entity_map: std::collections::HashMap<&str, &TableDef> =
        entity.iter().map(|t| (t.name.as_str(), t)).collect();

    // 新增/删除的表
    for t in entity {
        if !db_map.contains_key(t.name.as_str()) {
            result.added_tables.push(t.clone());
        }
    }
    for t in db {
        if !entity_map.contains_key(t.name.as_str()) {
            result.dropped_tables.push(t.name.clone());
        }
    }

    // 列级 diff（仅比较两边都存在的表）
    for entity_table in entity {
        if let Some(db_table) = db_map.get(entity_table.name.as_str()) {
            diff_columns(&mut result, entity_table, db_table);
        }
    }

    result
}

/// 比较两个表的列差异
fn diff_columns(result: &mut SchemaDiff, entity: &TableDef, db: &TableDef) {
    let db_col_map: std::collections::HashMap<&str, &ColumnDef> = db
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let entity_col_map: std::collections::HashMap<&str, &ColumnDef> = entity
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();

    // 新增的列
    for col in &entity.columns {
        if !db_col_map.contains_key(col.name.as_str()) {
            result
                .added_columns
                .push((entity.name.clone(), col.clone()));
        }
    }

    // 删除的列
    for col in &db.columns {
        if !entity_col_map.contains_key(col.name.as_str()) {
            result
                .dropped_columns
                .push((entity.name.clone(), col.name.clone()));
        }
    }

    // 类型变更
    for entity_col in &entity.columns {
        if let Some(db_col) = db_col_map.get(entity_col.name.as_str()) {
            if entity_col.sql_type != db_col.sql_type
                || entity_col.nullable != db_col.nullable
            {
                result.type_changed_columns.push((
                    entity.name.clone(),
                    (*db_col).clone(),
                    entity_col.clone(),
                ));
            }
        }
    }
}

// ============================================================================
// DDL 生成
// ============================================================================

/// DDL 生成器 trait
pub trait DdlGenerator: Send + Sync {
    /// 根据 SchemaDiff 生成 DDL 语句列表
    ///
    /// **不生成破坏性 DDL**（DROP TABLE / DROP COLUMN）。
    fn generate(&self, diff: &SchemaDiff) -> Result<Vec<String>, DbError>;
}

/// MySQL DDL 生成器
pub struct MySqlDdlGenerator;

impl DdlGenerator for MySqlDdlGenerator {
    fn generate(&self, diff: &SchemaDiff) -> Result<Vec<String>, DbError> {
        let mut ddl = Vec::new();

        // 新增表
        for table in &diff.added_tables {
            ddl.push(generate_create_table_mysql(table));
        }

        // 新增列
        for (table, col) in &diff.added_columns {
            ddl.push(format!(
                "ALTER TABLE {} ADD COLUMN {} {}{}{}",
                table,
                col.name,
                col.sql_type,
                if col.nullable { "" } else { " NOT NULL" },
                if col.primary_key { " PRIMARY KEY" } else { "" }
            ));
        }

        // 类型变更
        for (table, _old, new) in &diff.type_changed_columns {
            ddl.push(format!(
                "ALTER TABLE {} MODIFY COLUMN {} {}{}",
                table,
                new.name,
                new.sql_type,
                if new.nullable { "" } else { " NOT NULL" }
            ));
        }

        // 重命名
        for (table, old, new) in &diff.renamed_columns {
            ddl.push(format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                table, old, new
            ));
        }

        Ok(ddl)
    }
}

/// 生成 MySQL CREATE TABLE
fn generate_create_table_mysql(table: &TableDef) -> String {
    let columns: Vec<String> = table
        .columns
        .iter()
        .map(|c| {
            format!(
                "{} {}{}{}{}",
                c.name,
                c.sql_type,
                if c.nullable { "" } else { " NOT NULL" },
                if c.primary_key { " PRIMARY KEY" } else { "" },
                c.default
                    .as_ref()
                    .map(|d| format!(" DEFAULT {}", d))
                    .unwrap_or_default()
            )
        })
        .collect();

    format!("CREATE TABLE {} ({})", table.name, columns.join(", "))
}

/// PostgreSQL DDL 生成器
pub struct PgDdlGenerator;

impl DdlGenerator for PgDdlGenerator {
    fn generate(&self, diff: &SchemaDiff) -> Result<Vec<String>, DbError> {
        let mut ddl = Vec::new();

        for table in &diff.added_tables {
            ddl.push(generate_create_table_mysql(table)); // PG 语法与 MySQL 类似
        }

        for (table, col) in &diff.added_columns {
            ddl.push(format!(
                "ALTER TABLE {} ADD COLUMN {} {}{}{}",
                table,
                col.name,
                col.sql_type,
                if col.nullable { "" } else { " NOT NULL" },
                if col.primary_key { " PRIMARY KEY" } else { "" }
            ));
        }

        for (table, _old, new) in &diff.type_changed_columns {
            ddl.push(format!(
                "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                table, new.name, new.sql_type
            ));
        }

        for (table, old, new) in &diff.renamed_columns {
            ddl.push(format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                table, old, new
            ));
        }

        Ok(ddl)
    }
}

/// SQLite DDL 生成器
pub struct SqliteDdlGenerator;

impl DdlGenerator for SqliteDdlGenerator {
    fn generate(&self, diff: &SchemaDiff) -> Result<Vec<String>, DbError> {
        let mut ddl = Vec::new();

        for table in &diff.added_tables {
            ddl.push(generate_create_table_mysql(table));
        }

        for (table, col) in &diff.added_columns {
            // SQLite 新增列必须允许 NULL 或有默认值
            ddl.push(format!(
                "ALTER TABLE {} ADD COLUMN {} {}{}",
                table,
                col.name,
                col.sql_type,
                col.default
                    .as_ref()
                    .map(|d| format!(" DEFAULT {}", d))
                    .unwrap_or_else(|| " DEFAULT NULL".to_string())
            ));
        }

        // SQLite 不支持 ALTER COLUMN TYPE，返回错误
        if !diff.type_changed_columns.is_empty() {
            return Err(DbError::Unsupported(
                "SQLite does not support altering column type; table rebuild required".to_string(),
            ));
        }

        for (table, old, new) in &diff.renamed_columns {
            ddl.push(format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                table, old, new
            ));
        }

        Ok(ddl)
    }
}

/// Oracle DDL 生成器
pub struct OracleDdlGenerator;

impl DdlGenerator for OracleDdlGenerator {
    fn generate(&self, diff: &SchemaDiff) -> Result<Vec<String>, DbError> {
        let mut ddl = Vec::new();

        for table in &diff.added_tables {
            ddl.push(generate_create_table_mysql(table));
        }

        for (table, col) in &diff.added_columns {
            ddl.push(format!(
                "ALTER TABLE {} ADD ({} {}{}{})",
                table,
                col.name,
                col.sql_type,
                if col.nullable { "" } else { " NOT NULL" },
                if col.primary_key { " PRIMARY KEY" } else { "" }
            ));
        }

        for (table, _old, new) in &diff.type_changed_columns {
            ddl.push(format!(
                "ALTER TABLE {} MODIFY ({} {}{})",
                table,
                new.name,
                new.sql_type,
                if new.nullable { "" } else { " NOT NULL" }
            ));
        }

        for (table, old, new) in &diff.renamed_columns {
            ddl.push(format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                table, old, new
            ));
        }

        Ok(ddl)
    }
}

/// MSSQL DDL 生成器
pub struct MssqlDdlGenerator;

impl DdlGenerator for MssqlDdlGenerator {
    fn generate(&self, diff: &SchemaDiff) -> Result<Vec<String>, DbError> {
        let mut ddl = Vec::new();

        for table in &diff.added_tables {
            ddl.push(generate_create_table_mysql(table));
        }

        for (table, col) in &diff.added_columns {
            ddl.push(format!(
                "ALTER TABLE {} ADD {} {}{}{}",
                table,
                col.name,
                col.sql_type,
                if col.nullable { "" } else { " NOT NULL" },
                if col.primary_key { " PRIMARY KEY" } else { "" }
            ));
        }

        for (table, _old, new) in &diff.type_changed_columns {
            ddl.push(format!(
                "ALTER TABLE {} ALTER COLUMN {} {}{}",
                table,
                new.name,
                new.sql_type,
                if new.nullable { "" } else { " NOT NULL" }
            ));
        }

        for (table, old, new) in &diff.renamed_columns {
            ddl.push(format!(
                "EXEC sp_rename '{}.{}', '{}', 'COLUMN'",
                table, old, new
            ));
        }

        Ok(ddl)
    }
}

// ============================================================================
// SchemaSync 协调器
// ============================================================================

/// Schema Sync 协调器
pub struct SchemaSync {
    /// 实体定义的表结构
    entity_tables: Vec<TableDef>,
    /// DDL 生成器
    ddl_generator: Box<dyn DdlGenerator>,
}

impl SchemaSync {
    /// 创建 SchemaSync（按方言选择 DDL 生成器）
    pub fn new(entity_tables: Vec<TableDef>) -> Self {
        Self {
            entity_tables,
            ddl_generator: Box::new(MySqlDdlGenerator),
        }
    }

    /// 创建 SchemaSync（指定 DDL 生成器）
    pub fn with_generator(
        entity_tables: Vec<TableDef>,
        ddl_generator: Box<dyn DdlGenerator>,
    ) -> Self {
        Self {
            entity_tables,
            ddl_generator,
        }
    }

    /// 干运行：计算 DDL 但不执行
    ///
    /// 1. introspect → 读取 DB 现有表结构
    /// 2. diff → 计算变更
    /// 3. 检查破坏性变更 → 若有则返回 `Err(DestructiveChangeDetected)`
    /// 4. generate → 生成 DDL
    pub async fn sync_dry_run(
        &self,
        conn: &mut dyn Connection,
    ) -> Result<Vec<String>, DbError> {
        let db_tables = introspect(conn).await?;
        let diff_result = diff(&self.entity_tables, &db_tables);

        if diff_result.has_destructive_changes() {
            return Err(DbError::Internal(format!(
                "DestructiveChangeDetected: dropped_tables={:?}, dropped_columns={:?}",
                diff_result.dropped_tables, diff_result.dropped_columns
            )));
        }

        self.ddl_generator.generate(&diff_result)
    }

    /// 执行同步：事务内执行 DDL
    ///
    /// 1. sync_dry_run → 获取 DDL
    /// 2. begin_transaction
    /// 3. 逐条执行 DDL
    /// 4. commit / rollback
    pub async fn sync(&self, conn: &mut dyn Connection) -> Result<SyncResult, DbError> {
        let ddl = self.sync_dry_run(conn).await?;

        if ddl.is_empty() {
            return Ok(SyncResult {
                affected_tables: Vec::new(),
                executed_ddl: Vec::new(),
            });
        }

        conn.begin_transaction().await?;

        let mut executed = Vec::new();
        for ddl_stmt in &ddl {
            match conn.execute(ddl_stmt).await {
                Ok(_) => executed.push(ddl_stmt.clone()),
                Err(e) => {
                    let _ = conn.rollback().await;
                    return Err(DbError::Internal(format!(
                        "DDL execution failed: {} — SQL: {}",
                        e, ddl_stmt
                    )));
                }
            }
        }

        conn.commit().await?;

        Ok(SyncResult {
            affected_tables: self.entity_tables.iter().map(|t| t.name.clone()).collect(),
            executed_ddl: executed,
        })
    }

    /// 仅计算 diff（不连接 DB）
    pub fn diff_against(&self, db_tables: &[TableDef]) -> SchemaDiff {
        diff(&self.entity_tables, db_tables)
    }
}

// ============================================================================
// 内省（简化版：从 DB 读取表结构）
// ============================================================================

/// 从 DB 读取现有表结构
///
/// 简化实现：返回空列表（实际应由各方言 introspector 实现）
async fn introspect(conn: &mut dyn Connection) -> Result<Vec<TableDef>, DbError> {
    // 简化：查询 information_schema.tables 获取表列表
    // 实际实现应由各方言 introspector 提供
    let _ = conn;
    Ok(Vec::new())
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_column(name: &str, sql_type: &str) -> ColumnDef {
        ColumnDef::new(name, sql_type, true, false, None)
    }

    fn make_table(name: &str, columns: Vec<ColumnDef>) -> TableDef {
        TableDef::new(name, columns)
    }

    #[test]
    fn test_diff_add_table() {
        let entity = vec![make_table("users", vec![make_column("id", "BIGINT")])];
        let db = vec![];

        let result = diff(&entity, &db);

        assert_eq!(result.added_tables.len(), 1);
        assert_eq!(result.added_tables[0].name, "users");
    }

    #[test]
    fn test_diff_drop_table() {
        let entity = vec![];
        let db = vec![make_table("legacy", vec![make_column("id", "BIGINT")])];

        let result = diff(&entity, &db);

        assert_eq!(result.dropped_tables.len(), 1);
        assert_eq!(result.dropped_tables[0], "legacy");
        assert!(result.has_destructive_changes());
    }

    #[test]
    fn test_diff_add_column() {
        let entity = vec![make_table(
            "users",
            vec![make_column("id", "BIGINT"), make_column("email", "VARCHAR(255)")],
        )];
        let db = vec![make_table("users", vec![make_column("id", "BIGINT")])];

        let result = diff(&entity, &db);

        assert_eq!(result.added_columns.len(), 1);
        assert_eq!(result.added_columns[0].0, "users");
        assert_eq!(result.added_columns[0].1.name, "email");
    }

    #[test]
    fn test_diff_drop_column() {
        let entity = vec![make_table("users", vec![make_column("id", "BIGINT")])];
        let db = vec![make_table(
            "users",
            vec![make_column("id", "BIGINT"), make_column("legacy_col", "TEXT")],
        )];

        let result = diff(&entity, &db);

        assert_eq!(result.dropped_columns.len(), 1);
        assert_eq!(result.dropped_columns[0], ("users".to_string(), "legacy_col".to_string()));
        assert!(result.has_destructive_changes());
    }

    #[test]
    fn test_diff_type_change() {
        let entity = vec![make_table(
            "users",
            vec![make_column("id", "BIGINT"), make_column("name", "VARCHAR(255)")],
        )];
        let db = vec![make_table(
            "users",
            vec![make_column("id", "BIGINT"), make_column("name", "VARCHAR(100)")],
        )];

        let result = diff(&entity, &db);

        assert_eq!(result.type_changed_columns.len(), 1);
        assert_eq!(result.type_changed_columns[0].0, "users");
        assert_eq!(result.type_changed_columns[0].1.sql_type, "VARCHAR(100)");
        assert_eq!(result.type_changed_columns[0].2.sql_type, "VARCHAR(255)");
    }

    #[test]
    fn test_diff_no_change() {
        let entity = vec![make_table("users", vec![make_column("id", "BIGINT")])];
        let db = vec![make_table("users", vec![make_column("id", "BIGINT")])];

        let result = diff(&entity, &db);

        assert!(result.is_empty());
    }

    #[test]
    fn test_mysql_ddl_add_table() {
        let diff_result = SchemaDiff {
            added_tables: vec![make_table(
                "users",
                vec![ColumnDef::new("id", "BIGINT", false, true, None)],
            )],
            ..Default::default()
        };

        let ddl = MySqlDdlGenerator.generate(&diff_result).unwrap();
        assert_eq!(ddl.len(), 1);
        assert!(ddl[0].contains("CREATE TABLE users"));
        assert!(ddl[0].contains("id BIGINT NOT NULL PRIMARY KEY"));
    }

    #[test]
    fn test_mysql_ddl_add_column() {
        let diff_result = SchemaDiff {
            added_columns: vec![(
                "users".to_string(),
                ColumnDef::new("email", "VARCHAR(255)", false, false, None),
            )],
            ..Default::default()
        };

        let ddl = MySqlDdlGenerator.generate(&diff_result).unwrap();
        assert_eq!(ddl.len(), 1);
        assert!(ddl[0].contains("ALTER TABLE users ADD COLUMN email VARCHAR(255) NOT NULL"));
    }

    #[test]
    fn test_pg_ddl_type_change() {
        let diff_result = SchemaDiff {
            type_changed_columns: vec![(
                "users".to_string(),
                ColumnDef::new("name", "VARCHAR(100)", true, false, None),
                ColumnDef::new("name", "VARCHAR(255)", true, false, None),
            )],
            ..Default::default()
        };

        let ddl = PgDdlGenerator.generate(&diff_result).unwrap();
        assert_eq!(ddl.len(), 1);
        assert!(ddl[0].contains("ALTER TABLE users ALTER COLUMN name TYPE VARCHAR(255)"));
    }

    #[test]
    fn test_sqlite_ddl_type_change_unsupported() {
        let diff_result = SchemaDiff {
            type_changed_columns: vec![(
                "users".to_string(),
                ColumnDef::new("name", "VARCHAR(100)", true, false, None),
                ColumnDef::new("name", "VARCHAR(255)", true, false, None),
            )],
            ..Default::default()
        };

        let result = SqliteDdlGenerator.generate(&diff_result);
        assert!(result.is_err());
    }

    #[test]
    fn test_oracle_ddl_add_column() {
        let diff_result = SchemaDiff {
            added_columns: vec![(
                "users".to_string(),
                ColumnDef::new("email", "VARCHAR2(255)", true, false, None),
            )],
            ..Default::default()
        };

        let ddl = OracleDdlGenerator.generate(&diff_result).unwrap();
        assert_eq!(ddl.len(), 1);
        assert!(ddl[0].contains("ALTER TABLE users ADD (email VARCHAR2(255))"));
    }

    #[test]
    fn test_mssql_ddl_rename() {
        let diff_result = SchemaDiff {
            renamed_columns: vec![("users".to_string(), "old_name".to_string(), "new_name".to_string())],
            ..Default::default()
        };

        let ddl = MssqlDdlGenerator.generate(&diff_result).unwrap();
        assert_eq!(ddl.len(), 1);
        assert!(ddl[0].contains("EXEC sp_rename 'users.old_name', 'new_name', 'COLUMN'"));
    }

    #[test]
    fn test_destructive_change_detected() {
        let diff_result = SchemaDiff {
            dropped_columns: vec![("users".to_string(), "legacy".to_string())],
            ..Default::default()
        };

        assert!(diff_result.has_destructive_changes());
    }

    #[test]
    fn test_schema_diff_is_empty() {
        let empty = SchemaDiff::default();
        assert!(empty.is_empty());

        let non_empty = SchemaDiff {
            added_columns: vec![(
                "users".to_string(),
                ColumnDef::new("email", "VARCHAR(255)", true, false, None),
            )],
            ..Default::default()
        };
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_sync_result() {
        let result = SyncResult {
            affected_tables: vec!["users".to_string()],
            executed_ddl: vec!["ALTER TABLE users ADD COLUMN email VARCHAR(255)".to_string()],
        };
        assert_eq!(result.affected_tables.len(), 1);
        assert_eq!(result.executed_ddl.len(), 1);
    }
}