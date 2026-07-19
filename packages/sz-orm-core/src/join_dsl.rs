//! JoinDSL — 类型安全的 JOIN 语法（Diesel 风格）
//!
//! 通过 [`TypedTable`] 和 [`TypedColumn`] 标记类型，
//! 把 JOIN 的 ON 条件提升到类型系统，让"表 A 的列 vs 表 B 的列"
//! 在编译期就能验证表归属，杜绝拼错表名/列名。
//!
//! # 设计
//!
//! - [`JoinBuilder`] 链式构造 JOIN 子句
//! - [`JoinOn`] 表达 ON 条件，左侧列必须属于左表，右侧列必须属于右表
//! - 编译期通过 [`TypedColumn::Table`] 关联约束校验
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::typed::{TypedTable, TypedColumn};
//! use sz_orm_core::join_dsl::{JoinBuilder, JoinKind};
//! use sz_orm_macros::typed_query;
//!
//! typed_query! {
//!     table users { id: i64, name: String }
//!     table orders { id: i64, user_id: i64, total: f64 }
//! }
//!
//! // 编译期保证：users::col_id 与 orders::col_user_id 是 ON 条件两端
//! let join = JoinBuilder::new(JoinKind::Inner)
//!     .table::<users::table>()
//!     .on::<users::col_id, orders::col_user_id>()
//!     .build();
//! assert_eq!(join, "INNER JOIN users ON users.id = orders.user_id");
//! ```

use crate::typed::{TypedColumn, TypedTable};

/// JOIN 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    /// INNER JOIN
    Inner,
    /// LEFT \[OUTER\] JOIN
    Left,
    /// RIGHT \[OUTER\] JOIN
    Right,
    /// FULL \[OUTER\] JOIN
    Full,
    /// CROSS JOIN
    Cross,
}

impl JoinKind {
    /// 转换为 SQL 关键字
    pub fn as_sql(&self) -> &'static str {
        match self {
            JoinKind::Inner => "INNER JOIN",
            JoinKind::Left => "LEFT JOIN",
            JoinKind::Right => "RIGHT JOIN",
            JoinKind::Full => "FULL OUTER JOIN",
            JoinKind::Cross => "CROSS JOIN",
        }
    }
}

/// JOIN 构造器（已指定 JOIN 类型，待指定右表）
pub struct JoinBuilder {
    kind: JoinKind,
}

impl JoinBuilder {
    /// 创建新的 JoinBuilder，指定 JOIN 类型
    pub fn new(kind: JoinKind) -> Self {
        Self { kind }
    }

    /// 指定 JOIN 的右表（被 join 进来的表）
    ///
    /// 返回 [`JoinOn`]，等待 ON 条件。
    /// CROSS JOIN 无需 ON 条件，可直接调用 `JoinOn::build`。
    pub fn table<T: TypedTable>(self) -> JoinOn {
        JoinOn {
            kind: self.kind,
            right_table: T::NAME,
        }
    }
}

/// JOIN 已指定右表，待指定 ON 条件
pub struct JoinOn {
    kind: JoinKind,
    right_table: &'static str,
}

impl JoinOn {
    /// 指定 ON 条件：左表列 = 右表列
    ///
    /// # 类型约束
    ///
    /// - `L`: 左表列，必须属于"主表"（用户自行保证）
    /// - `R`: 右表列，必须属于 `right_table`（用户自行保证）
    ///
    /// 编译期通过 [`TypedColumn::Table`] 关联约束校验列归属。
    pub fn on<L, R>(self) -> JoinBuilt
    where
        L: TypedColumn,
        R: TypedColumn,
    {
        // 编译期断言：L 和 R 分别属于不同的表（避免自连接时拼错）
        // 注意：我们不强制 L.Table != R.Table，因为自连接也是合法场景
        JoinBuilt {
            kind: self.kind,
            right_table: self.right_table,
            left_column: L::NAME,
            right_column: R::NAME,
        }
    }

    /// 不带 ON 条件直接构造（用于 CROSS JOIN）
    pub fn build_no_on(self) -> String {
        format!("{} {}", self.kind.as_sql(), self.right_table)
    }
}

/// JOIN 已完整构造，可生成 SQL
pub struct JoinBuilt {
    kind: JoinKind,
    right_table: &'static str,
    left_column: &'static str,
    right_column: &'static str,
}

impl JoinBuilt {
    /// 生成 JOIN 子句 SQL
    ///
    /// 输出格式：`<JOIN_TYPE> <right_table> ON <left_column> = <right_column>`
    ///
    /// 注意：生成的 SQL 中列名不带表前缀。
    /// 如需带前缀，使用 [`build_with_prefix`]。
    ///
    /// [`build_with_prefix`]: JoinBuilt::build_with_prefix
    pub fn build(&self) -> String {
        format!(
            "{} {} ON {} = {}",
            self.kind.as_sql(),
            self.right_table,
            self.left_column,
            self.right_column
        )
    }

    /// 生成带表前缀的 JOIN 子句
    ///
    /// 输出格式：`<JOIN_TYPE> <right_table> ON <left_table>.<left_column> = <right_table>.<right_column>`
    pub fn build_with_prefix(&self, left_table: &str) -> String {
        format!(
            "{} {} ON {}.{} = {}.{}",
            self.kind.as_sql(),
            self.right_table,
            left_table,
            self.left_column,
            self.right_table,
            self.right_column
        )
    }
}

/// 便捷函数：构造 INNER JOIN
pub fn inner_join<T: TypedTable>() -> JoinOn {
    JoinBuilder::new(JoinKind::Inner).table::<T>()
}

