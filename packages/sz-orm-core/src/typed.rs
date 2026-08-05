//! 强类型 AST 支持模块
//!
//! 为 `typed_query!` 宏生成的代码提供 trait 基础。
//!
//! # 设计
//!
//! `typed_query!` 宏在编译期为每张表生成：
//! - 一个 `table` 标记类型，实现 [`TypedTable`]
//! - 每列一个零大小标记类型，实现 [`TypedColumn`]
//!
//! 这样可以把列名 + Rust 类型提升到类型系统，
//! 让 SQL 列名错误在编译期被捕获，而非运行时。
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_macros::typed_query;
//! use sz_orm_core::typed::{TypedTable, TypedColumn};
//!
//! // 1. 声明表 schema
//! typed_query! {
//!     table users {
//!         id: i64,
//!         name: String,
//!     }
//! }
//!
//! // 2. 通过标记类型引用列名（类型安全）
//! fn select_id() -> &'static str {
//!     <users::col_id as TypedColumn>::NAME
//! }
//! ```

/// 类型安全的表标记 trait
///
/// 由 `typed_query!(table <name> { ... })` 宏生成的 `table` 结构体实现。
///
/// # 实现约定
///
/// - `NAME` 为表名（与数据库表名一致）
/// - 实现者应为零大小类型（unit struct）
pub trait TypedTable: 'static {
    /// 表名
    const NAME: &'static str;
}

/// 类型安全的列标记 trait
///
/// 由 `typed_query!` 宏为每列生成的 `col_<name>` 结构体实现。
///
/// # 实现约定
///
/// - `NAME` 为列名（与数据库列名一致）
/// - `Table` 为该列所属的表标记类型
/// - `RustType` 为该列映射到的 Rust 类型（如 `i64`、`String`）
/// - `SqlType` 为该列对应的 SQL 类型标记（默认 `Untyped`，宏生成的列使用默认值）
/// - 实现者应为零大小类型（unit struct）
pub trait TypedColumn: 'static {
    /// 列名
    const NAME: &'static str;

    /// 该列所属的表
    type Table: TypedTable;

    /// 该列对应的 Rust 类型
    type RustType;

    /// 该列对应的 SQL 类型标记
    ///
    /// 宏生成的列默认使用 `crate::typed_ast::Untyped`。
    /// 需要强类型 SQL 检查的场景（如 `typed_ast`）应显式指定此类型。
    type SqlType: crate::typed_ast::SqlType;
}

/// 在编译期查找表 schema 常量
///
/// `typed_query!` 宏在生成表声明时，会同时生成一个常量
/// `__SZ_ORM_TYPED_SCHEMA_<TABLE>: &[(&str, &str)]`，
/// 包含该表所有列的 `(列名, 类型名)` 对。
///
/// 此函数在 SELECT 表达式分支中被宏自动调用，
/// 用于在编译期校验 SELECT 列名是否存在于表声明中。
///
/// # 注意
///
/// 这是一个常量函数（const fn），但实际上宏生成的代码会直接
/// 通过 `const` 上下文进行列名匹配，不会真正调用此函数。
/// 此函数仅作为占位，便于文档说明和未来扩展。
pub const fn _schema_lookup(_schema: &[(&str, &str)], _col: &str) -> Option<&'static str> {
    // 编译期 schema 查找由宏直接展开为 `match` 表达式完成
    // 此函数仅作为类型签名参考保留
    None
}

// ========================================================================
// M3 修复：多列类型安全 SELECT 宏
// ========================================================================

