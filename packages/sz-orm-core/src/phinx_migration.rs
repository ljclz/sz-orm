//! Phinx 风格 migration 链式 API
//!
//! 提供 Phinx 风格的链式建表/改表 API，更直观易用。
//!
//! # 设计
//!
//! Phinx 是 PHP 生态流行的 migration 工具（think-orm 默认集成），
//! 其 API 风格比传统 SchemaBuilder 更直观：
//!
//! ```php
//! $table = $this->table('users');
//! $table->addColumn('name', 'string', ['limit' => 255])
//!       ->addColumn('email', 'string', ['limit' => 255])
//!       ->addIndex(['email'], ['unique' => true])
//!       ->create();
//! ```
//!
//! 本模块提供 Rust 版本的等价 API：
//!
//! ```ignore
//! use sz_orm_core::phinx_migration::{PhinxTable, ColumnType};
//! use sz_orm_core::db_type::DbType;
//!
//! let sql = PhinxTable::new("users")
//!     .add_column("name", ColumnType::String, |c| c.limit(255).not_null())
//!     .add_column("email", ColumnType::String, |c| c.limit(255).not_null())
//!     .add_column("age", ColumnType::Integer, |_| {})
//!     .add_index(&["email"], |i| i.unique())
//!     .add_foreign_key("role_id", "roles", "id", |fk| fk.on_delete_cascade())
//!     .create(DbType::MySQL);
//! ```

use crate::db_type::DbType;

/// Phinx 风格列类型枚举
///
/// 对应 Phinx 的 `addColumn($name, $type)` 中的 `$type` 参数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    /// 大整数（BIGINT）
    BigIntermediate,
    /// 二进制（BLOB / BYTEA）
    Binary,
    /// 布尔（TINYINT(1) / BOOLEAN）
    Boolean,
    /// 日期（DATE）
    Date,
    /// 日期时间（DATETIME / TIMESTAMP）
    DateTime,
    /// 定点数（DECIMAL(p,s)）
    Decimal,
    /// 浮点数（FLOAT / DOUBLE）
    Float,
    /// 整数（INT）
    Integer,
    /// JSON（MySQL JSON / PG JSONB / SQLite TEXT）
    Json,
    /// 字符串（VARCHAR）
    String,
    /// 文本（TEXT）
    Text,
    /// 时间（TIME）
    Time,
    /// 时间戳（TIMESTAMP）
    Timestamp,
    /// UUID（CHAR(36) / UUID）
    Uuid,
}

impl ColumnType {
    /// 将抽象列类型转换为具体方言的 SQL 类型字符串
    pub fn to_sql(self, db_type: DbType) -> &'static str {
        match (self, db_type) {
            (ColumnType::BigIntermediate, _) => "BIGINT",
            (ColumnType::Binary, DbType::PostgreSQL) => "BYTEA",
            (ColumnType::Binary, DbType::Sqlite) => "BLOB",
            (ColumnType::Binary, _) => "BLOB",
            (ColumnType::Boolean, DbType::PostgreSQL) => "BOOLEAN",
            (ColumnType::Boolean, _) => "TINYINT(1)",
            (ColumnType::Date, _) => "DATE",
            (ColumnType::DateTime, DbType::PostgreSQL) => "TIMESTAMP",
            (ColumnType::DateTime, DbType::Sqlite) => "DATETIME",
            (ColumnType::DateTime, _) => "DATETIME",
            (ColumnType::Decimal, _) => "DECIMAL",
            (ColumnType::Float, DbType::PostgreSQL) => "DOUBLE PRECISION",
            (ColumnType::Float, _) => "FLOAT",
            (ColumnType::Integer, _) => "INT",
            (ColumnType::Json, DbType::PostgreSQL) => "JSONB",
            (ColumnType::Json, DbType::Sqlite) => "TEXT",
            (ColumnType::Json, _) => "JSON",
            (ColumnType::String, _) => "VARCHAR",
            (ColumnType::Text, _) => "TEXT",
            (ColumnType::Time, _) => "TIME",
            (ColumnType::Timestamp, DbType::PostgreSQL) => "TIMESTAMP",
            (ColumnType::Timestamp, _) => "TIMESTAMP",
            (ColumnType::Uuid, DbType::PostgreSQL) => "UUID",
            (ColumnType::Uuid, _) => "CHAR(36)",
        }
    }
}

