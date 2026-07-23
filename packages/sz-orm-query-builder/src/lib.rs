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

/// 用反引号包裹标识符并转义内部反引号（MySQL 标准：` → ``）
///
/// # 安全性（门禁 9 修复）
///
/// 不转义的反引号包裹允许恶意标识符通过 ` 逃逸注入。本函数将标识符内
/// 的反引号加倍（MySQL 标准转义），确保拼接后的 SQL 不会被恶意标识符突破。
///
/// 支持带点号的限定标识符: `u.id` → `u`.`id`
fn quote_ident(s: &str) -> String {
    s.split('.')
        .map(|part| format!("`{}`", part.replace('`', "``")))
        .collect::<Vec<_>>()
        .join(".")
}

/// 校验 WHERE 条件字符串，拒绝明显的 SQL 注入模式
///
/// v0.2.2 修复 C-6：公开 `where_clause(condition: &str)` 接受任意字符串，存在 SQL 注入风险。
/// 本函数检测高危模式（分号+SQL 关键字、行注释、块注释），拒绝明显恶意输入。
///
/// # 检测模式
///
/// - `;` 后跟 SQL 关键字（DROP/DELETE/UPDATE/INSERT/ALTER/TRUNCATE/EXEC/CREATE/GRANT/REVOKE）
/// - `--` 行注释序列
/// - `/*` 块注释起始
/// - `*/` 块注释结束
///
/// # 注意
///
/// 此校验是基础防线，不能替代参数化查询。复杂 WHERE 条件应使用参数化 API。
fn check_where_injection(condition: &str) {
    let upper = condition.to_uppercase();
    const SQL_KEYWORDS: &[&str] = &[
        "DROP", "DELETE", "UPDATE", "INSERT", "ALTER", "TRUNCATE", "EXEC", "CREATE", "GRANT",
        "REVOKE",
    ];
    for kw in SQL_KEYWORDS {
        let pattern1 = format!(";{}", kw);
        let pattern2 = format!("; {}", kw);
        if upper.contains(&pattern1) || upper.contains(&pattern2) {
            panic!(
                "SQL injection detected in where_clause: semicolon followed by {} keyword: {:?}",
                kw, condition
            );
        }
    }
    if condition.contains("--") {
        panic!(
            "SQL injection detected in where_clause: line comment '--' not allowed: {:?}",
            condition
        );
    }
    if condition.contains("/*") || condition.contains("*/") {
        panic!(
            "SQL injection detected in where_clause: block comment '/*' or '*/' not allowed: {:?}",
            condition
        );
    }
}

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
    /// CTE（Common Table Expression）子句列表：(名称, 子查询 SQL, 是否递归)
    ctes: Vec<(String, String, bool)>,
    /// 窗口函数列：原始表达式（如 `ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC)`）
    window_columns: Vec<String>,
    /// FOR UPDATE 锁提示
    for_update: bool,
    /// FOR UPDATE 的列限定（NOWAIT / SKIP LOCKED 等）
    for_update_options: Option<String>,
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
    ///
    /// # 安全性（门禁 9 修复）
    ///
    /// 表名经 `quote_ident()` 转义。`on` 条件为表达式，调用方应确保不使用恶意输入构造。
    pub fn inner_join(mut self, table: &str, on: &str) -> Self {
        self.joins.push(format!(
            "INNER JOIN {} ON {}",
            Self::quote_join_table(table),
            on
        ));
        self
    }

    /// 添加 LEFT JOIN
    ///
    /// # 安全性（门禁 9 修复）
    ///
    /// 同 `inner_join`，表名经 `quote_ident()` 转义。
    pub fn left_join(mut self, table: &str, on: &str) -> Self {
        self.joins.push(format!(
            "LEFT JOIN {} ON {}",
            Self::quote_join_table(table),
            on
        ));
        self
    }

    /// 添加 RIGHT JOIN
    ///
    /// # 安全性（门禁 9 修复）
    ///
    /// 同 `inner_join`，表名经 `quote_ident()` 转义。
    pub fn right_join(mut self, table: &str, on: &str) -> Self {
        self.joins.push(format!(
            "RIGHT JOIN {} ON {}",
            Self::quote_join_table(table),
            on
        ));
        self
    }

    /// 对 JOIN 表名部分进行转义（支持别名：`orders o` → `` `orders` o ``）
    fn quote_join_table(table: &str) -> String {
        if let Some((tbl, alias)) = table.rsplit_once(' ') {
            if alias.to_uppercase() == "AS" {
                // `orders AS o`
                format!("{} AS {}", quote_ident(tbl), alias)
            } else {
                // `orders o`
                format!("{} {}", quote_ident(tbl), alias)
            }
        } else {
            quote_ident(table)
        }
    }

    /// 添加 WHERE 条件（AND 连接）
    ///
    /// # 安全性（v0.2.2 修复 C-6）
    ///
    /// 调用 `check_where_injection` 检测高危模式（分号+SQL 关键字、行注释、块注释）。
    /// 复杂 WHERE 条件应使用参数化查询 API，避免直接拼接字符串。
    pub fn where_clause(mut self, condition: &str) -> Self {
        check_where_injection(condition);
        self.wheres.push(condition.to_string());
        self
    }

    /// 添加 OR WHERE 条件
    ///
    /// # 安全性（v0.2.2 修复 C-6）
    ///
    /// 同 `where_clause`，调用 `check_where_injection` 检测高危模式。
    pub fn or_where(mut self, condition: &str) -> Self {
        check_where_injection(condition);
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

    /// 添加 CTE（Common Table Expression / WITH 子句）。
    ///
    /// 生成形如 `WITH name AS (subquery) SELECT ...` 的 SQL。
    ///
    /// # 参数
    ///
    /// - `name`: CTE 名称
    /// - `subquery`: 子查询 SQL（完整的 SELECT 语句）
    pub fn with_cte(mut self, name: &str, subquery: &str) -> Self {
        self.ctes
            .push((name.to_string(), subquery.to_string(), false));
        self
    }

    /// 添加递归 CTE（`WITH RECURSIVE name AS (...) SELECT ...`）。
    ///
    /// # 参数
    ///
    /// - `name`: CTE 名称
    /// - `subquery`: 递归子查询 SQL
    pub fn with_recursive_cte(mut self, name: &str, subquery: &str) -> Self {
        self.ctes
            .push((name.to_string(), subquery.to_string(), true));
        self
    }

    /// 添加窗口函数列（作为 SELECT 列表的原始表达式）。
    ///
    /// 调用方负责构造完整的窗口函数表达式，例如：
    /// - `ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC)`
    /// - `RANK() OVER (ORDER BY score DESC)`
    /// - `SUM(amount) OVER (PARTITION BY user_id ORDER BY created_at)`
    ///
    /// # 参数
    ///
    /// - `expr`: 完整的窗口函数表达式
    pub fn window_function(mut self, expr: &str) -> Self {
        self.window_columns.push(expr.to_string());
        self
    }

    /// 添加 `ROW_NUMBER()` 窗口函数列。
    ///
    /// # 参数
    ///
    /// - `partition_by`: PARTITION BY 列（可为空）
    /// - `order_by`: ORDER BY 列（如 `salary DESC`）
    /// - `alias`: 结果列别名（如 `row_num`）
    pub fn row_number(self, partition_by: &str, order_by: &str, alias: &str) -> Self {
        let partition_clause = if partition_by.is_empty() {
            String::new()
        } else {
            format!("PARTITION BY {} ", partition_by)
        };
        let expr = format!(
            "ROW_NUMBER() OVER ({}ORDER BY {}) AS {}",
            partition_clause, order_by, alias
        );
        self.window_function(&expr)
    }

    /// 添加 `RANK()` 窗口函数列。
    ///
    /// # 参数
    ///
    /// - `partition_by`: PARTITION BY 列（可为空）
    /// - `order_by`: ORDER BY 列
    /// - `alias`: 结果列别名
    pub fn rank(self, partition_by: &str, order_by: &str, alias: &str) -> Self {
        let partition_clause = if partition_by.is_empty() {
            String::new()
        } else {
            format!("PARTITION BY {} ", partition_by)
        };
        let expr = format!(
            "RANK() OVER ({}ORDER BY {}) AS {}",
            partition_clause, order_by, alias
        );
        self.window_function(&expr)
    }

    /// 添加 `DENSE_RANK()` 窗口函数列。
    ///
    /// # 参数
    ///
    /// - `partition_by`: PARTITION BY 列（可为空）
    /// - `order_by`: ORDER BY 列
    /// - `alias`: 结果列别名
    pub fn dense_rank(self, partition_by: &str, order_by: &str, alias: &str) -> Self {
        let partition_clause = if partition_by.is_empty() {
            String::new()
        } else {
            format!("PARTITION BY {} ", partition_by)
        };
        let expr = format!(
            "DENSE_RANK() OVER ({}ORDER BY {}) AS {}",
            partition_clause, order_by, alias
        );
        self.window_function(&expr)
    }

    /// 设置 FOR UPDATE 行锁。
    ///
    /// 在生成的 SQL 末尾追加 `FOR UPDATE`，用于悲观锁。
    pub fn for_update(mut self) -> Self {
        self.for_update = true;
        self.for_update_options = None;
        self
    }

    /// 设置 FOR UPDATE 并附带选项（如 `NOWAIT`、`SKIP LOCKED`）。
    ///
    /// # 参数
    ///
    /// - `options`: 选项字符串，如 `"NOWAIT"` 或 `"SKIP LOCKED"`
    pub fn for_update_with_options(mut self, options: &str) -> Self {
        self.for_update = true;
        self.for_update_options = Some(options.to_string());
        self
    }

    /// 将当前查询与另一个查询进行 UNION 集合运算。
    ///
    /// 返回一个 [`SetQuery`]，可通过 `build()` 生成最终 SQL。
    pub fn union(self, other: SelectQuery) -> SetQuery {
        SetQuery::new(self, SetOperator::Union, other)
    }

    /// 将当前查询与另一个查询进行 UNION ALL 集合运算。
    pub fn union_all(self, other: SelectQuery) -> SetQuery {
        SetQuery::new(self, SetOperator::UnionAll, other)
    }

    /// 将当前查询与另一个查询进行 INTERSECT 集合运算。
    pub fn intersect(self, other: SelectQuery) -> SetQuery {
        SetQuery::new(self, SetOperator::Intersect, other)
    }

    /// 将当前查询与另一个查询进行 EXCEPT 集合运算。
    pub fn except(self, other: SelectQuery) -> SetQuery {
        SetQuery::new(self, SetOperator::Except, other)
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

        // CTE（WITH 子句）
        if !self.ctes.is_empty() {
            let has_recursive = self.ctes.iter().any(|(_, _, r)| *r);
            if has_recursive {
                sql.push_str("WITH RECURSIVE ");
            } else {
                sql.push_str("WITH ");
            }
            let cte_strs: Vec<String> = self
                .ctes
                .iter()
                .map(|(name, subquery, _)| format!("{} AS ({})", name, subquery))
                .collect();
            sql.push_str(&cte_strs.join(", "));
            sql.push(' ');
        }

        sql.push_str("SELECT ");

        if self.distinct {
            sql.push_str("DISTINCT ");
        }

        // 合并普通列与窗口函数列
        let mut all_columns: Vec<String> = self
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
        all_columns.extend(self.window_columns.iter().cloned());

        if all_columns.is_empty() {
            sql.push('*');
        } else {
            sql.push_str(&all_columns.join(", "));
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
            sql.push_str(
                &self
                    .group_by
                    .iter()
                    .map(|c| quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }

        if !self.having.is_empty() {
            sql.push_str(" HAVING ");
            sql.push_str(&self.having.join(" AND "));
        }

        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(
                &self
                    .order_by
                    .iter()
                    .map(|s| {
                        // 格式为 "column ASC" 或 "column DESC"
                        if let Some((col, dir)) = s.rsplit_once(' ') {
                            format!("{} {}", quote_ident(col), dir)
                        } else {
                            quote_ident(s)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        // FOR UPDATE 行锁
        if self.for_update {
            sql.push_str(" FOR UPDATE");
            if let Some(ref opts) = self.for_update_options {
                sql.push(' ');
                sql.push_str(opts);
            }
        }

        sql
    }
}

// ============================================================================
// 深度扩展：集合运算（UNION / INTERSECT / EXCEPT）
// ============================================================================

/// SQL 集合运算符类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOperator {
    /// `UNION`：合并去重
    Union,
    /// `UNION ALL`：合并不去重
    UnionAll,
    /// `INTERSECT`：交集
    Intersect,
    /// `EXCEPT`：差集（MySQL 8.0+ 称 `EXCEPT`，部分方言为 `MINUS`）
    Except,
}

impl SetOperator {
    /// 返回运算符对应的 SQL 关键字。
    pub fn as_sql(&self) -> &'static str {
        match self {
            SetOperator::Union => "UNION",
            SetOperator::UnionAll => "UNION ALL",
            SetOperator::Intersect => "INTERSECT",
            SetOperator::Except => "EXCEPT",
        }
    }
}

/// 集合运算查询，支持链式追加多个 SELECT 并以 UNION/INTERSECT/EXCEPT 连接。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_core::DbType;
/// use sz_orm_query_builder::Query;
///
/// let q1 = Query::select().column("id").from("active_users");
/// let q2 = Query::select().column("id").from("pending_users");
/// let sql = q1.union(q2).build(DbType::MySQL);
/// // SELECT `id` FROM `active_users` UNION SELECT `id` FROM `pending_users`
/// ```
#[derive(Debug, Clone)]
pub struct SetQuery {
    /// 第一个 SELECT 查询
    first: SelectQuery,
    /// 后续的 (运算符, 查询) 对
    rest: Vec<(SetOperator, SelectQuery)>,
    /// 全局 ORDER BY（作用于整个集合运算结果）
    order_by: Vec<String>,
    /// 全局 LIMIT
    limit: Option<u64>,
    /// 全局 OFFSET
    offset: Option<u64>,
}

impl SetQuery {
    /// 创建一个集合运算查询。
    pub fn new(first: SelectQuery, op: SetOperator, second: SelectQuery) -> Self {
        Self {
            first,
            rest: vec![(op, second)],
            order_by: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// 追加 UNION 查询。
    pub fn union(mut self, other: SelectQuery) -> Self {
        self.rest.push((SetOperator::Union, other));
        self
    }

    /// 追加 UNION ALL 查询。
    pub fn union_all(mut self, other: SelectQuery) -> Self {
        self.rest.push((SetOperator::UnionAll, other));
        self
    }

    /// 追加 INTERSECT 查询。
    pub fn intersect(mut self, other: SelectQuery) -> Self {
        self.rest.push((SetOperator::Intersect, other));
        self
    }

    /// 追加 EXCEPT 查询。
    pub fn except(mut self, other: SelectQuery) -> Self {
        self.rest.push((SetOperator::Except, other));
        self
    }

    /// 设置全局 ORDER BY（作用于整个集合运算结果）。
    pub fn order_by(mut self, column: &str, asc: bool) -> Self {
        let dir = if asc { "ASC" } else { "DESC" };
        self.order_by.push(format!("{} {}", column, dir));
        self
    }

    /// 设置全局 LIMIT。
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    /// 设置全局 OFFSET。
    pub fn offset(mut self, n: u64) -> Self {
        self.offset = Some(n);
        self
    }

    /// 生成 SQL。
    ///
    /// 将所有子查询用对应的集合运算符连接，并在末尾追加全局 ORDER BY / LIMIT / OFFSET。
    pub fn build(self, db_type: DbType) -> String {
        let mut sql = self.first.build(db_type);
        for (op, query) in &self.rest {
            sql.push(' ');
            sql.push_str(op.as_sql());
            sql.push(' ');
            sql.push_str(&query.clone().build(db_type));
        }
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(
                &self
                    .order_by
                    .iter()
                    .map(|s| {
                        if let Some((col, dir)) = s.rsplit_once(' ') {
                            format!("{} {}", quote_ident(col), dir)
                        } else {
                            quote_ident(s)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
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

    /// 构建 INSERT SQL（无方言，硬编码反引号）
    ///
    /// # 安全性（门禁 9 修复）
    ///
    /// 标识符经 `quote_ident()` 转义后包裹反引号，防止含 `` ` `` 的恶意标识符逃逸。
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.columns.is_empty() {
            return String::new();
        }

        let cols: Vec<String> = self.columns.iter().map(|c| quote_ident(c)).collect();
        let vals: Vec<String> = self.values.iter().map(|v| v.to_string()).collect();

        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quote_ident(&table),
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
    ///
    /// # 安全性（v0.2.2 修复 C-6）
    ///
    /// 调用 `check_where_injection` 检测高危模式。
    pub fn where_clause(mut self, condition: &str) -> Self {
        check_where_injection(condition);
        self.wheres.push(condition.to_string());
        self
    }

    /// 生成 SQL
    ///
    /// # 安全性（门禁 9 修复）
    ///
    /// 表名和列名经 `quote_ident()` 转义后包裹反引号，防止含 `` ` `` 的恶意标识符逃逸。
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.sets.is_empty() {
            return String::new();
        }

        let set_str: Vec<String> = self
            .sets
            .iter()
            .map(|(c, v)| format!("{} = {}", quote_ident(c), v))
            .collect();

        let mut sql = format!("UPDATE {} SET {}", quote_ident(&table), set_str.join(", "));

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
    ///
    /// # 安全性（v0.2.2 修复 C-6）
    ///
    /// 调用 `check_where_injection` 检测高危模式。
    pub fn where_clause(mut self, condition: &str) -> Self {
        check_where_injection(condition);
        self.wheres.push(condition.to_string());
        self
    }

    /// 生成 SQL
    ///
    /// # 安全性（门禁 9 修复）
    ///
    /// 表名经 `quote_ident()` 转义后包裹反引号，防止含 `` ` `` 的恶意表名逃逸。
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() {
            return String::new();
        }

        let mut sql = format!("DELETE FROM {}", quote_ident(&table));

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
        assert!(sql.contains("INNER JOIN `orders` o ON u.id = o.user_id"));
    }

    #[test]
    fn test_select_with_left_join() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .left_join("profiles p", "u.id = p.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("LEFT JOIN `profiles` p ON u.id = p.user_id"));
    }

    #[test]
    fn test_select_with_order_by() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .order_by("created_at", true)
            .order_by("id", false)
            .build(DbType::MySQL);
        assert!(sql.contains("ORDER BY `created_at` ASC, `id` DESC"));
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
        assert!(sql.contains("GROUP BY `status`"));
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
        assert!(sql.contains("INNER JOIN `orders` o"));
        assert!(sql.contains("LEFT JOIN `profiles` p"));
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
        assert!(sql.contains("INNER JOIN `orders` o"));
        assert!(sql.contains("WHERE u.status = 'active' AND o.total > 100"));
        assert!(sql.contains("GROUP BY"));
        assert!(sql.contains("HAVING SUM(o.total) > 1000"));
        assert!(sql.contains("ORDER BY `u`.`id` ASC"));
        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40"));
    }

    // ---- v0.2.2 修复 C-6：SQL 注入测试 ----

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_where_rejects_semicolon_drop() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("1=1; DROP TABLE users")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_where_rejects_semicolon_space_drop() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("1=1; DROP TABLE users")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_where_rejects_line_comment() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("id = 1 -- DROP TABLE users")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_where_rejects_block_comment() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("id = 1 /* comment */ OR 1=1")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_or_where_rejects_drop() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("id = 1")
            .or_where("1=1; DROP TABLE users")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_update_where_rejects_delete() {
        let _ = Query::update()
            .table("users")
            .set("name", "'x'")
            .where_clause("1=1; DELETE FROM users")
            .build();
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_update_where_rejects_line_comment() {
        let _ = Query::update()
            .table("users")
            .set("name", "'x'")
            .where_clause("id = 1 -- bypass")
            .build();
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_delete_where_rejects_drop() {
        let _ = Query::delete()
            .from_table("users")
            .where_clause("1=1; DROP TABLE users")
            .build();
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_delete_where_rejects_block_comment() {
        let _ = Query::delete()
            .from_table("users")
            .where_clause("id = 1 /* */ OR 1=1")
            .build();
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_delete_where_rejects_line_comment() {
        let _ = Query::delete()
            .from_table("users")
            .where_clause("id = 1--")
            .build();
    }

    #[test]
    fn test_safe_where_clauses_pass() {
        // 这些是合法的 WHERE 条件，不应触发 panic
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("age > 18")
            .where_clause("name = 'Alice;Bob'") // 分号在字符串字面量中
            .where_clause("id IN (1, 2, 3)")
            .where_clause("created_at > '2026-01-01'")
            .build(DbType::MySQL);

        let _ = Query::update()
            .table("users")
            .set("name", "'x'")
            .where_clause("id = 1")
            .build();

        let _ = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .build();
    }

    // ==================== 深度扩展：CTE / 窗口函数 / 集合运算 / FOR UPDATE 测试 ====================

    // ---- CTE 测试 ----

    #[test]
    fn test_cte_single_with_clause() {
        let sql = Query::select()
            .column("id")
            .column("name")
            .from("active_users")
            .with_cte("active_users", "SELECT * FROM users WHERE status = 'active'")
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH active_users AS ("));
        assert!(sql.contains("SELECT * FROM users WHERE status = 'active'"));
        assert!(sql.contains("SELECT `id`, `name` FROM `active_users`"));
    }

    #[test]
    fn test_cte_multiple_with_clauses() {
        let sql = Query::select()
            .column("id")
            .from("combined")
            .with_cte("a", "SELECT id FROM table_a")
            .with_cte("b", "SELECT id FROM table_b")
            .with_cte("combined", "SELECT id FROM a UNION SELECT id FROM b")
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH a AS (SELECT id FROM table_a), b AS (SELECT id FROM table_b), combined AS ("));
    }

    #[test]
    fn test_cte_recursive_with_clause() {
        let sql = Query::select()
            .column("id")
            .column("parent_id")
            .from("tree")
            .with_recursive_cte("tree", "SELECT id, parent_id FROM nodes WHERE id = 1")
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH RECURSIVE tree AS ("));
    }

    #[test]
    fn test_cte_no_cte_no_with_prefix() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .build(DbType::MySQL);
        assert!(!sql.contains("WITH"));
        assert!(sql.starts_with("SELECT"));
    }

    // ---- 窗口函数测试 ----

    #[test]
    fn test_window_function_raw_expr() {
        let sql = Query::select()
            .column("id")
            .column("salary")
            .from("employees")
            .window_function("ROW_NUMBER() OVER (ORDER BY salary DESC) AS rn")
            .build(DbType::MySQL);
        assert!(sql.contains("ROW_NUMBER() OVER (ORDER BY salary DESC) AS rn"));
    }

    #[test]
    fn test_row_number_helper_with_partition() {
        let sql = Query::select()
            .column("name")
            .column("dept")
            .from("employees")
            .row_number("dept", "salary DESC", "row_num")
            .build(DbType::MySQL);
        assert!(sql.contains(
            "ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC) AS row_num"
        ));
    }

    #[test]
    fn test_row_number_helper_without_partition() {
        let sql = Query::select()
            .column("name")
            .from("employees")
            .row_number("", "salary DESC", "rn")
            .build(DbType::MySQL);
        assert!(sql.contains("ROW_NUMBER() OVER (ORDER BY salary DESC) AS rn"));
        assert!(!sql.contains("PARTITION BY"));
    }

    #[test]
    fn test_rank_helper() {
        let sql = Query::select()
            .column("name")
            .from("scores")
            .rank("", "score DESC", "rank_num")
            .build(DbType::MySQL);
        assert!(sql.contains("RANK() OVER (ORDER BY score DESC) AS rank_num"));
    }

    #[test]
    fn test_dense_rank_helper_with_partition() {
        let sql = Query::select()
            .column("name")
            .from("scores")
            .dense_rank("class", "score DESC", "dr")
            .build(DbType::MySQL);
        assert!(sql.contains(
            "DENSE_RANK() OVER (PARTITION BY class ORDER BY score DESC) AS dr"
        ));
    }

    #[test]
    fn test_multiple_window_functions() {
        let sql = Query::select()
            .column("name")
            .column("salary")
            .from("employees")
            .row_number("dept", "salary DESC", "rn")
            .rank("dept", "salary DESC", "rk")
            .dense_rank("dept", "salary DESC", "dr")
            .build(DbType::MySQL);
        assert!(sql.contains("ROW_NUMBER()"));
        assert!(sql.contains("RANK()"));
        assert!(sql.contains("DENSE_RANK()"));
    }

    #[test]
    fn test_window_function_with_cte_combined() {
        let sql = Query::select()
            .column("name")
            .from("ranked")
            .with_cte(
                "ranked",
                "SELECT name, ROW_NUMBER() OVER (ORDER BY salary) AS rn FROM employees",
            )
            .where_clause("rn <= 10")
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH ranked AS ("));
        assert!(sql.contains("FROM `ranked`"));
        assert!(sql.contains("WHERE rn <= 10"));
    }

    // ---- FOR UPDATE 测试 ----

    #[test]
    fn test_for_update_basic() {
        let sql = Query::select()
            .column("id")
            .column("balance")
            .from("accounts")
            .where_clause("id = 1")
            .for_update()
            .build(DbType::MySQL);
        assert!(sql.ends_with(" FOR UPDATE"));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn test_for_update_with_nowait() {
        let sql = Query::select()
            .column("id")
            .from("accounts")
            .where_clause("id = 1")
            .for_update_with_options("NOWAIT")
            .build(DbType::MySQL);
        assert!(sql.ends_with(" FOR UPDATE NOWAIT"));
    }

    #[test]
    fn test_for_update_with_skip_locked() {
        let sql = Query::select()
            .column("id")
            .from("accounts")
            .where_clause("id = 1")
            .for_update_with_options("SKIP LOCKED")
            .build(DbType::MySQL);
        assert!(sql.ends_with(" FOR UPDATE SKIP LOCKED"));
    }

    #[test]
    fn test_for_update_with_limit_and_order() {
        let sql = Query::select()
            .column("id")
            .from("jobs")
            .order_by("priority", false)
            .limit(1)
            .for_update_with_options("SKIP LOCKED")
            .build(DbType::MySQL);
        assert!(sql.contains("ORDER BY `priority` DESC"));
        assert!(sql.contains("LIMIT 1"));
        assert!(sql.ends_with(" FOR UPDATE SKIP LOCKED"));
    }

    #[test]
    fn test_no_for_update_by_default() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .build(DbType::MySQL);
        assert!(!sql.contains("FOR UPDATE"));
    }

    // ---- 集合运算（UNION / INTERSECT / EXCEPT）测试 ----

    #[test]
    fn test_set_operator_as_sql() {
        assert_eq!(SetOperator::Union.as_sql(), "UNION");
        assert_eq!(SetOperator::UnionAll.as_sql(), "UNION ALL");
        assert_eq!(SetOperator::Intersect.as_sql(), "INTERSECT");
        assert_eq!(SetOperator::Except.as_sql(), "EXCEPT");
    }

    #[test]
    fn test_union_basic() {
        let q1 = Query::select().column("id").from("active_users");
        let q2 = Query::select().column("id").from("pending_users");
        let sql = q1.union(q2).build(DbType::MySQL);
        assert!(sql.contains("SELECT `id` FROM `active_users`"));
        assert!(sql.contains(" UNION "));
        assert!(sql.contains("SELECT `id` FROM `pending_users`"));
    }

    #[test]
    fn test_union_all_basic() {
        let q1 = Query::select().column("id").from("table_a");
        let q2 = Query::select().column("id").from("table_b");
        let sql = q1.union_all(q2).build(DbType::MySQL);
        assert!(sql.contains(" UNION ALL "));
    }

    #[test]
    fn test_intersect_basic() {
        let q1 = Query::select().column("id").from("table_a");
        let q2 = Query::select().column("id").from("table_b");
        let sql = q1.intersect(q2).build(DbType::MySQL);
        assert!(sql.contains(" INTERSECT "));
    }

    #[test]
    fn test_except_basic() {
        let q1 = Query::select().column("id").from("table_a");
        let q2 = Query::select().column("id").from("table_b");
        let sql = q1.except(q2).build(DbType::MySQL);
        assert!(sql.contains(" EXCEPT "));
    }

    #[test]
    fn test_union_chained_multiple() {
        let q1 = Query::select().column("id").from("t1");
        let q2 = Query::select().column("id").from("t2");
        let q3 = Query::select().column("id").from("t3");
        let sql = q1.union(q2).union(q3).build(DbType::MySQL);
        assert_eq!(sql.matches("UNION").count(), 2);
    }

    #[test]
    fn test_union_mixed_operators() {
        let q1 = Query::select().column("id").from("t1");
        let q2 = Query::select().column("id").from("t2");
        let q3 = Query::select().column("id").from("t3");
        let sql = q1.union(q2).intersect(q3).build(DbType::MySQL);
        assert!(sql.contains(" UNION "));
        assert!(sql.contains(" INTERSECT "));
    }

    #[test]
    fn test_union_with_order_by_limit() {
        let q1 = Query::select().column("id").from("t1");
        let q2 = Query::select().column("id").from("t2");
        let sql = q1
            .union(q2)
            .order_by("id", true)
            .limit(10)
            .offset(5)
            .build(DbType::MySQL);
        assert!(sql.contains("ORDER BY `id` ASC"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 5"));
    }

    #[test]
    fn test_union_postgres_dialect() {
        let q1 = Query::select().column("id").from("t1");
        let q2 = Query::select().column("id").from("t2");
        let sql = q1.union(q2).build(DbType::PostgreSQL);
        assert!(sql.contains("\"id\""));
        assert!(sql.contains(" UNION "));
    }

    #[test]
    fn test_union_with_where_clauses() {
        let q1 = Query::select()
            .column("id")
            .from("active_users")
            .where_clause("age > 18");
        let q2 = Query::select()
            .column("id")
            .from("pending_users")
            .where_clause("age > 18");
        let sql = q1.union(q2).build(DbType::MySQL);
        assert!(sql.contains("WHERE age > 18"));
        assert!(sql.contains(" UNION "));
    }

    // ---- 综合场景测试 ----

    #[test]
    fn test_cte_window_for_update_combined() {
        // 复杂查询：CTE + 窗口函数 + FOR UPDATE
        let sql = Query::select()
            .column("id")
            .column("salary")
            .from("ranked_salaries")
            .with_cte(
                "ranked_salaries",
                "SELECT id, salary, ROW_NUMBER() OVER (ORDER BY salary DESC) AS rn FROM employees",
            )
            .where_clause("rn = 1")
            .for_update()
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH ranked_salaries AS ("));
        assert!(sql.contains("FOR UPDATE"));
        assert!(sql.contains("WHERE rn = 1"));
    }

    #[test]
    fn test_complex_window_aggregation() {
        // 运行总和 + 排名
        let sql = Query::select()
            .column("user_id")
            .column("amount")
            .from("transactions")
            .window_function("SUM(amount) OVER (PARTITION BY user_id ORDER BY created_at) AS running_total")
            .rank("user_id", "created_at", "tx_rank")
            .build(DbType::MySQL);
        assert!(sql.contains("SUM(amount) OVER (PARTITION BY user_id ORDER BY created_at) AS running_total"));
        assert!(sql.contains("RANK() OVER (PARTITION BY user_id ORDER BY created_at) AS tx_rank"));
    }
}
