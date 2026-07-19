//! Dialect abstractions for different database types
//!
//! Provides a unified interface for database-specific SQL syntax

use crate::db_type::DbType;
use crate::error::DbError;
use std::fmt;

/// Database dialect trait
///
/// Implementors handle database-specific SQL syntax variations
pub trait Dialect: Send + Sync {
    /// Get the database type this dialect is for
    fn db_type(&self) -> DbType;

    /// Quote an identifier (table name, column name, etc.)
    fn quote(&self, identifier: &str) -> String;

    /// Escape a string value for safe inclusion in SQL
    fn escape_string(&self, s: &str) -> String;

    /// Check if this dialect supports RETURNING clause
    fn supports_returning(&self) -> bool;

    /// Build pagination SQL
    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String;

    /// Get the SQL type name for JSON
    fn json_type(&self) -> &'static str;

    /// Build JSON_EXTRACT SQL function call
    fn json_extract(&self, column: &str, path: &str) -> String;

    /// Build full-text search SQL
    fn full_text_search(&self, columns: &[&str], keyword: &str) -> String;

    /// Convert boolean to integer for storage
    fn bool_to_int(&self, expr: &str) -> String;

    /// Build CONCAT function call
    fn concat(&self, parts: &[&str]) -> String;

    /// Check if this dialect supports IF EXISTS
    fn supports_if_exists(&self) -> bool;

    /// Check if this dialect supports IF NOT EXISTS
    fn supports_if_not_exists(&self) -> bool;

    /// Get the auto-increment keyword
    fn auto_increment_keyword(&self) -> &'static str;

    /// Build the SQL to get the last inserted ID
    fn last_insert_id_sql(&self) -> &'static str;

    /// Build a CREATE TABLE statement
    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String;

    /// Build an ALTER TABLE statement
    fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String;

    /// Build a DROP TABLE statement
    fn build_drop_table(&self, table: &str, if_exists: bool) -> String;
}

/// Column definition for table creation
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub auto_increment: bool,
    pub primary_key: bool,
}

/// Table change for ALTER TABLE
#[derive(Debug, Clone)]
pub enum TableChange {
    AddColumn(ColumnDef),
    DropColumn(String),
    ModifyColumn(ColumnDef),
    AddIndex(String, Vec<String>),
    DropIndex(String),
    AddForeignKey {
        columns: Vec<String>,
        reference_table: String,
        reference_columns: Vec<String>,
    },
}

/// MySQL dialect implementation
pub struct MySqlDialect;

impl Dialect for MySqlDialect {
    fn db_type(&self) -> DbType {
        DbType::MySQL
    }

    fn quote(&self, identifier: &str) -> String {
        format!("`{}`", identifier.replace('`', "``"))
    }

    fn escape_string(&self, s: &str) -> String {
        let mut escaped = String::with_capacity(s.len() * 2);
        for c in s.chars() {
            match c {
                '\\' => escaped.push_str("\\\\"),
                '\'' => escaped.push_str("\\'"),
                '\0' => escaped.push_str("\\0"),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                '\x1a' => escaped.push_str("\\Z"),
                _ => escaped.push(c),
            }
        }
        escaped
    }

    fn supports_returning(&self) -> bool {
        false
    }

    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String {
        let offset = page.saturating_sub(1).saturating_mul(limit);
        format!("{} LIMIT {} OFFSET {}", sql, limit, offset)
    }

