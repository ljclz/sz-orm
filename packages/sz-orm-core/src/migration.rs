//! Migration system
//!
//! Provides database schema migration management

use crate::db_type::DbType;
use crate::error::DbError;
use std::path::PathBuf;

pub struct Migration {
    pub version: String,
    pub name: String,
    pub sql_up: String,
    pub sql_down: String,
    pub batch: i32,
    pub executed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Migration {
    pub fn new(version: &str, name: &str, sql_up: &str, sql_down: &str) -> Self {
        Self {
            version: version.to_string(),
            name: name.to_string(),
            sql_up: sql_up.to_string(),
            sql_down: sql_down.to_string(),
            batch: 0,
            executed_at: None,
        }
    }

    pub fn with_batch(mut self, batch: i32) -> Self {
        self.batch = batch;
        self
    }

    pub fn with_executed_at(mut self, time: chrono::DateTime<chrono::Utc>) -> Self {
        self.executed_at = Some(time);
        self
    }
}

impl std::fmt::Debug for Migration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Migration")
            .field("version", &self.version)
            .field("name", &self.name)
            .field("batch", &self.batch)
            .finish()
    }
}

pub trait MigrationResolver: Send + Sync {
    fn resolve(&self, db_type: DbType) -> Result<Vec<Migration>, DbError>;
}

pub struct FileMigrationResolver {
    pub path: PathBuf,
}

impl FileMigrationResolver {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl MigrationResolver for FileMigrationResolver {
    fn resolve(&self, db_type: DbType) -> Result<Vec<Migration>, DbError> {
        let mut migrations = Vec::new();

        // 读取迁移目录
        let entries = match std::fs::read_dir(&self.path) {
            Ok(entries) => entries,
            Err(e) => {
                return Err(DbError::MigrationError(format!(
                    "Cannot read migration directory {}: {}",
                    self.path.display(),
                    e
                )));
            }
        };

        let _ = db_type; // 当前实现不区分数据库类型

        // 收集所有 .sql 文件
        let mut sql_files: Vec<std::path::PathBuf> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| {
                DbError::MigrationError(format!("Cannot read directory entry: {}", e))
            })?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("sql") {
                sql_files.push(path);
            }
        }

        // 按文件名排序
        sql_files.sort();

        // 解析文件名格式：<version>_<name>_up.sql 或 <version>_<name>_down.sql
        // 也支持简单的 <name>.sql（不区分 up/down）
        let mut version_map: std::collections::HashMap<
            String,
            (Option<String>, Option<String>, String),
        > = std::collections::HashMap::new();

        for path in sql_files {
            let filename = match path.file_stem().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            let content = std::fs::read_to_string(&path).map_err(|e| {
                DbError::MigrationError(format!(
                    "Cannot read migration file {}: {}",
                    path.display(),
                    e
                ))
            })?;

            // 尝试解析文件名
            if filename.ends_with("_up") {
                let base = &filename[..filename.len() - 3];
                let (version, name) = parse_migration_filename(base);
                let entry = version_map
                    .entry(version.clone())
                    .or_insert((None, None, name));
                entry.0 = Some(content);
            } else if filename.ends_with("_down") {
                let base = &filename[..filename.len() - 5];
                let (version, name) = parse_migration_filename(base);
                let entry = version_map
                    .entry(version.clone())
                    .or_insert((None, None, name));
                entry.1 = Some(content);
            } else {
                // 简单格式：整个文件作为 up SQL，down 为空
                let (version, name) = parse_migration_filename(&filename);
                let entry = version_map
                    .entry(version.clone())
                    .or_insert((None, None, name));
                if entry.0.is_none() {
                    entry.0 = Some(content);
                }
            }
        }

        // 转换为 Migration 列表并按 version 排序
        type VersionEntry = (Option<String>, Option<String>, String);
        let mut sorted_versions: Vec<(String, VersionEntry)> = version_map.into_iter().collect();
        sorted_versions.sort_by(|a, b| a.0.cmp(&b.0));

