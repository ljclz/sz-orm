//! 不同数据库的方言抽象
//!
//! 为数据库特定的 SQL 语法提供统一接口

use crate::db_type::DbType;
use crate::error::DbError;
use std::fmt;

/// L-4 修复：SQL 标识符最大长度（取所有主流数据库最严格值）
///
/// - PostgreSQL: 63 chars (NAMEDATALEN default 64, minus 1)
/// - MySQL: 64 chars
/// - Oracle: 30 chars (12.2R2 之前) / 128 chars (12.2R2+)
/// - SQL Server: 128 chars
/// - SQLite: 实际无限制（但建议遵守 63）
///
/// 取 63 作为最严格值，覆盖所有主流数据库。
pub const MAX_IDENTIFIER_LEN: usize = 63;

/// 数据库方言 trait
///
/// 实现者负责处理各数据库特有的 SQL 语法差异
pub trait Dialect: Send + Sync {
    /// 返回该方言对应的数据库类型
    fn db_type(&self) -> DbType;

    /// 引用标识符（表名、列名等）
    fn quote(&self, identifier: &str) -> String;

    /// L-4 修复：带校验的引用标识符
    ///
    /// 与 `quote()` 不同，此方法会先校验标识符：
    /// - 非空
    /// - 长度 ≤ `MAX_IDENTIFIER_LEN` (63 chars)
    /// - 不含 SQL 元字符（引号、分号、空格、注释等）
    ///
    /// 校验失败时返回 `DbError::InvalidInput`。
    ///
    /// 建议在调用方不可信的场景（如用户输入的表名/列名）使用此方法替代 `quote()`。
    fn quote_checked(&self, identifier: &str) -> Result<String, DbError> {
        crate::sql_safety::validate_identifier(identifier, "identifier")?;
        Ok(self.quote(identifier))
    }

    /// 转义字符串字面量，确保可安全嵌入 SQL
    fn escape_string(&self, s: &str) -> String;

    /// 该方言是否支持 RETURNING 子句
    fn supports_returning(&self) -> bool;

    /// 生成分页 SQL
    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String;

    /// 获取 JSON 类型的 SQL 类型名
    fn json_type(&self) -> &'static str;

    /// 生成 JSON_EXTRACT 函数调用
    fn json_extract(&self, column: &str, path: &str) -> String;

    /// 生成全文检索 SQL
    fn full_text_search(&self, columns: &[&str], keyword: &str) -> String;

    /// 将布尔表达式转换为整型存储
    fn bool_to_int(&self, expr: &str) -> String;

    /// 生成 CONCAT 函数调用
    fn concat(&self, parts: &[&str]) -> String;

    /// 该方言是否支持 IF EXISTS
    fn supports_if_exists(&self) -> bool;

    /// 该方言是否支持 IF NOT EXISTS
    fn supports_if_not_exists(&self) -> bool;

    /// 获取自增列关键字
    fn auto_increment_keyword(&self) -> &'static str;

    /// 获取最近插入 ID 的 SQL（独立可执行语句）
    ///
    /// 返回 `None` 表示该方言不支持以独立 SQL 获取最后插入 ID（如 Oracle 只能通过
    /// 在 INSERT 语句末尾附加 `RETURNING ... INTO :bind` 子句的方式获取，无法独立执行）。
    /// 调用方在拿到 `None` 时必须改用 `supports_returning()` + 在 INSERT 后追加 RETURNING。
    fn last_insert_id_sql(&self) -> Option<&'static str>;

    /// 生成 CREATE TABLE 语句
    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String;

    /// 生成 ALTER TABLE 语句
    fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String;

    /// 生成 DROP TABLE 语句
    ///
    /// 默认实现生成标准 `DROP TABLE [IF EXISTS] <table>` 语法。
    /// 不支持 `IF EXISTS` 的方言（如 DB2）应覆盖此方法。
    fn build_drop_table(&self, table: &str, if_exists: bool) -> String {
        if if_exists && self.supports_if_exists() {
            format!("DROP TABLE IF EXISTS {}", self.quote(table))
        } else {
            format!("DROP TABLE {}", self.quote(table))
        }
    }
}

/// 建表时的列定义
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub auto_increment: bool,
    pub primary_key: bool,
}

/// ALTER TABLE 的变更操作
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

/// MySQL 方言实现
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
        // M-2 修复说明：
        //
        // 历史上 MySQL 支持 `SQL_CALC_FOUND_ROWS` 提示配合 `FOUND_ROWS()` 函数
        // 在不分页情况下获取总行数，但该特性在 MySQL 8.0.17 中被弃用并在后续版本移除。
        // 官方推荐使用独立的 `COUNT(*)` 查询。
        //
        // 因此本实现不使用 `SQL_CALC_FOUND_ROWS`，调用方如需总数应单独执行 COUNT 查询。
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

    fn last_insert_id_sql(&self) -> Option<&'static str> {
        Some("LAST_INSERT_ID()")
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
}

/// PostgreSQL 方言实现
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

    fn last_insert_id_sql(&self) -> Option<&'static str> {
        Some("lastval()")
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
}

/// SQLite 方言实现
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
        // SQLite FTS 的 MATCH 操作符必须作用于 FTS 虚拟表本身（`tbl MATCH 'query'`），
        // 不能用于单个列；本接口仅传入列名，没有 FTS 表名，因此降级使用 LIKE。
        // 这样既能避免生成错误的 MATCH 语法，又能在普通表上工作（不依赖 FTS 索引）。
        if columns.is_empty() {
            return "0".to_string();
        }
        let escaped = self.escape_string(keyword);
        columns
            .iter()
            .map(|c| format!("{} LIKE '%{}%'", c.trim(), escaped))
            .collect::<Vec<_>>()
            .join(" OR ")
    }

    fn bool_to_int(&self, expr: &str) -> String {
        expr.to_string()
    }

    fn concat(&self, parts: &[&str]) -> String {
        if parts.is_empty() {
            return "NULL".to_string();
        }
        // SQLite 的 || 操作符在任意参数为 NULL 时整体结果为 NULL，
        // 需要先用 COALESCE 把每个参数替换为空串，才能保证拼接结果非 NULL。
        let coalesced: Vec<String> = parts
            .iter()
            .map(|p| format!("COALESCE({}, '')", p))
            .collect();
        coalesced.join(" || ")
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

    fn last_insert_id_sql(&self) -> Option<&'static str> {
        Some("last_insert_rowid()")
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

/// Oracle 方言实现（Oracle 23ai）
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

    fn last_insert_id_sql(&self) -> Option<&'static str> {
        // Oracle 没有独立可执行的"获取最后插入 ID"语句。
        // `RETURNING {pk} INTO :bind` 是 PL/SQL 子句，必须附加在 INSERT 之后，
        // 不能作为独立 SQL 执行。调用方应改用 `supports_returning()` 在 INSERT
        // 末尾追加 RETURNING 子句获取自增值。
        None
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
}

