//! 快捷查询（Db::name 风格）
//!
//! 对应 think-orm 的 `Db::name('user')->where(...)->select()` API。
//! 无需定义 Model 即可直接基于表名查询/插入/更新/删除。
//!
//! # 与 QueryBuilder 的关系
//!
//! `QueryBuilder<M>` 要求泛型参数 `M: Model`，适合已知 Model 类型的场景。
//! `QuickQuery` 则用 `()` 占位 Model，仅依赖表名，避免为临时查询定义 Model。
//!
//! # 用法
//!
//! ```no_run
//! use sz_orm_core::quick_query::Db;
//! use sz_orm_core::{get_dialect, DbType, Value};
//!
//! let dialect = get_dialect(DbType::MySQL).unwrap();
//! // SELECT * FROM users WHERE age > 18 ORDER BY id DESC LIMIT 10
//! let sql = Db::new(dialect).name("users")
//!     .where_cond("age > 18")
//!     .order_desc("id")
//!     .limit(10)
//!     .build_select();
//! ```

use crate::dialect::Dialect;
use crate::query::QueryBuilder;
use crate::value::Value;
use std::collections::HashMap;

/// 内部占位 Model：仅用于满足 `QueryBuilder<M>` 的泛型约束，不携带任何行为
#[derive(Clone)]
struct AnonymousModel;