    fn json_type(&self) -> &'static str {
        "JSON"
    }

    fn json_extract(&self, column: &str, path: &str) -> String {
        // 规范化 path：确保以 $. 开头
        let normalized = if path.starts_with('$') {
            path.to_string()
        } else {
            format!("$.{}", path)
        };
        format!(
            "JSON_EXTRACT({}, '{}')",
            column,
            self.escape_string(&normalized)
        )
    }

    fn full_text_search(&self, columns: &[&str], keyword: &str) -> String {
        let cols = columns.join(", ");
        let escaped = self.escape_string(keyword);
        format!(
            "MATCH({}) AGAINST('{}' IN NATURAL LANGUAGE MODE)",
            cols, escaped
        )
    }

    fn bool_to_int(&self, expr: &str) -> String {
        // 将布尔表达式转换为整数（0/1）用于存储
        format!("IF({}, 1, 0)", expr)
    }

    fn concat(&self, parts: &[&str]) -> String {
        if parts.is_empty() {
            return "NULL".to_string();
        }
        let concat_parts: Vec<String> = parts
            .iter()
            .map(|p| format!("CAST({} AS CHAR)", p))
            .collect();
        format!("CONCAT({})", concat_parts.join(", "))
    }

    fn supports_if_exists(&self) -> bool {
        true
    }

    fn supports_if_not_exists(&self) -> bool {
        true
    }

    fn auto_increment_keyword(&self) -> &'static str {
        "AUTO_INCREMENT"
    }

    fn last_insert_id_sql(&self) -> &'static str {
        "LAST_INSERT_ID()"
    }

    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String {
        let cols: Vec<String> = columns
            .iter()
            .map(|col| {
                let mut sql = format!("{} {}", self.quote(&col.name), col.sql_type);
                if !col.nullable {
                    sql.push_str(" NOT NULL");
                }
                if let Some(default) = &col.default {
                    sql.push_str(&format!(" DEFAULT {}", default));
                }
                if col.auto_increment {
                    sql.push_str(&format!(" {}", self.auto_increment_keyword()));
                }
                if col.primary_key {
                    sql.push_str(" PRIMARY KEY");
                }
                sql
            })
            .collect();

        format!("CREATE TABLE {} ({})", self.quote(table), cols.join(", "))
    }

    fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String {
        let stmts: Vec<String> = changes.iter().map(|change| {
            match change {
                TableChange::AddColumn(col) => {
                    let mut sql = format!("ALTER TABLE {} ADD {}", self.quote(table), self.quote(&col.name));
                    sql.push_str(&format!(" {}", col.sql_type));
                    if !col.nullable {
                        sql.push_str(" NOT NULL");
                    }
                    if let Some(default) = &col.default {
                        sql.push_str(&format!(" DEFAULT {}", default));
                    }
                    sql
                }
                TableChange::DropColumn(name) => {
                    format!("ALTER TABLE {} DROP COLUMN {}", self.quote(table), self.quote(name))
                }
                TableChange::ModifyColumn(col) => {
                    // MySQL 使用 MODIFY COLUMN
                    let mut sql = format!("ALTER TABLE {} MODIFY COLUMN {} {}", self.quote(table), self.quote(&col.name), col.sql_type);
                    if !col.nullable {
                        sql.push_str(" NOT NULL");
                    }
                    if let Some(default) = &col.default {
                        sql.push_str(&format!(" DEFAULT {}", default));
                    }
                    sql
                }
                TableChange::AddIndex(name, cols) => {
                    format!("ALTER TABLE {} ADD INDEX {} ({})", self.quote(table), name, cols.join(", "))
                }
                TableChange::DropIndex(name) => {
                    format!("ALTER TABLE {} DROP INDEX {}", self.quote(table), name)
                }
                TableChange::AddForeignKey { columns, reference_table, reference_columns } => {
                    format!("ALTER TABLE {} ADD CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {} ({})",
                        self.quote(table),
                        table,
                        columns.join("_"),
                        columns.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", "),
                        self.quote(reference_table),
                        reference_columns.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", "))
                }
            }
        }).collect();

        stmts.join("; ")
    }

    fn build_drop_table(&self, table: &str, if_exists: bool) -> String {
        if if_exists {
            format!("DROP TABLE IF EXISTS {}", self.quote(table))
        } else {
            format!("DROP TABLE {}", self.quote(table))
        }
    }
}

/// PostgreSQL dialect implementation
pub struct PostgreSqlDialect;

impl Dialect for PostgreSqlDialect {
    fn db_type(&self) -> DbType {
        DbType::PostgreSQL
    }

    fn quote(&self, identifier: &str) -> String {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }

    fn escape_string(&self, s: &str) -> String {
        // PostgreSQL 标准：使用双单引号转义单引号（standard_conforming_strings=on 默认）
        // 反斜杠在 standard_conforming_strings=on 时不是转义字符
        let mut escaped = String::with_capacity(s.len() * 2);
        for c in s.chars() {
            match c {
                '\'' => escaped.push_str("''"),
                _ => escaped.push(c),
            }
        }
        escaped
    }

    fn supports_returning(&self) -> bool {
        true
    }

    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String {
        let offset = page.saturating_sub(1).saturating_mul(limit);
        format!("{} LIMIT {} OFFSET {}", sql, limit, offset)
    }

