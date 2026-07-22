//! find_with_related 关联查询流畅 API
//!
//! 对应 SeaORM 的 `Entity::find().find_with_related(RelatedEntity).all(db).await` API。
//! 在 SZ-ORM 中，由于 QueryBuilder 主要生成 SQL（不直接执行），本模块提供
//! "生成关联查询 SQL" 的辅助 API：
//!
//! 1. `find_with_related_join`：以 JOIN 方式生成单条 SQL（适合 1:1 / N:1 关联）
//! 2. `find_with_related_subquery`：以子查询方式生成 SQL（适合 1:N 关联，避免行膨胀）
//! 3. `find_with_related_eager_sql`：生成 eager load 的两条 SQL（先主表，后关联表 WHERE IN）
//!
//! # 用法
//!
//! ```no_run
//! use sz_orm_core::find_with_related::find_with_related_join;
//! use sz_orm_core::{get_dialect, DbType, Relation, HasMany};
//! use std::collections::HashMap;
//!
//! let mut relations = HashMap::new();
//! relations.insert("orders", Relation::HasMany(HasMany {
//!     foreign_key: "user_id".to_string(),
//!     child_model: "orders".to_string(),
//!     child_pk: "id".to_string(),
//! }));
//!
//! let dialect = get_dialect(DbType::MySQL).unwrap();
//! let sql = find_with_related_join(
//!     &*dialect,          // 方言（Box<dyn Dialect> 解引用为 &dyn Dialect）
//!     "users",            // 主表
//!     "orders",           // 关联表
//!     "user_id",          // 外键
//!     "id",               // 主表主键
//!     true,               // LEFT JOIN
//! )
//!     .where_cond("users.id = 1")
//!     .build();
//! ```

use crate::dialect::Dialect;
use crate::model::Relation;
use std::collections::HashMap;

/// find_with_related 关联查询构造器（JOIN 模式）
///
/// 适合 1:1 / N:1 关联（BelongsTo / HasOne）。
/// 对于 1:N 关联，JOIN 会导致主表行膨胀，应使用 `find_with_related_eager_sql`。
pub struct FindWithRelated<'a> {
    dialect: &'a dyn Dialect,
    main_table: String,
    related_table: String,
    foreign_key: String,
    primary_key: String,
    left_join: bool,
    where_conds: Vec<String>,
    order_by: Vec<(String, bool)>, // (field, is_desc)
    limit: Option<usize>,
    offset: Option<usize>,
}

impl<'a> FindWithRelated<'a> {
    /// 创建关联查询构造器
    ///
    /// - `dialect`：方言引用
    /// - `main_table`：主表名
    /// - `related_table`：关联表名
    /// - `foreign_key`：外键列名（在 related_table 中，指向 main_table.primary_key）
    /// - `primary_key`：主表主键列名
    /// - `left_join`：true = LEFT JOIN，false = INNER JOIN
    pub fn new(
        dialect: &'a dyn Dialect,
        main_table: impl Into<String>,
        related_table: impl Into<String>,
        foreign_key: impl Into<String>,
        primary_key: impl Into<String>,
        left_join: bool,
    ) -> Self {
        let main_table = main_table.into();
        let related_table = related_table.into();
        let foreign_key = foreign_key.into();
        let primary_key = primary_key.into();
        // H-2 修复：构造时校验所有标识符
        validate_find_identifiers(&[&main_table, &related_table, &foreign_key, &primary_key])
            .expect("invalid SQL identifier in FindWithRelated::new");
        Self {
            dialect,
            main_table,
            related_table,
            foreign_key,
            primary_key,
            left_join,
            where_conds: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// 追加 WHERE 条件（AND 连接）
    #[must_use]
    pub fn where_cond(mut self, cond: impl Into<String>) -> Self {
        self.where_conds.push(cond.into());
        self
    }

    /// 追加 ORDER BY（ASC）
    #[must_use]
    pub fn order_by(mut self, field: impl Into<String>) -> Self {
        self.order_by.push((field.into(), false));
        self
    }

    /// 追加 ORDER BY（DESC）
    #[must_use]
    pub fn order_desc(mut self, field: impl Into<String>) -> Self {
        self.order_by.push((field.into(), true));
        self
    }

    /// 设置 LIMIT
    #[must_use]
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// 设置 OFFSET
    #[must_use]
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = Some(n);
        self
    }

    /// 构建 SELECT SQL（JOIN 模式）
    ///
    /// 生成的 SQL 形如：
    /// ```sql
    /// SELECT `main`.*, `related`.*
    /// FROM `main`
    /// LEFT JOIN `related` ON `related`.`fk` = `main`.`pk`
    /// WHERE <conds>
    /// ORDER BY <field> [DESC]
    /// LIMIT <n> OFFSET <n>
    /// ```
    pub fn build(&self) -> String {
        let join_type = if self.left_join {
            "LEFT JOIN"
        } else {
            "INNER JOIN"
        };
        let mut sql = format!(
            "SELECT {}.*, {}.* FROM {} {} {} ON {}.{} = {}.{}",
            self.dialect.quote(&self.main_table),
            self.dialect.quote(&self.related_table),
            self.dialect.quote(&self.main_table),
            join_type,
            self.dialect.quote(&self.related_table),
            self.dialect.quote(&self.related_table),
            self.dialect.quote(&self.foreign_key),
            self.dialect.quote(&self.main_table),
            self.dialect.quote(&self.primary_key),
        );

        if !self.where_conds.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.where_conds.join(" AND "));
        }