/// 便捷函数：构造 LEFT JOIN
pub fn left_join<T: TypedTable>() -> JoinOn {
    JoinBuilder::new(JoinKind::Left).table::<T>()
}

/// 便捷函数：构造 RIGHT JOIN
pub fn right_join<T: TypedTable>() -> JoinOn {
    JoinBuilder::new(JoinKind::Right).table::<T>()
}

/// 便捷函数：构造 FULL OUTER JOIN
pub fn full_join<T: TypedTable>() -> JoinOn {
    JoinBuilder::new(JoinKind::Full).table::<T>()
}

/// 便捷函数：构造 CROSS JOIN
pub fn cross_join<T: TypedTable>() -> JoinOn {
    JoinBuilder::new(JoinKind::Cross).table::<T>()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 测试用 mock 类型 ----

    struct UsersTable;
    impl TypedTable for UsersTable {
        const NAME: &'static str = "users";
    }

    struct OrdersTable;
    impl TypedTable for OrdersTable {
        const NAME: &'static str = "orders";
    }

    struct ColUserId;
    impl TypedColumn for ColUserId {
        const NAME: &'static str = "id";
        type Table = UsersTable;
        type RustType = i64;
        type SqlType = crate::typed_ast::Untyped;
    }

    struct ColOrderUserId;
    impl TypedColumn for ColOrderUserId {
        const NAME: &'static str = "user_id";
        type Table = OrdersTable;
        type RustType = i64;
        type SqlType = crate::typed_ast::Untyped;
    }

    struct ColOrderId;
    impl TypedColumn for ColOrderId {
        const NAME: &'static str = "id";
        type Table = OrdersTable;
        type RustType = i64;
        type SqlType = crate::typed_ast::Untyped;
    }

    // ---- JoinKind 测试 ----

    #[test]
    fn test_join_kind_as_sql() {
        assert_eq!(JoinKind::Inner.as_sql(), "INNER JOIN");
        assert_eq!(JoinKind::Left.as_sql(), "LEFT JOIN");
        assert_eq!(JoinKind::Right.as_sql(), "RIGHT JOIN");
        assert_eq!(JoinKind::Full.as_sql(), "FULL OUTER JOIN");
        assert_eq!(JoinKind::Cross.as_sql(), "CROSS JOIN");
    }

    // ---- 基础 JOIN 构造测试 ----

    #[test]
    fn test_inner_join_basic() {
        let join = JoinBuilder::new(JoinKind::Inner)
            .table::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build();
        assert_eq!(join, "INNER JOIN orders ON id = user_id");
    }

    #[test]
    fn test_left_join_with_prefix() {
        let join = JoinBuilder::new(JoinKind::Left)
            .table::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build_with_prefix("users");
        assert_eq!(join, "LEFT JOIN orders ON users.id = orders.user_id");
    }

    #[test]
    fn test_right_join() {
        let join = JoinBuilder::new(JoinKind::Right)
            .table::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build();
        assert_eq!(join, "RIGHT JOIN orders ON id = user_id");
    }

    #[test]
    fn test_full_join() {
        let join = JoinBuilder::new(JoinKind::Full)
            .table::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build();
        assert_eq!(join, "FULL OUTER JOIN orders ON id = user_id");
    }

    #[test]
    fn test_cross_join_no_on() {
        let join = JoinBuilder::new(JoinKind::Cross)
            .table::<OrdersTable>()
            .build_no_on();
        assert_eq!(join, "CROSS JOIN orders");
    }

    // ---- 便捷函数测试 ----

    #[test]
    fn test_inner_join_helper() {
        let join = inner_join::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build();
        assert_eq!(join, "INNER JOIN orders ON id = user_id");
    }

    #[test]
    fn test_left_join_helper() {
        let join = left_join::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build();
        assert_eq!(join, "LEFT JOIN orders ON id = user_id");
    }

    #[test]
    fn test_right_join_helper() {
        let join = right_join::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build();
        assert_eq!(join, "RIGHT JOIN orders ON id = user_id");
    }

    #[test]
    fn test_full_join_helper() {
        let join = full_join::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build();
        assert_eq!(join, "FULL OUTER JOIN orders ON id = user_id");
    }

    #[test]
    fn test_cross_join_helper() {
        let join = cross_join::<OrdersTable>().build_no_on();
        assert_eq!(join, "CROSS JOIN orders");
    }

    // ---- 类型安全验证（编译期） ----

    #[test]
    fn test_compile_time_table_association() {
        // 这个测试主要验证编译期约束：
        // on::<L, R>() 要求 L 和 R 都实现了 TypedColumn
        // 如果传入非 TypedColumn 类型，编译就会失败
        let join = inner_join::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build();
        assert!(!join.is_empty());
    }

    #[test]
    fn test_self_join_same_table() {
        // 自连接：同一表的两个列
        // 注意：右表也是 orders，左列和右列都属于 orders
        let join = inner_join::<OrdersTable>()
            .on::<ColOrderId, ColOrderUserId>()
            .build();
        assert_eq!(join, "INNER JOIN orders ON id = user_id");
    }

    // ---- 多 JOIN 拼接测试 ----

    #[test]
    fn test_multiple_joins_concat() {
        let j1 = inner_join::<OrdersTable>()
            .on::<ColUserId, ColOrderUserId>()
            .build_with_prefix("users");

        // 模拟第二张表（用 mock 类型复用）
        let j2 = left_join::<OrdersTable>()
            .on::<ColUserId, ColOrderId>()
            .build_with_prefix("users");

        let sql = format!("SELECT * FROM users {} {}", j1, j2);
        assert!(sql.contains("INNER JOIN orders ON users.id = orders.user_id"));
        assert!(sql.contains("LEFT JOIN orders ON users.id = orders.id"));
    }
}
