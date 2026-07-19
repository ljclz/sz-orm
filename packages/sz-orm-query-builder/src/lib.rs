//! # SZ-ORM QueryBuilder — 独立 SQL 构造器（sea-query 风格）
//!
//! 一个不绑定 Model 的纯 SQL 构造器，可独立编译、独立发布到 crates.io。
//!
//! 设计灵感来自 [sea-query](https://crates.io/crates/sea-query)：
//! - 与 ORM 解耦：不依赖 `Model` trait，纯 SQL 构造
//! - 多方言支持：通过 [`DbType`] 适配 MySQL/PostgreSQL/SQLite/Oracle
//! - 链式 API：所有方法返回 `Self`
//! - 零运行时开销：构造过程零数据库连接
//!
//! # 快速入门
//!
//! ```rust
//! use sz_orm_core::DbType;
//! use sz_orm_query_builder::{Query, SelectQuery};
//!
//! // SELECT
//! let sql = Query::select()
//!     .column("id")
//!     .column("name")
//!     .from("users")
//!     .where_clause("age > 18")
//!     .order_by("id", true)
//!     .limit(10)
//!     .build(DbType::MySQL);
//! assert!(sql.contains("SELECT"));
//! assert!(sql.contains("FROM `users`"));
//!
//! // INSERT
//! let sql = Query::insert()
//!     .into_table("users")
//!     .value("name", "'Alice'")
//!     .value("age", "30")
//!     .build();
//! assert!(sql.contains("INSERT INTO `users`"));
//!
//! // UPDATE
//! let sql = Query::update()
//!     .table("users")
//!     .set("name", "'Bob'")
//!     .where_clause("id = 1")
//!     .build();
//! assert!(sql.contains("UPDATE `users`"));
//!
//! // DELETE
//! let sql = Query::delete()
//!     .from_table("users")
//!     .where_clause("id = 1")
//!     .build();
//! assert!(sql.contains("DELETE FROM"));
//! ```
//!
//! # 与 sz-orm-core::QueryBuilder 的区别
//!
//! | 特性 | `sz-orm-core::QueryBuilder<M>` | sz-orm-query-builder::Query |
//! |------|------------------------------|----------------------------|
//! | 绑定 Model | 是（`<M: Model>`） | 否 |
//! | 类型安全 | 编译期表/列校验 | 运行时字符串 |
//! | 适用场景 | ORM 完整流程 | 纯 SQL 构造、动态查询 |
//! | 依赖 | sz-orm-core 全部 | 仅 dialect 模块 |
//! | 独立发布 | 否 | 是 |

use sz_orm_core::DbType;

/// 查询构造器入口
pub struct Query;

impl Query {
    /// 创建 SELECT 查询
    pub fn select() -> SelectQuery {
        SelectQuery::new()
    }

    /// 创建 INSERT 查询
    pub fn insert() -> InsertQuery {
        InsertQuery::new()
    }

    /// 创建 UPDATE 查询
    pub fn update() -> UpdateQuery {
        UpdateQuery::new()
    }

    /// 创建 DELETE 查询
    pub fn delete() -> DeleteQuery {
        DeleteQuery::new()
    }
}

/// SELECT 查询构造器
#[derive(Debug, Clone, Default)]
pub struct SelectQuery {
    columns: Vec<String>,
    from_table: Option<String>,
    joins: Vec<String>,
    wheres: Vec<String>,
    order_by: Vec<String>,
    group_by: Vec<String>,
    having: Vec<String>,
    limit: Option<u64>,
    offset: Option<u64>,
    distinct: bool,
}

impl SelectQuery {
    /// 创建空的 SELECT 查询
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 DISTINCT
    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// 添加列
    pub fn column(mut self, name: &str) -> Self {
        self.columns.push(name.to_string());
        self
    }

    /// 添加多个列
    pub fn columns(mut self, names: &[&str]) -> Self {
        for n in names {
            self.columns.push(n.to_string());
        }
        self
    }

    /// 添加 `*` 列
    pub fn all_columns(self) -> Self {
        self.column("*")
    }

    /// 设置 FROM 表
    pub fn from(mut self, table: &str) -> Self {
        self.from_table = Some(table.to_string());
        self
    }