impl crate::model::Model for AnonymousModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        ""
    }
    fn pk(&self) -> Self::PrimaryKey {
        0
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

/// 快捷查询入口（think-orm `Db::name()` 风格）
///
/// 不要求定义 Model，仅靠表名 + 方言即可生成 SQL。
pub struct Db {
    qb: QueryBuilder<AnonymousModel>,
}

impl Db {
    /// 创建快捷查询入口
    pub fn new(dialect: Box<dyn Dialect>) -> Self {
        Self {
            qb: QueryBuilder::new(dialect),
        }
    }

    /// 指定表名（等价于 think-orm 的 `Db::name('user')`）
    #[must_use]
    pub fn name(mut self, table: impl Into<String>) -> Self {
        self.qb = self.qb.table(table);
        self
    }

    /// 选择列
    #[must_use]
    pub fn select(mut self, columns: Vec<&str>) -> Self {
        self.qb = self.qb.select(columns);
        self
    }

    /// WHERE 条件（AND）
    #[must_use]
    pub fn where_cond(mut self, condition: impl Into<String>) -> Self {
        self.qb = self.qb.where_cond(condition);
        self
    }

    /// WHERE 条件（OR）
    #[must_use]
    pub fn or_where(mut self, condition: impl Into<String>) -> Self {
        self.qb = self.qb.or_where(condition);
        self
    }

    /// WHERE IN
    #[must_use]
    pub fn where_in(mut self, field: impl Into<String>, values: Vec<Value>) -> Self {
        self.qb = self.qb.where_in(field, values);
        self
    }

    /// WHERE NOT IN
    #[must_use]
    pub fn where_not_in(mut self, field: impl Into<String>, values: Vec<Value>) -> Self {
        self.qb = self.qb.where_not_in(field, values);
        self
    }

    /// WHERE BETWEEN
    #[must_use]
    pub fn where_between(mut self, field: impl Into<String>, start: Value, end: Value) -> Self {
        self.qb = self.qb.where_between(field, start, end);
        self
    }

    /// WHERE IS NULL
    #[must_use]
    pub fn where_null(mut self, field: impl Into<String>) -> Self {
        self.qb = self.qb.where_null(field);
        self
    }

    /// WHERE IS NOT NULL
    #[must_use]
    pub fn where_not_null(mut self, field: impl Into<String>) -> Self {
        self.qb = self.qb.where_not_null(field);
        self
    }

    /// ORDER BY field ASC
    #[must_use]
    pub fn order_by(mut self, field: impl Into<String>) -> Self {
        self.qb = self.qb.order_by(field);
        self
    }

    /// ORDER BY field DESC
    #[must_use]
    pub fn order_desc(mut self, field: impl Into<String>) -> Self {
        self.qb = self.qb.order_desc(field);
        self
    }

    /// GROUP BY
    #[must_use]
    pub fn group_by(mut self, field: impl Into<String>) -> Self {
        self.qb = self.qb.group_by(field);
        self
    }

    /// HAVING
    #[must_use]
    pub fn having(mut self, condition: impl Into<String>) -> Self {
        self.qb = self.qb.having(condition);
        self
    }

    /// LIMIT
    #[must_use]
    pub fn limit(mut self, limit: usize) -> Self {
        self.qb = self.qb.limit(limit);
        self
    }

    /// OFFSET
    #[must_use]
    pub fn offset(mut self, offset: usize) -> Self {
        self.qb = self.qb.offset(offset);
        self
    }

    /// 分页（page 从 1 开始）
    #[must_use]
    pub fn page(mut self, page: usize, page_size: usize) -> Self {
        self.qb = self.qb.page(page, page_size);
        self
    }

    /// INNER JOIN
    #[must_use]
    pub fn join_inner(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.qb = self.qb.join_inner(table, on_left, on_right);
        self
    }

    /// LEFT JOIN
    #[must_use]
    pub fn join_left(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.qb = self.qb.join_left(table, on_left, on_right);
        self
    }

    /// RIGHT JOIN
    #[must_use]
    pub fn join_right(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.qb = self.qb.join_right(table, on_left, on_right);
        self
    }

    /// 构建 SELECT SQL
    pub fn build_select(&self) -> String {
        self.qb.build_select()
    }

    /// 构建 INSERT SQL
    pub fn build_insert(&self, data: &HashMap<String, Value>) -> String {
        self.qb.build_insert(data)
    }

    /// 构建 UPDATE SQL
    pub fn build_update(&self, data: &HashMap<String, Value>) -> String {
        self.qb.build_update(data)
    }

    /// 构建 DELETE SQL
    pub fn build_delete(&self) -> String {
        self.qb.build_delete()
    }

    /// 构建 COUNT SQL
    pub fn build_count(&self) -> String {
        self.qb.build_count()
    }

    /// 构建 EXISTS SQL
    pub fn build_exists(&self) -> String {
        self.qb.build_exists()
    }

    /// 构建 MAX SQL
    pub fn build_max(&self, field: &str) -> String {
        self.qb.build_max(field)
    }

    /// 构建 MIN SQL
    pub fn build_min(&self, field: &str) -> String {
        self.qb.build_min(field)
    }

    /// 构建 SUM SQL
    pub fn build_sum(&self, field: &str) -> String {
        self.qb.build_sum(field)
    }

    /// 构建 AVG SQL
    pub fn build_avg(&self, field: &str) -> String {
        self.qb.build_avg(field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_type::DbType;
    use crate::dialect::get_dialect;

    fn mysql() -> Box<dyn Dialect> {
        get_dialect(DbType::MySQL).expect("MySQL dialect")
    }

    fn pg() -> Box<dyn Dialect> {
        get_dialect(DbType::PostgreSQL).expect("PG dialect")
    }

    #[test]
    fn db_name_basic_select() {
        let sql = Db::new(mysql()).name("users").build_select();
        assert_eq!(sql, "SELECT * FROM `users`");
    }

    #[test]
    fn db_name_with_where_and_limit() {
        let sql = Db::new(mysql())
            .name("users")
            .where_cond("age > 18")
            .order_desc("id")
            .limit(10)
            .build_select();
        assert!(sql.contains("SELECT * FROM `users`"));
        assert!(sql.contains("WHERE age > 18"));
        assert!(sql.contains("ORDER BY `id` DESC"));
        assert!(sql.contains("LIMIT 10"));
    }

    #[test]
    fn db_name_insert() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("Alice".to_string()));
        data.insert("age".to_string(), Value::I64(30));
        let sql = Db::new(mysql()).name("users").build_insert(&data);
        assert!(sql.starts_with("INSERT INTO `users`"));
        assert!(sql.contains("`name`"));
        assert!(sql.contains("`age`"));
        assert!(sql.contains("'Alice'"));
        assert!(sql.contains("30"));
    }

    #[test]
    fn db_name_update_with_where() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("Bob".to_string()));
        let sql = Db::new(mysql())
            .name("users")
            .where_cond("id = 1")
            .build_update(&data);
        assert!(sql.starts_with("UPDATE `users` SET"));
        assert!(sql.contains("`name` = 'Bob'"));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn db_name_delete_with_where() {
        let sql = Db::new(mysql())
            .name("users")
            .where_cond("id = 1")
            .build_delete();
        assert_eq!(sql, "DELETE FROM `users` WHERE id = 1");
    }

    #[test]
    fn db_name_count() {
        let sql = Db::new(mysql())
            .name("users")
            .where_cond("age > 18")
            .build_count();
        assert!(sql.contains("SELECT COUNT(*)"));
        assert!(sql.contains("FROM `users`"));
        assert!(sql.contains("WHERE age > 18"));
    }

    #[test]
    fn db_name_with_in_clause() {
        let sql = Db::new(mysql())
            .name("users")
            .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
            .build_select();
        assert!(sql.contains("WHERE `id` IN (1, 2, 3)"));
    }

    #[test]
    fn db_name_with_between() {
        let sql = Db::new(mysql())
            .name("orders")
            .where_between("amount", Value::I64(100), Value::I64(1000))
            .build_select();
        assert!(sql.contains("`amount` BETWEEN 100 AND 1000"));
    }

    #[test]
    fn db_name_pg_dialect() {
        let sql = Db::new(pg()).name("users").build_select();
        assert_eq!(sql, "SELECT * FROM \"users\"");
    }

    #[test]
    fn db_name_join_inner() {
        let sql = Db::new(mysql())
            .name("orders")
            .join_inner("users", "orders.user_id", "users.id")
            .build_select();
        assert!(sql.contains("INNER JOIN `users` ON `orders.user_id` = `users.id`"));
    }

    #[test]
    fn db_name_pagination() {
        let sql = Db::new(mysql()).name("users").page(3, 20).build_select();
        // 第 3 页，每页 20 条 → LIMIT 20 OFFSET 40
        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40"));
    }

    #[test]
    fn db_name_aggregate_functions() {
        let db = Db::new(mysql())
            .name("orders")
            .where_cond("status = 'paid'");
        assert!(db.build_sum("amount").contains("SUM(`amount`)"));
        assert!(db.build_max("amount").contains("MAX(`amount`)"));
        assert!(db.build_min("amount").contains("MIN(`amount`)"));
        assert!(db.build_avg("amount").contains("AVG(`amount`)"));
        assert!(db.build_exists().contains("SELECT EXISTS("));
    }

    #[test]
    fn db_name_chained_or_where() {
        let sql = Db::new(mysql())
            .name("users")
            .where_cond("age < 18")
            .or_where("age > 65")
            .build_select();
        assert!(sql.contains("WHERE (age < 18 OR age > 65)"));
    }

    #[test]
    fn db_name_group_having() {
        let sql = Db::new(mysql())
            .name("orders")
            .select(vec!["user_id", "COUNT(*) as cnt"])
            .group_by("user_id")
            .having("COUNT(*) > 5")
            .build_select();
        assert!(sql.contains("GROUP BY `user_id`"));
        assert!(sql.contains("HAVING COUNT(*) > 5"));
    }
}