    fn json_type(&self) -> &'static str {
        "JSONB"
    }

    fn json_extract(&self, column: &str, path: &str) -> String {
        // PostgreSQL 使用 #>> 提取文本，path 以字符串数组形式
        // 支持 $.a.b 或 a.b 格式
        // 输出形式：column#>>'{a,b,c}'
        // 路径组件中的特殊字符（逗号、花括号、双引号、反斜杠）需用双引号包裹并转义
        let normalized = path.trim_start_matches("$.");
        let parts: Vec<&str> = normalized.split('.').filter(|s| !s.is_empty()).collect();
        let path_lit = parts
            .iter()
            .map(|p| {
                // 如果包含特殊字符，用双引号包裹并转义内部双引号和反斜杠
                let needs_quoting = p.chars().any(|c| matches!(c, ',' | '{' | '}' | '"' | '\\'));
                if needs_quoting {
                    let escaped = p.replace('\\', "\\\\").replace('"', "\\\"");
                    format!("\"{}\"", escaped)
                } else {
                    p.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        // 转义 SQL 字符串字面量中的单引号（PG 使用双单引号转义）
        let path_lit_escaped = path_lit.replace('\'', "''");
        format!("{}#>>'{{{}}}'", column, path_lit_escaped)
    }

    fn full_text_search(&self, columns: &[&str], keyword: &str) -> String {
        let cols = columns
            .iter()
            .map(|c| format!("{}::text", c))
            .collect::<Vec<_>>()
            .join(" || ' ' || ");
        let escaped = self.escape_string(keyword);
        format!("to_tsvector({}) @@ to_tsquery('{}')", cols, escaped)
    }

    fn bool_to_int(&self, expr: &str) -> String {
        format!("(CASE WHEN {} THEN 1 ELSE 0 END)", expr)
    }

    fn concat(&self, parts: &[&str]) -> String {
        if parts.is_empty() {
            return "NULL".to_string();
        }
        format!("CONCAT({})", parts.join(", "))
    }

    fn supports_if_exists(&self) -> bool {
        true
    }

    fn supports_if_not_exists(&self) -> bool {
        true
    }

    fn auto_increment_keyword(&self) -> &'static str {
        "GENERATED BY DEFAULT AS IDENTITY"
    }

    fn last_insert_id_sql(&self) -> &'static str {
        "lastval()"
    }

    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String {
        let cols: Vec<String> = columns
            .iter()
            .map(|col| {
                let mut sql = format!("{} {}", self.quote(&col.name), col.sql_type);
                if !col.nullable {
                    sql.push_str(" NOT NULL");
                }
                if let Some(default) = &col.default {
                    sql.push_str(&format!(" DEFAULT {}", default));
                }
                if col.primary_key {
                    sql.push_str(" PRIMARY KEY");
                }
                sql
            })
            .collect();

        format!("CREATE TABLE {} ({})", self.quote(table), cols.join(", "))
    }

    fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String {
        let stmts: Vec<String> = changes.iter().map(|change| {
            match change {
                TableChange::AddColumn(col) => {
                    let mut sql = format!("ALTER TABLE {} ADD COLUMN {} {}", self.quote(table), self.quote(&col.name), col.sql_type);
                    if !col.nullable {
                        sql.push_str(" NOT NULL");
                    }
                    if let Some(default) = &col.default {
                        sql.push_str(&format!(" DEFAULT {}", default));
                    }
                    sql
                }
                TableChange::DropColumn(name) => {
                    format!("ALTER TABLE {} DROP COLUMN {}", self.quote(table), self.quote(name))
                }
                TableChange::ModifyColumn(col) => {
                    // PostgreSQL 使用 ALTER COLUMN TYPE
                    let mut sql = format!("ALTER TABLE {} ALTER COLUMN {} TYPE {}", self.quote(table), self.quote(&col.name), col.sql_type);
                    if !col.nullable {
                        sql.push_str(&format!(", ALTER COLUMN {} SET NOT NULL", self.quote(&col.name)));
                    }
                    if let Some(default) = &col.default {
                        sql.push_str(&format!(", ALTER COLUMN {} SET DEFAULT {}", self.quote(&col.name), default));
                    }
                    sql
                }
                TableChange::AddIndex(name, cols) => {
                    format!("CREATE INDEX {} ON {} ({})", name, self.quote(table), cols.join(", "))
                }
                TableChange::DropIndex(name) => {
                    format!("DROP INDEX {}", name)
                }
                TableChange::AddForeignKey { columns, reference_table, reference_columns } => {
                    format!("ALTER TABLE {} ADD CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {} ({})",
                        self.quote(table),
                        table,
                        columns.join("_"),
                        columns.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", "),
                        self.quote(reference_table),
                        reference_columns.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", "))
                }
            }
        }).collect();

        stmts.join("; ")
    }

    fn build_drop_table(&self, table: &str, if_exists: bool) -> String {
        if if_exists {
            format!("DROP TABLE IF EXISTS {}", self.quote(table))
        } else {
            format!("DROP TABLE {}", self.quote(table))
        }
    }
}

/// SQLite dialect implementation
pub struct SqliteDialect;

impl Dialect for SqliteDialect {
    fn db_type(&self) -> DbType {
        DbType::Sqlite
    }

    fn quote(&self, identifier: &str) -> String {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }

    fn escape_string(&self, s: &str) -> String {
        let mut escaped = String::with_capacity(s.len() * 2);
        for c in s.chars() {
            match c {
                '\'' => escaped.push_str("''"),
                _ => escaped.push(c),
            }
        }
        escaped
    }

    fn supports_returning(&self) -> bool {
        true
    }

    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String {
        let offset = page.saturating_sub(1).saturating_mul(limit);
        format!("{} LIMIT {} OFFSET {}", sql, limit, offset)
    }

