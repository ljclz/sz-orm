//! 查询构造器
//!
//! 提供类似 ThinkORM 的链式查询构造 API

use crate::dialect::Dialect;
use crate::model::Model;
use crate::value::Value;
use std::fmt;

/// 用于构造 SQL 查询的查询构造器
pub struct QueryBuilder<M: Model> {
    table: Option<String>,
    select_columns: Vec<String>,
    where_conditions: Vec<WhereCondition>,
    order_by: Vec<OrderClause>,
    group_by: Vec<String>,
    having_conditions: Vec<WhereCondition>,
    limit_value: Option<usize>,
    offset_value: Option<usize>,
    joins: Vec<JoinClause>,
    dialect: Box<dyn Dialect>,
    #[allow(dead_code)]
    model: std::marker::PhantomData<M>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum WhereCondition {
    And(String),
    Or(String),
    In(String, Vec<Value>),
    NotIn(String, Vec<Value>),
    Between(String, Value, Value),
    NotBetween(String, Value, Value),
    Null(String),
    NotNull(String),
    Exists(String),
    NotExists(String),
}

#[derive(Debug, Clone)]
struct OrderClause {
    field: String,
    direction: OrderDirection,
}

#[derive(Debug, Clone)]
enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum JoinClause {
    Inner(String, String, String),
    Left(String, String, String),
    Right(String, String, String),
    Cross(String, String),
}

impl<M: Model> QueryBuilder<M> {
    pub fn new(dialect: Box<dyn Dialect>) -> Self {
        Self {
            table: None,
            select_columns: vec!["*".to_string()],
            where_conditions: Vec::new(),
            order_by: Vec::new(),
            group_by: Vec::new(),
            having_conditions: Vec::new(),
            limit_value: None,
            offset_value: None,
            joins: Vec::new(),
            dialect,
            model: std::marker::PhantomData,
        }
    }

    pub fn table(mut self, table: impl Into<String>) -> Self {
        self.table = Some(table.into());
        self
    }

    pub fn select(mut self, columns: Vec<&str>) -> Self {
        self.select_columns = columns.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn where_cond(mut self, condition: impl Into<String>) -> Self {
        self.where_conditions
            .push(WhereCondition::And(condition.into()));
        self
    }

    pub fn or_where(mut self, condition: impl Into<String>) -> Self {
        self.where_conditions
            .push(WhereCondition::Or(condition.into()));
        self
    }

    pub fn where_in(mut self, field: impl Into<String>, values: Vec<Value>) -> Self {
        self.where_conditions
            .push(WhereCondition::In(field.into(), values));
        self
    }

    pub fn where_not_in(mut self, field: impl Into<String>, values: Vec<Value>) -> Self {
        self.where_conditions
            .push(WhereCondition::NotIn(field.into(), values));
        self
    }

    pub fn where_between(mut self, field: impl Into<String>, start: Value, end: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Between(field.into(), start, end));
        self
    }

    pub fn where_not_between(mut self, field: impl Into<String>, start: Value, end: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::NotBetween(field.into(), start, end));
        self
    }

    pub fn where_null(mut self, field: impl Into<String>) -> Self {
        self.where_conditions
            .push(WhereCondition::Null(field.into()));
        self
    }

    pub fn where_not_null(mut self, field: impl Into<String>) -> Self {
        self.where_conditions
            .push(WhereCondition::NotNull(field.into()));
        self
    }

    pub fn order_by(mut self, field: impl Into<String>) -> Self {
        self.order_by.push(OrderClause {
            field: field.into(),
            direction: OrderDirection::Asc,
        });
        self
    }

    pub fn order_desc(mut self, field: impl Into<String>) -> Self {
        self.order_by.push(OrderClause {
            field: field.into(),
            direction: OrderDirection::Desc,
        });
        self
    }

    pub fn group_by(mut self, field: impl Into<String>) -> Self {
        self.group_by.push(field.into());
        self
    }

    pub fn having(mut self, condition: impl Into<String>) -> Self {
        self.having_conditions
            .push(WhereCondition::And(condition.into()));
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit_value = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset_value = Some(offset);
        self
    }

    pub fn page(mut self, page: usize, page_size: usize) -> Self {
        self.limit_value = Some(page_size);
        self.offset_value = Some((page.saturating_sub(1)) * page_size);
        self
    }

    pub fn join_inner(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.joins.push(JoinClause::Inner(
            table.into(),
            on_left.into(),
            on_right.into(),
        ));
        self
    }

    pub fn join_left(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.joins.push(JoinClause::Left(
            table.into(),
            on_left.into(),
            on_right.into(),
        ));
        self
    }

    pub fn join_right(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.joins.push(JoinClause::Right(
            table.into(),
            on_left.into(),
            on_right.into(),
        ));
        self
    }