/// 列选项构建器（Phinx 风格链式配置）
#[derive(Debug, Clone)]
pub struct ColumnOptions {
    pub limit: Option<usize>,
    pub nullable: bool,
    pub default: Option<String>,
    pub auto_increment: bool,
    pub unique: bool,
    pub comment: Option<String>,
    pub precision: Option<(u32, u32)>,
    pub after: Option<String>,
}

impl Default for ColumnOptions {
    fn default() -> Self {
        Self {
            limit: None,
            nullable: true,
            default: None,
            auto_increment: false,
            unique: false,
            comment: None,
            precision: None,
            after: None,
        }
    }
}

impl ColumnOptions {
    /// 设置列长度（VARCHAR 长度等）
    pub fn limit(mut self, len: usize) -> Self {
        self.limit = Some(len);
        self
    }

    /// 设置为 NOT NULL（默认是 nullable）
    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }

    /// 设置默认值
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// 设置为自增主键
    pub fn auto_increment(mut self) -> Self {
        self.auto_increment = true;
        self
    }

    /// 设置为唯一
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// 设置列注释
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// 设置 DECIMAL 精度（precision, scale）
    pub fn precision(mut self, p: u32, s: u32) -> Self {
        self.precision = Some((p, s));
        self
    }

    /// MySQL AFTER 子句（指定列插入位置）
    pub fn after(mut self, column: impl Into<String>) -> Self {
        self.after = Some(column.into());
        self
    }
}

/// 索引选项构建器（Phinx 风格链式配置）
#[derive(Debug, Clone, Default)]
pub struct IndexOptions {
    pub unique: bool,
    pub name: Option<String>,
    pub index_type: Option<String>,
}

impl IndexOptions {
    /// 设置为唯一索引
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// 设置索引名
    pub fn name(mut self, n: impl Into<String>) -> Self {
        self.name = Some(n.into());
        self
    }

    /// 设置索引类型（BTREE / HASH / FULLTEXT 等）
    pub fn index_type(mut self, t: impl Into<String>) -> Self {
        self.index_type = Some(t.into());
        self
    }
}

/// 外键选项构建器（Phinx 风格链式配置）
#[derive(Debug, Clone, Default)]
pub struct ForeignKeyOptions {
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

impl ForeignKeyOptions {
    /// ON DELETE CASCADE
    pub fn on_delete_cascade(mut self) -> Self {
        self.on_delete = Some("CASCADE".to_string());
        self
    }

    /// ON DELETE SET NULL
    pub fn on_delete_set_null(mut self) -> Self {
        self.on_delete = Some("SET NULL".to_string());
        self
    }

    /// ON DELETE RESTRICT
    pub fn on_delete_restrict(mut self) -> Self {
        self.on_delete = Some("RESTRICT".to_string());
        self
    }

    /// ON DELETE NO ACTION
    pub fn on_delete_no_action(mut self) -> Self {
        self.on_delete = Some("NO ACTION".to_string());
        self
    }

    /// ON UPDATE CASCADE
    pub fn on_update_cascade(mut self) -> Self {
        self.on_update = Some("CASCADE".to_string());
        self
    }

    /// 自定义 ON DELETE 规则
    pub fn on_delete(mut self, action: impl Into<String>) -> Self {
        self.on_delete = Some(action.into());
        self
    }

    /// 自定义 ON UPDATE 规则
    pub fn on_update(mut self, action: impl Into<String>) -> Self {
        self.on_update = Some(action.into());
        self
    }
}

/// 内部表示：列定义（在 PhinxTable 中累积）
#[derive(Debug, Clone)]
struct PhinxColumn {
    name: String,
    col_type: ColumnType,
    options: ColumnOptions,
}

/// 内部表示：索引定义
#[derive(Debug, Clone)]
struct PhinxIndex {
    columns: Vec<String>,
    options: IndexOptions,
}

/// 内部表示：外键定义
#[derive(Debug, Clone)]
struct PhinxForeignKey {
    column: String,
    referenced_table: String,
    referenced_column: String,
    options: ForeignKeyOptions,
}

/// Phinx 风格表构建器
///
/// 提供链式 API 构建 CREATE TABLE / ALTER TABLE 语句
pub struct PhinxTable {
    table_name: String,
    columns: Vec<PhinxColumn>,
    indexes: Vec<PhinxIndex>,
    foreign_keys: Vec<PhinxForeignKey>,
    primary_key: Option<Vec<String>>,
    if_not_exists: bool,
}

impl PhinxTable {
    /// 创建新的表构建器
    pub fn new(table_name: impl Into<String>) -> Self {
        Self {
            table_name: table_name.into(),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            primary_key: None,
            if_not_exists: false,
        }
    }