    /// 添加 INNER JOIN
    pub fn inner_join(mut self, table: &str, on: &str) -> Self {
        self.joins.push(format!("INNER JOIN {} ON {}", table, on));
        self
    }

    /// 添加 LEFT JOIN
    pub fn left_join(mut self, table: &str, on: &str) -> Self {
        self.joins.push(format!("LEFT JOIN {} ON {}", table, on));
        self
    }

    /// 添加 RIGHT JOIN
    pub fn right_join(mut self, table: &str, on: &str) -> Self {
        self.joins.push(format!("RIGHT JOIN {} ON {}", table, on));
        self
    }

    /// 添加 WHERE 条件（AND 连接）
    pub fn where_clause(mut self, condition: &str) -> Self {
        self.wheres.push(condition.to_string());
        self
    }

    /// 添加 OR WHERE 条件
    pub fn or_where(mut self, condition: &str) -> Self {
        self.wheres.push(format!("OR {}", condition));
        self
    }

    /// 添加 GROUP BY
    pub fn group_by(mut self, column: &str) -> Self {
        self.group_by.push(column.to_string());
        self
    }

    /// 添加 HAVING
    pub fn having(mut self, condition: &str) -> Self {
        self.having.push(condition.to_string());
        self
    }

    /// 添加 ORDER BY
    ///
    /// # 参数
    ///
    /// - `column`: 列名
    /// - `asc`: true=ASC, false=DESC
    pub fn order_by(mut self, column: &str, asc: bool) -> Self {
        let dir = if asc { "ASC" } else { "DESC" };
        self.order_by.push(format!("{} {}", column, dir));
        self
    }

    /// 设置 LIMIT
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    /// 设置 OFFSET
    pub fn offset(mut self, n: u64) -> Self {
        self.offset = Some(n);
        self
    }

    /// 生成分页（同时设置 LIMIT 和 OFFSET）
    ///
    /// # 参数
    ///
    /// - `page`: 页码（从 1 开始）
    /// - `size`: 每页大小
    pub fn paginate(self, page: u64, size: u64) -> Self {
        let offset = (page.saturating_sub(1)) * size;
        self.limit(size).offset(offset)
    }

    /// 生成 SQL
    ///
    /// # 参数
    ///
    /// - `db_type`: 数据库类型，用于选择方言
    pub fn build(self, db_type: DbType) -> String {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let mut sql = String::new();
        sql.push_str("SELECT ");

        if self.distinct {
            sql.push_str("DISTINCT ");
        }

        if self.columns.is_empty() {
            sql.push('*');
        } else {
            let cols: Vec<String> = self
                .columns
                .iter()
                .map(|c| {
                    if c == "*" {
                        c.clone()
                    } else {
                        dialect.quote(c)
                    }
                })
                .collect();
            sql.push_str(&cols.join(", "));
        }

        if let Some(table) = self.from_table {
            sql.push_str(" FROM ");
            sql.push_str(&dialect.quote(&table));
        }

        for join in &self.joins {
            sql.push(' ');
            sql.push_str(join);
        }

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            // 第一个条件不加 AND/OR 前缀
            sql.push_str(&self.wheres[0]);
            for w in &self.wheres[1..] {
                if w.starts_with("OR ") {
                    sql.push(' ');
                    sql.push_str(w);
                } else {
                    sql.push_str(" AND ");
                    sql.push_str(w);
                }
            }
        }

        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            sql.push_str(&self.group_by.join(", "));
        }

        if !self.having.is_empty() {
            sql.push_str(" HAVING ");
            sql.push_str(&self.having.join(" AND "));
        }

        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&self.order_by.join(", "));
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        sql
    }
}

/// INSERT 查询构造器
#[derive(Debug, Clone, Default)]
pub struct InsertQuery {
    table: Option<String>,
    columns: Vec<String>,
    values: Vec<String>,
}

impl InsertQuery {
    /// 创建空的 INSERT 查询
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置目标表
    pub fn into_table(mut self, table: &str) -> Self {
        self.table = Some(table.to_string());
        self
    }