        for (version, (sql_up, sql_down, name)) in sorted_versions {
            let migration = Migration::new(
                &version,
                &name,
                sql_up.unwrap_or_default().as_str(),
                sql_down.unwrap_or_default().as_str(),
            );
            migrations.push(migration);
        }

        Ok(migrations)
    }
}

/// 解析迁移文件名：格式 <version>_<name>，如 "001_create_users"
fn parse_migration_filename(filename: &str) -> (String, String) {
    if let Some(underscore_pos) = filename.find('_') {
        let version = filename[..underscore_pos].to_string();
        let name = filename[underscore_pos + 1..].to_string();
        (version, name)
    } else {
        // 没有下划线，整个作为 version，name 为空
        (filename.to_string(), filename.to_string())
    }
}

pub struct MigrationContext {
    pub table_name: String,
    pub connection: Option<Box<dyn crate::pool::Connection>>,
    /// 数据库类型（用于判断是否支持 DDL 事务包裹）
    pub db_type: Option<DbType>,
}

impl Default for MigrationContext {
    fn default() -> Self {
        Self {
            table_name: "__migrations".to_string(),
            connection: None,
            db_type: None,
        }
    }
}

impl MigrationContext {
    /// 设置数据库类型
    pub fn with_db_type(mut self, db_type: DbType) -> Self {
        self.db_type = Some(db_type);
        self
    }
}

/// 判断指定数据库方言是否支持 DDL 事务
///
/// - PostgreSQL：✅ 支持 DDL 事务（CREATE/ALTER/DROP 可回滚）
/// - SQLite：✅ 支持 DDL 事务
/// - MySQL：❌ DDL 语句隐式提交，无法回滚
/// - Oracle：❌ DDL 语句前后隐式 COMMIT
/// - SQL Server：❌ 部分 DDL 不支持事务内执行（保守处理）
/// - 其他：❌ 默认不支持
fn supports_ddl_transactions(db_type: DbType) -> bool {
    matches!(db_type, DbType::PostgreSQL | DbType::Sqlite)
}

#[derive(Debug, Clone, PartialEq)]
pub enum MigrationDirection {
    Up,
    Down,
}

pub struct Migrator {
    context: MigrationContext,
    migrations: Vec<Migration>,
}

impl Migrator {
    pub fn new(context: MigrationContext) -> Self {
        Self {
            context,
            migrations: Vec::new(),
        }
    }

    pub fn add_migration(mut self, migration: Migration) -> Self {
        self.migrations.push(migration);
        self
    }

    pub fn add_migrations(mut self, migrations: Vec<Migration>) -> Self {
        self.migrations.extend(migrations);
        self
    }

    pub fn get_migrations(&self) -> &Vec<Migration> {
        &self.migrations
    }

    pub fn get_pending_migrations(&self) -> Vec<&Migration> {
        self.migrations.iter().filter(|m| m.batch == 0).collect()
    }

    pub fn get_applied_migrations(&self) -> Vec<&Migration> {
        self.migrations.iter().filter(|m| m.batch > 0).collect()
    }

    pub fn latest_version(&self) -> Option<&str> {
        self.migrations.last().map(|m| m.version.as_str())
    }

    pub fn find_migration(&self, version: &str) -> Option<&Migration> {
        self.migrations.iter().find(|m| m.version == version)
    }

    /// 检测迁移版本冲突（重复版本号）
    ///
    /// 返回第一个冲突的版本号（如有）。
    /// 在 `migrate`/`up`/`down` 等执行方法入口处调用，确保迁移列表无重复版本。
    pub fn check_version_conflicts(&self) -> Result<(), DbError> {
        let mut seen = std::collections::HashSet::new();
        for m in &self.migrations {
            if !seen.insert(&m.version) {
                return Err(DbError::MigrationError(format!(
                    "迁移版本冲突：版本号 '{}' 重复定义",
                    m.version
                )));
            }
        }
        Ok(())
    }

