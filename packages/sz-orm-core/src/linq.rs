//! LINQ 风格查询 API
//!
//! 对标 C# LINQ / EF Core `IQueryable<T>`。
//!
//! 提供流式查询构建器，方法名与 LINQ 标准操作符一致。
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::linq::LinqQuery;
//! use sz_orm_core::Value;
//!
//! let query = LinqQuery::from("users")
//!     .select(vec!["id", "name", "age"])
//!     .where_eq("age", Value::I64(25))
//!     .order_by("name")
//!     .take(10)
//!     .skip(0);
//!
//! let sql = query.build();
//! assert!(sql.contains("SELECT id, name, age FROM `users`"));
//! assert!(sql.contains("WHERE"));
//! assert!(sql.contains("ORDER BY `name` ASC"));
//! assert!(sql.contains("LIMIT 10"));
//! ```

use crate::dialect::{Dialect, MySqlDialect};
use crate::value::Value;

/// LINQ 风格查询构建器
pub struct LinqQuery {
    table: String,
    columns: Vec<String>,
    conditions: Vec<String>,
    params: Vec<Value>,
    order_by: Vec<(String, bool)>,
    limit: Option<usize>,
    offset: Option<usize>,
    distinct: bool,
    group_by: Vec<String>,
    dialect: Box<dyn Dialect>,
}

impl LinqQuery {
    /// FROM 子句 — 指定表名
    pub fn from(table: &str) -> Self {
        Self {
            table: table.to_string(),
            columns: Vec::new(),
            conditions: Vec::new(),
            params: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            distinct: false,
            group_by: Vec::new(),
            dialect: Box::new(MySqlDialect),
        }
    }

    /// SELECT 子句 — 指定列
    pub fn select(mut self, cols: Vec<&str>) -> Self {
        self.columns = cols.into_iter().map(String::from).collect();
        self
    }

    /// WHERE 等于条件
    pub fn where_eq(mut self, field: &str, value: Value) -> Self {
        self.conditions
            .push(format!("{} = ?", self.dialect.quote(field)));
        self.params.push(value);
        self
    }

    /// WHERE 不等于条件
    pub fn where_ne(mut self, field: &str, value: Value) -> Self {
        self.conditions
            .push(format!("{} != ?", self.dialect.quote(field)));
        self.params.push(value);
        self
    }

    /// WHERE 大于条件
    pub fn where_gt(mut self, field: &str, value: Value) -> Self {
        self.conditions
            .push(format!("{} > ?", self.dialect.quote(field)));
        self.params.push(value);
        self
    }

    /// WHERE 小于条件
    pub fn where_lt(mut self, field: &str, value: Value) -> Self {
        self.conditions
            .push(format!("{} < ?", self.dialect.quote(field)));
        self.params.push(value);
        self
    }

    /// WHERE 大于等于条件
    pub fn where_ge(mut self, field: &str, value: Value) -> Self {
        self.conditions
            .push(format!("{} >= ?", self.dialect.quote(field)));
        self.params.push(value);
        self
    }

    /// WHERE 小于等于条件
    pub fn where_le(mut self, field: &str, value: Value) -> Self {
        self.conditions
            .push(format!("{} <= ?", self.dialect.quote(field)));
        self.params.push(value);
        self
    }

    /// WHERE LIKE 条件
    pub fn where_like(mut self, field: &str, pattern: Value) -> Self {
        self.conditions
            .push(format!("{} LIKE ?", self.dialect.quote(field)));
        self.params.push(pattern);
        self
    }

    /// WHERE IN 条件
    pub fn where_in(mut self, field: &str, values: Vec<Value>) -> Self {
        if values.is_empty() {
            self.conditions.push("1 = 0".to_string());
            return self;
        }
        let placeholders: Vec<String> = (0..values.len()).map(|_| "?".to_string()).collect();
        self.conditions.push(format!(
            "{} IN ({})",
            self.dialect.quote(field),
            placeholders.join(", ")
        ));
        self.params.extend(values);
        self
    }

    /// WHERE IS NULL
    pub fn where_null(mut self, field: &str) -> Self {
        self.conditions
            .push(format!("{} IS NULL", self.dialect.quote(field)));
        self
    }

    /// WHERE IS NOT NULL
    pub fn where_not_null(mut self, field: &str) -> Self {
        self.conditions
            .push(format!("{} IS NOT NULL", self.dialect.quote(field)));
        self
    }

    /// ORDER BY 升序
    pub fn order_by(mut self, field: &str) -> Self {
        self.order_by.push((field.to_string(), false));
        self
    }

    /// ORDER BY 降序
    pub fn order_by_desc(mut self, field: &str) -> Self {
        self.order_by.push((field.to_string(), true));
        self
    }