    fn json_type(&self) -> &'static str {
        "TEXT"
    }

    fn json_extract(&self, column: &str, path: &str) -> String {
        // SQLite 使用 json_extract(column, '$.path')
        let normalized = if path.starts_with('$') {
            path.to_string()
        } else {
            format!("$.{}", path)
        };
        format!(
            "json_extract({}, '{}')",
            column,
            self.escape_string(&normalized)
        )
    }

    fn full_text_search(&self, columns: &[&str], keyword: &str) -> String {
        // SQLite 使用 MATCH 操作符（不是 MATCHES）
        let cols = columns.join(", ");
        let escaped = self.escape_string(keyword);
        format!("{} MATCH '{}'", cols, escaped)
    }

    fn bool_to_int(&self, expr: &str) -> String {
        expr.to_string()
    }

    fn concat(&self, parts: &[&str]) -> String {
        if parts.is_empty() {
            return "NULL".to_string();
        }
        format!("COALESCE({})", parts.join(" || "))
    }

    fn supports_if_exists(&self) -> bool {
        true
    }

    fn supports_if_not_exists(&self) -> bool {
        true
    }

    fn auto_increment_keyword(&self) -> &'static str {
        "AUTOINCREMENT"
    }

    fn last_insert_id_sql(&self) -> &'static str {
        "last_insert_rowid()"
    }

    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String {
        let cols: Vec<String> = columns
            .iter()
            .map(|col| {
                let mut sql = format!("{} {}", self.quote(&col.name), col.sql_type);
                if !col.nullable {
                    sql.push_str(" NOT NULL");
                }
                if let Some(default) = &col.default {
                    sql.push_str(&format!(" DEFAULT {}", default));
                }
                if col.auto_increment {
                    sql.push_str(" PRIMARY KEY AUTOINCREMENT");
                } else if col.primary_key {
                    sql.push_str(" PRIMARY KEY");
                }
                sql
            })
            .collect();

        format!("CREATE TABLE {} ({})", self.quote(table), cols.join(", "))
    }

    fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String {
        // SQLite 支持的 ALTER 操作：ADD COLUMN, RENAME COLUMN, DROP COLUMN (3.35+), RENAME TABLE
        // 不支持：MODIFY COLUMN, ADD INDEX (需用 CREATE INDEX), ADD FOREIGN KEY (语法层面不支持)
        let stmts: Vec<String> = changes
            .iter()
            .map(|change| {
                match change {
                    TableChange::AddColumn(col) => {
                        let mut sql = format!(
                            "ALTER TABLE {} ADD COLUMN {} {}",
                            self.quote(table),
                            self.quote(&col.name),
                            col.sql_type
                        );
                        if !col.nullable {
                            sql.push_str(" NOT NULL");
                        }
                        if let Some(default) = &col.default {
                            sql.push_str(&format!(" DEFAULT {}", default));
                        }
                        sql
                    }
                    TableChange::DropColumn(name) => {
                        // SQLite 3.35.0+ 支持 DROP COLUMN
                        format!(
                            "ALTER TABLE {} DROP COLUMN {}",
                            self.quote(table),
                            self.quote(name)
                        )
                    }
                    TableChange::ModifyColumn(col) => {
                        // SQLite 不直接支持 MODIFY COLUMN，需要用 12 步流程
                        // 这里生成注释 SQL，提示用户需手动处理
                        format!(
                            "-- SQLite 不支持 MODIFY COLUMN（{} {}），需重建表",
                            col.name, col.sql_type
                        )
                    }
                    TableChange::AddIndex(name, cols) => {
                        format!(
                            "CREATE INDEX {} ON {} ({})",
                            name,
                            self.quote(table),
                            cols.join(", ")
                        )
                    }
                    TableChange::DropIndex(name) => {
                        format!("DROP INDEX {}", name)
                    }
                    TableChange::AddForeignKey {
                        columns,
                        reference_table,
                        reference_columns: _,
                    } => {
                        // SQLite 不支持 ALTER TABLE ADD FOREIGN KEY，需重建表
                        format!(
                            "-- SQLite 不支持 ADD FOREIGN KEY（{} -> {}），需重建表",
                            columns.join(","),
                            reference_table
                        )
                    }
                }
            })
            .collect();

        stmts.join("; ")
    }

    fn build_drop_table(&self, table: &str, if_exists: bool) -> String {
        if if_exists {
            format!("DROP TABLE IF EXISTS {}", self.quote(table))
        } else {
            format!("DROP TABLE {}", self.quote(table))
        }
    }
}

/// 将通用 SQL 类型映射为 Oracle 23ai 类型
///
/// Oracle 类型与常见类型的对应关系：
/// - BIGINT → NUMBER(19)
/// - INT/INTEGER → NUMBER(10)
/// - VARCHAR(n) → VARCHAR2(n)
/// - TEXT 系列 → CLOB
/// - BOOLEAN/BOOL → NUMBER(1)
fn map_to_oracle_type(sql_type: &str) -> String {
    let upper = sql_type.to_uppercase();
    let trimmed = upper.trim();

    if trimmed.starts_with("BIGINT") {
        sql_type.replacen("BIGINT", "NUMBER(19)", 1)
    } else if trimmed.starts_with("VARCHAR2") {
        sql_type.to_string()
    } else if trimmed.starts_with("VARCHAR") {
        sql_type.replacen("VARCHAR", "VARCHAR2", 1)
    } else if matches!(trimmed, "TEXT" | "MEDIUMTEXT" | "LONGTEXT" | "TINYTEXT") {
        "CLOB".to_string()
    } else if matches!(trimmed, "BOOLEAN" | "BOOL") {
        "NUMBER(1)".to_string()
    } else if trimmed == "INTEGER" {
        "NUMBER(10)".to_string()
    } else if trimmed.starts_with("INT") {
        sql_type.replacen("INT", "NUMBER(10)", 1)
    } else {
        sql_type.to_string()
    }
}

/// Oracle dialect implementation (Oracle 23ai)
pub struct OracleDialect;

impl Dialect for OracleDialect {
    fn db_type(&self) -> DbType {
        DbType::Oracle
    }