    /// 添加列值对（值应为已转义的 SQL 字面量）
    pub fn value(mut self, column: &str, value: &str) -> Self {
        self.columns.push(column.to_string());
        self.values.push(value.to_string());
        self
    }

    /// 批量添加列值对
    pub fn values(mut self, pairs: &[(&str, &str)]) -> Self {
        for (c, v) in pairs {
            self.columns.push(c.to_string());
            self.values.push(v.to_string());
        }
        self
    }

    /// 生成 SQL（使用 MySQL/PG/SQLite 通用语法）
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.columns.is_empty() {
            return String::new();
        }

        let cols: Vec<String> = self.columns.iter().map(|c| format!("`{}`", c)).collect();
        let vals: Vec<String> = self.values.iter().map(|v| v.to_string()).collect();

        format!(
            "INSERT INTO `{}` ({}) VALUES ({})",
            table,
            cols.join(", "),
            vals.join(", ")
        )
    }

    /// 按指定方言生成 SQL
    pub fn build_with_dialect(self, db_type: DbType) -> String {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.columns.is_empty() {
            return String::new();
        }

        let cols: Vec<String> = self.columns.iter().map(|c| dialect.quote(c)).collect();

        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            dialect.quote(&table),
            cols.join(", "),
            self.values.join(", ")
        )
    }
}

/// UPDATE 查询构造器
#[derive(Debug, Clone, Default)]
pub struct UpdateQuery {
    table: Option<String>,
    sets: Vec<(String, String)>,
    wheres: Vec<String>,
}

impl UpdateQuery {
    /// 创建空的 UPDATE 查询
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置目标表
    pub fn table(mut self, table: &str) -> Self {
        self.table = Some(table.to_string());
        self
    }

    /// 添加 SET 赋值（值应为已转义的 SQL 字面量）
    pub fn set(mut self, column: &str, value: &str) -> Self {
        self.sets.push((column.to_string(), value.to_string()));
        self
    }

    /// 批量添加 SET 赋值
    pub fn sets(mut self, pairs: &[(&str, &str)]) -> Self {
        for (c, v) in pairs {
            self.sets.push((c.to_string(), v.to_string()));
        }
        self
    }

    /// 添加 WHERE 条件
    pub fn where_clause(mut self, condition: &str) -> Self {
        self.wheres.push(condition.to_string());
        self
    }

    /// 生成 SQL
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.sets.is_empty() {
            return String::new();
        }

        let set_str: Vec<String> = self
            .sets
            .iter()
            .map(|(c, v)| format!("`{}` = {}", c, v))
            .collect();

        let mut sql = format!("UPDATE `{}` SET {}", table, set_str.join(", "));

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.wheres.join(" AND "));
        }

        sql
    }

    /// 按指定方言生成 SQL
    pub fn build_with_dialect(self, db_type: DbType) -> String {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.sets.is_empty() {
            return String::new();
        }

        let set_str: Vec<String> = self
            .sets
            .iter()
            .map(|(c, v)| format!("{} = {}", dialect.quote(c), v))
            .collect();

        let mut sql = format!(
            "UPDATE {} SET {}",
            dialect.quote(&table),
            set_str.join(", ")
        );

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.wheres.join(" AND "));
        }

        sql
    }
}

/// DELETE 查询构造器
#[derive(Debug, Clone, Default)]
pub struct DeleteQuery {
    table: Option<String>,
    wheres: Vec<String>,
}

impl DeleteQuery {
    /// 创建空的 DELETE 查询
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置目标表
    pub fn from_table(mut self, table: &str) -> Self {
        self.table = Some(table.to_string());
        self
    }

    /// 添加 WHERE 条件
    pub fn where_clause(mut self, condition: &str) -> Self {
        self.wheres.push(condition.to_string());
        self
    }

    /// 生成 SQL
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() {
            return String::new();
        }