/// 将通用 SQL 类型映射为 SQL Server 类型
///
/// - BIGINT → BIGINT（保持）
/// - INT/INTEGER → INT
/// - VARCHAR(n) → NVARCHAR(n)（统一使用 Unicode）
/// - TEXT 系列 → NVARCHAR(MAX)
/// - BOOLEAN/BOOL → BIT
/// - 其他类型保持不变
fn map_to_sqlserver_type(sql_type: &str) -> String {
    let upper = sql_type.to_uppercase();
    let trimmed = upper.trim();

    if trimmed.starts_with("BIGINT") {
        sql_type.to_string()
    } else if matches!(trimmed, "INT" | "INTEGER") {
        "INT".to_string()
    } else if trimmed.starts_with("NVARCHAR") {
        sql_type.to_string()
    } else if trimmed.starts_with("VARCHAR") {
        sql_type.replacen("VARCHAR", "NVARCHAR", 1)
    } else if matches!(trimmed, "TEXT" | "MEDIUMTEXT" | "LONGTEXT" | "TINYTEXT") {
        "NVARCHAR(MAX)".to_string()
    } else if matches!(trimmed, "BOOLEAN" | "BOOL") {
        "BIT".to_string()
    } else {
        sql_type.to_string()
    }
}

/// SQL Server 方言实现（SQL Server 2012+ / T-SQL）
pub struct SqlServerDialect;

impl Dialect for SqlServerDialect {
    fn db_type(&self) -> DbType {
        DbType::SqlServer
    }

    fn quote(&self, identifier: &str) -> String {
        // SQL Server 使用 [name] 包裹标识符，内部 ] 双写为 ]]
        format!("[{}]", identifier.replace(']', "]]"))
    }

    fn escape_string(&self, s: &str) -> String {
        // SQL Server 标准：单引号双写（'O''Brien'），反斜杠不转义
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
        // SQL Server 使用 OUTPUT 子句，语义上等价于 RETURNING
        true
    }

    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String {
        // SQL Server 2012+ 使用 OFFSET ... ROWS FETCH NEXT ... ROWS ONLY
        let offset = page.saturating_sub(1).saturating_mul(limit);
        format!(
            "{} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
            sql, offset, limit
        )
    }

