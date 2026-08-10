//! Migration system
//!
//! Provides database schema migration management

use crate::db_type::DbType;
use crate::error::DbError;
use std::path::PathBuf;

/// 数据库迁移定义
pub struct Migration {
    /// 版本号
    pub version: String,
    /// 迁移名称
    pub name: String,
    /// 正向 SQL（升级）
    pub sql_up: String,
    /// 反向 SQL（回滚）
    pub sql_down: String,
    /// 批次号（0 表示未执行）
    pub batch: i32,
    /// 执行时间（None 表示未执行）
    pub executed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Migration {
    /// 创建迁移，指定版本号、名称、正向和反向 SQL
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

    /// 设置批次号
    pub fn with_batch(mut self, batch: i32) -> Self {
        self.batch = batch;
        self
    }

    /// 设置执行时间
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

/// 迁移解析器 trait
pub trait MigrationResolver: Send + Sync {
    /// 解析指定数据库类型的迁移列表
    fn resolve(&self, db_type: DbType) -> Result<Vec<Migration>, DbError>;
}

/// 文件迁移解析器
pub struct FileMigrationResolver {
    /// 迁移文件目录
    pub path: PathBuf,
}

impl FileMigrationResolver {
    /// 创建文件迁移解析器，指定迁移文件目录
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

/// 迁移上下文
pub struct MigrationContext {
    /// 迁移记录表名
    pub table_name: String,
    /// 数据库连接
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

/// P1-4：校验迁移版本号安全性
///
/// 允许的格式：字母、数字、下划线、连字符、点（支持 "001"、"20240101"、"v1.0"、"create_users" 等常见格式）。
/// 拒绝单引号、分号、注释、空格等可能用于 SQL 注入的字符。
///
/// 与 `validate_identifier` 的区别：版本号允许以数字开头（如 "001"），
/// 而标识符通常要求以字母或下划线开头。
fn validate_migration_version(version: &str) -> Result<(), DbError> {
    if version.is_empty() || version.len() > 255 {
        return Err(DbError::InvalidInput(format!(
            "invalid migration version: empty or too long (max 255 chars): {:?}",
            version
        )));
    }
    // 只允许字母、数字、下划线、连字符、点
    let valid = version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
    if !valid {
        return Err(DbError::InvalidInput(format!(
            "invalid migration version: only ASCII alphanumeric, underscore, hyphen, dot allowed, got {:?}",
            version
        )));
    }
    // 额外拒绝 "--" 注释序列
    if version.contains("--") {
        return Err(DbError::InvalidInput(format!(
            "invalid migration version: SQL comment sequence '--' not allowed: {:?}",
            version
        )));
    }
    Ok(())
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

/// 迁移方向
#[derive(Debug, Clone, PartialEq)]
pub enum MigrationDirection {
    /// 正向迁移（升级）
    Up,
    /// 反向迁移（回滚）
    Down,
}

/// 迁移执行器
pub struct Migrator {
    context: MigrationContext,
    migrations: Vec<Migration>,
}

impl Migrator {
    /// 创建迁移执行器，指定上下文
    pub fn new(context: MigrationContext) -> Self {
        Self {
            context,
            migrations: Vec::new(),
        }
    }

    /// 添加单个迁移
    pub fn add_migration(mut self, migration: Migration) -> Self {
        self.migrations.push(migration);
        self
    }

    /// 添加多个迁移
    pub fn add_migrations(mut self, migrations: Vec<Migration>) -> Self {
        self.migrations.extend(migrations);
        self
    }

    /// 返回所有迁移的引用
    pub fn get_migrations(&self) -> &Vec<Migration> {
        &self.migrations
    }

    /// 返回待执行的迁移（batch == 0）
    pub fn get_pending_migrations(&self) -> Vec<&Migration> {
        self.migrations.iter().filter(|m| m.batch == 0).collect()
    }

    /// 返回已执行的迁移（batch > 0）
    pub fn get_applied_migrations(&self) -> Vec<&Migration> {
        self.migrations.iter().filter(|m| m.batch > 0).collect()
    }

    /// 返回最新版本号
    pub fn latest_version(&self) -> Option<&str> {
        self.migrations.last().map(|m| m.version.as_str())
    }

    /// 按版本号查找迁移
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

    // ==================== P1-4：__migrations 持久化表自动创建 ====================

    /// P1-4：生成 `__migrations` 表的 CREATE TABLE IF NOT EXISTS SQL
    ///
    /// 表结构（跨方言兼容）：
    /// ```sql
    /// CREATE TABLE IF NOT EXISTS __migrations (
    ///     version VARCHAR(255) NOT NULL PRIMARY KEY,
    ///     name VARCHAR(255),
    ///     batch INTEGER NOT NULL,
    ///     executed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
    /// );
    /// ```
    ///
    /// 不同方言的差异：
    /// - MySQL/PG/SQLite：`TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP`
    /// - Oracle：`TIMESTAMP DEFAULT CURRENT_TIMESTAMP`
    /// - SQL Server：`DATETIME DEFAULT GETDATE()`
    pub fn build_create_migrations_table_sql(&self) -> String {
        let table = &self.context.table_name;
        let timestamp_default = match self.context.db_type {
            Some(DbType::SqlServer) => "DATETIME DEFAULT GETDATE()",
            Some(DbType::Oracle) => "TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
            _ => "TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP",
        };
        format!(
            "CREATE TABLE IF NOT EXISTS {} (\
                version VARCHAR(255) NOT NULL PRIMARY KEY, \
                name VARCHAR(255), \
                batch INTEGER NOT NULL, \
                executed_at {ts}\
            )",
            table,
            ts = timestamp_default
        )
    }

    /// P1-4：确保 `__migrations` 表存在（若连接可用）
    ///
    /// 在 `migrate`/`up`/`down`/`rollback` 等方法入口处调用，
    /// 自动创建持久化表（若不存在）。
    ///
    /// 若 `context.connection` 为 None，跳过（纯内存模式）。
    async fn ensure_migrations_table(&mut self) -> Result<(), DbError> {
        let sql = self.build_create_migrations_table_sql();
        if let Some(ref mut conn) = self.context.connection {
            conn.execute(&sql).await?;
        }
        Ok(())
    }

    /// P1-4：从 `__migrations` 表加载已执行的迁移记录
    ///
    /// 返回 `HashMap<version, batch>`，表示每个已执行迁移的版本号和批次号。
    /// 调用方应根据返回值更新 `self.migrations` 中对应迁移的 `batch` 字段。
    ///
    /// 若 `context.connection` 为 None，返回空 map（纯内存模式）。
    async fn load_applied_migrations(
        &mut self,
    ) -> Result<std::collections::HashMap<String, i32>, DbError> {
        let mut applied = std::collections::HashMap::new();
        if let Some(ref mut conn) = self.context.connection {
            let sql = format!("SELECT version, batch FROM {}", self.context.table_name);
            let rows = conn.query(&sql).await?;
            for row in rows {
                if let Some(crate::Value::String(version)) = row.get("version") {
                    let batch = match row.get("batch") {
                        Some(crate::Value::I32(b)) => *b,
                        Some(crate::Value::I64(b)) => *b as i32,
                        _ => 0,
                    };
                    applied.insert(version.clone(), batch);
                }
            }
        }
        Ok(applied)
    }

    /// P1-4：根据 `__migrations` 表的记录同步内存中的迁移状态
    ///
    /// 在 `migrate`/`up`/`down` 等方法入口处调用，确保内存状态与数据库持久化状态一致。
    async fn sync_state_from_db(&mut self) -> Result<(), DbError> {
        let applied = self.load_applied_migrations().await?;
        for migration in &mut self.migrations {
            if let Some(batch) = applied.get(&migration.version) {
                migration.batch = *batch;
                migration.executed_at = Some(chrono::Utc::now());
            } else {
                migration.batch = 0;
                migration.executed_at = None;
            }
        }
        Ok(())
    }

    /// P1-4：向 `__migrations` 表插入一条执行记录
    ///
    /// 在每个迁移成功执行 up SQL 后调用。
    async fn record_migration(
        &mut self,
        version: &str,
        name: &str,
        batch: i32,
    ) -> Result<(), DbError> {
        if let Some(ref mut conn) = self.context.connection {
            // 安全校验：version 允许字母/数字/下划线/连字符/点（支持 "001"、"20240101"、"v1.0" 等格式），
            // 但拒绝单引号、分号、注释等危险字符
            validate_migration_version(version)?;
            // name 可能为空或含下划线，放宽校验：仅拒绝单引号和分号
            if name.contains('\'') || name.contains(';') {
                return Err(DbError::MigrationError(format!("非法迁移名称: {}", name)));
            }
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S");
            let sql = format!(
                "INSERT INTO {} (version, name, batch, executed_at) VALUES ('{}', '{}', {}, '{}')",
                self.context.table_name, version, name, batch, now
            );
            conn.execute(&sql).await?;
        }
        Ok(())
    }

    /// P1-4：从 `__migrations` 表删除一条执行记录
    ///
    /// 在每个迁移成功执行 down SQL 后调用。
    async fn remove_migration(&mut self, version: &str) -> Result<(), DbError> {
        if let Some(ref mut conn) = self.context.connection {
            validate_migration_version(version)?;
            let sql = format!(
                "DELETE FROM {} WHERE version = '{}'",
                self.context.table_name, version
            );
            conn.execute(&sql).await?;
        }
        Ok(())
    }

    /// 执行所有待迁移（batch=0）的 up SQL
    ///
    /// 若数据库方言支持 DDL 事务（PostgreSQL/SQLite），则用事务包裹所有待执行迁移，
    /// 任一迁移失败时回滚全部变更，避免部分迁移导致的状态不一致。
    /// 不支持 DDL 事务的方言（MySQL/Oracle/SQL Server）逐条执行，失败时保留已执行的变更。
    ///
    /// P1-4：若连接可用，会自动创建 `__migrations` 持久化表，并在执行前从表中
    /// 同步已执行记录、执行后向表插入新记录。
    pub async fn migrate(&mut self) -> Result<Vec<String>, DbError> {
        // 版本冲突检测
        self.check_version_conflicts()?;

        // P1-4：确保 __migrations 表存在
        self.ensure_migrations_table().await?;

        // P1-4：从 __migrations 表同步已执行状态
        self.sync_state_from_db().await?;

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
            let version = self.migrations[*migration_idx].version.clone();
            let name = self.migrations[*migration_idx].name.clone();

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

            // P1-4：向 __migrations 表插入记录
            if let Err(e) = self.record_migration(&version, &name, current_batch).await {
                // 事务包裹下回滚
                if use_transaction {
                    if let Some(ref mut conn) = self.context.connection {
                        let _ = conn.rollback().await;
                    }
                }
                return Err(e);
            }

            applied.push(version);
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
    ///
    /// P1-4：若连接可用，会自动创建 `__migrations` 表，并在回滚后从表中删除记录。
    pub async fn rollback(&mut self, version: &str) -> Result<(), DbError> {
        // P1-4：确保 __migrations 表存在并同步状态
        self.ensure_migrations_table().await?;
        self.sync_state_from_db().await?;

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

        // P1-4：从 __migrations 表删除记录
        self.remove_migration(version).await?;

        Ok(())
    }

    /// 执行到指定版本（包括该版本）
    ///
    /// P1-4：若连接可用，会自动创建 `__migrations` 表，并在执行前从表中
    /// 同步已执行记录、执行后向表插入新记录。
    pub async fn up(&mut self, target_version: Option<&str>) -> Result<Vec<String>, DbError> {
        // 版本冲突检测
        self.check_version_conflicts()?;

        // P1-4：确保 __migrations 表存在并同步状态
        self.ensure_migrations_table().await?;
        self.sync_state_from_db().await?;

        let mut applied = Vec::new();
        let current_batch = self.migrations.iter().map(|m| m.batch).max().unwrap_or(0) + 1;

        // 收集待执行迁移的版本和索引（避免在循环中借用冲突）
        let pending: Vec<(usize, String, String)> = self
            .migrations
            .iter()
            .enumerate()
            .filter(|(_, m)| m.batch == 0)
            .take_while(|(_, m)| {
                if let Some(target) = target_version {
                    m.version.as_str() <= target
                } else {
                    true
                }
            })
            .map(|(idx, m)| (idx, m.version.clone(), m.name.clone()))
            .collect();

        for (idx, version, name) in pending {
            let sql_up = self.migrations[idx].sql_up.clone();
            if let Some(ref mut conn) = self.context.connection {
                if !sql_up.is_empty() {
                    conn.execute(&sql_up).await?;
                }
            }

            self.migrations[idx].batch = current_batch;
            self.migrations[idx].executed_at = Some(chrono::Utc::now());

            // P1-4：向 __migrations 表插入记录
            self.record_migration(&version, &name, current_batch)
                .await?;

            applied.push(version);
        }

        Ok(applied)
    }

    /// 回滚到指定版本（执行该版本之后所有迁移的 down SQL）
    ///
    /// P1-4：若连接可用，会自动创建 `__migrations` 表，并在回滚后从表中删除记录。
    pub async fn down(&mut self, target_version: Option<&str>) -> Result<Vec<String>, DbError> {
        // 版本冲突检测
        self.check_version_conflicts()?;

        // P1-4：确保 __migrations 表存在并同步状态
        self.ensure_migrations_table().await?;
        self.sync_state_from_db().await?;

        let mut rolled_back = Vec::new();

        // 从后往前回滚，收集待回滚的索引和版本
        let mut indices: Vec<usize> = (0..self.migrations.len()).collect();
        indices.reverse();

        let pending_rollback: Vec<(usize, String)> = indices
            .iter()
            .filter(|&&idx| self.migrations[idx].batch > 0)
            .take_while(|&&idx| {
                if let Some(target) = target_version {
                    self.migrations[idx].version.as_str() > target
                } else {
                    true
                }
            })
            .map(|&idx| (idx, self.migrations[idx].version.clone()))
            .collect();

        for (idx, version) in pending_rollback {
            let sql_down = self.migrations[idx].sql_down.clone();
            if let Some(ref mut conn) = self.context.connection {
                if !sql_down.is_empty() {
                    conn.execute(&sql_down).await?;
                }
            }

            self.migrations[idx].batch = 0;
            self.migrations[idx].executed_at = None;

            // P1-4：从 __migrations 表删除记录
            self.remove_migration(&version).await?;

            rolled_back.push(version);
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

/// 迁移进度
#[derive(Debug, Clone)]
pub struct MigrationProgress {
    /// 总迁移数
    pub total: usize,
    /// 已执行数
    pub applied: usize,
    /// 待执行数
    pub pending: usize,
    /// 当前批次号
    pub current_batch: i32,
}

impl MigrationProgress {
    /// 创建迁移进度，指定总数和已执行数
    pub fn new(total: usize, applied: usize) -> Self {
        Self {
            total,
            applied,
            pending: total - applied,
            current_batch: 0,
        }
    }

    /// 返回完成百分比
    pub fn percent_complete(&self) -> f64 {
        if self.total == 0 {
            return 100.0;
        }
        (self.applied as f64 / self.total as f64) * 100.0
    }
}

/// Schema 构建器
pub struct SchemaBuilder {
    table_name: String,
    columns: Vec<ColumnDef>,
    indexes: Vec<IndexDef>,
    foreign_keys: Vec<ForeignKeyDef>,
    if_not_exists: bool,
}

impl SchemaBuilder {
    /// 创建 Schema 构建器，指定表名
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            if_not_exists: true,
        }
    }

    /// 添加列定义
    pub fn add_column(mut self, column: ColumnDef) -> Self {
        self.columns.push(column);
        self
    }

    /// 添加索引定义
    pub fn add_index(mut self, index: IndexDef) -> Self {
        self.indexes.push(index);
        self
    }

    /// 添加外键定义
    pub fn add_foreign_key(mut self, fk: ForeignKeyDef) -> Self {
        self.foreign_keys.push(fk);
        self
    }

    /// 设置是否添加 `IF NOT EXISTS`
    pub fn if_not_exists(mut self, value: bool) -> Self {
        self.if_not_exists = value;
        self
    }

    /// 构建 CREATE TABLE SQL
    pub fn build(&self, db_type: DbType) -> Result<String, DbError> {
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
            sql.push_str(&fk.build(db_type)?);
        }

        sql.push(')');
        Ok(sql)
    }
}

/// 列定义
#[derive(Debug, Clone)]
pub struct ColumnDef {
    /// 列名
    pub name: String,
    /// 列类型
    pub col_type: String,
    /// 长度（如 VARCHAR(255)）
    pub length: Option<usize>,
    /// 精度与小数位（如 DECIMAL(10,2)）
    pub precision: Option<(u32, u32)>,
    /// 是否允许 NULL
    pub nullable: bool,
    /// 默认值表达式
    pub default: Option<String>,
    /// 是否自增
    pub auto_increment: bool,
    /// 是否唯一
    pub unique: bool,
    /// 列注释
    pub comment: Option<String>,
}

impl ColumnDef {
    /// 创建列定义，指定列名和类型
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

    /// 设置 NOT NULL
    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }

    /// 设置默认值
    pub fn default(mut self, value: &str) -> Self {
        self.default = Some(value.to_string());
        self
    }

    /// 设置自增
    pub fn auto_increment(mut self) -> Self {
        self.auto_increment = true;
        self
    }

    /// 设置唯一约束
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// 设置列注释
    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = Some(comment.to_string());
        self
    }

    /// 设置列长度
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

/// 索引定义
#[derive(Debug, Clone)]
pub struct IndexDef {
    /// 索引名
    pub name: String,
    /// 索引列列表
    pub columns: Vec<String>,
    /// 是否唯一索引
    pub unique: bool,
    /// 索引类型（如 BTREE、HASH）
    pub index_type: Option<String>,
}

impl IndexDef {
    /// 创建索引定义，指定索引名和列列表
    pub fn new(name: &str, columns: Vec<&str>) -> Self {
        Self {
            name: name.to_string(),
            columns: columns.into_iter().map(|s| s.to_string()).collect(),
            unique: false,
            index_type: None,
        }
    }

    /// 设置为唯一索引
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

/// 外键定义
#[derive(Debug, Clone)]
pub struct ForeignKeyDef {
    /// 约束名
    pub name: String,
    /// 本表列名
    pub column: String,
    /// 引用表名
    pub referenced_table: String,
    /// 引用列名
    pub referenced_column: String,
    /// ON DELETE 动作
    pub on_delete: Option<String>,
    /// ON UPDATE 动作
    pub on_update: Option<String>,
}

impl ForeignKeyDef {
    /// 创建外键定义，指定约束名、本表列、引用表和引用列
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

    /// 设置 ON DELETE 动作
    pub fn on_delete(mut self, action: &str) -> Self {
        self.on_delete = Some(action.to_string());
        self
    }

    /// 设置 ON UPDATE 动作
    pub fn on_update(mut self, action: &str) -> Self {
        self.on_update = Some(action.to_string());
        self
    }

    fn build(&self, _db_type: DbType) -> Result<String, DbError> {
        // v0.2.2 修复 C-3：FOREIGN KEY 标识符与 ON DELETE/ON UPDATE 动作严格校验
        crate::sql_safety::validate_identifier(&self.name, "foreign key constraint name")?;
        crate::sql_safety::validate_identifier(&self.column, "foreign key column")?;
        crate::sql_safety::validate_identifier(
            &self.referenced_table,
            "foreign key referenced table",
        )?;
        crate::sql_safety::validate_identifier(
            &self.referenced_column,
            "foreign key referenced column",
        )?;
        if let Some(ref on_delete) = self.on_delete {
            crate::sql_safety::validate_fk_action(on_delete)?;
        }
        if let Some(ref on_update) = self.on_update {
            crate::sql_safety::validate_fk_action(on_update)?;
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
        Ok(sql)
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
        let sql = fk.build(DbType::MySQL).unwrap();
        assert!(sql.contains("FOREIGN KEY"));
        assert!(sql.contains("ON DELETE CASCADE"));
    }

    #[test]
    fn test_foreign_key_build_normalizes_action_case() {
        // v0.2.2 修复 C-3：动作大小写不敏感，输出统一为大写
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users", "id").on_delete("cascade");
        let sql = fk.build(DbType::MySQL).unwrap();
        assert!(sql.contains("ON DELETE CASCADE"));
    }

    #[test]
    fn test_foreign_key_rejects_sql_injection_in_column() {
        let fk = ForeignKeyDef::new("fk_user", "user_id; DROP TABLE users", "users", "id");
        let result = fk.build(DbType::MySQL);
        assert!(result.is_err());
    }

    #[test]
    fn test_foreign_key_rejects_sql_injection_in_ref_table() {
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users; DROP TABLE users", "id");
        let result = fk.build(DbType::MySQL);
        assert!(result.is_err());
    }

    #[test]
    fn test_foreign_key_rejects_sql_injection_in_on_delete() {
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users", "id")
            .on_delete("CASCADE; DROP TABLE users");
        let result = fk.build(DbType::MySQL);
        assert!(result.is_err());
    }

    #[test]
    fn test_foreign_key_rejects_invalid_on_update_action() {
        let fk = ForeignKeyDef::new("fk_user", "user_id", "users", "id").on_update("EVIL_ACTION");
        let result = fk.build(DbType::MySQL);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_builder() {
        let schema = SchemaBuilder::new("users")
            .add_column(ColumnDef::new("id", "INT").not_null().auto_increment())
            .add_column(ColumnDef::new("name", "VARCHAR").length(255));

        let sql = schema.build(DbType::MySQL).unwrap();
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
