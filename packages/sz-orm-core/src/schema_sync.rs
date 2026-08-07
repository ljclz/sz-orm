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

/// 破坏性同步确认枚举（v2.2.0 B-2）
///
/// 调用 [`SchemaSync::destructive_sync`] 时必须显式传入确认值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    /// 确认执行破坏性 DDL
    Yes,
    /// 拒绝执行破坏性 DDL
    No,
}

/// 数据迁移钩子 trait（v2.2.0 B-2）
///
/// 在执行破坏性 DDL 前调用，允许用户执行数据备份或迁移。
/// 手动解糖 async（与 `Connection` trait 一致）。
pub trait DataMigrationHook: Send + Sync {
    /// 删除列前钩子（可执行数据备份）
    fn before_drop_column<'a>(
        &'a self,
        conn: &'a mut dyn Connection,
        table: &'a str,
        column: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DbError>> + Send + 'a>>;

    /// 重命名列前钩子（可执行数据校验）
    fn before_rename_column<'a>(
        &'a self,
        conn: &'a mut dyn Connection,
        table: &'a str,
        old_name: &'a str,
        new_name: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DbError>> + Send + 'a>>;
}

/// 破坏性同步结果（v2.2.0 B-2）
#[derive(Debug, Clone)]
pub struct DestructiveSyncResult {
    /// 执行的 DDL 语句列表
    pub executed_ddl: Vec<String>,
    /// 调用的钩子次数
    pub hooks_called: usize,
    /// 审计日志条目数
    pub audit_entries: usize,
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
    diff_columns_with_threshold(result, entity, db, 2, 0.3);
}

/// 比较两个表的列差异（带重命名检测阈值，v2.2.0 B-2）
fn diff_columns_with_threshold(
    result: &mut SchemaDiff,
    entity: &TableDef,
    db: &TableDef,
    max_distance: usize,
    max_ratio: f64,
) {
    let db_col_map: std::collections::HashMap<&str, &ColumnDef> =
        db.columns.iter().map(|c| (c.name.as_str(), c)).collect();
    let entity_col_map: std::collections::HashMap<&str, &ColumnDef> = entity
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();

    let mut added_columns: Vec<&ColumnDef> = Vec::new();
    let mut dropped_columns: Vec<&ColumnDef> = Vec::new();

    for col in &entity.columns {
        if !db_col_map.contains_key(col.name.as_str()) {
            added_columns.push(col);
        }
    }
    for col in &db.columns {
        if !entity_col_map.contains_key(col.name.as_str()) {
            dropped_columns.push(col);
        }
    }

    // 重命名检测：Levenshtein 启发式（v2.2.0 B-2）
    let mut renamed_added: Vec<usize> = Vec::new();
    let mut renamed_dropped: Vec<usize> = Vec::new();
    for (i, dropped_col) in dropped_columns.iter().enumerate() {
        if renamed_dropped.contains(&i) {
            continue;
        }
        for (j, added_col) in added_columns.iter().enumerate() {
            if renamed_added.contains(&j) {
                continue;
            }
            if dropped_col.sql_type != added_col.sql_type {
                continue;
            }
            let dist = levenshtein(&dropped_col.name, &added_col.name);
            let ratio = dist as f64 / dropped_col.name.len().max(added_col.name.len()) as f64;
            if dist <= max_distance || ratio <= max_ratio {
                result.renamed_columns.push((
                    entity.name.clone(),
                    dropped_col.name.clone(),
                    added_col.name.clone(),
                ));
                renamed_dropped.push(i);
                renamed_added.push(j);
                break;
            }
        }
    }

    for (j, col) in added_columns.iter().enumerate() {
        if !renamed_added.contains(&j) {
            result
                .added_columns
                .push((entity.name.clone(), (*col).clone()));
        }
    }
    for (i, col) in dropped_columns.iter().enumerate() {
        if !renamed_dropped.contains(&i) {
            result
                .dropped_columns
                .push((entity.name.clone(), col.name.clone()));
        }
    }

    // 类型变更
    for entity_col in &entity.columns {
        if let Some(db_col) = db_col_map.get(entity_col.name.as_str()) {
            if entity_col.sql_type != db_col.sql_type || entity_col.nullable != db_col.nullable {
                result.type_changed_columns.push((
                    entity.name.clone(),
                    (*db_col).clone(),
                    entity_col.clone(),
                ));
            }
        }
    }
}

/// Levenshtein 编辑距离（v2.2.0 B-2）
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
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
    /// 重命名检测最大 Levenshtein 距离（v2.2.0 B-2）
    rename_max_distance: usize,
    /// 重命名检测最大距离/长度比（v2.2.0 B-2）
    rename_max_ratio: f64,
}