        let mut sql = format!("DELETE FROM `{}`", table);

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.wheres.join(" AND "));
        }

        sql
    }

    /// 按指定方言生成 SQL
    pub fn build_with_dialect(self, db_type: DbType) -> String {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let table = self.table.unwrap_or_default();
        if table.is_empty() {
            return String::new();
        }

        let mut sql = format!("DELETE FROM {}", dialect.quote(&table));

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.wheres.join(" AND "));
        }

        sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Query::select 测试 ----

    #[test]
    fn test_select_basic() {
        let sql = Query::select()
            .column("id")
            .column("name")
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.starts_with("SELECT "));
        assert!(sql.contains("`id`"));
        assert!(sql.contains("`name`"));
        assert!(sql.contains("FROM `users`"));
    }

    #[test]
    fn test_select_star() {
        let sql = Query::select()
            .all_columns()
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.contains("SELECT *"));
        assert!(sql.contains("FROM `users`"));
    }

    #[test]
    fn test_select_distinct() {
        let sql = Query::select()
            .distinct()
            .column("name")
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.contains("SELECT DISTINCT"));
    }

    #[test]
    fn test_select_with_where() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .where_clause("age > 18")
            .where_clause("status = 'active'")
            .build(DbType::MySQL);
        assert!(sql.contains("WHERE age > 18 AND status = 'active'"));
    }

    #[test]
    fn test_select_with_or_where() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .where_clause("age > 18")
            .or_where("role = 'admin'")
            .build(DbType::MySQL);
        assert!(sql.contains("WHERE age > 18 OR role = 'admin'"));
    }

    #[test]
    fn test_select_with_inner_join() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join("orders o", "u.id = o.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("INNER JOIN orders o ON u.id = o.user_id"));
    }

    #[test]
    fn test_select_with_left_join() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .left_join("profiles p", "u.id = p.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("LEFT JOIN profiles p ON u.id = p.user_id"));
    }

    #[test]
    fn test_select_with_order_by() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .order_by("created_at", true)
            .order_by("id", false)
            .build(DbType::MySQL);
        assert!(sql.contains("ORDER BY created_at ASC, id DESC"));
    }

    #[test]
    fn test_select_with_limit_offset() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .limit(10)
            .offset(20)
            .build(DbType::MySQL);
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn test_select_paginate() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .paginate(3, 20)
            .build(DbType::MySQL);
        // page 3, size 20 -> offset = (3-1)*20 = 40
        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40"));
    }

    #[test]
    fn test_select_with_group_by_having() {
        let sql = Query::select()
            .column("status")
            .from("users")
            .group_by("status")
            .having("COUNT(*) > 5")
            .build(DbType::MySQL);
        assert!(sql.contains("GROUP BY status"));
        assert!(sql.contains("HAVING COUNT(*) > 5"));
    }

    #[test]
    fn test_select_postgres_dialect() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .build(DbType::PostgreSQL);
        assert!(sql.contains("\"id\""));
        assert!(sql.contains("FROM \"users\""));
    }

    #[test]
    fn test_select_sqlite_dialect() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .build(DbType::Sqlite);
        assert!(sql.contains("\"id\""));
    }

    #[test]
    fn test_select_multiple_joins() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join("orders o", "u.id = o.user_id")
            .left_join("profiles p", "u.id = p.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("INNER JOIN orders o"));
        assert!(sql.contains("LEFT JOIN profiles p"));
    }

    #[test]
    fn test_select_columns_multiple() {
        let sql = Query::select()
            .columns(&["id", "name", "email"])
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.contains("`id`, `name`, `email`"));
    }

    #[test]
    fn test_select_no_columns_defaults_star() {
        let sql = Query::select().from("users").build(DbType::MySQL);
        assert!(sql.contains("SELECT *"));
    }

    // ---- Query::insert 测试 ----

    #[test]
    fn test_insert_basic() {
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .value("age", "30")
            .build();
        assert!(sql.starts_with("INSERT INTO `users`"));
        assert!(sql.contains("`name`, `age`"));
        assert!(sql.contains("'Alice', 30"));
    }

    #[test]
    fn test_insert_values_batch() {
        let sql = Query::insert()
            .into_table("users")
            .values(&[("name", "'Bob'"), ("age", "25"), ("email", "'bob@x.com'")])
            .build();
        assert!(sql.contains("`name`, `age`, `email`"));
        assert!(sql.contains("'Bob', 25, 'bob@x.com'"));
    }

    #[test]
    fn test_insert_empty_returns_empty() {
        let sql = Query::insert().into_table("users").build();
        assert_eq!(sql, "");
    }

    #[test]
    fn test_insert_with_dialect() {
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("\"name\""));
        assert!(sql.contains("\"users\""));
    }

    // ---- Query::update 测试 ----

    #[test]
    fn test_update_basic() {
        let sql = Query::update()
            .table("users")
            .set("name", "'Bob'")
            .where_clause("id = 1")
            .build();
        assert!(sql.starts_with("UPDATE `users` SET"));
        assert!(sql.contains("`name` = 'Bob'"));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn test_update_multiple_sets() {
        let sql = Query::update()
            .table("users")
            .sets(&[("name", "'Bob'"), ("age", "30")])
            .where_clause("id = 1")
            .build();
        assert!(sql.contains("`name` = 'Bob', `age` = 30"));
    }

    #[test]
    fn test_update_no_where() {
        let sql = Query::update()
            .table("users")
            .set("status", "'active'")
            .build();
        assert!(sql.contains("UPDATE `users` SET `status` = 'active'"));
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn test_update_empty_returns_empty() {
        let sql = Query::update().table("users").build();
        assert_eq!(sql, "");
    }

    #[test]
    fn test_update_with_dialect() {
        let sql = Query::update()
            .table("users")
            .set("name", "'Bob'")
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("\"users\""));
        assert!(sql.contains("\"name\""));
    }

    // ---- Query::delete 测试 ----

    #[test]
    fn test_delete_basic() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .build();
        assert!(sql.starts_with("DELETE FROM `users`"));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn test_delete_no_where() {
        let sql = Query::delete().from_table("users").build();
        assert!(sql.contains("DELETE FROM `users`"));
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn test_delete_multiple_wheres() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id > 100")
            .where_clause("status = 'inactive'")
            .build();
        assert!(sql.contains("WHERE id > 100 AND status = 'inactive'"));
    }

    #[test]
    fn test_delete_empty_returns_empty() {
        let sql = Query::delete().build();
        assert_eq!(sql, "");
    }

    #[test]
    fn test_delete_with_dialect() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("\"users\""));
    }

    // ---- 完整流程测试 ----

    #[test]
    fn test_full_crud_flow() {
        // CREATE (用 INSERT 模拟)
        let insert = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .value("age", "30")
            .build();
        assert!(insert.contains("INSERT INTO"));

        // READ
        let select = Query::select()
            .column("id")
            .column("name")
            .from("users")
            .where_clause("age > 18")
            .order_by("id", true)
            .limit(10)
            .build(DbType::MySQL);
        assert!(select.contains("SELECT"));
        assert!(select.contains("FROM"));
        assert!(select.contains("WHERE"));
        assert!(select.contains("ORDER BY"));
        assert!(select.contains("LIMIT"));

        // UPDATE
        let update = Query::update()
            .table("users")
            .set("name", "'Bob'")
            .where_clause("id = 1")
            .build();
        assert!(update.contains("UPDATE"));
        assert!(update.contains("SET"));
        assert!(update.contains("WHERE"));

        // DELETE
        let delete = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .build();
        assert!(delete.contains("DELETE FROM"));
    }

    #[test]
    fn test_complex_select_query() {
        let sql = Query::select()
            .distinct()
            .columns(&["u.id", "u.name", "o.total"])
            .from("users u")
            .inner_join("orders o", "u.id = o.user_id")
            .where_clause("u.status = 'active'")
            .where_clause("o.total > 100")
            .group_by("u.id")
            .having("SUM(o.total) > 1000")
            .order_by("u.id", true)
            .limit(20)
            .offset(40)
            .build(DbType::MySQL);

        assert!(sql.contains("SELECT DISTINCT"));
        assert!(sql.contains("INNER JOIN orders o"));
        assert!(sql.contains("WHERE u.status = 'active' AND o.total > 100"));
        assert!(sql.contains("GROUP BY"));
        assert!(sql.contains("HAVING SUM(o.total) > 1000"));
        assert!(sql.contains("ORDER BY u.id ASC"));
        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40"));
    }
}