    /// TAKE — 取前 N 条（等价于 LIMIT）
    pub fn take(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// SKIP — 跳过 N 条（等价于 OFFSET）
    pub fn skip(mut self, n: usize) -> Self {
        if n > 0 {
            self.offset = Some(n);
        }
        self
    }

    /// DISTINCT — 去重
    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// GROUP BY — 分组
    pub fn group_by(mut self, cols: Vec<&str>) -> Self {
        self.group_by = cols.into_iter().map(String::from).collect();
        self
    }

    /// 构建 SQL
    pub fn build(&self) -> String {
        let mut sql = String::new();

        let cols = if self.columns.is_empty() {
            "*".to_string()
        } else {
            self.columns.join(", ")
        };

        if self.distinct {
            sql.push_str(&format!(
                "SELECT DISTINCT {} FROM {}",
                cols,
                self.dialect.quote(&self.table)
            ));
        } else {
            sql.push_str(&format!(
                "SELECT {} FROM {}",
                cols,
                self.dialect.quote(&self.table)
            ));
        }

        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.conditions.join(" AND "));
        }

        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            sql.push_str(&self.group_by.join(", "));
        }

        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            let orders: Vec<String> = self
                .order_by
                .iter()
                .map(|(f, desc)| {
                    if *desc {
                        format!("{} DESC", self.dialect.quote(f))
                    } else {
                        format!("{} ASC", self.dialect.quote(f))
                    }
                })
                .collect();
            sql.push_str(&orders.join(", "));
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        sql
    }

    /// 获取参数
    pub fn params(&self) -> &[Value] {
        &self.params
    }

    /// 构建 COUNT 查询
    pub fn build_count(&self) -> String {
        let mut sql = format!("SELECT COUNT(*) FROM {}", self.dialect.quote(&self.table));

        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.conditions.join(" AND "));
        }

        sql
    }

    /// 构建 EXISTS 查询
    pub fn build_exists(&self) -> String {
        let mut sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {}",
            self.dialect.quote(&self.table)
        );

        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.conditions.join(" AND "));
        }

        sql.push(')');
        sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linq_basic_select() {
        let q = LinqQuery::from("users").select(vec!["id", "name"]);
        let sql = q.build();
        assert!(sql.contains("SELECT id, name FROM"));
        assert!(sql.contains("users"));
    }

    #[test]
    fn test_linq_select_all() {
        let q = LinqQuery::from("users");
        let sql = q.build();
        assert!(sql.contains("SELECT * FROM"));
        assert!(sql.contains("users"));
    }

    #[test]
    fn test_linq_where_eq() {
        let q = LinqQuery::from("users").where_eq("age", Value::I64(25));
        let sql = q.build();
        assert!(sql.contains("WHERE `age` = ?"));
        assert_eq!(q.params(), &[Value::I64(25)]);
    }

    #[test]
    fn test_linq_where_in() {
        let q = LinqQuery::from("users")
            .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
        let sql = q.build();
        assert!(sql.contains("IN (?, ?, ?)"));
        assert_eq!(q.params().len(), 3);
    }

    #[test]
    fn test_linq_where_in_empty() {
        let q = LinqQuery::from("users").where_in("id", vec![]);
        let sql = q.build();
        assert!(sql.contains("1 = 0"));
    }

    #[test]
    fn test_linq_order_by() {
        let q = LinqQuery::from("users").order_by("name");
        let sql = q.build();
        assert!(sql.contains("ORDER BY `name` ASC"));
    }

    #[test]
    fn test_linq_order_by_desc() {
        let q = LinqQuery::from("users").order_by_desc("age");
        let sql = q.build();
        assert!(sql.contains("ORDER BY `age` DESC"));
    }

    #[test]
    fn test_linq_take_skip() {
        let q = LinqQuery::from("users").take(10).skip(5);
        let sql = q.build();
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 5"));
    }

    #[test]
    fn test_linq_distinct() {
        let q = LinqQuery::from("users").select(vec!["city"]).distinct();
        let sql = q.build();
        assert!(sql.starts_with("SELECT DISTINCT"));
    }

    #[test]
    fn test_linq_group_by() {
        let q = LinqQuery::from("users")
            .select(vec!["city"])
            .group_by(vec!["city"]);
        let sql = q.build();
        assert!(sql.contains("GROUP BY city"));
    }

    #[test]
    fn test_linq_chained() {
        let q = LinqQuery::from("users")
            .select(vec!["id", "name", "age"])
            .where_eq("age", Value::I64(25))
            .where_ne("name", Value::String("admin".into()))
            .order_by("name")
            .take(10);

        let sql = q.build();
        assert!(sql.contains("SELECT id, name, age FROM"));
        assert!(sql.contains("users"));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("AND"));
        assert!(sql.contains("ORDER BY `name` ASC"));
        assert!(sql.contains("LIMIT 10"));
        assert_eq!(q.params().len(), 2);
    }

    #[test]
    fn test_linq_build_count() {
        let q = LinqQuery::from("users").where_eq("age", Value::I64(25));
        let sql = q.build_count();
        assert!(sql.starts_with("SELECT COUNT(*)"));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_linq_build_exists() {
        let q = LinqQuery::from("users").where_eq("email", Value::String("test@test.com".into()));
        let sql = q.build_exists();
        assert!(sql.starts_with("SELECT EXISTS("));
    }

    #[test]
    fn test_linq_where_null() {
        let q = LinqQuery::from("users").where_null("deleted_at");
        let sql = q.build();
        assert!(sql.contains("IS NULL"));
    }

    #[test]
    fn test_linq_where_not_null() {
        let q = LinqQuery::from("users").where_not_null("email");
        let sql = q.build();
        assert!(sql.contains("IS NOT NULL"));
    }

    #[test]
    fn test_linq_where_gt_lt() {
        let q = LinqQuery::from("users")
            .where_gt("age", Value::I64(18))
            .where_lt("age", Value::I64(65));
        let sql = q.build();
        assert!(sql.contains("> ?"));
        assert!(sql.contains("< ?"));
        assert_eq!(q.params().len(), 2);
    }

    #[test]
    fn test_e2e_linq_realistic_user_query() {
        let q = LinqQuery::from("users")
            .select(vec!["id", "name", "email", "age"])
            .where_eq("status", Value::String("active".into()))
            .where_gt("age", Value::I64(18))
            .where_like("email", Value::String("%@gmail.com".into()))
            .order_by("name")
            .take(20)
            .skip(0);

        let sql = q.build();
        assert!(sql.contains("SELECT id, name, email, age FROM `users`"));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("`status` = ?"));
        assert!(sql.contains("`age` > ?"));
        assert!(sql.contains("`email` LIKE ?"));
        assert!(sql.contains("AND"));
        assert!(sql.contains("ORDER BY `name` ASC"));
        assert!(sql.contains("LIMIT 20"));

        assert_eq!(
            q.params(),
            &[
                Value::String("active".into()),
                Value::I64(18),
                Value::String("%@gmail.com".into()),
            ]
        );
    }

    #[test]
    fn test_e2e_linq_pagination_with_count() {
        let make_base = || {
            LinqQuery::from("orders")
                .where_eq("user_id", Value::I64(42))
                .where_eq("status", Value::String("paid".into()))
        };

        let page1 = make_base()
            .select(vec!["id", "amount"])
            .order_by_desc("created_at")
            .take(10)
            .skip(0);
        let page2 = make_base()
            .select(vec!["id", "amount"])
            .order_by_desc("created_at")
            .take(10)
            .skip(10);
        let count = make_base().build_count();

        let sql1 = page1.build();
        let sql2 = page2.build();

        assert!(sql1.contains("LIMIT 10"));
        assert!(!sql1.contains("OFFSET"));
        assert!(sql2.contains("LIMIT 10"));
        assert!(sql2.contains("OFFSET 10"));
        assert!(count.starts_with("SELECT COUNT(*)"));
        assert!(count.contains("`user_id` = ?"));
        assert!(count.contains("`status` = ?"));
    }

    #[test]
    fn test_e2e_linq_exists_check() {
        let q = LinqQuery::from("users")
            .where_eq("email", Value::String("alice@example.com".into()))
            .where_null("deleted_at");

        let exists_sql = q.build_exists();
        assert!(exists_sql.starts_with("SELECT EXISTS(SELECT 1 FROM `users`"));
        assert!(exists_sql.contains("`email` = ?"));
        assert!(exists_sql.contains("`deleted_at` IS NULL"));
        assert!(exists_sql.ends_with(')'));
        assert_eq!(q.params().len(), 1);
    }

    #[test]
    fn test_e2e_linq_in_clause_batch_lookup() {
        let ids: Vec<Value> = (1..=5).map(Value::I64).collect();
        let q = LinqQuery::from("products")
            .select(vec!["id", "name", "price"])
            .where_in("id", ids)
            .where_eq("active", Value::Bool(true))
            .order_by("price");

        let sql = q.build();
        assert!(sql.contains("IN (?, ?, ?, ?, ?)"));
        assert!(sql.contains("`active` = ?"));
        assert_eq!(q.params().len(), 6);
    }

    #[test]
    fn test_e2e_linq_distinct_cities() {
        let q = LinqQuery::from("users")
            .select(vec!["city"])
            .distinct()
            .where_not_null("city")
            .order_by("city");

        let sql = q.build();
        assert!(sql.starts_with("SELECT DISTINCT city FROM `users`"));
        assert!(sql.contains("`city` IS NOT NULL"));
        assert!(sql.contains("ORDER BY `city` ASC"));
    }
}
