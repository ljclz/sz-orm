//! 方言感知 NL2SQL 渲染器（v6.8.0 AI-NL2SQL-01 + AI-NL2SQL-02）
//!
//! 包装既有 `Nl2SqlEngine`，按 `SqlDialect` 分支渲染 LIMIT/分页/类型转换。
//! 方言分页语法：
//! - MySQL/SQLite: `LIMIT N` / `LIMIT N OFFSET M`
//! - PostgreSQL: `FETCH FIRST N ROWS ONLY` / `OFFSET M ROWS FETCH NEXT N ROWS ONLY`
//! - Oracle: `ROWNUM <= N`（无 OFFSET）/ `OFFSET M ROWS FETCH NEXT N ROWS ONLY`
//! - SQL Server: `TOP N`（无 OFFSET）/ `OFFSET M ROWS FETCH NEXT N ROWS ONLY`

use std::sync::Arc;

use sz_orm_ai::nl2sql::{Nl2SqlEngine, Nl2SqlError, SchemaContext, SqlDialect, SqlQuery};

/// 方言感知 NL2SQL 渲染器
pub struct DialectAwareNl2SqlRenderer<E: Nl2SqlEngine> {
    engine: Arc<E>,
}

impl<E: Nl2SqlEngine> DialectAwareNl2SqlRenderer<E> {
    /// 创建渲染器
    pub fn new(engine: E) -> Self {
        Self {
            engine: Arc::new(engine),
        }
    }

    /// 从 Arc 创建渲染器
    pub fn with_arc(engine: Arc<E>) -> Self {
        Self { engine }
    }

    /// 渲染：调用引擎生成 SQL，按方言适配
    pub async fn render(
        &self,
        nl_query: &str,
        schema: &SchemaContext,
        dialect: SqlDialect,
    ) -> Result<SqlQuery, Nl2SqlError> {
        let mut query = self
            .engine
            .generate_with_dialect(nl_query, schema, dialect)
            .await?;
        query.sql = adapt_sql_to_dialect(&query.sql, dialect);
        query.dialect = Some(dialect);
        Ok(query)
    }

    /// 返回引擎引用
    pub fn engine(&self) -> &E {
        &self.engine
    }
}

/// 提取分页信息（limit, offset）
fn extract_pagination(sql: &str) -> (Option<usize>, Option<usize>) {
    let upper = sql.to_uppercase();

    if let Some(pos) = upper.find(" LIMIT ") {
        let after = &sql[pos + 7..];
        let after_upper = &upper[pos + 7..];
        if let Some(off_pos) = after_upper.find(" OFFSET ") {
            let limit_str = after[..off_pos].trim();
            let offset_str = after[off_pos + 8..].trim();
            let limit = limit_str.parse::<usize>().ok();
            let offset = offset_str.parse::<usize>().ok();
            return (limit, offset);
        } else {
            let limit_str = after.trim();
            let limit = limit_str.parse::<usize>().ok();
            return (limit, None);
        }
    }

    if let Some(pos) = upper.find(" OFFSET ") {
        let after = &sql[pos + 8..];
        let after_upper = &upper[pos + 8..];
        if let Some(fetch_pos) = after_upper.find(" ROWS FETCH ") {
            let offset_str = after[..fetch_pos].trim();
            let offset = offset_str.parse::<usize>().ok();
            let fetch_after = &after[fetch_pos + 12..];
            let fetch_after_upper = &after_upper[fetch_pos + 12..];

            let n_part = fetch_after[..keyword_after_n(fetch_after_upper)].trim();
            let limit = n_part.parse::<usize>().ok();
            return (limit, offset);
        }
    }

    if let Some(pos) = upper.find(" FETCH FIRST ") {
        let after = &sql[pos + 13..];
        let after_upper = &upper[pos + 13..];
        if let Some(rows_pos) = after_upper.find(" ROWS ONLY") {
            let n_str = after[..rows_pos].trim();
            let limit = n_str.parse::<usize>().ok();
            return (limit, None);
        }
    }

    if let Some(pos) = upper.find(" FETCH NEXT ") {
        let after = &sql[pos + 12..];
        let after_upper = &upper[pos + 12..];
        if let Some(rows_pos) = after_upper.find(" ROWS ONLY") {
            let n_str = after[..rows_pos].trim();
            let limit = n_str.parse::<usize>().ok();
            return (limit, None);
        }
    }

    (None, None)
}