        if !self.order_by.is_empty() {
            let parts: Vec<String> = self
                .order_by
                .iter()
                .map(|(f, desc)| {
                    let d = if *desc { " DESC" } else { "" };
                    format!("{}{}", self.dialect.quote(f), d)
                })
                .collect();
            sql.push_str(" ORDER BY ");
            sql.push_str(&parts.join(", "));
        }

        if let Some(n) = self.limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }
        if let Some(n) = self.offset {
            sql.push_str(&format!(" OFFSET {}", n));
        }

        sql
    }
}

/// 高级辅助：从 relations map 中提取关联表的元数据
///
/// 返回 `(related_table, foreign_key, primary_key, is_many)` 四元组。
/// `is_many` 为 true 表示 HasMany / BelongsToMany / MorphMany，建议使用 eager load。
pub fn inspect_relation<'a>(
    relations: &'a HashMap<&'a str, Relation>,
    name: &'a str,
) -> Option<(&'a str, &'a str, &'a str, bool)> {
    let rel = relations.get(name)?;
    match rel {
        Relation::HasMany(h) => Some((
            h.child_model.as_str(),
            h.foreign_key.as_str(),
            h.child_pk.as_str(),
            true,
        )),
        Relation::HasOne(h) => Some((
            h.child_model.as_str(),
            h.foreign_key.as_str(),
            h.child_pk.as_str(),
            false,
        )),
        Relation::BelongsTo(b) => Some((
            b.parent_model.as_str(),
            b.foreign_key.as_str(),
            b.parent_pk.as_str(),
            false,
        )),
        Relation::BelongsToMany(b) => Some((
            b.target_model.as_str(),
            b.foreign_key.as_str(),
            b.other_key.as_str(),
            true,
        )),
        Relation::MorphMany(m) => Some((
            m.child_model.as_str(),
            m.morph_id_column.as_str(),
            "id",
            true,
        )),
        Relation::MorphTo(m) => Some(("", m.morph_id_column.as_str(), "id", false)),
    }
}

/// 生成 JOIN 模式 SQL（便捷函数）
///
/// 等价于 `FindWithRelated::new(...).build()`。
pub fn find_with_related_join<'a>(
    dialect: &'a dyn Dialect,
    main_table: &'a str,
    related_table: &'a str,
    foreign_key: &'a str,
    primary_key: &'a str,
    left_join: bool,
) -> FindWithRelated<'a> {
    FindWithRelated::new(
        dialect,
        main_table,
        related_table,
        foreign_key,
        primary_key,
        left_join,
    )
}

/// 生成 eager load 的两条 SQL（适合 1:N 关联）
///
/// 返回 `(main_sql, related_sql_template)`：
/// - `main_sql`：SELECT 主表所有行（带用户 WHERE）
/// - `related_sql_template`：SELECT 关联表 WHERE foreign_key IN (?) — 占位符由调用方填充
///
/// # 参数
/// - `dialect`：方言
/// - `main_table` / `related_table`：表名
/// - `foreign_key`：关联表中的外键列
/// - `main_where`：主表 WHERE 条件（可为空）
///
/// # 安全
/// - `main_table` / `related_table` / `foreign_key` 会校验为合法 SQL 标识符
/// - **`main_where` 由调用方负责安全**：调用方必须使用参数化查询或 `WhereBuilder` 构造，
///   严禁直接拼接用户输入（H-2 风险点）
#[tracing::instrument(skip(dialect), fields(main_table = main_table, related_table = related_table, strategy = "eager_sql"))]
pub fn find_with_related_eager_sql(
    dialect: &dyn Dialect,
    main_table: &str,
    related_table: &str,
    foreign_key: &str,
    main_where: Option<&str>,
) -> (String, String) {
    // H-2 修复：校验表名/列名为合法标识符
    validate_find_identifiers(&[main_table, related_table, foreign_key])
        .expect("invalid SQL identifier in find_with_related_eager_sql");

    let main_sql = if let Some(w) = main_where {
        format!("SELECT * FROM {} WHERE {}", dialect.quote(main_table), w)
    } else {
        format!("SELECT * FROM {}", dialect.quote(main_table))
    };

    // 关联表 SQL 模板：调用方应将 ? 替换为实际主键列表（如 1,2,3）
    let related_sql = format!(
        "SELECT * FROM {} WHERE {} IN (?)",
        dialect.quote(related_table),
        dialect.quote(foreign_key),
    );

    (main_sql, related_sql)
}