impl SchemaSync {
    /// 创建 SchemaSync（按方言选择 DDL 生成器）
    pub fn new(entity_tables: Vec<TableDef>) -> Self {
        Self {
            entity_tables,
            ddl_generator: Box::new(MySqlDdlGenerator),
            rename_max_distance: 2,
            rename_max_ratio: 0.3,
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
            rename_max_distance: 2,
            rename_max_ratio: 0.3,
        }
    }

    /// 配置重命名检测阈值（v2.2.0 B-2）
    pub fn with_rename_threshold(mut self, max_distance: usize, max_ratio: f64) -> Self {
        self.rename_max_distance = max_distance;
        self.rename_max_ratio = max_ratio;
        self
    }

    /// 干运行：计算 DDL 但不执行
    ///
    /// 1. introspect → 读取 DB 现有表结构
    /// 2. diff → 计算变更
    /// 3. 检查破坏性变更 → 若有则返回 `Err(DestructiveChangeDetected)`
    /// 4. generate → 生成 DDL
    pub async fn sync_dry_run(&self, conn: &mut dyn Connection) -> Result<Vec<String>, DbError> {
        let db_tables = introspect(conn).await?;
        let diff_result = self.diff_against(&db_tables);

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
        let mut result = SchemaDiff::default();

        let db_map: std::collections::HashMap<&str, &TableDef> =
            db_tables.iter().map(|t| (t.name.as_str(), t)).collect();
        let entity_map: std::collections::HashMap<&str, &TableDef> = self
            .entity_tables
            .iter()
            .map(|t| (t.name.as_str(), t))
            .collect();

        for t in &self.entity_tables {
            if !db_map.contains_key(t.name.as_str()) {
                result.added_tables.push(t.clone());
            }
        }
        for t in db_tables {
            if !entity_map.contains_key(t.name.as_str()) {
                result.dropped_tables.push(t.name.clone());
            }
        }

        for entity_table in &self.entity_tables {
            if let Some(db_table) = db_map.get(entity_table.name.as_str()) {
                diff_columns_with_threshold(
                    &mut result,
                    entity_table,
                    db_table,
                    self.rename_max_distance,
                    self.rename_max_ratio,
                );
            }
        }

        result
    }

    /// 执行破坏性同步（v2.2.0 B-2）
    ///
    /// 显式执行破坏性 DDL（DROP COLUMN / RENAME COLUMN），需 `Confirm::Yes` 确认。
    /// 事务内原子执行，每条破坏性 DDL 前调用对应钩子。
    ///
    /// # 参数
    ///
    /// - `conn`：数据库连接
    /// - `confirm`：显式确认（必须 `Confirm::Yes` 才执行）
    /// - `hooks`：可选数据迁移钩子
    ///
    /// # 异常处理
    ///
    /// - `confirm == Confirm::No` → 返回 `Err` 要求显式确认
    /// - 钩子失败 → ROLLBACK 返回 Err
    /// - DDL 执行失败 → ROLLBACK 返回 Err
    pub async fn destructive_sync(
        &self,
        conn: &mut dyn Connection,
        confirm: Confirm,
        hooks: Option<&dyn DataMigrationHook>,
    ) -> Result<DestructiveSyncResult, DbError> {
        if confirm != Confirm::Yes {
            return Err(DbError::InvalidInput(
                "破坏性同步需要显式确认：请传入 Confirm::Yes".to_string(),
            ));
        }

        let db_tables = introspect(conn).await?;
        let diff_result = self.diff_against(&db_tables);
        let ddl = self.ddl_generator.generate(&diff_result)?;

        let mut destructive_ddl = Vec::new();
        for (table, col) in &diff_result.dropped_columns {
            destructive_ddl.push(format!("ALTER TABLE {} DROP COLUMN {}", table, col));
        }
        for (table, old, new) in &diff_result.renamed_columns {
            destructive_ddl.push(format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                table, old, new
            ));
        }

        let all_ddl: Vec<String> = ddl.into_iter().chain(destructive_ddl).collect();
        if all_ddl.is_empty() {
            return Ok(DestructiveSyncResult {
                executed_ddl: Vec::new(),
                hooks_called: 0,
                audit_entries: 0,
            });
        }

        conn.begin_transaction().await?;

        let mut executed = Vec::new();
        let mut hooks_called = 0usize;

        for ddl_stmt in &all_ddl {
            if let Some(hook) = hooks {
                if ddl_stmt.contains("DROP COLUMN") {
                    let parts: Vec<&str> = ddl_stmt.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let table = parts[2];
                        let column = parts[4];
                        if let Err(e) = hook.before_drop_column(conn, table, column).await {
                            let _ = conn.rollback().await;
                            return Err(DbError::Hook(format!(
                                "before_drop_column 钩子失败: {}",
                                e
                            )));
                        }
                        hooks_called += 1;
                    }
                } else if ddl_stmt.contains("RENAME COLUMN") {
                    let parts: Vec<&str> = ddl_stmt.split_whitespace().collect();
                    if parts.len() >= 6 {
                        let table = parts[2];
                        let old_name = parts[4];
                        let new_name = parts[6];
                        if let Err(e) = hook
                            .before_rename_column(conn, table, old_name, new_name)
                            .await
                        {
                            let _ = conn.rollback().await;
                            return Err(DbError::Hook(format!(
                                "before_rename_column 钩子失败: {}",
                                e
                            )));
                        }
                        hooks_called += 1;
                    }
                }
            }

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