fn keyword_after_n(after_upper: &str) -> usize {
    if let Some(p) = after_upper.find(" ROWS ONLY") {
        p
    } else {
        after_upper.len()
    }
}

/// 移除 SQL 中的分页子句，返回基础 SQL
fn remove_pagination_clause(sql: &str) -> String {
    let upper = sql.to_uppercase();

    if let Some(pos) = upper.find(" LIMIT ") {
        let after_upper = &upper[pos + 7..];
        if let Some(off_pos) = after_upper.find(" OFFSET ") {
            let end = pos + 7 + off_pos + 8;
            let after_full = &sql[end..];
            let trimmed = after_full.trim();
            if trimmed.is_empty() {
                return sql[..pos].trim_end().to_string();
            }
            return sql[..pos].trim_end().to_string();
        } else {
            return sql[..pos].trim_end().to_string();
        }
    }

    if let Some(pos) = upper.find(" OFFSET ") {
        let after_upper = &upper[pos + 8..];
        if let Some(fetch_pos) = after_upper.find(" ROWS FETCH ") {
            let after_fetch = &after_upper[fetch_pos + 12..];
            if let Some(only_pos) = after_fetch.find(" ROWS ONLY") {
                let end = pos + 8 + fetch_pos + 12 + only_pos + 9;
                let after_full = &sql[end..];
                let trimmed = after_full.trim();
                if trimmed.is_empty() {
                    return sql[..pos].trim_end().to_string();
                }
                return sql[..pos].trim_end().to_string();
            }
        }
    }

    if let Some(pos) = upper.find(" FETCH FIRST ") {
        let after_upper = &upper[pos + 13..];
        if let Some(rows_pos) = after_upper.find(" ROWS ONLY") {
            let end = pos + 13 + rows_pos + 9;
            let after_full = &sql[end..];
            let trimmed = after_full.trim();
            if trimmed.is_empty() {
                return sql[..pos].trim_end().to_string();
            }
            return sql[..pos].trim_end().to_string();
        }
    }

    if let Some(pos) = upper.find(" FETCH NEXT ") {
        let after_upper = &upper[pos + 12..];
        if let Some(rows_pos) = after_upper.find(" ROWS ONLY") {
            let end = pos + 12 + rows_pos + 9;
            let after_full = &sql[end..];
            let trimmed = after_full.trim();
            if trimmed.is_empty() {
                return sql[..pos].trim_end().to_string();
            }
            return sql[..pos].trim_end().to_string();
        }
    }

    sql.to_string()
}

/// 按方言适配 SQL 分页语法
fn adapt_sql_to_dialect(sql: &str, dialect: SqlDialect) -> String {
    let (limit, offset) = extract_pagination(sql);
    if limit.is_none() && offset.is_none() {
        return sql.to_string();
    }

    let base = remove_pagination_clause(sql);
    let n = limit.unwrap_or(0);
    let m = offset.unwrap_or(0);

    match dialect {
        SqlDialect::MySQL | SqlDialect::Sqlite => {
            if m > 0 {
                format!("{} LIMIT {} OFFSET {}", base, n, m)
            } else {
                format!("{} LIMIT {}", base, n)
            }
        }
        SqlDialect::PostgreSQL => {
            if m > 0 {
                format!("{} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY", base, m, n)
            } else {
                format!("{} FETCH FIRST {} ROWS ONLY", base, n)
            }
        }
        SqlDialect::Oracle => {
            if m > 0 {
                format!("{} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY", base, m, n)
            } else {
                inject_rownum_predicate(&base, n)
            }
        }
        SqlDialect::SqlServer => {
            if m > 0 {
                format!("{} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY", base, m, n)
            } else {
                inject_top_n(&base, n)
            }
        }
    }
}

/// 向 SQL 注入 Oracle ROWNUM <= N 谓词
fn inject_rownum_predicate(base: &str, n: usize) -> String {
    let upper = base.to_uppercase();
    if let Some(where_pos) = upper.find(" WHERE ") {
        let before = &base[..where_pos + 7];
        let after = &base[where_pos + 7..];
        format!("{}ROWNUM <= {} AND {}", before, n, after)
    } else if let Some(order_pos) = upper.find(" ORDER BY") {
        let before = &base[..order_pos];
        let after = &base[order_pos..];
        format!("{} WHERE ROWNUM <= {}{}", before, n, after)
    } else {
        format!("{} WHERE ROWNUM <= {}", base, n)
    }
}