    fn quote(&self, identifier: &str) -> String {
        // Oracle 标准使用双引号包裹标识符，内部双引号双写
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }

    fn escape_string(&self, s: &str) -> String {
        // Oracle 标准转义：单引号双写（'O''Brien'），反斜杠不转义
        let mut escaped = String::with_capacity(s.len() * 2);
        for c in s.chars() {
            match c {
                '\'' => escaped.push_str("''"),
                _ => escaped.push(c),
            }
        }
        escaped
    }

    fn supports_returning(&self) -> bool {
        // Oracle 12c+ 支持 RETURNING，23ai 当然支持
        true
    }

    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String {
        // Oracle 12c+ 使用 OFFSET/FETCH NEXT 语法
        // 与其他方言保持一致：page=1 为第一页（offset=0）
        let offset = page.saturating_sub(1).saturating_mul(limit);
        format!(
            "{} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
            sql, offset, limit
        )
    }

    fn json_type(&self) -> &'static str {
        // Oracle 21+ 原生支持 JSON 类型，23ai 完整支持
        "JSON"
    }

    fn json_extract(&self, column: &str, path: &str) -> String {
        // Oracle 使用 JSON_VALUE 提取标量值，path 需以 $. 开头
        let normalized = if path.starts_with('$') {
            path.to_string()
        } else {
            format!("$.{}", path)
        };
        format!(
            "JSON_VALUE({}, '{}')",
            column,
            self.escape_string(&normalized)
        )
    }

    fn full_text_search(&self, columns: &[&str], keyword: &str) -> String {
        // Oracle 使用 CONTAINS 函数（需要 CONTEXT 索引）
        // CONTAINS(column, keyword, 1) > 0
        if columns.is_empty() {
            return "0".to_string();
        }
        let escaped = self.escape_string(keyword);
        let parts: Vec<String> = columns
            .iter()
            .map(|c| format!("CONTAINS({}, '{}', 1) > 0", c, escaped))
            .collect();
        parts.join(" OR ")
    }

    fn bool_to_int(&self, expr: &str) -> String {
        // Oracle 没有原生 BOOL 类型，使用 CASE WHEN 转换为 0/1
        format!("(CASE WHEN {} THEN 1 ELSE 0 END)", expr)
    }

    fn concat(&self, parts: &[&str]) -> String {
        // Oracle 使用 || 操作符进行字符串拼接
        if parts.is_empty() {
            return "NULL".to_string();
        }
        parts.join(" || ")
    }

    fn supports_if_exists(&self) -> bool {
        // Oracle 23ai 支持 DROP TABLE IF EXISTS
        true
    }

    fn supports_if_not_exists(&self) -> bool {
        // Oracle 23ai 支持 CREATE TABLE IF NOT EXISTS
        true
    }

    fn auto_increment_keyword(&self) -> &'static str {
        // Oracle 12c+ 使用 IDENTITY 列
        "GENERATED BY DEFAULT AS IDENTITY"
    }

    fn last_insert_id_sql(&self) -> &'static str {
        // Oracle 使用 RETURNING 子句获取自增列的值（占位符由调用方替换）
        "RETURNING {pk} INTO ?"
    }

    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String {
        let cols: Vec<String> = columns
            .iter()
            .map(|col| {
                let oracle_type = map_to_oracle_type(&col.sql_type);
                let mut sql = format!("{} {}", self.quote(&col.name), oracle_type);
                // Oracle IDENTITY 列隐式 NOT NULL，不允许显式 NOT NULL（ORA-03076）
                if !col.nullable && !col.auto_increment {
                    sql.push_str(" NOT NULL");
                }
                if let Some(default) = &col.default {
                    sql.push_str(&format!(" DEFAULT {}", default));
                }
                if col.auto_increment {
                    sql.push_str(&format!(" {}", self.auto_increment_keyword()));
                }
                if col.primary_key {
                    sql.push_str(" PRIMARY KEY");
                }
                sql
            })
            .collect();

        format!("CREATE TABLE {} ({})", self.quote(table), cols.join(", "))
    }

    fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String {
        let stmts: Vec<String> = changes
            .iter()
            .map(|change| match change {
                TableChange::AddColumn(col) => {
                    let oracle_type = map_to_oracle_type(&col.sql_type);
                    let mut sql = format!(
                        "ALTER TABLE {} ADD {} {}",
                        self.quote(table),
                        self.quote(&col.name),
                        oracle_type
                    );
                    if !col.nullable {
                        sql.push_str(" NOT NULL");
                    }
                    if let Some(default) = &col.default {
                        sql.push_str(&format!(" DEFAULT {}", default));
                    }
                    sql
                }
                TableChange::DropColumn(name) => {
                    format!(
                        "ALTER TABLE {} DROP COLUMN {}",
                        self.quote(table),
                        self.quote(name)
                    )
                }
                TableChange::ModifyColumn(col) => {
                    // Oracle 使用 MODIFY 关键字（不需 COLUMN）
                    let oracle_type = map_to_oracle_type(&col.sql_type);
                    let mut sql = format!(
                        "ALTER TABLE {} MODIFY {} {}",
                        self.quote(table),
                        self.quote(&col.name),
                        oracle_type
                    );
                    if !col.nullable {
                        sql.push_str(" NOT NULL");
                    }
                    if let Some(default) = &col.default {
                        sql.push_str(&format!(" DEFAULT {}", default));
                    }
                    sql
                }
                TableChange::AddIndex(name, cols) => {
                    format!(
                        "CREATE INDEX {} ON {} ({})",
                        name,
                        self.quote(table),
                        cols.join(", ")
                    )
                }
                TableChange::DropIndex(name) => {
                    format!("DROP INDEX {}", name)
                }
                TableChange::AddForeignKey {
                    columns,
                    reference_table,
                    reference_columns,
                } => {
                    format!(
                        "ALTER TABLE {} ADD CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {} ({})",
                        self.quote(table),
                        table,
                        columns.join("_"),
                        columns.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", "),
                        self.quote(reference_table),
                        reference_columns.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", ")
                    )
                }
            })
            .collect();

        stmts.join("; ")
    }

    fn build_drop_table(&self, table: &str, if_exists: bool) -> String {
        if if_exists {
            format!("DROP TABLE IF EXISTS {}", self.quote(table))
        } else {
            format!("DROP TABLE {}", self.quote(table))
        }
    }
}