/// 类型安全多列 SELECT 宏（M3 验收标准）
///
/// 解决 Rust 无法将异构类型放入同一数组的限制，
/// 允许一次性指定多个 `TypedColumn` 类型作为 SELECT 列。
///
/// # 用法
///
/// ```ignore
/// use sz_orm_core::select_typed;
/// use sz_orm_core::typed::{TypedColumn, TypedTable};
///
/// // 假设已定义 UserColId / UserColName / UserColEmail
/// let (sql, params) = select_typed!(query, UserColId, UserColName, UserColEmail)
///     .build_select_with_params();
/// // 生成：SELECT id, name, email FROM users
/// ```
///
/// # 编译期保证
///
/// 每个参数必须是实现了 `TypedColumn` 的类型。
/// 若传入非 `TypedColumn` 类型，编译期即报错。
///
/// # 与 `select_typed` 方法的关系
///
/// `QueryBuilder::select_typed::<Col>()` 是单列版本，需链式调用：
/// ```ignore
/// query.select_typed::<Col1>().select_typed::<Col2>().select_typed::<Col3>()
/// ```
/// 本宏将其简化为：
/// ```ignore
/// select_typed!(query, Col1, Col2, Col3)
/// ```
#[macro_export]
macro_rules! select_typed {
    ($query:expr, $col:ty) => {
        $query.select_typed::<$col>()
    };
    ($query:expr, $col:ty, $($cols:ty),+) => {
        $crate::select_typed!($query.select_typed::<$col>(), $($cols),+)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 测试用的 mock 类型 ----

    struct MockUsersTable;
    impl TypedTable for MockUsersTable {
        const NAME: &'static str = "users";
    }

    struct MockColId;
    impl TypedColumn for MockColId {
        const NAME: &'static str = "id";
        type Table = MockUsersTable;
        type RustType = i64;
        type SqlType = crate::typed_ast::Untyped;
    }

    struct MockColName;
    impl TypedColumn for MockColName {
        const NAME: &'static str = "name";
        type Table = MockUsersTable;
        type RustType = String;
        type SqlType = crate::typed_ast::Untyped;
    }

    // ---- 测试 ----

    #[test]
    fn test_typed_table_name() {
        assert_eq!(MockUsersTable::NAME, "users");
    }

    #[test]
    fn test_typed_column_name() {
        assert_eq!(MockColId::NAME, "id");
        assert_eq!(MockColName::NAME, "name");
    }

    #[test]
    fn test_typed_column_table_association() {
        // 编译期校验：col_id 属于 users 表
        fn _assert_table<T: TypedColumn<Table = MockUsersTable>>(_: T) {}
        _assert_table(MockColId);
        _assert_table(MockColName);
    }

    #[test]
    fn test_typed_column_rust_type() {
        // 编译期校验：col_id 的 Rust 类型是 i64
        fn _assert_type<T: TypedColumn<RustType = i64>>(_: T) {}
        _assert_type(MockColId);

        // col_name 的 Rust 类型是 String
        fn _assert_string_type<T: TypedColumn<RustType = String>>(_: T) {}
        _assert_string_type(MockColName);
    }

    #[test]
    fn test_zero_sized_types() {
        // 标记类型应为零大小
        assert_eq!(std::mem::size_of::<MockUsersTable>(), 0);
        assert_eq!(std::mem::size_of::<MockColId>(), 0);
        assert_eq!(std::mem::size_of::<MockColName>(), 0);
    }

    #[test]
    fn test_schema_lookup_returns_none() {
        // 占位函数总是返回 None（实际查找由宏展开完成）
        let schema: &[(&str, &str)] = &[("id", "i64"), ("name", "String")];
        assert_eq!(_schema_lookup(schema, "id"), None);
    }

    // ---- M3 宏测试 ----

    use crate::db_type::DbType;
    use crate::dialect::get_dialect;

    /// Mock Model for macro tests (uses "users" table)
    #[derive(Debug, Clone, Default)]
    struct MockModel;
    impl crate::model::Model for MockModel {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "users"
        }
        fn pk_name() -> &'static str {
            "id"
        }
        fn pk(&self) -> Self::PrimaryKey {
            0
        }
        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
        fn timestamp_fields() -> Option<crate::model::TimestampFields> {
            None
        }
        fn soft_delete_field() -> Option<&'static str> {
            None
        }
    }

    struct MockColEmail;
    impl TypedColumn for MockColEmail {
        const NAME: &'static str = "email";
        type Table = MockUsersTable;
        type RustType = String;
        type SqlType = crate::typed_ast::Untyped;
    }

    #[test]
    fn test_select_typed_macro_single_column() {
        // 单列：select_typed!(q, Col) → q.select_typed::<Col>()
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let q = crate::QueryBuilder::<MockModel>::new(dialect);
        let (sql, _params) =
            select_typed!(q.select(Vec::new()), MockColId).build_select_with_params();
        assert_eq!(sql, "SELECT id FROM `users`");
    }

    #[test]
    fn test_select_typed_macro_multi_column() {
        // 多列：select_typed!(q, Col1, Col2) → q.select_typed::<Col1>().select_typed::<Col2>()
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let q = crate::QueryBuilder::<MockModel>::new(dialect);
        let (sql, _params) =
            select_typed!(q.select(Vec::new()), MockColId, MockColName).build_select_with_params();
        assert_eq!(sql, "SELECT id, name FROM `users`");
    }

    #[test]
    fn test_select_typed_macro_three_columns() {
        // 三列宏展开
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let q = crate::QueryBuilder::<MockModel>::new(dialect);
        let (sql, _params) =
            select_typed!(q.select(Vec::new()), MockColId, MockColName, MockColEmail)
                .build_select_with_params();
        assert_eq!(sql, "SELECT id, name, email FROM `users`");
    }
}