        Ok(DestructiveSyncResult {
            audit_entries: executed.len(),
            executed_ddl: executed,
            hooks_called,
        })
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
            vec![
                make_column("id", "BIGINT"),
                make_column("email", "VARCHAR(255)"),
            ],
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
            vec![
                make_column("id", "BIGINT"),
                make_column("legacy_col", "TEXT"),
            ],
        )];

        let result = diff(&entity, &db);

        assert_eq!(result.dropped_columns.len(), 1);
        assert_eq!(
            result.dropped_columns[0],
            ("users".to_string(), "legacy_col".to_string())
        );
        assert!(result.has_destructive_changes());
    }

    #[test]
    fn test_diff_type_change() {
        let entity = vec![make_table(
            "users",
            vec![
                make_column("id", "BIGINT"),
                make_column("name", "VARCHAR(255)"),
            ],
        )];
        let db = vec![make_table(
            "users",
            vec![
                make_column("id", "BIGINT"),
                make_column("name", "VARCHAR(100)"),
            ],
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
            renamed_columns: vec![(
                "users".to_string(),
                "old_name".to_string(),
                "new_name".to_string(),
            )],
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

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("user_name", "username"), 1);
        assert_eq!(levenshtein("name", "title"), 4);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }

    #[test]
    fn test_rename_detection() {
        let entity = vec![TableDef::new(
            "users",
            vec![
                ColumnDef::new("id", "BIGINT", false, true, None),
                ColumnDef::new("username", "VARCHAR(255)", true, false, None),
            ],
        )];
        let db = vec![TableDef::new(
            "users",
            vec![
                ColumnDef::new("id", "BIGINT", false, true, None),
                ColumnDef::new("user_name", "VARCHAR(255)", true, false, None),
            ],
        )];
        let diff_result = diff(&entity, &db);
        assert!(!diff_result.renamed_columns.is_empty());
        assert_eq!(
            diff_result.renamed_columns[0],
            (
                "users".to_string(),
                "user_name".to_string(),
                "username".to_string()
            )
        );
        assert!(diff_result.dropped_columns.is_empty());
        assert!(diff_result.added_columns.is_empty());
    }

    #[test]
    fn test_rename_no_match_different_type() {
        let entity = vec![TableDef::new(
            "users",
            vec![
                ColumnDef::new("id", "BIGINT", false, true, None),
                ColumnDef::new("username", "INT", true, false, None),
            ],
        )];
        let db = vec![TableDef::new(
            "users",
            vec![
                ColumnDef::new("id", "BIGINT", false, true, None),
                ColumnDef::new("user_name", "VARCHAR(255)", true, false, None),
            ],
        )];
        let diff_result = diff(&entity, &db);
        assert!(diff_result.renamed_columns.is_empty());
        assert!(!diff_result.dropped_columns.is_empty());
        assert!(!diff_result.added_columns.is_empty());
    }

    #[test]
    fn test_rename_no_match_distance_too_large() {
        let entity = vec![TableDef::new(
            "users",
            vec![
                ColumnDef::new("id", "BIGINT", false, true, None),
                ColumnDef::new("title", "VARCHAR(255)", true, false, None),
            ],
        )];
        let db = vec![TableDef::new(
            "users",
            vec![
                ColumnDef::new("id", "BIGINT", false, true, None),
                ColumnDef::new("name", "VARCHAR(255)", true, false, None),
            ],
        )];
        let diff_result = diff(&entity, &db);
        assert!(diff_result.renamed_columns.is_empty());
    }

    #[test]
    fn test_confirm_enum() {
        assert_eq!(Confirm::Yes, Confirm::Yes);
        assert_ne!(Confirm::Yes, Confirm::No);
    }

    #[test]
    fn test_destructive_sync_result() {
        let result = DestructiveSyncResult {
            executed_ddl: vec!["ALTER TABLE users DROP COLUMN old_col".to_string()],
            hooks_called: 1,
            audit_entries: 1,
        };
        assert_eq!(result.executed_ddl.len(), 1);
        assert_eq!(result.hooks_called, 1);
    }

    #[test]
    fn test_schema_sync_with_rename_threshold() {
        let sync = SchemaSync::new(vec![]).with_rename_threshold(5, 0.5);
        assert_eq!(sync.rename_max_distance, 5);
        assert!((sync.rename_max_ratio - 0.5).abs() < f64::EPSILON);
    }
}