/// Get the dialect for a specific database type
pub fn get_dialect(db_type: DbType) -> Result<Box<dyn Dialect>, DbError> {
    match db_type {
        DbType::MySQL => Ok(Box::new(MySqlDialect)),
        DbType::PostgreSQL => Ok(Box::new(PostgreSqlDialect)),
        DbType::Sqlite => Ok(Box::new(SqliteDialect)),
        DbType::Redis => Err(DbError::Unsupported(
            "Redis does not support standard SQL dialect".to_string(),
        )),
        DbType::MongoDB => Err(DbError::Unsupported(
            "MongoDB uses different query syntax".to_string(),
        )),
        DbType::ClickHouse => Ok(Box::new(MySqlDialect)),
        DbType::Oracle => Ok(Box::new(OracleDialect)),
        DbType::OceanBase => Ok(Box::new(MySqlDialect)),
        DbType::SqlServer => Ok(Box::new(MySqlDialect)),
        DbType::VectorDb => Err(DbError::Unsupported(
            "Vector databases have specific APIs".to_string(),
        )),
        DbType::PureJsDb => Err(DbError::Unsupported(
            "PureJS database uses JavaScript".to_string(),
        )),
    }
}

impl fmt::Display for dyn Dialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dialect({})", self.db_type())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_quote() {
        let dialect = MySqlDialect;
        assert_eq!(dialect.quote("users"), "`users`");
        assert_eq!(dialect.quote("user`id"), "`user``id`");
    }

    #[test]
    fn test_mysql_escape() {
        let dialect = MySqlDialect;
        assert_eq!(dialect.escape_string("hello"), "hello");
        assert_eq!(dialect.escape_string("it's"), "it\\'s");
        assert_eq!(dialect.escape_string("line\nbreak"), "line\\nbreak");
    }

    #[test]
    fn test_mysql_pagination() {
        let dialect = MySqlDialect;
        let sql = dialect.build_pagination("SELECT * FROM users", 2, 10);
        assert_eq!(sql, "SELECT * FROM users LIMIT 10 OFFSET 10");
    }

    #[test]
    fn test_postgres_quote() {
        let dialect = PostgreSqlDialect;
        assert_eq!(dialect.quote("users"), "\"users\"");
        assert_eq!(dialect.quote("user\"id"), "\"user\"\"id\"");
    }

    #[test]
    fn test_postgres_pagination() {
        let dialect = PostgreSqlDialect;
        let sql = dialect.build_pagination("SELECT * FROM users", 3, 20);
        assert_eq!(sql, "SELECT * FROM users LIMIT 20 OFFSET 40");
    }

    #[test]
    fn test_postgres_returning() {
        let dialect = PostgreSqlDialect;
        assert!(dialect.supports_returning());
    }

    #[test]
    fn test_sqlite_quote() {
        let dialect = SqliteDialect;
        assert_eq!(dialect.quote("users"), "\"users\"");
        assert_eq!(dialect.quote("user\"id"), "\"user\"\"id\"");
    }

    #[test]
    fn test_sqlite_escape() {
        let dialect = SqliteDialect;
        assert_eq!(dialect.escape_string("hello"), "hello");
        assert_eq!(dialect.escape_string("it's"), "it''s");
    }

    #[test]
    fn test_get_dialect() {
        let dialect = get_dialect(DbType::MySQL);
        assert!(dialect.is_ok());

        let dialect = get_dialect(DbType::Redis);
        assert!(dialect.is_err());
    }

    #[test]
    fn test_bool_to_int() {
        let mysql = MySqlDialect;
        assert_eq!(mysql.bool_to_int("active"), "IF(active, 1, 0)");

        let pg = PostgreSqlDialect;
        assert_eq!(
            pg.bool_to_int("active"),
            "(CASE WHEN active THEN 1 ELSE 0 END)"
        );
    }

    #[test]
    fn test_json_extract_with_path() {
        let mysql = MySqlDialect;
        let sql = mysql.json_extract("data", "$.user.name");
        assert!(sql.contains("$.user.name"));
        assert!(sql.contains("JSON_EXTRACT"));

        let pg = PostgreSqlDialect;
        let sql = pg.json_extract("data", "user.name");
        assert!(sql.contains("#>>"));

        let sqlite = SqliteDialect;
        let sql = sqlite.json_extract("data", "$.user.name");
        assert!(sql.contains("$.user.name"));
        assert!(sql.contains("json_extract"));
    }

    #[test]
    fn test_sqlite_full_text_search() {
        let sqlite = SqliteDialect;
        let sql = sqlite.full_text_search(&["title", "content"], "hello");
        // SQLite 使用 MATCH，不是 MATCHES
        assert!(sql.contains("MATCH"));
        assert!(!sql.contains("MATCHES"));
    }

    #[test]
    fn test_alter_table_modify_column() {
        let mysql = MySqlDialect;
        let col = ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        };
        let sql = mysql.build_alter_table("users", &[TableChange::ModifyColumn(col)]);
        assert!(sql.contains("MODIFY COLUMN"));

        let pg = PostgreSqlDialect;
        let col = ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        };
        let sql = pg.build_alter_table("users", &[TableChange::ModifyColumn(col)]);
        assert!(sql.contains("ALTER COLUMN"));
        assert!(sql.contains("TYPE"));
    }

    #[test]
    fn test_alter_table_add_foreign_key() {
        let mysql = MySqlDialect;
        let sql = mysql.build_alter_table(
            "orders",
            &[TableChange::AddForeignKey {
                columns: vec!["user_id".to_string()],
                reference_table: "users".to_string(),
                reference_columns: vec!["id".to_string()],
            }],
        );
        assert!(sql.contains("FOREIGN KEY"));
        assert!(sql.contains("REFERENCES"));

        let sqlite = SqliteDialect;
        let sql = sqlite.build_alter_table(
            "orders",
            &[TableChange::AddForeignKey {
                columns: vec!["user_id".to_string()],
                reference_table: "users".to_string(),
                reference_columns: vec!["id".to_string()],
            }],
        );
        // SQLite 不支持，应返回注释
        assert!(sql.starts_with("--"));
    }

    #[test]
    fn test_sqlite_alter_table_add_column() {
        let sqlite = SqliteDialect;
        let col = ColumnDef {
            name: "email".to_string(),
            sql_type: "TEXT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        };
        let sql = sqlite.build_alter_table("users", &[TableChange::AddColumn(col)]);
        assert!(sql.contains("ADD COLUMN"));
        assert!(sql.contains("email"));
    }

    // ===================== Oracle 方言测试 =====================

    #[test]
    fn test_oracle_quote_and_escape() {
        let dialect = OracleDialect;
        // 标识符使用双引号包裹，内部双引号双写
        assert_eq!(dialect.quote("users"), "\"users\"");
        assert_eq!(dialect.quote("user\"id"), "\"user\"\"id\"");
        assert_eq!(dialect.quote("column_name"), "\"column_name\"");

        // 字符串字面量转义：单引号双写，反斜杠不转义
        assert_eq!(dialect.escape_string("hello"), "hello");
        assert_eq!(dialect.escape_string("it's"), "it''s");
        assert_eq!(dialect.escape_string("O'Brien"), "O''Brien");
        assert_eq!(dialect.escape_string("a'b'c"), "a''b''c");
        // 反斜杠按原样保留（Oracle 标准行为）
        assert_eq!(dialect.escape_string("path\\to"), "path\\to");
    }

    #[test]
    fn test_oracle_pagination() {
        let dialect = OracleDialect;
        // page=1 为第一页（offset=0），与其他方言保持一致
        let sql = dialect.build_pagination("SELECT * FROM users", 1, 10);
        assert_eq!(
            sql,
            "SELECT * FROM users OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"
        );
        // page=3, limit=20 → offset=40
        let sql = dialect.build_pagination("SELECT * FROM users", 3, 20);
        assert_eq!(
            sql,
            "SELECT * FROM users OFFSET 40 ROWS FETCH NEXT 20 ROWS ONLY"
        );
        // page=0（边界）→ offset=0
        let sql = dialect.build_pagination("SELECT * FROM users", 0, 10);
        assert_eq!(
            sql,
            "SELECT * FROM users OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"
        );
    }

    #[test]
    fn test_oracle_json_extract() {
        let dialect = OracleDialect;
        // 标准 $.path 格式
        let sql = dialect.json_extract("data", "$.user.name");
        assert!(sql.contains("JSON_VALUE"));
        assert!(sql.contains("$.user.name"));
        assert!(sql.starts_with("JSON_VALUE(data, '$.user.name')"));

        // 自动补全 $. 前缀
        let sql = dialect.json_extract("data", "user.name");
        assert!(sql.contains("$.user.name"));
        assert!(sql.contains("JSON_VALUE"));

        // 单引号转义
        let sql = dialect.json_extract("data", "$.key's");
        assert!(sql.contains("$.key''s"));
    }

    #[test]
    fn test_oracle_create_table() {
        let dialect = OracleDialect;
        let columns = vec![
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
                name: "bio".to_string(),
                sql_type: "TEXT".to_string(),
                nullable: true,
                default: None,
                auto_increment: false,
                primary_key: false,
            },
            ColumnDef {
                name: "is_active".to_string(),
                sql_type: "BOOLEAN".to_string(),
                nullable: false,
                default: Some("1".to_string()),
                auto_increment: false,
                primary_key: false,
            },
        ];
        let sql = dialect.build_create_table("users", &columns);
        // Oracle 类型映射
        assert!(
            sql.contains("NUMBER(19)"),
            "BIGINT should map to NUMBER(19): {}",
            sql
        );
        assert!(
            sql.contains("VARCHAR2(255)"),
            "VARCHAR should map to VARCHAR2: {}",
            sql
        );
        assert!(sql.contains("CLOB"), "TEXT should map to CLOB: {}", sql);
        assert!(
            sql.contains("NUMBER(1)"),
            "BOOLEAN should map to NUMBER(1): {}",
            sql
        );
        // 自增与主键
        assert!(sql.contains("GENERATED BY DEFAULT AS IDENTITY"));
        assert!(sql.contains("PRIMARY KEY"));
        assert!(sql.contains("NOT NULL"));
        assert!(sql.contains("DEFAULT 1"));
        // 标识符使用双引号
        assert!(sql.contains("\"users\""));
        assert!(sql.contains("\"id\""));
    }

    #[test]
    fn test_oracle_bool_to_int_and_concat() {
        let dialect = OracleDialect;
        // bool_to_int 使用 CASE WHEN
        assert_eq!(
            dialect.bool_to_int("active"),
            "(CASE WHEN active THEN 1 ELSE 0 END)"
        );
        assert_eq!(
            dialect.bool_to_int("x > 0"),
            "(CASE WHEN x > 0 THEN 1 ELSE 0 END)"
        );
        // concat 使用 || 操作符
        assert_eq!(dialect.concat(&["a", "b", "c"]), "a || b || c");
        assert_eq!(
            dialect.concat(&["first_name", "last_name"]),
            "first_name || last_name"
        );
        // 空列表返回 NULL
        assert_eq!(dialect.concat(&[]), "NULL");
    }

    #[test]
    fn test_oracle_misc_dialect_methods() {
        let dialect = OracleDialect;
        // db_type
        assert_eq!(dialect.db_type(), DbType::Oracle);
        // supports_returning: Oracle 12c+ 支持
        assert!(dialect.supports_returning());
        // supports_if_exists / supports_if_not_exists: Oracle 23ai 支持
        assert!(dialect.supports_if_exists());
        assert!(dialect.supports_if_not_exists());
        // auto_increment_keyword
        assert_eq!(
            dialect.auto_increment_keyword(),
            "GENERATED BY DEFAULT AS IDENTITY"
        );
        // last_insert_id_sql: RETURNING ... INTO ?
        assert_eq!(dialect.last_insert_id_sql(), "RETURNING {pk} INTO ?");
        // json_type
        assert_eq!(dialect.json_type(), "JSON");
    }

    #[test]
    fn test_oracle_get_dialect() {
        // get_dialect 应返回 OracleDialect 而非 MySqlDialect
        let dialect = get_dialect(DbType::Oracle);
        assert!(dialect.is_ok(), "Oracle dialect should be available");
        let dialect = dialect.unwrap();
        assert_eq!(dialect.db_type(), DbType::Oracle);
        // 验证不是 MySqlDialect 的回退：Oracle 使用双引号，MySQL 使用反引号
        assert_eq!(dialect.quote("users"), "\"users\"");
        // 验证 Oracle 特有功能
        assert!(dialect.supports_returning());
        assert_eq!(dialect.last_insert_id_sql(), "RETURNING {pk} INTO ?");
    }

    #[test]
    fn test_oracle_drop_table() {
        let dialect = OracleDialect;
        // IF EXISTS
        let sql = dialect.build_drop_table("users", true);
        assert_eq!(sql, "DROP TABLE IF EXISTS \"users\"");
        // 不带 IF EXISTS
        let sql = dialect.build_drop_table("users", false);
        assert_eq!(sql, "DROP TABLE \"users\"");
    }

    #[test]
    fn test_oracle_alter_table() {
        let dialect = OracleDialect;
        // MODIFY COLUMN: Oracle 使用 MODIFY（不带 COLUMN 关键字）
        let col = ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        };
        let sql = dialect.build_alter_table("users", &[TableChange::ModifyColumn(col)]);
        assert!(sql.contains("MODIFY"));
        assert!(sql.contains("VARCHAR2(255)"));
        assert!(!sql.contains("MODIFY COLUMN")); // Oracle 不使用 COLUMN 关键字

        // ADD COLUMN
        let col = ColumnDef {
            name: "email".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        };
        let sql = dialect.build_alter_table("users", &[TableChange::AddColumn(col)]);
        assert!(sql.contains("ADD \"email\""));
        assert!(sql.contains("VARCHAR2(255)"));

        // DROP COLUMN
        let sql =
            dialect.build_alter_table("users", &[TableChange::DropColumn("email".to_string())]);
        assert!(sql.contains("DROP COLUMN"));
        assert!(sql.contains("\"email\""));
    }
}