/// 生成子查询模式 SQL（适合 1:N 关联，避免主表行膨胀）
///
/// 生成的 SQL 形如：
/// ```sql
/// SELECT * FROM `main`
/// WHERE `pk` IN (
///   SELECT `fk` FROM `related` WHERE <related_where>
/// )
/// ```
#[tracing::instrument(skip(dialect), fields(main_table = main_table, related_table = related_table, strategy = "subquery"))]
pub fn find_with_related_subquery(
    dialect: &dyn Dialect,
    main_table: &str,
    related_table: &str,
    foreign_key: &str,
    primary_key: &str,
    related_where: Option<&str>,
) -> String {
    // H-2 修复：校验表名/列名为合法标识符
    validate_find_identifiers(&[main_table, related_table, foreign_key, primary_key])
        .expect("invalid SQL identifier in find_with_related_subquery");

    let inner = if let Some(w) = related_where {
        format!(
            "SELECT {} FROM {} WHERE {}",
            dialect.quote(foreign_key),
            dialect.quote(related_table),
            w
        )
    } else {
        format!(
            "SELECT {} FROM {}",
            dialect.quote(foreign_key),
            dialect.quote(related_table)
        )
    };
    format!(
        "SELECT * FROM {} WHERE {} IN ({})",
        dialect.quote(main_table),
        dialect.quote(primary_key),
        inner
    )
}

// ============================================================================
// WithRelation::load() 风格 API（SeaORM find_with_related 对应）
// ============================================================================

/// 关联关系描述（内部用）
#[derive(Debug, Clone)]
enum WithRelationKind {
    /// 一对多：主表.id ← 关联表.fk
    HasMany {
        foreign_key: String,
        primary_key: String,
    },
    /// 一对一：主表.id ← 关联表.fk
    HasOne {
        foreign_key: String,
        primary_key: String,
    },
    /// 多对一：关联表.id ← 主表.fk
    BelongsTo {
        foreign_key: String,
        primary_key: String,
    },
}

/// 关联配置项
#[derive(Debug, Clone)]
struct WithRelationItem {
    related_table: String,
    kind: WithRelationKind,
}

/// SeaORM find_with_related 风格的关联加载器
///
/// # 用法
///
/// ```ignore
/// use sz_orm_core::find_with_related::WithRelation;
/// use sz_orm_core::dialect::get_dialect;
/// use sz_orm_core::DbType;
///
/// let dialect = get_dialect(DbType::MySQL).unwrap();
/// let loader = WithRelation::new(&*dialect, "users")
///     .with_has_many("orders", "user_id", "id")
///     .with_has_one("profiles", "user_id", "id")
///     .load_eager(Some("users.id IN (1, 2, 3)"));
///
/// println!("{}", loader.main_sql());        // 主表 SQL
/// println!("{}", loader.related_sql("orders").unwrap());  // 关联表 SQL
/// ```
pub struct WithRelation<'a> {
    dialect: &'a dyn Dialect,
    main_table: String,
    relations: Vec<(&'a str, WithRelationItem)>,
    main_where: Option<String>,
}

impl<'a> WithRelation<'a> {
    /// 创建关联加载器
    pub fn new(dialect: &'a dyn Dialect, main_table: impl Into<String>) -> Self {
        let main_table = main_table.into();
        // H-2 修复：校验主表名
        validate_find_identifiers(&[&main_table])
            .expect("invalid SQL identifier in WithRelation::new");
        Self {
            dialect,
            main_table,
            relations: Vec::new(),
            main_where: None,
        }
    }