    fn json_type(&self) -> &'static str {
        // SQL Server 2016+ 使用 NVARCHAR(MAX) 存储 JSON
        "NVARCHAR(MAX)"
    }

    fn json_extract(&self, column: &str, path: &str) -> String {
        // SQL Server 2016+ 使用 JSON_VALUE 提取标量
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
        // SQL Server 使用 CONTAINS（需要 FULLTEXT 索引）
        if columns.is_empty() {
            return "0".to_string();
        }
        let escaped = self.escape_string(keyword);
        let cols = columns.join(", ");
        format!("CONTAINS({}, '{}')", cols, escaped)
    }

    fn bool_to_int(&self, expr: &str) -> String {
        // SQL Server 使用 BIT 类型，CASE WHEN 转换为 0/1
        format!("(CASE WHEN {} THEN 1 ELSE 0 END)", expr)
    }

    fn concat(&self, parts: &[&str]) -> String {
        if parts.is_empty() {
            return "NULL".to_string();
        }
        format!("CONCAT({})", parts.join(", "))
    }

    fn supports_if_exists(&self) -> bool {
        // SQL Server 2016+ 支持 IF EXISTS
        true
    }

    fn supports_if_not_exists(&self) -> bool {
        // SQL Server 2016+ 支持 IF NOT EXISTS
        true
    }

    fn auto_increment_keyword(&self) -> &'static str {
        // SQL Server 使用 IDENTITY(1,1) 列属性
        "IDENTITY(1,1)"
    }

    fn last_insert_id_sql(&self) -> Option<&'static str> {
        // SCOPE_IDENTITY() 返回当前作用域内最后生成的标识值
        Some("SCOPE_IDENTITY()")
    }

    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String {
        let cols: Vec<String> = columns
            .iter()
            .map(|col| {
                let sqlserver_type = map_to_sqlserver_type(&col.sql_type);
                let mut sql = format!("{} {}", self.quote(&col.name), sqlserver_type);
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
        let stmts: Vec<String> = changes
            .iter()
            .map(|change| match change {
                TableChange::AddColumn(col) => {
                    let sqlserver_type = map_to_sqlserver_type(&col.sql_type);
                    let mut sql = format!(
                        "ALTER TABLE {} ADD {} {}",
                        self.quote(table),
                        self.quote(&col.name),
                        sqlserver_type
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
                    // SQL Server 使用 ALTER COLUMN（不是 MODIFY）
                    let sqlserver_type = map_to_sqlserver_type(&col.sql_type);
                    let mut sql = format!(
                        "ALTER TABLE {} ALTER COLUMN {} {}",
                        self.quote(table),
                        self.quote(&col.name),
                        sqlserver_type
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
                    // SQL Server 的 DROP INDEX 必须指定表名
                    format!("DROP INDEX {} ON {}", name, self.quote(table))
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
}

// TODO: build_create_table / build_alter_table / build_drop_table 在四个方言中
// 存在重复代码，未来可抽出公共构建器减少维护成本（参见 dialect 重构 RFC）。

// ============================================================================
// 兼容方言（基于现有方言委派实现）
//
// 以下方言在 SQL 语法上与某个基础方言完全兼容，仅在 db_type() 上有区别。
// 使用宏减少重复代码，保持维护性。
// ============================================================================

/// 将方言实现委派给基础方言的宏
///
/// `$wrapper`：新方言结构体名
/// `$base`：基础方言结构体名（如 MySqlDialect）
/// `$db_type`：返回的 DbType 变体
macro_rules! delegate_dialect_to {
    ($wrapper:ident, $base:ident, $db_type:expr) => {
        /// 兼容方言（委派给基础方言实现）
        pub struct $wrapper;

        impl Dialect for $wrapper {
            fn db_type(&self) -> DbType {
                $db_type
            }
            fn quote(&self, identifier: &str) -> String {
                $base.quote(identifier)
            }
            fn escape_string(&self, s: &str) -> String {
                $base.escape_string(s)
            }
            fn supports_returning(&self) -> bool {
                $base.supports_returning()
            }
            fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String {
                $base.build_pagination(sql, page, limit)
            }
            fn json_type(&self) -> &'static str {
                $base.json_type()
            }
            fn json_extract(&self, column: &str, path: &str) -> String {
                $base.json_extract(column, path)
            }
            fn full_text_search(&self, columns: &[&str], keyword: &str) -> String {
                $base.full_text_search(columns, keyword)
            }
            fn bool_to_int(&self, expr: &str) -> String {
                $base.bool_to_int(expr)
            }
            fn concat(&self, parts: &[&str]) -> String {
                $base.concat(parts)
            }
            fn supports_if_exists(&self) -> bool {
                $base.supports_if_exists()
            }
            fn supports_if_not_exists(&self) -> bool {
                $base.supports_if_not_exists()
            }
            fn auto_increment_keyword(&self) -> &'static str {
                $base.auto_increment_keyword()
            }
            fn last_insert_id_sql(&self) -> Option<&'static str> {
                $base.last_insert_id_sql()
            }
            fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String {
                $base.build_create_table(table, columns)
            }
            fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String {
                $base.build_alter_table(table, changes)
            }
            fn build_drop_table(&self, table: &str, if_exists: bool) -> String {
                $base.build_drop_table(table, if_exists)
            }
        }
    };
}

// MariaDB：MySQL 兼容方言
delegate_dialect_to!(MariaDbDialect, MySqlDialect, DbType::MariaDB);

// TiDB：MySQL 兼容分布式数据库
delegate_dialect_to!(TiDbDialect, MySqlDialect, DbType::TiDB);

// KingbaseES：人大金仓，PostgreSQL 兼容方言
delegate_dialect_to!(KingbaseDialect, PostgreSqlDialect, DbType::Kingbase);

// PolarDB：阿里云，PostgreSQL 兼容（PG 版本）
delegate_dialect_to!(PolarDbDialect, PostgreSqlDialect, DbType::PolarDB);

// GaussDB：华为云，PostgreSQL 兼容分布式数据库
delegate_dialect_to!(GaussDbDialect, PostgreSqlDialect, DbType::GaussDB);

// Dameng：达梦 DM8，Oracle 兼容方言
delegate_dialect_to!(DamengDialect, OracleDialect, DbType::Dameng);

// Sybase ASE：与 SQL Server T-SQL 高度兼容
delegate_dialect_to!(SybaseDialect, SqlServerDialect, DbType::Sybase);

// GBase 8s：南大通用，Informix 兼容方言，SQL 语法接近 T-SQL
delegate_dialect_to!(GBaseDialect, SqlServerDialect, DbType::GBase);

// ============================================================================
// ClickHouse 方言（独立实现）
//
// ClickHouse 是列式 OLAP 数据库，有独特的类型系统和函数：
// - 使用 backtick 标识符（与 MySQL 一致）
// - 字符串字面量单引号转义为 \'（与 MySQL 一致）
// - 不支持事务、不支持 RETURNING
// - 分页使用 LIMIT offset, limit 语法（与 MySQL 一致）
// - 自增列不支持（使用 UUID 或物化列）
// - 类型系统：String/UInt64/Int64/Float64/DateTime 等
// ============================================================================

/// ClickHouse 方言实现（列式 OLAP 数据库）
pub struct ClickHouseDialect;

impl Dialect for ClickHouseDialect {
    fn db_type(&self) -> DbType {
        DbType::ClickHouse
    }

    fn quote(&self, identifier: &str) -> String {
        // ClickHouse 使用 backquote 包裹标识符（与 MySQL 一致）
        format!("`{}`", identifier.replace('`', "``"))
    }

    fn escape_string(&self, s: &str) -> String {
        // ClickHouse 使用反斜杠转义（与 MySQL 一致）
        let mut escaped = String::with_capacity(s.len() * 2);
        for c in s.chars() {
            match c {
                '\'' => escaped.push_str("\\'"),
                '\\' => escaped.push_str("\\\\"),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                _ => escaped.push(c),
            }
        }
        escaped
    }

    fn supports_returning(&self) -> bool {
        // ClickHouse 不支持 RETURNING 子句
        false
    }

    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String {
        // ClickHouse 使用 LIMIT offset, limit 语法
        let offset = page.saturating_sub(1).saturating_mul(limit);
        format!("{} LIMIT {}, {}", sql, offset, limit)
    }

    fn json_type(&self) -> &'static str {
        // ClickHouse 使用 String 存储 JSON，或使用 JSON 类型（实验性）
        "String"
    }

    fn json_extract(&self, column: &str, path: &str) -> String {
        // ClickHouse 使用 JSONExtractString 函数
        let normalized = if path.starts_with('$') {
            path.to_string()
        } else {
            format!("$.{}", path)
        };
        format!(
            "JSONExtractString({}, '{}')",
            column,
            self.escape_string(&normalized)
        )
    }

    fn full_text_search(&self, columns: &[&str], keyword: &str) -> String {
        // ClickHouse 使用 position() + like 进行全文检索（无原生 FTS）
        if columns.is_empty() {
            return "0".to_string();
        }
        let escaped = self.escape_string(keyword);
        let parts: Vec<String> = columns
            .iter()
            .map(|c| format!("position({}, '{}') > 0", c, escaped))
            .collect();
        parts.join(" OR ")
    }

    fn bool_to_int(&self, expr: &str) -> String {
        // ClickHouse 支持 toUInt8 转换
        format!("toUInt8({})", expr)
    }

    fn concat(&self, parts: &[&str]) -> String {
        // ClickHouse 使用 concat() 函数
        if parts.is_empty() {
            return "''".to_string();
        }
        format!("concat({})", parts.join(", "))
    }

    fn supports_if_exists(&self) -> bool {
        true
    }

    fn supports_if_not_exists(&self) -> bool {
        true
    }

    fn auto_increment_keyword(&self) -> &'static str {
        // ClickHouse 不支持自增列，使用 UUID 默认值
        ""
    }

    fn last_insert_id_sql(&self) -> Option<&'static str> {
        // ClickHouse 不支持 last_insert_id
        None
    }

    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String {
        let cols: Vec<String> = columns
            .iter()
            .map(|col| {
                let ch_type = map_to_clickhouse_type(&col.sql_type);
                let mut sql = format!("{} {}", self.quote(&col.name), ch_type);
                if let Some(default) = &col.default {
                    sql.push_str(&format!(" DEFAULT {}", default));
                }
                if col.primary_key {
                    sql.push_str(" PRIMARY KEY");
                }
                sql
            })
            .collect();

        // ClickHouse 必须指定 Engine，默认使用 MergeTree
        format!(
            "CREATE TABLE {} ({}) ENGINE = MergeTree()",
            self.quote(table),
            cols.join(", ")
        )
    }

    fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String {
        let stmts: Vec<String> = changes
            .iter()
            .map(|change| match change {
                TableChange::AddColumn(col) => {
                    let ch_type = map_to_clickhouse_type(&col.sql_type);
                    format!(
                        "ALTER TABLE {} ADD COLUMN {} {}",
                        self.quote(table),
                        self.quote(&col.name),
                        ch_type
                    )
                }
                TableChange::DropColumn(name) => {
                    format!(
                        "ALTER TABLE {} DROP COLUMN {}",
                        self.quote(table),
                        self.quote(name)
                    )
                }
                TableChange::ModifyColumn(col) => {
                    let ch_type = map_to_clickhouse_type(&col.sql_type);
                    format!(
                        "ALTER TABLE {} MODIFY COLUMN {} {}",
                        self.quote(table),
                        self.quote(&col.name),
                        ch_type
                    )
                }
                TableChange::AddIndex(name, cols) => {
                    format!(
                        "ALTER TABLE {} ADD INDEX {} ({})",
                        self.quote(table),
                        name,
                        cols.join(", ")
                    )
                }
                TableChange::DropIndex(name) => {
                    format!("ALTER TABLE {} DROP INDEX {}", self.quote(table), name)
                }
                TableChange::AddForeignKey { .. } => {
                    // ClickHouse 不支持外键，跳过
                    String::new()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();

        stmts.join("; ")
    }
}

/// 将通用 SQL 类型映射为 ClickHouse 类型
///
/// - BIGINT → Int64
/// - INT/INTEGER → Int32
/// - VARCHAR(n)/TEXT → String
/// - BOOLEAN/BOOL → UInt8
/// - FLOAT → Float32
/// - DOUBLE → Float64
/// - DATETIME/TIMESTAMP → DateTime
fn map_to_clickhouse_type(sql_type: &str) -> String {
    let upper = sql_type.to_uppercase();
    let trimmed = upper.trim();

    if trimmed.starts_with("BIGINT") {
        "Int64".to_string()
    } else if matches!(trimmed, "INT" | "INTEGER") {
        "Int32".to_string()
    } else if matches!(trimmed, "TINYINT" | "SMALLINT") {
        "Int16".to_string()
    } else if trimmed.starts_with("VARCHAR")
        || trimmed.starts_with("CHAR")
        || matches!(trimmed, "TEXT" | "MEDIUMTEXT" | "LONGTEXT" | "TINYTEXT")
    {
        "String".to_string()
    } else if matches!(trimmed, "BOOLEAN" | "BOOL") {
        "UInt8".to_string()
    } else if matches!(trimmed, "FLOAT" | "REAL") {
        "Float32".to_string()
    } else if matches!(trimmed, "DOUBLE" | "DOUBLE PRECISION") {
        "Float64".to_string()
    } else if matches!(trimmed, "DATETIME" | "TIMESTAMP") {
        "DateTime".to_string()
    } else if matches!(trimmed, "DATE") {
        "Date".to_string()
    } else if trimmed.starts_with("DECIMAL") || trimmed.starts_with("NUMERIC") {
        "Decimal(38, 4)".to_string()
    } else {
        sql_type.to_string()
    }
}

// ============================================================================
// IBM DB2 方言（独立实现）
//
// DB2 LUW 语法特性：
// - 双引号标识符（标准 SQL 风格）
// - 字符串字面量单引号转义为 ''（标准 SQL 风格）
// - 支持 RETURNING（DB2 11.5+）
// - 分页使用 OFFSET x ROWS FETCH NEXT y ROWS ONLY（标准 SQL:2008）
// - 自增列使用 GENERATED ALWAYS AS IDENTITY
// - 类型系统：VARCHAR/INTEGER/BIGINT/TIMESTAMP/DECFLOAT 等
// ============================================================================

/// IBM DB2 LUW 方言实现
pub struct Db2Dialect;

impl Dialect for Db2Dialect {
    fn db_type(&self) -> DbType {
        DbType::Db2
    }

    fn quote(&self, identifier: &str) -> String {
        // DB2 使用双引号包裹标识符（标准 SQL 风格）
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }

    fn escape_string(&self, s: &str) -> String {
        // DB2 标准转义：单引号双写（'O''Brien'）
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
        // DB2 11.5+ 支持 RETURNING（实际使用 SELECT FROM FINAL TABLE 代替）
        false
    }

    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String {
        // DB2 使用 OFFSET ... ROWS FETCH NEXT ... ROWS ONLY（SQL:2008 标准）
        let offset = page.saturating_sub(1).saturating_mul(limit);
        format!(
            "{} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
            sql, offset, limit
        )
    }

    fn json_type(&self) -> &'static str {
        // DB2 11.5+ 原生支持 JSON 类型
        "JSON"
    }

    fn json_extract(&self, column: &str, path: &str) -> String {
        // DB2 使用 JSON_VALUE 函数
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
        // DB2 使用 CONTAINS 函数（需 DB2TEXT 索引）
        if columns.is_empty() {
            return "0".to_string();
        }
        let escaped = self.escape_string(keyword);
        let parts: Vec<String> = columns
            .iter()
            .map(|c| format!("CONTAINS({}, '{}') > 0", c, escaped))
            .collect();
        parts.join(" OR ")
    }

    fn bool_to_int(&self, expr: &str) -> String {
        // DB2 没有原生 BOOL（11.5+ 有 BOOLEAN），使用 CASE WHEN 转换
        format!("(CASE WHEN {} THEN 1 ELSE 0 END)", expr)
    }

    fn concat(&self, parts: &[&str]) -> String {
        // DB2 使用 || 操作符进行字符串拼接
        if parts.is_empty() {
            return "''".to_string();
        }
        parts.join(" || ")
    }

    fn supports_if_exists(&self) -> bool {
        // DB2 不支持 DROP TABLE IF EXISTS（直到 11.5）
        false
    }

    fn supports_if_not_exists(&self) -> bool {
        // DB2 不支持 CREATE TABLE IF NOT EXISTS
        false
    }

    fn auto_increment_keyword(&self) -> &'static str {
        // DB2 使用 GENERATED ALWAYS AS IDENTITY
        "GENERATED ALWAYS AS IDENTITY"
    }

    fn last_insert_id_sql(&self) -> Option<&'static str> {
        // DB2 使用 IDENTITY_VAL_LOCAL() 函数获取最后插入的 IDENTITY 值
        Some("SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1")
    }

    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String {
        let cols: Vec<String> = columns
            .iter()
            .map(|col| {
                let db2_type = map_to_db2_type(&col.sql_type);
                let mut sql = format!("{} {}", self.quote(&col.name), db2_type);
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
                    let db2_type = map_to_db2_type(&col.sql_type);
                    let mut sql = format!(
                        "ALTER TABLE {} ADD COLUMN {} {}",
                        self.quote(table),
                        self.quote(&col.name),
                        db2_type
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
                    let db2_type = map_to_db2_type(&col.sql_type);
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET DATA TYPE {}",
                        self.quote(table),
                        self.quote(&col.name),
                        db2_type
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
        // DB2 不支持 IF EXISTS，但为兼容性保留参数
        let _ = if_exists;
        format!("DROP TABLE {}", self.quote(table))
    }
}

/// 将通用 SQL 类型映射为 IBM DB2 类型
///
/// - BIGINT → BIGINT
/// - INT/INTEGER → INTEGER
/// - VARCHAR(n) → VARCHAR(n)
/// - TEXT 系列 → CLOB(2G)
/// - BOOLEAN/BOOL → SMALLINT（DB2 11.5+ 才有 BOOLEAN）
/// - DATETIME/TIMESTAMP → TIMESTAMP
fn map_to_db2_type(sql_type: &str) -> String {
    let upper = sql_type.to_uppercase();
    let trimmed = upper.trim();

    if trimmed.starts_with("BIGINT") {
        "BIGINT".to_string()
    } else if matches!(trimmed, "INT" | "INTEGER") {
        "INTEGER".to_string()
    } else if matches!(trimmed, "TINYINT" | "SMALLINT") {
        "SMALLINT".to_string()
    } else if trimmed.starts_with("VARCHAR") || trimmed.starts_with("CHAR") {
        sql_type.to_string()
    } else if matches!(trimmed, "TEXT" | "MEDIUMTEXT" | "LONGTEXT" | "TINYTEXT") {
        "CLOB(2G)".to_string()
    } else if matches!(trimmed, "BOOLEAN" | "BOOL") {
        "SMALLINT".to_string()
    } else if matches!(trimmed, "FLOAT" | "REAL") {
        "REAL".to_string()
    } else if matches!(trimmed, "DOUBLE" | "DOUBLE PRECISION") {
        "DOUBLE".to_string()
    } else if matches!(trimmed, "DATETIME" | "TIMESTAMP") {
        "TIMESTAMP".to_string()
    } else if matches!(trimmed, "DATE") {
        "DATE".to_string()
    } else {
        // DECIMAL/NUMERIC 和其他未匹配类型保持原样
        sql_type.to_string()
    }
}

/// 根据数据库类型获取对应的方言实例
///
/// L-5 修复：补充示例文档
///
/// 返回 `Box<dyn Dialect>`，可用于与 `QueryBuilder`、`Schema` 等组件配合。
/// 对于不支持 SQL 方言的数据库类型（如 Redis、MongoDB、向量库），返回
/// `DbError::Unsupported`。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_core::db_type::DbType;
/// use sz_orm_core::dialect::{get_dialect, Dialect};
///
/// let dialect = get_dialect(DbType::MySQL).unwrap();
/// assert_eq!(dialect.quote("user"), "`user`");
/// assert_eq!(dialect.db_type(), DbType::MySQL);
///
/// // 不支持的类型返回错误
/// let err = get_dialect(DbType::Redis).unwrap_err();
/// assert!(matches!(err, sz_orm_core::DbError::Unsupported(_)));
/// ```
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
        DbType::ClickHouse => Ok(Box::new(ClickHouseDialect)),
        DbType::Oracle => Ok(Box::new(OracleDialect)),
        DbType::OceanBase => Ok(Box::new(MySqlDialect)),
        DbType::SqlServer => Ok(Box::new(SqlServerDialect)),
        DbType::VectorDb => Err(DbError::Unsupported(
            "Vector databases have specific APIs".to_string(),
        )),
        DbType::PureJsDb => Err(DbError::Unsupported(
            "PureJS database uses JavaScript".to_string(),
        )),
        // 国产数据库 & 兼容方言
        DbType::Dameng => Ok(Box::new(DamengDialect)),
        DbType::Kingbase => Ok(Box::new(KingbaseDialect)),
        DbType::Db2 => Ok(Box::new(Db2Dialect)),
        DbType::MariaDB => Ok(Box::new(MariaDbDialect)),
        DbType::TiDB => Ok(Box::new(TiDbDialect)),
        DbType::PolarDB => Ok(Box::new(PolarDbDialect)),
        DbType::GaussDB => Ok(Box::new(GaussDbDialect)),
        DbType::GBase => Ok(Box::new(GBaseDialect)),
        DbType::Sybase => Ok(Box::new(SybaseDialect)),
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
        // SQLite 接口只传入列名，无 FTS 表名，因此降级使用 LIKE
        assert!(sql.contains("LIKE"));
        assert!(sql.contains("title LIKE '%hello%'"));
        assert!(sql.contains("content LIKE '%hello%'"));
        assert!(sql.contains(" OR "));

        // 空列列表返回 "0"（短路避免空 IN/OR）
        assert_eq!(sqlite.full_text_search(&[], "hello"), "0");

        // 含单引号的关键字需正确转义（SQLite 双单引号）
        let sql = sqlite.full_text_search(&["title"], "it's");
        assert!(sql.contains("title LIKE '%it''s%'"));
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
        // last_insert_id_sql: Oracle 不支持独立可执行的获取最后插入 ID 语句
        // RETURNING ... INTO 是 PL/SQL 子句，必须附加在 INSERT 之后
        assert_eq!(dialect.last_insert_id_sql(), None);
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
        // Oracle 没有 standalone 的 last_insert_id SQL
        assert_eq!(dialect.last_insert_id_sql(), None);
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

    // ===================== SQLite concat 修复测试 =====================

    #[test]
    fn test_sqlite_concat_handles_null() {
        let sqlite = SqliteDialect;
        // 修复前：COALESCE(a || b) 在 a 为 NULL 时整体已 NULL，COALESCE 失效
        // 修复后：每个参数 COALESCE 为空串，保证拼接结果非 NULL
        let sql = sqlite.concat(&["a", "b"]);
        assert_eq!(sql, "COALESCE(a, '') || COALESCE(b, '')");
        // 单参数
        let sql = sqlite.concat(&["a"]);
        assert_eq!(sql, "COALESCE(a, '')");
        // 空列表
        assert_eq!(sqlite.concat(&[]), "NULL");
    }

    // ===================== SQL Server 方言测试 =====================

    #[test]
    fn test_sqlserver_quote_and_escape() {
        let dialect = SqlServerDialect;
        // 标识符使用 [name] 包裹，内部 ] 双写
        assert_eq!(dialect.quote("users"), "[users]");
        assert_eq!(dialect.quote("col]name"), "[col]]name]");
        // 字符串字面量：单引号双写，反斜杠不转义
        assert_eq!(dialect.escape_string("hello"), "hello");
        assert_eq!(dialect.escape_string("it's"), "it''s");
        assert_eq!(dialect.escape_string("O'Brien"), "O''Brien");
        assert_eq!(dialect.escape_string("path\\to"), "path\\to");
    }

    #[test]
    fn test_sqlserver_pagination() {
        let dialect = SqlServerDialect;
        // SQL Server 2012+ 使用 OFFSET ... ROWS FETCH NEXT ... ROWS ONLY
        let sql = dialect.build_pagination("SELECT * FROM users", 1, 10);
        assert_eq!(
            sql,
            "SELECT * FROM users OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"
        );
        let sql = dialect.build_pagination("SELECT * FROM users", 3, 20);
        assert_eq!(
            sql,
            "SELECT * FROM users OFFSET 40 ROWS FETCH NEXT 20 ROWS ONLY"
        );
        // page=0 边界
        let sql = dialect.build_pagination("SELECT * FROM users", 0, 10);
        assert_eq!(
            sql,
            "SELECT * FROM users OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"
        );
    }

    #[test]
    fn test_sqlserver_misc_dialect_methods() {
        let dialect = SqlServerDialect;
        assert_eq!(dialect.db_type(), DbType::SqlServer);
        // supports_returning: SQL Server 使用 OUTPUT，语义等价
        assert!(dialect.supports_returning());
        // supports_if_exists / supports_if_not_exists: SQL Server 2016+
        assert!(dialect.supports_if_exists());
        assert!(dialect.supports_if_not_exists());
        // auto_increment_keyword
        assert_eq!(dialect.auto_increment_keyword(), "IDENTITY(1,1)");
        // last_insert_id_sql
        assert_eq!(dialect.last_insert_id_sql(), Some("SCOPE_IDENTITY()"));
        // json_type
        assert_eq!(dialect.json_type(), "NVARCHAR(MAX)");
    }

    #[test]
    fn test_sqlserver_json_extract() {
        let dialect = SqlServerDialect;
        let sql = dialect.json_extract("data", "$.user.name");
        assert!(sql.starts_with("JSON_VALUE(data, '$.user.name')"));
        // 自动补 $. 前缀
        let sql = dialect.json_extract("data", "user.name");
        assert!(sql.contains("$.user.name"));
        assert!(sql.contains("JSON_VALUE"));
        // 单引号转义
        let sql = dialect.json_extract("data", "$.key's");
        assert!(sql.contains("$.key''s"));
    }

    #[test]
    fn test_sqlserver_full_text_search() {
        let dialect = SqlServerDialect;
        let sql = dialect.full_text_search(&["title", "content"], "hello");
        assert!(sql.starts_with("CONTAINS(title, content, 'hello')"));
        // 空列列表
        assert_eq!(dialect.full_text_search(&[], "hello"), "0");
        // 单引号转义
        let sql = dialect.full_text_search(&["title"], "it's");
        assert!(sql.contains("it''s"));
    }

    #[test]
    fn test_sqlserver_bool_to_int_and_concat() {
        let dialect = SqlServerDialect;
        assert_eq!(
            dialect.bool_to_int("active"),
            "(CASE WHEN active THEN 1 ELSE 0 END)"
        );
        assert_eq!(dialect.concat(&["a", "b", "c"]), "CONCAT(a, b, c)");
        assert_eq!(dialect.concat(&[]), "NULL");
    }

    #[test]
    fn test_sqlserver_create_table() {
        let dialect = SqlServerDialect;
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
        // 标识符使用方括号
        assert!(sql.contains("[users]"));
        assert!(sql.contains("[id]"));
        // 类型映射
        assert!(sql.contains("IDENTITY(1,1)"));
        assert!(
            sql.contains("NVARCHAR(255)"),
            "VARCHAR should map to NVARCHAR: {}",
            sql
        );
        assert!(
            sql.contains("NVARCHAR(MAX)"),
            "TEXT should map to NVARCHAR(MAX): {}",
            sql
        );
        assert!(sql.contains("BIT"), "BOOLEAN should map to BIT: {}", sql);
        assert!(sql.contains("PRIMARY KEY"));
        assert!(sql.contains("NOT NULL"));
        assert!(sql.contains("DEFAULT 1"));
    }

    #[test]
    fn test_sqlserver_drop_table() {
        let dialect = SqlServerDialect;
        assert_eq!(
            dialect.build_drop_table("users", true),
            "DROP TABLE IF EXISTS [users]"
        );
        assert_eq!(
            dialect.build_drop_table("users", false),
            "DROP TABLE [users]"
        );
    }

    #[test]
    fn test_sqlserver_alter_table() {
        let dialect = SqlServerDialect;
        // MODIFY COLUMN: SQL Server 使用 ALTER COLUMN（不是 MODIFY）
        let col = ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        };
        let sql = dialect.build_alter_table("users", &[TableChange::ModifyColumn(col)]);
        assert!(sql.contains("ALTER COLUMN"));
        assert!(sql.contains("NVARCHAR(255)"));
        assert!(!sql.contains("MODIFY"));

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
        assert!(sql.contains("ADD [email]"));
        assert!(sql.contains("NVARCHAR(255)"));

        // DROP COLUMN
        let sql =
            dialect.build_alter_table("users", &[TableChange::DropColumn("email".to_string())]);
        assert!(sql.contains("DROP COLUMN"));
        assert!(sql.contains("[email]"));

        // DROP INDEX 必须指定表名
        let sql =
            dialect.build_alter_table("users", &[TableChange::DropIndex("idx_name".to_string())]);
        assert!(sql.contains("DROP INDEX idx_name ON [users]"));
    }

    #[test]
    fn test_sqlserver_get_dialect() {
        // get_dialect 应返回 SqlServerDialect，而不是 MySqlDialect 回退
        let dialect = get_dialect(DbType::SqlServer);
        assert!(dialect.is_ok(), "SqlServer dialect should be available");
        let dialect = dialect.unwrap();
        assert_eq!(dialect.db_type(), DbType::SqlServer);
        // 验证不是 MySqlDialect 的回退：SQL Server 使用方括号，MySQL 使用反引号
        assert_eq!(dialect.quote("users"), "[users]");
        // SQL Server 特有功能
        assert_eq!(dialect.last_insert_id_sql(), Some("SCOPE_IDENTITY()"));
        assert_eq!(dialect.auto_increment_keyword(), "IDENTITY(1,1)");
    }

    #[test]
    fn test_clickhouse_get_dialect_unsupported() {
        // ClickHouse 现已支持独立方言（不再回退到 MySqlDialect）
        let dialect = get_dialect(DbType::ClickHouse);
        assert!(dialect.is_ok(), "ClickHouse should be supported");
        let dialect = dialect.unwrap();
        assert_eq!(dialect.db_type(), DbType::ClickHouse);
        // ClickHouse 使用 backquote 标识符（与 MySQL 一致）
        assert_eq!(dialect.quote("users"), "`users`");
        // ClickHouse 不支持 RETURNING
        assert!(!dialect.supports_returning());
        // ClickHouse 使用 LIMIT offset, limit 分页
        let sql = dialect.build_pagination("SELECT * FROM t", 2, 10);
        assert_eq!(sql, "SELECT * FROM t LIMIT 10, 10");
        // ClickHouse 自增列为空字符串
        assert_eq!(dialect.auto_increment_keyword(), "");
    }

    #[test]
    fn test_get_dialect_all_supported_types() {
        // 所有声称为支持的方言应正确返回，不支持的应返回 Err
        assert!(get_dialect(DbType::MySQL).is_ok());
        assert!(get_dialect(DbType::PostgreSQL).is_ok());
        assert!(get_dialect(DbType::Sqlite).is_ok());
        assert!(get_dialect(DbType::Oracle).is_ok());
        assert!(get_dialect(DbType::SqlServer).is_ok());
        assert!(get_dialect(DbType::OceanBase).is_ok());
        assert!(get_dialect(DbType::ClickHouse).is_ok());
        // 国产数据库 & 兼容方言
        assert!(get_dialect(DbType::Dameng).is_ok());
        assert!(get_dialect(DbType::Kingbase).is_ok());
        assert!(get_dialect(DbType::Db2).is_ok());
        assert!(get_dialect(DbType::MariaDB).is_ok());
        assert!(get_dialect(DbType::TiDB).is_ok());
        assert!(get_dialect(DbType::PolarDB).is_ok());
        assert!(get_dialect(DbType::GaussDB).is_ok());
        assert!(get_dialect(DbType::GBase).is_ok());
        assert!(get_dialect(DbType::Sybase).is_ok());
        // 不支持标准 SQL 的
        assert!(get_dialect(DbType::Redis).is_err());
        assert!(get_dialect(DbType::MongoDB).is_err());
        assert!(get_dialect(DbType::VectorDb).is_err());
        assert!(get_dialect(DbType::PureJsDb).is_err());
    }

    // ===== 国产数据库兼容方言测试 =====

    #[test]
    fn test_mariadb_dialect() {
        let dialect = get_dialect(DbType::MariaDB).unwrap();
        assert_eq!(dialect.db_type(), DbType::MariaDB);
        // MariaDB 兼容 MySQL 语法：backquote 标识符 + 反斜杠转义
        assert_eq!(dialect.quote("users"), "`users`");
        assert_eq!(dialect.escape_string("it's"), "it\\'s");
        assert_eq!(dialect.auto_increment_keyword(), "AUTO_INCREMENT");
        // MySQL 家族不支持 RETURNING（MySQL 协议层限制）
        assert!(!dialect.supports_returning());
    }

    #[test]
    fn test_tidb_dialect() {
        let dialect = get_dialect(DbType::TiDB).unwrap();
        assert_eq!(dialect.db_type(), DbType::TiDB);
        // TiDB 兼容 MySQL 语法
        assert_eq!(dialect.quote("users"), "`users`");
        assert_eq!(dialect.escape_string("it's"), "it\\'s");
        assert_eq!(dialect.auto_increment_keyword(), "AUTO_INCREMENT");
    }

    #[test]
    fn test_dameng_dialect() {
        let dialect = get_dialect(DbType::Dameng).unwrap();
        assert_eq!(dialect.db_type(), DbType::Dameng);
        // 达梦兼容 Oracle 语法：双引号标识符 + 单引号双写
        assert_eq!(dialect.quote("users"), "\"users\"");
        assert_eq!(dialect.escape_string("it's"), "it''s");
        // Oracle 兼容：使用 IDENTITY 列
        assert_eq!(
            dialect.auto_increment_keyword(),
            "GENERATED BY DEFAULT AS IDENTITY"
        );
        // Oracle 兼容：支持 RETURNING
        assert!(dialect.supports_returning());
    }

    #[test]
    fn test_kingbase_dialect() {
        let dialect = get_dialect(DbType::Kingbase).unwrap();
        assert_eq!(dialect.db_type(), DbType::Kingbase);
        // 人大金仓兼容 PostgreSQL 语法：双引号标识符 + 单引号双写
        assert_eq!(dialect.quote("users"), "\"users\"");
        assert_eq!(dialect.escape_string("it's"), "it''s");
        // PG 兼容：支持 RETURNING
        assert!(dialect.supports_returning());
        // PG 兼容：使用 GENERATED BY DEFAULT AS IDENTITY（PG 10+ 标准）
        assert_eq!(
            dialect.auto_increment_keyword(),
            "GENERATED BY DEFAULT AS IDENTITY"
        );
    }

    #[test]
    fn test_polardb_dialect() {
        let dialect = get_dialect(DbType::PolarDB).unwrap();
        assert_eq!(dialect.db_type(), DbType::PolarDB);
        // PolarDB-PG 兼容 PostgreSQL
        assert_eq!(dialect.quote("users"), "\"users\"");
        assert!(dialect.supports_returning());
    }

    #[test]
    fn test_gaussdb_dialect() {
        let dialect = get_dialect(DbType::GaussDB).unwrap();
        assert_eq!(dialect.db_type(), DbType::GaussDB);
        // GaussDB 兼容 PostgreSQL
        assert_eq!(dialect.quote("users"), "\"users\"");
        assert!(dialect.supports_returning());
    }

    #[test]
    fn test_gbase_dialect() {
        let dialect = get_dialect(DbType::GBase).unwrap();
        assert_eq!(dialect.db_type(), DbType::GBase);
        // GBase 8s 兼容 T-SQL（使用方括号）
        assert_eq!(dialect.quote("users"), "[users]");
    }

    #[test]
    fn test_sybase_dialect() {
        let dialect = get_dialect(DbType::Sybase).unwrap();
        assert_eq!(dialect.db_type(), DbType::Sybase);
        // Sybase ASE 兼容 T-SQL
        assert_eq!(dialect.quote("users"), "[users]");
    }

    // ===== DB2 独立方言测试 =====

    #[test]
    fn test_db2_dialect_basic() {
        let dialect = get_dialect(DbType::Db2).unwrap();
        assert_eq!(dialect.db_type(), DbType::Db2);
        // DB2 使用双引号标识符
        assert_eq!(dialect.quote("users"), "\"users\"");
        // DB2 单引号双写
        assert_eq!(dialect.escape_string("it's"), "it''s");
        // DB2 使用 IDENTITY
        assert_eq!(
            dialect.auto_increment_keyword(),
            "GENERATED ALWAYS AS IDENTITY"
        );
        // DB2 不支持 IF EXISTS / IF NOT EXISTS（11.5 之前）
        assert!(!dialect.supports_if_exists());
        assert!(!dialect.supports_if_not_exists());
        // DB2 不支持 RETURNING（使用 SELECT FROM FINAL TABLE 代替）
        assert!(!dialect.supports_returning());
    }

    #[test]
    fn test_db2_pagination() {
        let dialect = Db2Dialect;
        // DB2 使用 OFFSET x ROWS FETCH NEXT y ROWS ONLY
        let sql = dialect.build_pagination("SELECT * FROM users", 2, 10);
        assert_eq!(
            sql,
            "SELECT * FROM users OFFSET 10 ROWS FETCH NEXT 10 ROWS ONLY"
        );
    }

    #[test]
    fn test_db2_last_insert_id() {
        let dialect = Db2Dialect;
        // DB2 使用 IDENTITY_VAL_LOCAL() 获取最后插入的 IDENTITY
        assert_eq!(
            dialect.last_insert_id_sql(),
            Some("SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1")
        );
    }

    #[test]
    fn test_db2_concat() {
        let dialect = Db2Dialect;
        // DB2 使用 || 拼接
        assert_eq!(dialect.concat(&["a", "b", "c"]), "a || b || c");
        assert_eq!(dialect.concat(&[]), "''");
    }

    #[test]
    fn test_db2_create_table() {
        let dialect = Db2Dialect;
        let cols = vec![ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        }];
        let sql = dialect.build_create_table("users", &cols);
        assert!(sql.contains("\"id\" BIGINT"));
        assert!(sql.contains("GENERATED ALWAYS AS IDENTITY"));
        assert!(sql.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_db2_type_mapping() {
        // 验证通用类型到 DB2 类型的映射
        assert_eq!(map_to_db2_type("BIGINT"), "BIGINT");
        assert_eq!(map_to_db2_type("INT"), "INTEGER");
        assert_eq!(map_to_db2_type("INTEGER"), "INTEGER");
        assert_eq!(map_to_db2_type("TINYINT"), "SMALLINT");
        assert_eq!(map_to_db2_type("SMALLINT"), "SMALLINT");
        assert_eq!(map_to_db2_type("TEXT"), "CLOB(2G)");
        assert_eq!(map_to_db2_type("LONGTEXT"), "CLOB(2G)");
        assert_eq!(map_to_db2_type("BOOLEAN"), "SMALLINT");
        assert_eq!(map_to_db2_type("BOOL"), "SMALLINT");
        assert_eq!(map_to_db2_type("DATETIME"), "TIMESTAMP");
        assert_eq!(map_to_db2_type("TIMESTAMP"), "TIMESTAMP");
        assert_eq!(map_to_db2_type("DATE"), "DATE");
        assert_eq!(map_to_db2_type("VARCHAR(255)"), "VARCHAR(255)");
    }

    // ===== ClickHouse 独立方言测试 =====

    #[test]
    fn test_clickhouse_dialect_basic() {
        let dialect = get_dialect(DbType::ClickHouse).unwrap();
        assert_eq!(dialect.db_type(), DbType::ClickHouse);
        // ClickHouse 使用 backquote 标识符（与 MySQL 一致）
        assert_eq!(dialect.quote("users"), "`users`");
        // ClickHouse 反斜杠转义（与 MySQL 一致）
        assert_eq!(dialect.escape_string("it's"), "it\\'s");
        // ClickHouse 不支持 RETURNING
        assert!(!dialect.supports_returning());
        // ClickHouse 不支持自增列
        assert_eq!(dialect.auto_increment_keyword(), "");
        // ClickHouse 支持 IF EXISTS / IF NOT EXISTS
        assert!(dialect.supports_if_exists());
        assert!(dialect.supports_if_not_exists());
    }

    #[test]
    fn test_clickhouse_type_mapping() {
        assert_eq!(map_to_clickhouse_type("BIGINT"), "Int64");
        assert_eq!(map_to_clickhouse_type("INT"), "Int32");
        assert_eq!(map_to_clickhouse_type("INTEGER"), "Int32");
        assert_eq!(map_to_clickhouse_type("TINYINT"), "Int16");
        assert_eq!(map_to_clickhouse_type("SMALLINT"), "Int16");
        assert_eq!(map_to_clickhouse_type("VARCHAR(255)"), "String");
        assert_eq!(map_to_clickhouse_type("TEXT"), "String");
        assert_eq!(map_to_clickhouse_type("BOOLEAN"), "UInt8");
        assert_eq!(map_to_clickhouse_type("BOOL"), "UInt8");
        assert_eq!(map_to_clickhouse_type("FLOAT"), "Float32");
        assert_eq!(map_to_clickhouse_type("DOUBLE"), "Float64");
        assert_eq!(map_to_clickhouse_type("DATETIME"), "DateTime");
        assert_eq!(map_to_clickhouse_type("TIMESTAMP"), "DateTime");
        assert_eq!(map_to_clickhouse_type("DATE"), "Date");
    }

    #[test]
    fn test_clickhouse_create_table() {
        let dialect = ClickHouseDialect;
        let cols = vec![ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false, // ClickHouse 不支持自增
            primary_key: true,
        }];
        let sql = dialect.build_create_table("users", &cols);
        // 必须包含 ENGINE = MergeTree()
        assert!(
            sql.contains("ENGINE = MergeTree()"),
            "ClickHouse CREATE TABLE 必须指定 ENGINE: {}",
            sql
        );
        assert!(sql.contains("`id` Int64"));
        assert!(sql.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_clickhouse_json_extract() {
        let dialect = ClickHouseDialect;
        let sql = dialect.json_extract("data", "$.name");
        assert!(
            sql.contains("JSONExtractString"),
            "ClickHouse 应使用 JSONExtractString: {}",
            sql
        );
    }

    #[test]
    fn test_clickhouse_concat() {
        let dialect = ClickHouseDialect;
        // ClickHouse 使用 concat() 函数
        assert_eq!(dialect.concat(&["a", "b", "c"]), "concat(a, b, c)");
        assert_eq!(dialect.concat(&[]), "''");
    }

    // ===== DbType 国产数据库变体测试 =====

    #[test]
    fn test_db_type_dameng_str() {
        assert_eq!(DbType::Dameng.as_str(), "dameng");
        assert_eq!(DbType::from_str("dameng"), Some(DbType::Dameng));
        assert_eq!(DbType::from_str("DM"), Some(DbType::Dameng));
        assert_eq!(DbType::from_str("dm8"), Some(DbType::Dameng));
        assert_eq!(DbType::Dameng.default_port(), 5236);
    }

    #[test]
    fn test_db_type_kingbase_str() {
        assert_eq!(DbType::Kingbase.as_str(), "kingbase");
        assert_eq!(DbType::from_str("kingbase"), Some(DbType::Kingbase));
        assert_eq!(DbType::Kingbase.default_port(), 54321);
    }

    #[test]
    fn test_db_type_db2_str() {
        assert_eq!(DbType::Db2.as_str(), "db2");
        assert_eq!(DbType::from_str("db2"), Some(DbType::Db2));
        assert_eq!(DbType::Db2.default_port(), 50000);
    }

    #[test]
    fn test_db_type_mariadb_str() {
        assert_eq!(DbType::MariaDB.as_str(), "mariadb");
        assert_eq!(DbType::from_str("mariadb"), Some(DbType::MariaDB));
        assert_eq!(DbType::MariaDB.default_port(), 3306);
    }

    #[test]
    fn test_db_type_tidb_str() {
        assert_eq!(DbType::TiDB.as_str(), "tidb");
        assert_eq!(DbType::from_str("tidb"), Some(DbType::TiDB));
        assert_eq!(DbType::TiDB.default_port(), 4000);
    }

    #[test]
    fn test_db_type_polardb_str() {
        assert_eq!(DbType::PolarDB.as_str(), "polardb");
        assert_eq!(DbType::from_str("polardb"), Some(DbType::PolarDB));
        assert_eq!(DbType::PolarDB.default_port(), 5432);
    }

    #[test]
    fn test_db_type_gaussdb_str() {
        assert_eq!(DbType::GaussDB.as_str(), "gaussdb");
        assert_eq!(DbType::from_str("gaussdb"), Some(DbType::GaussDB));
        assert_eq!(DbType::GaussDB.default_port(), 25308);
    }

    #[test]
    fn test_db_type_gbase_str() {
        assert_eq!(DbType::GBase.as_str(), "gbase");
        assert_eq!(DbType::from_str("gbase"), Some(DbType::GBase));
        assert_eq!(DbType::GBase.default_port(), 9088);
    }

    #[test]
    fn test_db_type_sybase_str() {
        assert_eq!(DbType::Sybase.as_str(), "sybase");
        assert_eq!(DbType::from_str("sybase"), Some(DbType::Sybase));
        assert_eq!(DbType::Sybase.default_port(), 5000);
    }

    #[test]
    fn test_db_type_family_classification() {
        // MySQL 家族
        assert!(DbType::MySQL.is_mysql_family());
        assert!(DbType::MariaDB.is_mysql_family());
        assert!(DbType::TiDB.is_mysql_family());
        assert!(DbType::OceanBase.is_mysql_family());
        assert!(!DbType::PostgreSQL.is_mysql_family());

        // PostgreSQL 家族
        assert!(DbType::PostgreSQL.is_postgres_family());
        assert!(DbType::Kingbase.is_postgres_family());
        assert!(DbType::GaussDB.is_postgres_family());
        assert!(!DbType::MySQL.is_postgres_family());

        // Oracle 家族
        assert!(DbType::Oracle.is_oracle_family());
        assert!(DbType::Dameng.is_oracle_family());
        assert!(!DbType::MySQL.is_oracle_family());
    }

    #[test]
    fn test_db_type_supports_stored_procedure_extended() {
        // 所有 SQL 数据库应支持存储过程
        assert!(DbType::Dameng.supports_stored_procedure());
        assert!(DbType::Kingbase.supports_stored_procedure());
        assert!(DbType::Db2.supports_stored_procedure());
        assert!(DbType::MariaDB.supports_stored_procedure());
        assert!(DbType::TiDB.supports_stored_procedure());
        assert!(DbType::PolarDB.supports_stored_procedure());
        assert!(DbType::GaussDB.supports_stored_procedure());
        assert!(DbType::GBase.supports_stored_procedure());
        assert!(DbType::Sybase.supports_stored_procedure());
    }

    // ===== L-4 修复：表名/列名长度校验 =====

    #[test]
    fn test_l4_max_identifier_len_constant() {
        // MAX_IDENTIFIER_LEN 应为 63（PostgreSQL 最严格值）
        assert_eq!(MAX_IDENTIFIER_LEN, 63);
    }

    #[test]
    fn test_l4_quote_checked_valid_identifier() {
        let dialect = MySqlDialect;
        assert_eq!(dialect.quote_checked("users").unwrap(), "`users`");
        assert_eq!(dialect.quote_checked("user_id").unwrap(), "`user_id`");
        // 边界：恰好 63 字符
        let name_63 = "a".repeat(63);
        assert!(dialect.quote_checked(&name_63).is_ok());
    }

    #[test]
    fn test_l4_quote_checked_rejects_too_long() {
        let dialect = MySqlDialect;
        let long_name = "a".repeat(64); // 64 > 63
        let result = dialect.quote_checked(&long_name);
        assert!(result.is_err());
        match result {
            Err(DbError::InvalidInput(msg)) => {
                assert!(
                    msg.contains("too long"),
                    "expected 'too long' error, got: {}",
                    msg
                );
            }
            _ => panic!("Expected DbError::InvalidInput"),
        }
    }

    #[test]
    fn test_l4_quote_checked_rejects_empty() {
        let dialect = MySqlDialect;
        let result = dialect.quote_checked("");
        assert!(result.is_err());
    }

    #[test]
    fn test_l4_quote_checked_rejects_sql_injection() {
        let dialect = MySqlDialect;
        // 含分号
        assert!(dialect.quote_checked("users; DROP TABLE users").is_err());
        // 含引号
        assert!(dialect.quote_checked("user'name").is_err());
        // 含空格
        assert!(dialect.quote_checked("user name").is_err());
        // 数字开头
        assert!(dialect.quote_checked("1users").is_err());
        // 含点号
        assert!(dialect.quote_checked("schema.table").is_err());
    }

    #[test]
    fn test_l4_quote_checked_postgres() {
        let dialect = PostgreSqlDialect;
        assert_eq!(dialect.quote_checked("users").unwrap(), "\"users\"");
        assert!(dialect.quote_checked(&"a".repeat(64)).is_err());
    }

    #[test]
    fn test_l4_quote_checked_sqlite() {
        let dialect = SqliteDialect;
        assert_eq!(dialect.quote_checked("users").unwrap(), "\"users\"");
        assert!(dialect.quote_checked(&"a".repeat(64)).is_err());
    }

    #[test]
    fn test_l4_quote_checked_oracle() {
        let dialect = OracleDialect;
        assert_eq!(dialect.quote_checked("users").unwrap(), "\"users\"");
        assert!(dialect.quote_checked(&"a".repeat(64)).is_err());
    }

    #[test]
    fn test_l4_quote_checked_sql_server() {
        let dialect = SqlServerDialect;
        assert_eq!(dialect.quote_checked("users").unwrap(), "[users]");
        assert!(dialect.quote_checked(&"a".repeat(64)).is_err());
    }
}