    /// 执行所有待迁移（batch=0）的 up SQL
    ///
    /// 若数据库方言支持 DDL 事务（PostgreSQL/SQLite），则用事务包裹所有待执行迁移，
    /// 任一迁移失败时回滚全部变更，避免部分迁移导致的状态不一致。
    /// 不支持 DDL 事务的方言（MySQL/Oracle/SQL Server）逐条执行，失败时保留已执行的变更。
    pub async fn migrate(&mut self) -> Result<Vec<String>, DbError> {
        // 版本冲突检测
        self.check_version_conflicts()?;

        let mut applied = Vec::new();
        let current_batch = self.migrations.iter().map(|m| m.batch).max().unwrap_or(0) + 1;

        // 收集待迁移的索引（避免在循环中再次 position()，消除 O(n²) 复杂度）
        let pending_indices: Vec<usize> = self
            .migrations
            .iter()
            .enumerate()
            .filter(|(_, m)| m.batch == 0)
            .map(|(idx, _)| idx)
            .collect();

        if pending_indices.is_empty() {
            return Ok(applied);
        }

        // 判断是否需要事务包裹
        let use_transaction = self
            .context
            .db_type
            .map(supports_ddl_transactions)
            .unwrap_or(false);

        // 开启事务（若方言支持）
        if use_transaction {
            if let Some(ref mut conn) = self.context.connection {
                conn.begin_transaction().await?;
            }
        }

        // 逐条执行迁移
        for migration_idx in &pending_indices {
            let sql_up = self.migrations[*migration_idx].sql_up.clone();

            let exec_result = async {
                if let Some(ref mut conn) = self.context.connection {
                    if !sql_up.is_empty() {
                        conn.execute(&sql_up).await?;
                    }
                }
                Ok::<(), DbError>(())
            }
            .await;

            if let Err(e) = exec_result {
                // 事务包裹下回滚
                if use_transaction {
                    if let Some(ref mut conn) = self.context.connection {
                        let _ = conn.rollback().await;
                    }
                }
                return Err(e);
            }

            // 标记为已执行
            let now = chrono::Utc::now();
            self.migrations[*migration_idx].batch = current_batch;
            self.migrations[*migration_idx].executed_at = Some(now);

            applied.push(self.migrations[*migration_idx].version.clone());
        }

        // 提交事务（若方言支持）
        if use_transaction {
            if let Some(ref mut conn) = self.context.connection {
                conn.commit().await?;
            }
        }

        Ok(applied)
    }

    /// 回滚指定版本（执行 down SQL）
    pub async fn rollback(&mut self, version: &str) -> Result<(), DbError> {
        let migration_idx = self
            .migrations
            .iter()
            .position(|m| m.version == version)
            .ok_or_else(|| DbError::MigrationError(format!("Migration {} not found", version)))?;

        if self.migrations[migration_idx].batch == 0 {
            return Err(DbError::MigrationError(format!(
                "Migration {} not applied",
                version
            )));
        }

        let sql_down = self.migrations[migration_idx].sql_down.clone();

        if let Some(ref mut conn) = self.context.connection {
            if !sql_down.is_empty() {
                conn.execute(&sql_down).await?;
            }
        }

        self.migrations[migration_idx].batch = 0;
        self.migrations[migration_idx].executed_at = None;
        Ok(())
    }

    /// 执行到指定版本（包括该版本）
    pub async fn up(&mut self, target_version: Option<&str>) -> Result<Vec<String>, DbError> {
        // 版本冲突检测
        self.check_version_conflicts()?;

        let mut applied = Vec::new();
        let current_batch = self.migrations.iter().map(|m| m.batch).max().unwrap_or(0) + 1;

        for migration in &mut self.migrations {
            if migration.batch > 0 {
                continue; // 已执行
            }

            if let Some(target) = target_version {
                if migration.version.as_str() > target {
                    break; // 超过目标版本
                }
            }

            let sql_up = migration.sql_up.clone();
            if let Some(ref mut conn) = self.context.connection {
                if !sql_up.is_empty() {
                    conn.execute(&sql_up).await?;
                }
            }

            migration.batch = current_batch;
            migration.executed_at = Some(chrono::Utc::now());
            applied.push(migration.version.clone());
        }

        Ok(applied)
    }