    pub fn build_select(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let columns = if self.select_columns.is_empty() {
            "*".to_string()
        } else {
            self.select_columns.join(", ")
        };

        let mut sql = format!("SELECT {} FROM {}", columns, self.dialect.quote(&table));

        for join in &self.joins {
            match join {
                JoinClause::Inner(t, l, r) => {
                    sql.push_str(&format!(
                        " INNER JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Left(t, l, r) => {
                    sql.push_str(&format!(
                        " LEFT JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Right(t, l, r) => {
                    sql.push_str(&format!(
                        " RIGHT JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Cross(t, on) => {
                    sql.push_str(&format!(
                        " CROSS JOIN {} ON {}",
                        self.dialect.quote(t),
                        self.dialect.quote(on)
                    ));
                }
            }
        }

        if !self.where_conditions.is_empty() {
            sql.push_str(&self.build_where_clause());
        }

        if !self.group_by.is_empty() {
            let cols: Vec<String> = self
                .group_by
                .iter()
                .map(|c| self.dialect.quote(c))
                .collect();
            sql.push_str(" GROUP BY ");
            sql.push_str(&cols.join(", "));
        }

        if !self.having_conditions.is_empty() {
            sql.push_str(" HAVING ");
            for (i, cond) in self.having_conditions.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" AND ");
                }
                if let WhereCondition::And(c) = cond {
                    sql.push_str(c);
                }
            }
        }

        if !self.order_by.is_empty() {
            let order_cols: Vec<String> = self
                .order_by
                .iter()
                .map(|o| {
                    let dir = match o.direction {
                        OrderDirection::Asc => " ASC",
                        OrderDirection::Desc => " DESC",
                    };
                    format!("{}{}", self.dialect.quote(&o.field), dir)
                })
                .collect();
            sql.push_str(" ORDER BY ");
            sql.push_str(&order_cols.join(", "));
        }

        if let Some(limit) = self.limit_value {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset_value {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        sql
    }

    /// 构建 WHERE 子句（处理所有条件类型：And/Or/In/NotIn/Between/Null 等）
    /// 返回空字符串表示无 WHERE 子句
    fn build_where_clause(&self) -> String {
        if self.where_conditions.is_empty() {
            return String::new();
        }

        // 将每个条件转换为字符串，OR 条件标记前缀
        let conditions: Vec<String> = self
            .where_conditions
            .iter()
            .map(|cond| match cond {
                WhereCondition::And(c) => c.clone(),
                WhereCondition::Or(c) => format!("OR {}", c),
                WhereCondition::In(f, vals) => {
                    // v0.2.2 修复 H-1：使用方言感知的转义
                    let vals_str: Vec<String> = vals
                        .iter()
                        .map(|v| v.to_param_with_dialect(&*self.dialect).to_string())
                        .collect();
                    format!("{} IN ({})", self.dialect.quote(f), vals_str.join(", "))
                }
                WhereCondition::NotIn(f, vals) => {
                    let vals_str: Vec<String> = vals
                        .iter()
                        .map(|v| v.to_param_with_dialect(&*self.dialect).to_string())
                        .collect();
                    format!("{} NOT IN ({})", self.dialect.quote(f), vals_str.join(", "))
                }
                WhereCondition::Between(f, start, end) => {
                    format!(
                        "{} BETWEEN {} AND {}",
                        self.dialect.quote(f),
                        start.to_param_with_dialect(&*self.dialect),
                        end.to_param_with_dialect(&*self.dialect)
                    )
                }
                WhereCondition::NotBetween(f, start, end) => {
                    format!(
                        "{} NOT BETWEEN {} AND {}",
                        self.dialect.quote(f),
                        start.to_param_with_dialect(&*self.dialect),
                        end.to_param_with_dialect(&*self.dialect)
                    )
                }
                WhereCondition::Null(f) => format!("{} IS NULL", self.dialect.quote(f)),
                WhereCondition::NotNull(f) => format!("{} IS NOT NULL", self.dialect.quote(f)),
                WhereCondition::Exists(s) => format!("EXISTS ({})", s),
                WhereCondition::NotExists(s) => format!("NOT EXISTS ({})", s),
            })
            .collect();

        if conditions.is_empty() {
            return String::new();
        }

        // OR 分组逻辑：将相邻的 OR 条件组合成 (cond1 OR cond2) 形式
        // 边界处理：如果第一个条件就是 OR（不合理但需防御），当作 AND 处理
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut current_group: Vec<String> = Vec::new();
        for cond in conditions.iter() {
            if let Some(stripped) = cond.strip_prefix("OR ") {
                // OR 条件：无论是否首个，都把 OR 前缀去掉当作普通条件加入当前组
                current_group.push(stripped.to_string());
            } else {
                // AND 条件：如果当前组非空，先保存
                if !current_group.is_empty() {
                    groups.push(std::mem::take(&mut current_group));
                }
                current_group.push(cond.clone());
            }
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }

        let group_strs: Vec<String> = groups
            .iter()
            .map(|g| {
                if g.len() == 1 {
                    g[0].clone()
                } else {
                    format!("({})", g.join(" OR "))
                }
            })
            .collect();

        format!(" WHERE {}", group_strs.join(" AND "))
    }

    pub fn build_insert(&self, data: &std::collections::HashMap<String, Value>) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        if data.is_empty() {
            return String::new();
        }

        let columns: Vec<String> = data.keys().map(|k| self.dialect.quote(k)).collect();
        // v0.2.2 修复 H-1：使用方言感知的转义
        let values: Vec<String> = data
            .values()
            .map(|v| v.to_param_with_dialect(&*self.dialect).to_string())
            .collect();

        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.dialect.quote(&table),
            columns.join(", "),
            values.join(", ")
        )
    }

    pub fn build_update(&self, data: &std::collections::HashMap<String, Value>) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        if data.is_empty() {
            return String::new();
        }

        let set_clauses: Vec<String> = data
            .iter()
            .map(|(k, v)| {
                format!(
                    "{} = {}",
                    self.dialect.quote(k),
                    v.to_param_with_dialect(&*self.dialect)
                )
            })
            .collect();

        let mut sql = format!(
            "UPDATE {} SET {}",
            self.dialect.quote(&table),
            set_clauses.join(", ")
        );

        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_delete(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!("DELETE FROM {}", self.dialect.quote(&table));
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_count(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT COUNT(*) as total FROM {}",
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_exists(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!("SELECT 1 FROM {}", self.dialect.quote(&table));
        sql.push_str(&self.build_where_clause());
        sql.push_str(" LIMIT 1");
        format!("SELECT EXISTS({})", sql)
    }

    pub fn build_max(&self, field: &str) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT MAX({}) as max_val FROM {}",
            self.dialect.quote(field),
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_min(&self, field: &str) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT MIN({}) as min_val FROM {}",
            self.dialect.quote(field),
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_sum(&self, field: &str) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT SUM({}) as sum_val FROM {}",
            self.dialect.quote(field),
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_avg(&self, field: &str) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT AVG({}) as avg_val FROM {}",
            self.dialect.quote(field),
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    /// 校验生成的 SELECT SQL 语句
    /// 检查 SQL 语法、JOIN 列名、表名合法性
    pub fn validate(&self) -> Result<(), Vec<sz_orm_sql_validator::SqlValidationError>> {
        let sql = self.build_select();
        let mut errors = Vec::new();

        if let Err(e) = sz_orm_sql_validator::validate_select(&sql) {
            errors.push(e);
        }

        // 校验 JOIN 子句产生的 SQL 是否合法
        if !self.joins.is_empty() {
            for join in &self.joins {
                match join {
                    JoinClause::Inner(_, left, right)
                    | JoinClause::Left(_, left, right)
                    | JoinClause::Right(_, left, right) => {
                        if let Err(e) = sz_orm_sql_validator::validate_column_name(left) {
                            errors.push(e);
                        }
                        if let Err(e) = sz_orm_sql_validator::validate_column_name(right) {
                            errors.push(e);
                        }
                    }
                    _ => {}
                }
            }
        }

        // 校验表名合法性
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());
        if let Err(e) = sz_orm_sql_validator::validate_table_name(&table) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 校验生成的 INSERT SQL 语句
    /// 含空数据检测（EmptyInsertData 错误）
    pub fn validate_insert(
        &self,
        data: &std::collections::HashMap<String, Value>,
    ) -> Result<(), Vec<sz_orm_sql_validator::SqlValidationError>> {
        let sql = self.build_insert(data);
        let mut errors = Vec::new();

        if sql.is_empty() {
            errors.push(sz_orm_sql_validator::SqlValidationError::EmptyInsertData);
            return Err(errors);
        }

        if let Err(e) = sz_orm_sql_validator::validate_insert(&sql) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 校验生成的 UPDATE SQL 语句
    /// 含空数据检测（EmptyUpdateData 错误）
    pub fn validate_update(
        &self,
        data: &std::collections::HashMap<String, Value>,
    ) -> Result<(), Vec<sz_orm_sql_validator::SqlValidationError>> {
        let sql = self.build_update(data);
        let mut errors = Vec::new();

        if sql.is_empty() {
            errors.push(sz_orm_sql_validator::SqlValidationError::EmptyUpdateData);
            return Err(errors);
        }

        if let Err(e) = sz_orm_sql_validator::validate_update(&sql) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 校验生成的 DELETE SQL 语句
    pub fn validate_delete(&self) -> Result<(), Vec<sz_orm_sql_validator::SqlValidationError>> {
        let sql = self.build_delete();
        let mut errors = Vec::new();

        if let Err(e) = sz_orm_sql_validator::validate_delete(&sql) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl<M: Model> fmt::Debug for QueryBuilder<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryBuilder")
            .field("table", &self.table)
            .field("select_columns", &self.select_columns)
            .field("where_conditions", &self.where_conditions.len())
            .field("limit", &self.limit_value)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_type::DbType;
    use crate::dialect::get_dialect;

    struct TestModel;
    impl Model for TestModel {
        type PrimaryKey = i64;

        fn table_name() -> &'static str {
            "test_models"
        }

        fn pk(&self) -> Self::PrimaryKey {
            1
        }

        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
    }

    #[test]
    fn test_query_builder_select() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .select(vec!["id", "name"])
            .build_select();
        assert!(sql.contains("SELECT id, name FROM"));
        assert!(sql.contains("`users`"));
    }

    #[test]
    fn test_query_builder_where() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .where_cond("status = 'active'")
            .where_cond("age > 18")
            .build_select();

        assert!(sql.contains("WHERE"));
        assert!(sql.contains("status = 'active'"));
        assert!(sql.contains("age > 18"));
    }

    #[test]
    fn test_query_builder_order_by() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .order_by("created_at")
            .order_desc("id")
            .build_select();

        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("`created_at` ASC"));
        assert!(sql.contains("`id` DESC"));
    }

    #[test]
    fn test_query_builder_limit_offset() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").limit(10).offset(20).build_select();

        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn test_query_builder_page() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").page(3, 20).build_select();

        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40"));
    }

    #[test]
    fn test_query_builder_insert() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("test".to_string()));
        data.insert("age".to_string(), Value::I64(25));

        let sql = builder.table("users").build_insert(&data);

        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("`name`"));
        assert!(sql.contains("'test'"));
    }

    #[test]
    fn test_query_builder_update() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("updated".to_string()));

        let sql = builder
            .table("users")
            .where_cond("id = 1")
            .build_update(&data);

        assert!(sql.contains("UPDATE"));
        assert!(sql.contains("`name` = 'updated'"));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_query_builder_delete() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").where_cond("id = 1").build_delete();

        assert!(sql.contains("DELETE FROM"));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_query_builder_count() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").build_count();

        assert!(sql.contains("SELECT COUNT(*)"));
        assert!(sql.contains("FROM"));
    }

    #[test]
    fn test_query_builder_where_in() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
            .build_select();

        assert!(sql.contains("IN ("));
    }

    #[test]
    fn test_query_builder_where_between() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .where_between("age", Value::I64(18), Value::I64(30))
            .build_select();

        assert!(sql.contains("BETWEEN"));
    }

    #[test]
    fn test_query_builder_where_null() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .where_null("deleted_at")
            .build_select();

        assert!(sql.contains("IS NULL"));
    }

    #[test]
    fn test_query_builder_join() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .join_inner("posts", "users.id", "posts.user_id")
            .build_select();

        assert!(sql.contains("INNER JOIN"));
        assert!(sql.contains("`posts`"));
    }

    #[test]
    fn test_query_builder_group_by() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").group_by("status").build_select();

        assert!(sql.contains("GROUP BY"));
        assert!(sql.contains("`status`"));
    }

    #[test]
    fn test_query_builder_max() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").build_max("score");

        assert!(sql.contains("MAX("));
        assert!(sql.contains("`score`"));
    }

    #[test]
    fn test_query_builder_min() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").build_min("price");

        assert!(sql.contains("MIN("));
        assert!(sql.contains("`price`"));
    }

    #[test]
    fn test_query_builder_sum() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("orders").build_sum("amount");

        assert!(sql.contains("SUM("));
        assert!(sql.contains("`amount`"));
    }

    #[test]
    fn test_query_builder_avg() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("scores").build_avg("value");

        assert!(sql.contains("AVG("));
        assert!(sql.contains("`value`"));
    }

    #[test]
    fn test_validator_select() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let result = builder.table("users").select(vec!["id", "name"]).validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_select_with_join() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let result = builder
            .table("users")
            .join_inner("posts", "users.id", "posts.user_id")
            .validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_insert() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("test".to_string()));

        let result = builder.table("users").validate_insert(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_insert_empty_data() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let data = std::collections::HashMap::new();
        let result = builder.table("users").validate_insert(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_validator_update() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("updated".to_string()));

        let result = builder.table("users").validate_update(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_update_empty_data() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let data = std::collections::HashMap::new();
        let result = builder.table("users").validate_update(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_validator_delete() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let result = builder
            .table("users")
            .where_cond("id = 1")
            .validate_delete();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_delete_no_where() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        // DELETE without WHERE still produces valid SQL (just no filter)
        let result = builder.table("users").validate_delete();
        assert!(result.is_ok());
    }
}