    /// 添加列（Phinx `addColumn` 等价物）
    ///
    /// # 用法
    ///
    /// ```ignore
    /// PhinxTable::new("users")
    ///     .add_column("name", ColumnType::String, |c| c.limit(255).not_null())
    ///     .add_column("age", ColumnType::Integer, |_| {})
    /// ```
    pub fn add_column<F>(
        mut self,
        name: impl Into<String>,
        col_type: ColumnType,
        options_fn: F,
    ) -> Self
    where
        F: FnOnce(ColumnOptions) -> ColumnOptions,
    {
        let options = options_fn(ColumnOptions::default());
        self.columns.push(PhinxColumn {
            name: name.into(),
            col_type,
            options,
        });
        self
    }

    /// 添加索引（Phinx `addIndex` 等价物）
    pub fn add_index<F>(mut self, columns: &[&str], options_fn: F) -> Self
    where
        F: FnOnce(IndexOptions) -> IndexOptions,
    {
        let options = options_fn(IndexOptions::default());
        self.indexes.push(PhinxIndex {
            columns: columns.iter().map(|s| s.to_string()).collect(),
            options,
        });
        self
    }

    /// 添加外键（Phinx `addForeignKey` 等价物）
    pub fn add_foreign_key<F>(
        mut self,
        column: impl Into<String>,
        referenced_table: impl Into<String>,
        referenced_column: impl Into<String>,
        options_fn: F,
    ) -> Self
    where
        F: FnOnce(ForeignKeyOptions) -> ForeignKeyOptions,
    {
        let options = options_fn(ForeignKeyOptions::default());
        self.foreign_keys.push(PhinxForeignKey {
            column: column.into(),
            referenced_table: referenced_table.into(),
            referenced_column: referenced_column.into(),
            options,
        });
        self
    }

    /// 设置主键（Phinx 风格，可复合主键）
    pub fn set_primary_key(mut self, columns: Vec<String>) -> Self {
        self.primary_key = Some(columns);
        self
    }

    /// 设置 IF NOT EXISTS
    pub fn if_not_exists(mut self) -> Self {
        self.if_not_exists = true;
        self
    }

    /// 生成 CREATE TABLE SQL（Phinx `create()` 等价物）
    pub fn create(&self, db_type: DbType) -> String {
        // v0.2.2 修复 C-2：表名/主键列名严格校验
        crate::sql_safety::validate_identifier(&self.table_name, "table")
            .expect("invalid table name");
        if let Some(pk) = &self.primary_key {
            for col in pk {
                crate::sql_safety::validate_identifier(col, "primary key column")
                    .expect("invalid primary key column name");
            }
        }
        let mut sql = String::new();
        sql.push_str("CREATE TABLE ");
        if self.if_not_exists {
            sql.push_str("IF NOT EXISTS ");
        }
        sql.push_str(&self.table_name);
        sql.push_str(" (");

        // 列定义
        let col_defs: Vec<String> = self
            .columns
            .iter()
            .map(|c| build_column_sql(c, db_type))
            .collect();
        sql.push_str(&col_defs.join(", "));

        // 主键
        if let Some(pk) = &self.primary_key {
            sql.push_str(&format!(", PRIMARY KEY ({})", pk.join(", ")));
        }

        // 索引
        for index in &self.indexes {
            sql.push_str(", ");
            sql.push_str(&build_index_sql(index));
        }

        // 外键（约束名含引用表，避免多表指向同一引用表时冲突）
        // v0.2.2 修复 C-2：FOREIGN KEY 标识符与 ON DELETE/ON UPDATE 动作严格校验，杜绝 SQL 注入
        for fk in &self.foreign_keys {
            crate::sql_safety::validate_identifier(&fk.column, "foreign key column")
                .expect("invalid foreign key column name");
            crate::sql_safety::validate_identifier(
                &fk.referenced_table,
                "foreign key referenced table",
            )
            .expect("invalid foreign key referenced table name");
            crate::sql_safety::validate_identifier(
                &fk.referenced_column,
                "foreign key referenced column",
            )
            .expect("invalid foreign key referenced column name");
            if let Some(on_delete) = &fk.options.on_delete {
                crate::sql_safety::validate_fk_action(on_delete).expect("invalid ON DELETE action");
            }
            if let Some(on_update) = &fk.options.on_update {
                crate::sql_safety::validate_fk_action(on_update).expect("invalid ON UPDATE action");
            }
            // 约束名 fk_{table}_{column}_{ref_table} 由 table/column/ref_table 拼接而成，
            // 此三者均已校验为合法标识符，故约束名也必然合法（仅含字母数字下划线）
            sql.push_str(&format!(
                ", CONSTRAINT fk_{}_{}_{} FOREIGN KEY ({}) REFERENCES {} ({})",
                self.table_name,
                fk.column,
                fk.referenced_table,
                fk.column,
                fk.referenced_table,
                fk.referenced_column
            ));
            if let Some(on_delete) = &fk.options.on_delete {
                sql.push_str(&format!(" ON DELETE {}", on_delete.trim().to_uppercase()));
            }
            if let Some(on_update) = &fk.options.on_update {
                sql.push_str(&format!(" ON UPDATE {}", on_update.trim().to_uppercase()));
            }
        }

        sql.push(')');
        sql
    }