    /// 回滚到指定版本（执行该版本之后所有迁移的 down SQL）
    pub async fn down(&mut self, target_version: Option<&str>) -> Result<Vec<String>, DbError> {
        // 版本冲突检测
        self.check_version_conflicts()?;

        let mut rolled_back = Vec::new();

        // 从后往前回滚
        let mut indices: Vec<usize> = (0..self.migrations.len()).collect();
        indices.reverse();

        for idx in indices {
            let migration = &mut self.migrations[idx];
            if migration.batch == 0 {
                continue; // 未执行
            }

            if let Some(target) = target_version {
                if migration.version.as_str() <= target {
                    break; // 到达目标版本
                }
            }

            let sql_down = migration.sql_down.clone();
            if let Some(ref mut conn) = self.context.connection {
                if !sql_down.is_empty() {
                    conn.execute(&sql_down).await?;
                }
            }

            migration.batch = 0;
            migration.executed_at = None;
            rolled_back.push(migration.version.clone());
        }

        Ok(rolled_back)
    }

    /// 重置：回滚所有已执行的迁移，然后重新执行
    pub async fn reset(&mut self) -> Result<Vec<String>, DbError> {
        // 先全部回滚
        self.down(None).await?;
        // 再全部执行
        self.migrate().await
    }

    /// 刷新：回滚所有已执行的迁移，然后重新执行
    pub async fn refresh(&mut self) -> Result<Vec<String>, DbError> {
        self.reset().await
    }

    /// 获取迁移进度
    pub fn progress(&self) -> MigrationProgress {
        let total = self.migrations.len();
        let applied = self.migrations.iter().filter(|m| m.batch > 0).count();
        MigrationProgress::new(total, applied)
    }
}

#[derive(Debug, Clone)]
pub struct MigrationProgress {
    pub total: usize,
    pub applied: usize,
    pub pending: usize,
    pub current_batch: i32,
}

impl MigrationProgress {
    pub fn new(total: usize, applied: usize) -> Self {
        Self {
            total,
            applied,
            pending: total - applied,
            current_batch: 0,
        }
    }

    pub fn percent_complete(&self) -> f64 {
        if self.total == 0 {
            return 100.0;
        }
        (self.applied as f64 / self.total as f64) * 100.0
    }
}

pub struct SchemaBuilder {
    table_name: String,
    columns: Vec<ColumnDef>,
    indexes: Vec<IndexDef>,
    foreign_keys: Vec<ForeignKeyDef>,
    if_not_exists: bool,
}

impl SchemaBuilder {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            if_not_exists: true,
        }
    }

    pub fn add_column(mut self, column: ColumnDef) -> Self {
        self.columns.push(column);
        self
    }

    pub fn add_index(mut self, index: IndexDef) -> Self {
        self.indexes.push(index);
        self
    }

    pub fn add_foreign_key(mut self, fk: ForeignKeyDef) -> Self {
        self.foreign_keys.push(fk);
        self
    }

    pub fn if_not_exists(mut self, value: bool) -> Self {
        self.if_not_exists = value;
        self
    }

    pub fn build(&self, db_type: DbType) -> String {
        let mut sql = String::new();
        sql.push_str("CREATE TABLE ");
        if self.if_not_exists {
            sql.push_str("IF NOT EXISTS ");
        }
        sql.push_str(&self.table_name);
        sql.push_str(" (");

        let col_defs: Vec<String> = self.columns.iter().map(|c| c.build(db_type)).collect();
        sql.push_str(&col_defs.join(", "));

        for index in &self.indexes {
            sql.push_str(", ");
            sql.push_str(&index.build(db_type));
        }

        for fk in &self.foreign_keys {
            sql.push_str(", ");
            sql.push_str(&fk.build(db_type));
        }

        sql.push(')');
        sql
    }
}

#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: String,
    pub length: Option<usize>,
    pub precision: Option<(u32, u32)>,
    pub nullable: bool,
    pub default: Option<String>,
    pub auto_increment: bool,
    pub unique: bool,
    pub comment: Option<String>,
}

