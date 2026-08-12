//! 五方言索引 DDL 生成（`query-advisor` feature）
//!
//! 为 `AddIndex` 建议按目标方言生成正确的 `CREATE INDEX` DDL 文本。
//! 复用既有 28 方言枚举概念 `packages/sz-orm-core/src/db_type.rs:11`。

use crate::suggestion::SuggestionType;

/// 支持的方言（简化版，覆盖五种主流数据库）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvisorDialect {
    MySQL,
    PostgreSQL,
    SQLite,
    Oracle,
    MSSQL,
}

impl AdvisorDialect {
    /// 方言可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            AdvisorDialect::MySQL => "mysql",
            AdvisorDialect::PostgreSQL => "postgres",
            AdvisorDialect::SQLite => "sqlite",
            AdvisorDialect::Oracle => "oracle",
            AdvisorDialect::MSSQL => "mssql",
        }
    }
}

/// 生成 `CREATE INDEX` DDL 文本
///
/// 各方言差异：
/// - MySQL/SQLite：`CREATE INDEX idx_name ON table(col)`
/// - PostgreSQL：`CREATE INDEX idx_name ON table(col)`（可加 `USING GIST` 等扩展）
/// - Oracle：`CREATE INDEX idx_name ON table(col)`
/// - MSSQL：`CREATE INDEX idx_name ON table(col)`（可加 `WITH (ONLINE = ON)`）
pub fn create_index_ddl(
    dialect: AdvisorDialect,
    table: &str,
    columns: &[&str],
    index_name: Option<&str>,
) -> String {
    let cols_joined = columns.join("_");
    let default_name = format!("idx_{}_{}", table, cols_joined);
    let idx_name = index_name.unwrap_or(&default_name);
    let cols = columns.join(", ");
    let base = format!("CREATE INDEX {idx_name} ON {table}({cols})");
    match dialect {
        AdvisorDialect::PostgreSQL => format!("{base} -- PostgreSQL: 可选 USING GIST/GIN"),
        AdvisorDialect::MSSQL => format!("{base} WITH (ONLINE = ON) -- MSSQL: 在线创建"),
        _ => base,
    }
}

/// 生成 `DROP INDEX` DDL 文本
pub fn drop_index_ddl(dialect: AdvisorDialect, index_name: &str) -> String {
    match dialect {
        AdvisorDialect::MySQL | AdvisorDialect::SQLite => format!("DROP INDEX {index_name}"),
        AdvisorDialect::PostgreSQL => format!("DROP INDEX IF EXISTS {index_name}"),
        AdvisorDialect::Oracle => format!("DROP INDEX {index_name}"),
        AdvisorDialect::MSSQL => format!("DROP INDEX {index_name}"),
    }
}

/// 建议类型的方言特定动作文本
pub fn dialect_action(
    suggestion_type: SuggestionType,
    dialect: AdvisorDialect,
    table: &str,
    columns: &[&str],
) -> String {
    match suggestion_type {
        SuggestionType::AddIndex => create_index_ddl(dialect, table, columns, None),
        SuggestionType::DropIndex => {
            let idx_name = format!("idx_{}_{}", table, columns.join("_"));
            drop_index_ddl(dialect, &idx_name)
        }
        _ => format!("{:?} 不涉及 DDL", suggestion_type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_create_index_syntax() {
        let ddl = create_index_ddl(AdvisorDialect::MySQL, "users", &["email"], None);
        assert!(ddl.contains("CREATE INDEX idx_users_email ON users(email)"));
    }

    #[test]
    fn postgres_create_index_with_hint() {
        let ddl = create_index_ddl(AdvisorDialect::PostgreSQL, "users", &["email"], None);
        assert!(ddl.contains("USING GIST/GIN"));
    }

    #[test]
    fn mssql_create_index_online() {
        let ddl = create_index_ddl(AdvisorDialect::MSSQL, "users", &["email"], None);
        assert!(ddl.contains("WITH (ONLINE = ON)"));
    }

    #[test]
    fn sqlite_create_index_basic() {
        let ddl = create_index_ddl(AdvisorDialect::SQLite, "orders", &["user_id"], None);
        assert!(ddl.contains("CREATE INDEX idx_orders_user_id ON orders(user_id)"));
    }

    #[test]
    fn oracle_create_index_basic() {
        let ddl = create_index_ddl(AdvisorDialect::Oracle, "products", &["category_id"], None);
        assert!(ddl.contains("CREATE INDEX idx_products_category_id ON products(category_id)"));
    }

    #[test]
    fn multi_column_index() {
        let ddl = create_index_ddl(
            AdvisorDialect::MySQL,
            "orders",
            &["user_id", "status"],
            None,
        );
        assert!(ddl.contains("user_id, status"));
    }

    #[test]
    fn custom_index_name() {
        let ddl = create_index_ddl(
            AdvisorDialect::MySQL,
            "users",
            &["email"],
            Some("idx_custom"),
        );
        assert!(ddl.contains("idx_custom"));
    }

    #[test]
    fn drop_index_postgres_if_exists() {
        let ddl = drop_index_ddl(AdvisorDialect::PostgreSQL, "idx_test");
        assert!(ddl.contains("IF EXISTS"));
    }
}