    /// 添加 HasMany 关联
    pub fn with_has_many(
        mut self,
        related: &'a str,
        foreign_key: impl Into<String>,
        primary_key: impl Into<String>,
    ) -> Self {
        let foreign_key = foreign_key.into();
        let primary_key = primary_key.into();
        // H-2 修复：校验关联表名/列名
        validate_find_identifiers(&[related, &foreign_key, &primary_key])
            .expect("invalid SQL identifier in with_has_many");
        self.relations.push((
            related,
            WithRelationItem {
                related_table: related.to_string(),
                kind: WithRelationKind::HasMany {
                    foreign_key,
                    primary_key,
                },
            },
        ));
        self
    }

    /// 添加 HasOne 关联
    pub fn with_has_one(
        mut self,
        related: &'a str,
        foreign_key: impl Into<String>,
        primary_key: impl Into<String>,
    ) -> Self {
        let foreign_key = foreign_key.into();
        let primary_key = primary_key.into();
        // H-2 修复：校验关联表名/列名
        validate_find_identifiers(&[related, &foreign_key, &primary_key])
            .expect("invalid SQL identifier in with_has_one");
        self.relations.push((
            related,
            WithRelationItem {
                related_table: related.to_string(),
                kind: WithRelationKind::HasOne {
                    foreign_key,
                    primary_key,
                },
            },
        ));
        self
    }

    /// 添加 BelongsTo 关联
    pub fn with_belongs_to(
        mut self,
        related: &'a str,
        foreign_key: impl Into<String>,
        primary_key: impl Into<String>,
    ) -> Self {
        let foreign_key = foreign_key.into();
        let primary_key = primary_key.into();
        // H-2 修复：校验关联表名/列名
        validate_find_identifiers(&[related, &foreign_key, &primary_key])
            .expect("invalid SQL identifier in with_belongs_to");
        self.relations.push((
            related,
            WithRelationItem {
                related_table: related.to_string(),
                kind: WithRelationKind::BelongsTo {
                    foreign_key,
                    primary_key,
                },
            },
        ));
        self
    }

    /// 执行 eager load（生成主表 SQL + 各关联表 SQL）
    ///
    /// - `main_where`：主表 WHERE 条件（None 表示无 WHERE）
    ///
    /// 返回 `self` 后通过 [`main_sql`](Self::main_sql) 和
    /// [`related_sql`](Self::related_sql) 获取生成的 SQL。
    #[tracing::instrument(skip(self), fields(strategy = "eager", main_table = &self.main_table))]
    pub fn load_eager(mut self, main_where: Option<&str>) -> Self {
        self.main_where = main_where.map(String::from);
        self
    }

    /// 执行 JOIN load（生成单条 JOIN SQL）
    ///
    /// HasMany / HasOne → LEFT JOIN
    /// BelongsTo → INNER JOIN
    #[tracing::instrument(skip(self), fields(strategy = "join", main_table = &self.main_table))]
    pub fn load_join(&self, main_where: Option<&str>) -> String {
        let mut sql = format!("SELECT {}.*", self.dialect.quote(&self.main_table));
        // 添加所有关联表的列
        for (_, item) in &self.relations {
            sql.push_str(&format!(", {}.*", self.dialect.quote(&item.related_table)));
        }
        sql.push_str(&format!(" FROM {}", self.dialect.quote(&self.main_table)));

        for (_, item) in &self.relations {
            let (join_type, left_col, right_col) = match &item.kind {
                WithRelationKind::HasMany {
                    foreign_key,
                    primary_key,
                }
                | WithRelationKind::HasOne {
                    foreign_key,
                    primary_key,
                } => (
                    "LEFT JOIN",
                    format!("{}.{}", item.related_table, foreign_key),
                    format!("{}.{}", self.main_table, primary_key),
                ),
                // BelongsTo: 主表.fk 引用关联表.pk
                // JOIN 条件: main.fk = related.pk（语义直观，等值连接可交换）
                WithRelationKind::BelongsTo {
                    foreign_key,
                    primary_key,
                } => (
                    "INNER JOIN",
                    format!("{}.{}", self.main_table, foreign_key),
                    format!("{}.{}", item.related_table, primary_key),
                ),
            };
            // 拆分 table.column 形式，分别 quote
            let (l_table, l_col) = split_qualified(&left_col);
            let (r_table, r_col) = split_qualified(&right_col);
            sql.push_str(&format!(
                " {} {} ON {}.{} = {}.{}",
                join_type,
                self.dialect.quote(&item.related_table),
                self.dialect.quote(l_table),
                self.dialect.quote(l_col),
                self.dialect.quote(r_table),
                self.dialect.quote(r_col),
            ));
        }

        if let Some(w) = main_where {
            sql.push_str(&format!(" WHERE {}", w));
        }
        sql
    }