impl ColumnDef {
    pub fn new(name: &str, col_type: &str) -> Self {
        Self {
            name: name.to_string(),
            col_type: col_type.to_string(),
            length: None,
            precision: None,
            nullable: true,
            default: None,
            auto_increment: false,
            unique: false,
            comment: None,
        }
    }

    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }

    pub fn default(mut self, value: &str) -> Self {
        self.default = Some(value.to_string());
        self
    }

    pub fn auto_increment(mut self) -> Self {
        self.auto_increment = true;
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = Some(comment.to_string());
        self
    }

    pub fn length(mut self, len: usize) -> Self {
        self.length = Some(len);
        self
    }

    fn build(&self, db_type: DbType) -> String {
        let mut sql = format!("{} {}", self.name, self.col_type);
        if let Some(len) = self.length {
            if matches!(db_type, DbType::MySQL) {
                sql.push_str(&format!("({})", len));
            }
        }
        if self.auto_increment {
            match db_type {
                DbType::MySQL => sql.push_str(" AUTO_INCREMENT"),
                DbType::PostgreSQL => sql.push_str(" GENERATED BY DEFAULT AS IDENTITY"),
                DbType::Sqlite => sql.push_str(" AUTOINCREMENT"),
                _ => {}
            }
        }
        if !self.nullable {
            sql.push_str(" NOT NULL");
        }
        if let Some(ref def) = self.default {
            sql.push_str(&format!(" DEFAULT {}", def));
        }
        if self.unique {
            sql.push_str(" UNIQUE");
        }
        sql
    }
}

#[derive(Debug, Clone)]
pub struct IndexDef {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub index_type: Option<String>,
}

impl IndexDef {
    pub fn new(name: &str, columns: Vec<&str>) -> Self {
        Self {
            name: name.to_string(),
            columns: columns.into_iter().map(|s| s.to_string()).collect(),
            unique: false,
            index_type: None,
        }
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    fn build(&self, _db_type: DbType) -> String {
        let unique_str = if self.unique { "UNIQUE " } else { "" };
        format!(
            "{}KEY {} ({})",
            unique_str,
            self.name,
            self.columns.join(", ")
        )
    }
}

#[derive(Debug, Clone)]
pub struct ForeignKeyDef {
    pub name: String,
    pub column: String,
    pub referenced_table: String,
    pub referenced_column: String,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

impl ForeignKeyDef {
    pub fn new(name: &str, column: &str, referenced_table: &str, referenced_column: &str) -> Self {
        Self {
            name: name.to_string(),
            column: column.to_string(),
            referenced_table: referenced_table.to_string(),
            referenced_column: referenced_column.to_string(),
            on_delete: None,
            on_update: None,
        }
    }

    pub fn on_delete(mut self, action: &str) -> Self {
        self.on_delete = Some(action.to_string());
        self
    }

    pub fn on_update(mut self, action: &str) -> Self {
        self.on_update = Some(action.to_string());
        self
    }