    /// 生成 ALTER TABLE 添加列 SQL（Phinx `change()` 等价物的一部分）
    pub fn add_columns_sql(&self, db_type: DbType) -> String {
        let add_clauses: Vec<String> = self
            .columns
            .iter()
            .map(|c| {
                let col_sql = build_column_sql(c, db_type);
                let after_clause = c
                    .options
                    .after
                    .as_ref()
                    .map(|a| format!(" AFTER {}", a))
                    .unwrap_or_default();
                format!("ADD COLUMN {}{}", col_sql, after_clause)
            })
            .collect();
        format!("ALTER TABLE {} {}", self.table_name, add_clauses.join(", "))
    }

    /// 生成 DROP TABLE SQL（Phinx `drop()` 等价物）
    pub fn drop(&self) -> String {
        format!("DROP TABLE {}", self.table_name)
    }

    /// 生成 DROP COLUMN SQL（Phinx `removeColumn` 等价物）
    pub fn drop_column_sql(&self, column: &str) -> String {
        format!("ALTER TABLE {} DROP COLUMN {}", self.table_name, column)
    }

    /// 生成 RENAME COLUMN SQL（Phinx `renameColumn` 等价物）
    pub fn rename_column_sql(&self, old_name: &str, new_name: &str) -> String {
        format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            self.table_name, old_name, new_name
        )
    }

    /// 生成 CHANGE COLUMN SQL（Phinx `changeColumn` 等价物）
    pub fn change_column_sql(&self, column: &str, new_type: ColumnType, db_type: DbType) -> String {
        let type_str = new_type.to_sql(db_type);
        match db_type {
            DbType::MySQL => format!(
                "ALTER TABLE {} MODIFY COLUMN {} {}",
                self.table_name, column, type_str
            ),
            DbType::PostgreSQL | DbType::Sqlite => format!(
                "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                self.table_name, column, type_str
            ),
            _ => format!(
                "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                self.table_name, column, type_str
            ),
        }
    }

    /// 生成 TRUNCATE SQL（Phinx `truncate()` 等价物）
    pub fn truncate_sql(&self) -> String {
        format!("TRUNCATE TABLE {}", self.table_name)
    }
}