    /// 获取主表 SQL
    pub fn main_sql(&self) -> String {
        let base = format!("SELECT * FROM {}", self.dialect.quote(&self.main_table));
        if let Some(w) = &self.main_where {
            format!("{} WHERE {}", base, w)
        } else {
            base
        }
    }

    /// 获取指定关联表的 SQL（默认占位符 `?`）
    pub fn related_sql(&self, name: &str) -> Option<String> {
        let (_, item) = self.relations.iter().find(|(n, _)| *n == name)?;
        let foreign_key = match &item.kind {
            WithRelationKind::HasMany { foreign_key, .. }
            | WithRelationKind::HasOne { foreign_key, .. }
            | WithRelationKind::BelongsTo { foreign_key, .. } => foreign_key.clone(),
        };
        // 默认使用 ? 占位符，调用方应在执行时绑定具体 ID
        Some(format!(
            "SELECT * FROM {} WHERE {} IN (?)",
            self.dialect.quote(&item.related_table),
            self.dialect.quote(&foreign_key),
        ))
    }

    /// 获取指定关联表 SQL，使用具体的 ID 列表
    ///
    /// 接受任意可迭代且元素可 `ToString` 的输入（`&[i64]`、`Vec<String>`、`["a", "b"]` 等）。
    ///
    /// # 安全性（v0.2.2 修复 C-5）
    ///
    /// 每个 id 经 `sql_safety::validate_id_value` 严格校验，仅允许字母数字+下划线+减号，
    /// 杜绝通过 id 拼接 SQL 注入。
    pub fn related_sql_with_ids(
        &self,
        name: &str,
        ids: impl IntoIterator<Item = impl ToString>,
    ) -> Option<String> {
        let (_, item) = self.relations.iter().find(|(n, _)| *n == name)?;
        let foreign_key = match &item.kind {
            WithRelationKind::HasMany { foreign_key, .. }
            | WithRelationKind::HasOne { foreign_key, .. }
            | WithRelationKind::BelongsTo { foreign_key, .. } => foreign_key.clone(),
        };
        // v0.2.2 修复 C-5：每个 id 必须通过严格校验，拒绝 SQL 注入
        let ids_str = ids
            .into_iter()
            .map(|v| {
                let s = v.to_string();
                crate::sql_safety::validate_id_value(&s)
                    .expect("invalid id value in related_sql_with_ids");
                s
            })
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "SELECT * FROM {} WHERE {} IN ({})",
            self.dialect.quote(&item.related_table),
            self.dialect.quote(&foreign_key),
            ids_str,
        ))
    }

    /// 获取所有已注册的关联名
    pub fn relation_names(&self) -> Vec<&str> {
        self.relations.iter().map(|(n, _)| *n).collect()
    }
}

/// 将 `table.column` 拆分为 `(table, column)`
fn split_qualified(s: &str) -> (&str, &str) {
    match s.rfind('.') {
        Some(idx) => (&s[..idx], &s[idx + 1..]),
        None => (s, ""),
    }
}