/// 向 SQL 注入 MSSQL TOP N
fn inject_top_n(base: &str, n: usize) -> String {
    let upper = base.to_uppercase();
    if let Some(select_pos) = upper.find("SELECT ") {
        let after_select = &base[select_pos + 7..];
        let after_select_upper = &upper[select_pos + 7..];
        if after_select_upper.starts_with("DISTINCT ") {
            format!(
                "{}SELECT DISTINCT TOP {} {}",
                &base[..select_pos],
                n,
                &after_select[9..]
            )
        } else {
            format!("{}SELECT TOP {} {}", &base[..select_pos], n, after_select)
        }
    } else {
        format!("{} /* TOP {} */", base, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_ai::nl2sql::{ColumnInfo, SimpleNl2SqlEngine, TableInfo};

    fn make_schema() -> SchemaContext {
        SchemaContext {
            tables: vec![TableInfo {
                name: "users".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                        is_primary_key: true,
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        data_type: "TEXT".to_string(),
                        nullable: true,
                        is_primary_key: false,
                    },
                ],
            }],
        }
    }

    #[tokio::test]
    async fn render_postgresql_dialect() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(result.dialect, Some(SqlDialect::PostgreSQL));
    }

    #[tokio::test]
    async fn render_postgresql_uses_fetch_first() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show top 10 users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert!(
            result.sql.contains("FETCH FIRST"),
            "PostgreSQL should use FETCH FIRST, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("LIMIT"),
            "PostgreSQL should not use LIMIT, got: {}",
            result.sql
        );
    }

    #[tokio::test]
    async fn render_mysql_dialect() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show all users", &schema, SqlDialect::MySQL)
            .await
            .unwrap();
        assert_eq!(result.dialect, Some(SqlDialect::MySQL));
    }

    #[tokio::test]
    async fn render_mysql_uses_limit() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show top 10 users", &schema, SqlDialect::MySQL)
            .await
            .unwrap();
        assert!(
            result.sql.contains("LIMIT"),
            "MySQL should use LIMIT, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("FETCH"),
            "MySQL should not use FETCH, got: {}",
            result.sql
        );
    }

    #[tokio::test]
    async fn render_sqlite_dialect() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show all users", &schema, SqlDialect::Sqlite)
            .await
            .unwrap();
        assert_eq!(result.dialect, Some(SqlDialect::Sqlite));
    }

    #[tokio::test]
    async fn render_sqlite_uses_limit() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show top 10 users", &schema, SqlDialect::Sqlite)
            .await
            .unwrap();
        assert!(
            result.sql.contains("LIMIT"),
            "SQLite should use LIMIT, got: {}",
            result.sql
        );
    }

    #[tokio::test]
    async fn render_oracle_dialect() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show all users", &schema, SqlDialect::Oracle)
            .await
            .unwrap();
        assert_eq!(result.dialect, Some(SqlDialect::Oracle));
    }

    #[tokio::test]
    async fn render_oracle_uses_rownum() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show top 10 users", &schema, SqlDialect::Oracle)
            .await
            .unwrap();
        assert!(
            result.sql.contains("ROWNUM"),
            "Oracle should use ROWNUM, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("LIMIT"),
            "Oracle should not use LIMIT, got: {}",
            result.sql
        );
    }

    #[tokio::test]
    async fn render_sqlserver_dialect() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show all users", &schema, SqlDialect::SqlServer)
            .await
            .unwrap();
        assert_eq!(result.dialect, Some(SqlDialect::SqlServer));
    }

    #[tokio::test]
    async fn render_sqlserver_uses_top() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show top 10 users", &schema, SqlDialect::SqlServer)
            .await
            .unwrap();
        assert!(
            result.sql.contains("TOP"),
            "SQL Server should use TOP, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("LIMIT"),
            "SQL Server should not use LIMIT, got: {}",
            result.sql
        );
    }

    #[tokio::test]
    async fn render_preserves_cache_hit_false() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let schema = make_schema();
        let result = renderer
            .render("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert!(!result.cache_hit);
    }
}