/// 构建单列的 SQL 片段
fn build_column_sql(col: &PhinxColumn, db_type: DbType) -> String {
    let mut sql = format!("{} {}", col.name, col.col_type.to_sql(db_type));

    // VARCHAR 长度
    if let Some(limit) = col.options.limit {
        match col.col_type {
            ColumnType::String => sql.push_str(&format!("({})", limit)),
            ColumnType::Decimal => sql.push_str(&format!("({})", limit)),
            _ => {}
        }
    }

    // DECIMAL 精度
    if let Some((p, s)) = col.options.precision {
        sql.push_str(&format!("({}, {})", p, s));
    }

    // 自增
    if col.options.auto_increment {
        match db_type {
            DbType::MySQL => sql.push_str(" AUTO_INCREMENT"),
            DbType::PostgreSQL => sql.push_str(" GENERATED BY DEFAULT AS IDENTITY"),
            DbType::Sqlite => sql.push_str(" AUTOINCREMENT"),
            _ => {}
        }
    }

    // NOT NULL
    if !col.options.nullable {
        sql.push_str(" NOT NULL");
    }

    // DEFAULT
    if let Some(default) = &col.options.default {
        sql.push_str(&format!(" DEFAULT {}", default));
    }

    // UNIQUE
    if col.options.unique {
        sql.push_str(" UNIQUE");
    }

    // COMMENT（仅 MySQL 支持）
    if let Some(comment) = &col.options.comment {
        if matches!(db_type, DbType::MySQL) {
            sql.push_str(&format!(" COMMENT '{}'", comment.replace('\'', "''")));
        }
    }

    sql
}