/// 校验 SQL 标识符（表名/列名）是否合法
///
/// H-2 修复：find_with_related 中的所有表名/列名拼接前必须校验，防止 SQL 注入。
/// 校验规则与 `crate::model::is_valid_sql_identifier` 一致。
fn is_valid_sql_identifier(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 批量校验 find_with_related 中的 SQL 标识符
fn validate_find_identifiers(idents: &[&str]) -> Result<(), String> {
    for ident in idents {
        if !is_valid_sql_identifier(ident) {
            return Err(format!(
                "invalid SQL identifier in find_with_related (potential SQL injection): {}",
                ident
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_type::DbType;
    use crate::dialect::get_dialect;
    use crate::model::{BelongsTo, BelongsToMany, HasMany, HasOne, MorphMany, MorphTo};

    fn mysql_dialect() -> Box<dyn Dialect> {
        get_dialect(DbType::MySQL).expect("MySQL dialect")
    }

    fn pg_dialect() -> Box<dyn Dialect> {
        get_dialect(DbType::PostgreSQL).expect("PG dialect")
    }

    fn sqlite_dialect() -> Box<dyn Dialect> {
        get_dialect(DbType::Sqlite).expect("SQLite dialect")
    }

    #[test]
    fn join_left_basic() {
        let d = mysql_dialect();
        let sql = FindWithRelated::new(&*d, "users", "profiles", "user_id", "id", true).build();
        assert!(sql.contains("SELECT `users`.*, `profiles`.*"));
        assert!(sql.contains("FROM `users`"));
        assert!(sql.contains("LEFT JOIN `profiles`"));
        assert!(sql.contains("ON `profiles`.`user_id` = `users`.`id`"));
    }

    #[test]
    fn join_inner_basic() {
        let d = mysql_dialect();
        let sql = FindWithRelated::new(&*d, "users", "orders", "user_id", "id", false).build();
        assert!(sql.contains("INNER JOIN `orders`"));
        assert!(!sql.contains("LEFT JOIN"));
    }

    #[test]
    fn join_with_where_order_limit() {
        let d = mysql_dialect();
        let sql = FindWithRelated::new(&*d, "users", "orders", "user_id", "id", true)
            .where_cond("users.status = 'active'")
            .where_cond("orders.amount > 100")
            .order_desc("orders.created_at")
            .limit(10)
            .offset(20)
            .build();
        assert!(sql.contains("WHERE users.status = 'active' AND orders.amount > 100"));
        assert!(sql.contains("ORDER BY `orders.created_at` DESC"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn join_pg_dialect() {
        let d = pg_dialect();
        let sql = FindWithRelated::new(&*d, "users", "orders", "user_id", "id", true).build();
        assert!(sql.contains("SELECT \"users\".*, \"orders\".*"));
        assert!(sql.contains("LEFT JOIN \"orders\""));
        assert!(sql.contains("ON \"orders\".\"user_id\" = \"users\".\"id\""));
    }

    #[test]
    fn join_sqlite_dialect() {
        let d = sqlite_dialect();
        let sql = FindWithRelated::new(&*d, "users", "orders", "user_id", "id", true).build();
        // SQLite 方言使用双引号（与 PG 类似）
        assert!(sql.contains("LEFT JOIN \"orders\""));
    }

    #[test]
    fn eager_sql_basic() {
        let d = mysql_dialect();
        let (main_sql, related_sql) =
            find_with_related_eager_sql(&*d, "users", "orders", "user_id", Some("users.id > 0"));
        assert_eq!(main_sql, "SELECT * FROM `users` WHERE users.id > 0");
        assert_eq!(related_sql, "SELECT * FROM `orders` WHERE `user_id` IN (?)");
    }

    #[test]
    fn eager_sql_no_where() {
        let d = mysql_dialect();
        let (main_sql, related_sql) =
            find_with_related_eager_sql(&*d, "users", "orders", "user_id", None);
        assert_eq!(main_sql, "SELECT * FROM `users`");
        assert_eq!(related_sql, "SELECT * FROM `orders` WHERE `user_id` IN (?)");
    }

    #[test]
    fn subquery_basic() {
        let d = mysql_dialect();
        let sql = find_with_related_subquery(
            &*d,
            "users",
            "orders",
            "user_id",
            "id",
            Some("orders.amount > 100"),
        );
        assert_eq!(
            sql,
            "SELECT * FROM `users` WHERE `id` IN (SELECT `user_id` FROM `orders` WHERE orders.amount > 100)"
        );
    }

    #[test]
    fn subquery_no_where() {
        let d = mysql_dialect();
        let sql = find_with_related_subquery(&*d, "users", "orders", "user_id", "id", None);
        assert_eq!(
            sql,
            "SELECT * FROM `users` WHERE `id` IN (SELECT `user_id` FROM `orders`)"
        );
    }

    #[test]
    fn inspect_relation_has_many() {
        let mut rels = HashMap::new();
        rels.insert(
            "orders",
            Relation::HasMany(HasMany {
                foreign_key: "user_id".to_string(),
                child_model: "orders".to_string(),
                child_pk: "id".to_string(),
            }),
        );
        let info = inspect_relation(&rels, "orders").expect("relation exists");
        assert_eq!(info.0, "orders");
        assert_eq!(info.1, "user_id");
        assert_eq!(info.2, "id");
        assert!(info.3, "HasMany 应标记为 is_many=true");
    }

    #[test]
    fn inspect_relation_has_one() {
        let mut rels = HashMap::new();
        rels.insert(
            "profile",
            Relation::HasOne(HasOne {
                foreign_key: "user_id".to_string(),
                child_model: "profiles".to_string(),
                child_pk: "id".to_string(),
            }),
        );
        let info = inspect_relation(&rels, "profile").expect("relation exists");
        assert_eq!(info.0, "profiles");
        assert!(!info.3, "HasOne 应标记为 is_many=false");
    }

    #[test]
    fn inspect_relation_belongs_to() {
        let mut rels = HashMap::new();
        rels.insert(
            "user",
            Relation::BelongsTo(BelongsTo {
                foreign_key: "user_id".to_string(),
                parent_model: "users".to_string(),
                parent_pk: "id".to_string(),
            }),
        );
        let info = inspect_relation(&rels, "user").expect("relation exists");
        assert_eq!(info.0, "users");
        assert!(!info.3, "BelongsTo 应标记为 is_many=false");
    }

    #[test]
    fn inspect_relation_belongs_to_many() {
        let mut rels = HashMap::new();
        rels.insert(
            "roles",
            Relation::BelongsToMany(BelongsToMany {
                junction_table: "user_role".to_string(),
                foreign_key: "user_id".to_string(),
                other_key: "role_id".to_string(),
                target_model: "roles".to_string(),
                target_pk: "id".to_string(),
            }),
        );
        let info = inspect_relation(&rels, "roles").expect("relation exists");
        assert_eq!(info.0, "roles");
        assert!(info.3, "BelongsToMany 应标记为 is_many=true");
    }

    #[test]
    fn inspect_relation_morph_many() {
        let mut rels = HashMap::new();
        rels.insert(
            "comments",
            Relation::MorphMany(MorphMany {
                child_model: "comments".to_string(),
                morph_type_column: "commentable_type".to_string(),
                morph_id_column: "commentable_id".to_string(),
                morph_type_value: "Post".to_string(),
            }),
        );
        let info = inspect_relation(&rels, "comments").expect("relation exists");
        assert_eq!(info.0, "comments");
        assert_eq!(info.1, "commentable_id");
        assert!(info.3, "MorphMany 应标记为 is_many=true");
    }

    #[test]
    fn inspect_relation_morph_to() {
        let mut rels = HashMap::new();
        rels.insert(
            "commentable",
            Relation::MorphTo(MorphTo {
                morph_type_column: "commentable_type".to_string(),
                morph_id_column: "commentable_id".to_string(),
            }),
        );
        let info = inspect_relation(&rels, "commentable").expect("relation exists");
        assert_eq!(info.1, "commentable_id");
        assert!(!info.3, "MorphTo 应标记为 is_many=false");
    }

    #[test]
    fn inspect_relation_not_found() {
        let rels = HashMap::<&str, Relation>::new();
        assert!(inspect_relation(&rels, "nonexistent").is_none());
    }

    // ===== 边界测试 =====

    #[test]
    fn empty_where_produces_no_where_clause() {
        let d = mysql_dialect();
        let sql = FindWithRelated::new(&*d, "users", "orders", "user_id", "id", true).build();
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn multiple_order_by() {
        let d = mysql_dialect();
        let sql = FindWithRelated::new(&*d, "users", "orders", "user_id", "id", true)
            .order_by("users.id")
            .order_desc("orders.created_at")
            .build();
        assert!(sql.contains("ORDER BY `users.id`, `orders.created_at` DESC"));
    }

    #[test]
    fn find_with_related_join_convenience_fn() {
        let d = mysql_dialect();
        let sql = find_with_related_join(&*d, "users", "orders", "user_id", "id", true)
            .where_cond("users.id = 1")
            .build();
        assert!(sql.contains("WHERE users.id = 1"));
    }

    // ---- WithRelation::load 风格 API 测试 ----

    #[test]
    fn with_relation_load_has_many_eager() {
        let d = mysql_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .load_eager(Some("users.id IN (1, 2, 3)"));
        assert_eq!(
            loader.main_sql(),
            "SELECT * FROM `users` WHERE users.id IN (1, 2, 3)"
        );
        // related_sql 默认用 ? 占位符；调用方应在执行时绑定具体 ID 列表
        assert_eq!(
            loader.related_sql("orders").unwrap(),
            "SELECT * FROM `orders` WHERE `user_id` IN (?)"
        );
        // related_sql_with_ids 可传入具体 ID 列表（支持 i64/String/&str）
        assert_eq!(
            loader
                .related_sql_with_ids("orders", [1_i64, 2, 3])
                .unwrap(),
            "SELECT * FROM `orders` WHERE `user_id` IN (1, 2, 3)"
        );
        assert_eq!(
            loader
                .related_sql_with_ids("orders", ["a", "b", "c"])
                .unwrap(),
            "SELECT * FROM `orders` WHERE `user_id` IN (a, b, c)"
        );
    }

    #[test]
    fn with_relation_load_has_one_join() {
        let d = mysql_dialect();
        let sql = WithRelation::new(&*d, "users")
            .with_has_one("profiles", "user_id", "id")
            .load_join(Some("users.id = 1"));
        assert!(sql.contains("LEFT JOIN `profiles`"));
        assert!(sql.contains("ON `profiles`.`user_id` = `users`.`id`"));
        assert!(sql.contains("WHERE users.id = 1"));
    }

    #[test]
    fn with_relation_load_belongs_to_join() {
        let d = mysql_dialect();
        let sql = WithRelation::new(&*d, "orders")
            .with_belongs_to("users", "user_id", "id")
            .load_join(None);
        // BelongsTo: orders.user_id → users.id（INNER JOIN）
        // JOIN 条件方向：main.fk = related.pk（语义直观）
        assert!(sql.contains("INNER JOIN `users`"));
        assert!(sql.contains("ON `orders`.`user_id` = `users`.`id`"));
    }

    #[test]
    fn with_relation_multiple_relations_eager() {
        let d = mysql_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .with_has_many("posts", "author_id", "id")
            .with_has_one("profiles", "user_id", "id")
            .load_eager(None);
        assert_eq!(loader.main_sql(), "SELECT * FROM `users`");
        // 三个关联都应该有对应的 SQL
        assert!(loader.related_sql("orders").is_some());
        assert!(loader.related_sql("posts").is_some());
        assert!(loader.related_sql("profiles").is_some());
        // 不存在的关联应返回 None
        assert!(loader.related_sql("nonexistent").is_none());
    }

    #[test]
    fn with_relation_load_eager_with_specific_ids() {
        let d = mysql_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .load_eager(None);
        // 自定义 ID 列表（i64 数组直接传入，泛型自动推断）
        let orders_sql = loader
            .related_sql_with_ids("orders", [1_i64, 5, 10])
            .unwrap();
        assert_eq!(
            orders_sql,
            "SELECT * FROM `orders` WHERE `user_id` IN (1, 5, 10)"
        );
    }

    #[test]
    fn with_relation_pg_dialect_eager() {
        let d = pg_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .load_eager(Some("users.id > 100"));
        assert_eq!(
            loader.main_sql(),
            "SELECT * FROM \"users\" WHERE users.id > 100"
        );
        // 默认占位符 ?，调用方应替换为实际 ID
        assert_eq!(
            loader.related_sql("orders").unwrap(),
            "SELECT * FROM \"orders\" WHERE \"user_id\" IN (?)"
        );
        // 使用具体 ID 列表（PG 方言）
        assert_eq!(
            loader.related_sql_with_ids("orders", [100_i64]).unwrap(),
            "SELECT * FROM \"orders\" WHERE \"user_id\" IN (100)"
        );
    }

    #[test]
    fn with_relation_sqlite_dialect_join() {
        let d = sqlite_dialect();
        let sql = WithRelation::new(&*d, "users")
            .with_has_one("profiles", "user_id", "id")
            .load_join(None);
        assert!(sql.contains("LEFT JOIN \"profiles\""));
    }

    // v0.2.2 修复 C-5：SQL 注入测试套件

    #[test]
    #[should_panic(expected = "invalid id value")]
    fn with_relation_rejects_sql_injection_in_id_semicolon() {
        let d = mysql_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .load_eager(None);
        let _ = loader.related_sql_with_ids("orders", ["1; DROP TABLE users"]);
    }

    #[test]
    #[should_panic(expected = "invalid id value")]
    fn with_relation_rejects_sql_injection_in_id_or() {
        let d = mysql_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .load_eager(None);
        let _ = loader.related_sql_with_ids("orders", ["1) OR 1=1"]);
    }

    #[test]
    #[should_panic(expected = "invalid id value")]
    fn with_relation_rejects_sql_injection_in_id_quote() {
        let d = mysql_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .load_eager(None);
        let _ = loader.related_sql_with_ids("orders", ["' OR '1'='1"]);
    }

    #[test]
    #[should_panic(expected = "invalid id value")]
    fn with_relation_rejects_sql_injection_in_id_comment() {
        let d = mysql_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .load_eager(None);
        let _ = loader.related_sql_with_ids("orders", ["1--"]);
    }

    #[test]
    #[should_panic(expected = "invalid id value")]
    fn with_relation_rejects_sql_injection_in_id_with_space() {
        let d = mysql_dialect();
        let loader = WithRelation::new(&*d, "users")
            .with_has_many("orders", "user_id", "id")
            .load_eager(None);
        let _ = loader.related_sql_with_ids("orders", ["1 2"]);
    }
}