    fn build(&self, _db_type: DbType) -> String {
        // v0.2.2 修复 C-3：FOREIGN KEY 标识符与 ON DELETE/ON UPDATE 动作严格校验
        crate::sql_safety::validate_identifier(&self.name, "foreign key constraint name")
            .expect("invalid foreign key constraint name");
        crate::sql_safety::validate_identifier(&self.column, "foreign key column")
            .expect("invalid foreign key column name");
        crate::sql_safety::validate_identifier(
            &self.referenced_table,
            "foreign key referenced table",
        )
        .expect("invalid foreign key referenced table name");
        crate::sql_safety::validate_identifier(
            &self.referenced_column,
            "foreign key referenced column",
        )
        .expect("invalid foreign key referenced column name");
        if let Some(ref on_delete) = self.on_delete {
            crate::sql_safety::validate_fk_action(on_delete).expect("invalid ON DELETE action");
        }
        if let Some(ref on_update) = self.on_update {
            crate::sql_safety::validate_fk_action(on_update).expect("invalid ON UPDATE action");
        }
        let mut sql = format!(
            "CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            self.name, self.column, self.referenced_table, self.referenced_column
        );
        if let Some(ref on_delete) = self.on_delete {
            sql.push_str(&format!(" ON DELETE {}", on_delete.trim().to_uppercase()));
        }
        if let Some(ref on_update) = self.on_update {
            sql.push_str(&format!(" ON UPDATE {}", on_update.trim().to_uppercase()));
        }
        sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_new() {
        let m = Migration::new("001", "create_users", "CREATE TABLE...", "DROP TABLE...");
        assert_eq!(m.version, "001");
        assert_eq!(m.name, "create_users");
    }

    #[test]
    fn test_migration_with_batch() {
        let m = Migration::new("001", "create_users", "UP", "DOWN").with_batch(1);
        assert_eq!(m.batch, 1);
    }

    #[test]
    fn test_migrator_latest_version() {
        let ctx = MigrationContext::default();
        let migrator = Migrator::new(ctx)
            .add_migration(Migration::new("001", "v1", "UP", "DOWN"))
            .add_migration(Migration::new("002", "v2", "UP", "DOWN"));

        assert_eq!(migrator.latest_version(), Some("002"));
    }

    #[test]
    fn test_migrator_find_migration() {
        let ctx = MigrationContext::default();
        let migrator =
            Migrator::new(ctx).add_migration(Migration::new("001", "create_users", "UP", "DOWN"));

        assert!(migrator.find_migration("001").is_some());
        assert!(migrator.find_migration("999").is_none());
    }

    #[test]
    fn test_column_def() {
        let col = ColumnDef::new("id", "INT").not_null().auto_increment();
        assert_eq!(col.name, "id");
        assert!(!col.nullable);
        assert!(col.auto_increment);
    }

    #[test]
    fn test_column_build_mysql() {
        let col = ColumnDef::new("id", "INT").not_null();
        let sql = col.build(DbType::MySQL);
        assert!(sql.contains("NOT NULL"));
    }

    #[test]
    fn test_index_build() {
        let idx = IndexDef::new("idx_name", vec!["name"]).unique();
        let sql = idx.build(DbType::MySQL);
        assert!(sql.contains("UNIQUE KEY"));
    }

    #[test]
    fn test_foreign_key_build() {
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users", "id").on_delete("CASCADE");
        let sql = fk.build(DbType::MySQL);
        assert!(sql.contains("FOREIGN KEY"));
        assert!(sql.contains("ON DELETE CASCADE"));
    }

    #[test]
    fn test_foreign_key_build_normalizes_action_case() {
        // v0.2.2 修复 C-3：动作大小写不敏感，输出统一为大写
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users", "id").on_delete("cascade");
        let sql = fk.build(DbType::MySQL);
        assert!(sql.contains("ON DELETE CASCADE"));
    }

    #[test]
    #[should_panic(expected = "invalid foreign key column name")]
    fn test_foreign_key_rejects_sql_injection_in_column() {
        let fk = ForeignKeyDef::new("fk_user", "user_id; DROP TABLE users", "users", "id");
        let _ = fk.build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "invalid foreign key referenced table name")]
    fn test_foreign_key_rejects_sql_injection_in_ref_table() {
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users; DROP TABLE users", "id");
        let _ = fk.build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "invalid ON DELETE action")]
    fn test_foreign_key_rejects_sql_injection_in_on_delete() {
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users", "id")
            .on_delete("CASCADE; DROP TABLE users");
        let _ = fk.build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "invalid ON UPDATE action")]
    fn test_foreign_key_rejects_invalid_on_update_action() {
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users", "id").on_update("EVIL_ACTION");
        let _ = fk.build(DbType::MySQL);
    }

    #[test]
    fn test_schema_builder() {
        let schema = SchemaBuilder::new("users")
            .add_column(ColumnDef::new("id", "INT").not_null().auto_increment())
            .add_column(ColumnDef::new("name", "VARCHAR").length(255));

        let sql = schema.build(DbType::MySQL);
        assert!(sql.contains("CREATE TABLE"));
        assert!(sql.contains("users"));
    }

    #[test]
    fn test_migration_progress() {
        let progress = MigrationProgress::new(10, 4);
        assert_eq!(progress.pending, 6);
        assert!((progress.percent_complete() - 40.0).abs() < 0.01);
    }
}