/// 构建索引 SQL 片段
fn build_index_sql(index: &PhinxIndex) -> String {
    let unique_str = if index.options.unique { "UNIQUE " } else { "" };
    let name = index
        .options
        .name
        .clone()
        .unwrap_or_else(|| index.columns.join("_"));
    let type_str = index
        .options
        .index_type
        .as_deref()
        .map(|t| format!(" USING {}", t))
        .unwrap_or_default();
    format!(
        "{}KEY {} ({}){}",
        unique_str,
        name,
        index.columns.join(", "),
        type_str
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_type_to_sql_mysql() {
        assert_eq!(ColumnType::Integer.to_sql(DbType::MySQL), "INT");
        assert_eq!(ColumnType::String.to_sql(DbType::MySQL), "VARCHAR");
        assert_eq!(ColumnType::Boolean.to_sql(DbType::MySQL), "TINYINT(1)");
        assert_eq!(ColumnType::Json.to_sql(DbType::MySQL), "JSON");
        assert_eq!(ColumnType::Binary.to_sql(DbType::MySQL), "BLOB");
    }

    #[test]
    fn test_column_type_to_sql_pg() {
        assert_eq!(ColumnType::Boolean.to_sql(DbType::PostgreSQL), "BOOLEAN");
        assert_eq!(ColumnType::Json.to_sql(DbType::PostgreSQL), "JSONB");
        assert_eq!(ColumnType::Binary.to_sql(DbType::PostgreSQL), "BYTEA");
        assert_eq!(
            ColumnType::Float.to_sql(DbType::PostgreSQL),
            "DOUBLE PRECISION"
        );
        assert_eq!(ColumnType::Uuid.to_sql(DbType::PostgreSQL), "UUID");
    }

    #[test]
    fn test_column_type_to_sql_sqlite() {
        assert_eq!(ColumnType::Binary.to_sql(DbType::Sqlite), "BLOB");
        assert_eq!(ColumnType::Json.to_sql(DbType::Sqlite), "TEXT");
        assert_eq!(ColumnType::DateTime.to_sql(DbType::Sqlite), "DATETIME");
    }

    #[test]
    fn test_column_options_chain() {
        let opts = ColumnOptions::default()
            .limit(255)
            .not_null()
            .default_value("'active'")
            .unique()
            .comment("user status");
        assert_eq!(opts.limit, Some(255));
        assert!(!opts.nullable);
        assert_eq!(opts.default, Some("'active'".to_string()));
        assert!(opts.unique);
        assert_eq!(opts.comment, Some("user status".to_string()));
    }

    #[test]
    fn test_foreign_key_options_chain() {
        let opts = ForeignKeyOptions::default()
            .on_delete_cascade()
            .on_update_cascade();
        assert_eq!(opts.on_delete, Some("CASCADE".to_string()));
        assert_eq!(opts.on_update, Some("CASCADE".to_string()));
    }

    #[test]
    fn test_phinx_table_basic_create() {
        let sql = PhinxTable::new("users")
            .add_column("id", ColumnType::BigIntermediate, |c| {
                c.auto_increment().not_null()
            })
            .add_column("name", ColumnType::String, |c| c.limit(255).not_null())
            .add_column("email", ColumnType::String, |c| c.limit(255).not_null())
            .add_column("age", ColumnType::Integer, |c| c)
            .set_primary_key(vec!["id".to_string()])
            .create(DbType::MySQL);

        assert!(sql.contains("CREATE TABLE users"));
        assert!(sql.contains("id BIGINT AUTO_INCREMENT NOT NULL"));
        assert!(sql.contains("name VARCHAR(255) NOT NULL"));
        assert!(sql.contains("email VARCHAR(255) NOT NULL"));
        assert!(sql.contains("age INT"));
        assert!(sql.contains("PRIMARY KEY (id)"));
    }

    #[test]
    fn test_phinx_table_with_index() {
        let sql = PhinxTable::new("users")
            .add_column("id", ColumnType::BigIntermediate, |c| c.auto_increment())
            .add_column("email", ColumnType::String, |c| c.limit(255))
            .add_index(&["email"], |i| i.unique().name("idx_email"))
            .create(DbType::MySQL);

        assert!(sql.contains("UNIQUE KEY idx_email (email)"));
    }

    #[test]
    fn test_phinx_table_with_foreign_key() {
        let sql = PhinxTable::new("orders")
            .add_column("id", ColumnType::BigIntermediate, |c| c.auto_increment())
            .add_column("user_id", ColumnType::BigIntermediate, |c| c.not_null())
            .add_foreign_key("user_id", "users", "id", |fk| fk.on_delete_cascade())
            .create(DbType::MySQL);

        assert!(sql.contains(
            "CONSTRAINT fk_orders_user_id_users FOREIGN KEY (user_id) REFERENCES users (id)"
        ));
        assert!(sql.contains("ON DELETE CASCADE"));
    }

    #[test]
    fn test_phinx_table_foreign_key_normalizes_action_case() {
        // v0.2.2 修复 C-2：ON DELETE 大小写不敏感，输出统一为大写
        let sql = PhinxTable::new("orders")
            .add_column("id", ColumnType::BigIntermediate, |c| c.auto_increment())
            .add_column("user_id", ColumnType::BigIntermediate, |c| c.not_null())
            .add_foreign_key("user_id", "users", "id", |fk| fk.on_delete("cascade"))
            .create(DbType::MySQL);
        assert!(sql.contains("ON DELETE CASCADE"));
    }

    #[test]
    #[should_panic(expected = "invalid foreign key column name")]
    fn test_phinx_table_rejects_sql_injection_in_fk_column() {
        let _ = PhinxTable::new("orders")
            .add_column("id", ColumnType::BigIntermediate, |c| c.auto_increment())
            .add_foreign_key("user_id; DROP TABLE", "users", "id", |fk| fk)
            .create(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "invalid foreign key referenced table name")]
    fn test_phinx_table_rejects_sql_injection_in_fk_ref_table() {
        let _ = PhinxTable::new("orders")
            .add_column("id", ColumnType::BigIntermediate, |c| c.auto_increment())
            .add_foreign_key("user_id", "users; DROP TABLE users", "id", |fk| fk)
            .create(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "invalid ON DELETE action")]
    fn test_phinx_table_rejects_sql_injection_in_on_delete() {
        let _ = PhinxTable::new("orders")
            .add_column("id", ColumnType::BigIntermediate, |c| c.auto_increment())
            .add_foreign_key("user_id", "users", "id", |fk| {
                fk.on_delete("CASCADE; DROP TABLE users")
            })
            .create(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "invalid table name")]
    fn test_phinx_table_rejects_sql_injection_in_table_name() {
        let _ = PhinxTable::new("orders; DROP TABLE orders")
            .add_column("id", ColumnType::Integer, |c| c.not_null())
            .create(DbType::MySQL);
    }

    #[test]
    fn test_phinx_table_pg_dialect() {
        let sql = PhinxTable::new("users")
            .add_column("id", ColumnType::BigIntermediate, |c| {
                c.auto_increment().not_null()
            })
            .add_column("data", ColumnType::Json, |c| c)
            .add_column("is_active", ColumnType::Boolean, |c| c)
            .create(DbType::PostgreSQL);

        assert!(sql.contains("id BIGINT GENERATED BY DEFAULT AS IDENTITY NOT NULL"));
        assert!(sql.contains("data JSONB"));
        assert!(sql.contains("is_active BOOLEAN"));
    }

    #[test]
    fn test_phinx_table_sqlite_dialect() {
        let sql = PhinxTable::new("users")
            .add_column("id", ColumnType::BigIntermediate, |c| {
                c.auto_increment().not_null()
            })
            .add_column("data", ColumnType::Json, |c| c)
            .create(DbType::Sqlite);

        assert!(sql.contains("id BIGINT AUTOINCREMENT NOT NULL"));
        assert!(sql.contains("data TEXT"));
    }

    #[test]
    fn test_phinx_table_if_not_exists() {
        let sql = PhinxTable::new("users")
            .if_not_exists()
            .add_column("id", ColumnType::Integer, |c| c.not_null())
            .create(DbType::MySQL);

        assert!(sql.contains("CREATE TABLE IF NOT EXISTS users"));
    }

    #[test]
    fn test_phinx_table_drop() {
        let table = PhinxTable::new("users");
        assert_eq!(table.drop(), "DROP TABLE users");
    }

    #[test]
    fn test_phinx_table_truncate() {
        let table = PhinxTable::new("users");
        assert_eq!(table.truncate_sql(), "TRUNCATE TABLE users");
    }

    #[test]
    fn test_phinx_table_drop_column() {
        let table = PhinxTable::new("users");
        assert_eq!(
            table.drop_column_sql("old_col"),
            "ALTER TABLE users DROP COLUMN old_col"
        );
    }

    #[test]
    fn test_phinx_table_rename_column() {
        let table = PhinxTable::new("users");
        assert_eq!(
            table.rename_column_sql("old", "new"),
            "ALTER TABLE users RENAME COLUMN old TO new"
        );
    }

    #[test]
    fn test_phinx_table_change_column_mysql() {
        let table = PhinxTable::new("users");
        let sql = table.change_column_sql("name", ColumnType::String, DbType::MySQL);
        assert_eq!(sql, "ALTER TABLE users MODIFY COLUMN name VARCHAR");
    }

    #[test]
    fn test_phinx_table_change_column_pg() {
        let table = PhinxTable::new("users");
        let sql = table.change_column_sql("name", ColumnType::String, DbType::PostgreSQL);
        assert_eq!(sql, "ALTER TABLE users ALTER COLUMN name TYPE VARCHAR");
    }

    #[test]
    fn test_phinx_table_decimal_precision() {
        let sql = PhinxTable::new("products")
            .add_column("price", ColumnType::Decimal, |c| {
                c.precision(10, 2).not_null()
            })
            .create(DbType::MySQL);

        assert!(sql.contains("price DECIMAL(10, 2) NOT NULL"));
    }

    #[test]
    fn test_phinx_table_add_columns_sql() {
        let table = PhinxTable::new("users")
            .add_column("email", ColumnType::String, |c| c.limit(255))
            .add_column("age", ColumnType::Integer, |c| c);
        let sql = table.add_columns_sql(DbType::MySQL);
        assert!(sql.contains("ALTER TABLE users"));
        assert!(sql.contains("ADD COLUMN email VARCHAR(255)"));
        assert!(sql.contains("ADD COLUMN age INT"));
    }

    #[test]
    fn test_phinx_table_compound_primary_key() {
        let sql = PhinxTable::new("user_roles")
            .add_column("user_id", ColumnType::BigIntermediate, |c| c.not_null())
            .add_column("role_id", ColumnType::BigIntermediate, |c| c.not_null())
            .set_primary_key(vec!["user_id".to_string(), "role_id".to_string()])
            .create(DbType::MySQL);

        assert!(sql.contains("PRIMARY KEY (user_id, role_id)"));
    }

    #[test]
    fn test_phinx_table_comment_mysql() {
        let sql = PhinxTable::new("users")
            .add_column("status", ColumnType::String, |c| {
                c.limit(50).comment("用户状态：active/inactive")
            })
            .create(DbType::MySQL);

        assert!(sql.contains("COMMENT '用户状态：active/inactive'"));
    }

    #[test]
    fn test_phinx_table_after_mysql() {
        let table =
            PhinxTable::new("users").add_column("email", ColumnType::String, |c| c.after("name"));
        let sql = table.add_columns_sql(DbType::MySQL);
        assert!(sql.contains("AFTER name"));
    }

    #[test]
    fn test_index_options_chain() {
        let opts = IndexOptions::default()
            .unique()
            .name("idx_custom")
            .index_type("BTREE");
        assert!(opts.unique);
        assert_eq!(opts.name, Some("idx_custom".to_string()));
        assert_eq!(opts.index_type, Some("BTREE".to_string()));
    }
}
