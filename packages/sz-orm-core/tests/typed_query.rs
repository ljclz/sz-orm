//! typed_query! 宏集成测试
//!
//! 这些测试在 `tests/` 目录下，将 `sz-orm-core` 视为外部依赖，
//! 因此宏生成的 `::sz_orm_core::typed::...` 路径可以正确解析。

use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_query;

// ============================================================================
// 表声明测试
// ============================================================================

/// 表声明应生成 table 标记类型 + 每列 col_* 标记类型
#[test]
fn test_typed_query_table_decl() {
    typed_query! {
        table typed_users {
            id: i64,
            name: String,
            email: String,
        }
    }

    // 验证生成的表名常量
    assert_eq!(<typed_users::table as TypedTable>::NAME, "typed_users");

    // 验证生成的列名常量
    assert_eq!(<typed_users::col_id as TypedColumn>::NAME, "id");
    assert_eq!(<typed_users::col_name as TypedColumn>::NAME, "name");
    assert_eq!(<typed_users::col_email as TypedColumn>::NAME, "email");

    // 编译期校验：列的 Table 关联类型正确
    fn _check<T: TypedColumn<Table = typed_users::table>>(_: T) {}
    _check(typed_users::col_id);
    _check(typed_users::col_name);
    _check(typed_users::col_email);

    // 编译期校验：RustType 与声明一致
    fn _check_i64<T: TypedColumn<RustType = i64>>(_: T) {}
    fn _check_string<T: TypedColumn<RustType = String>>(_: T) {}
    _check_i64(typed_users::col_id);
    _check_string(typed_users::col_name);
    _check_string(typed_users::col_email);
}

/// 表声明的内部 schema 常量应可访问
#[test]
fn test_typed_query_schema_const() {
    typed_query! {
        table typed_orders {
            order_id: i64,
            user_id: i64,
            total: f64,
        }
    }
    // 宏生成的 schema 常量
    assert_eq!(
        __SZ_ORM_TYPED_SCHEMA_TYPED_ORDERS,
        &[("order_id", "i64"), ("user_id", "i64"), ("total", "f64")]
    );
}

/// 表声明支持 Option<T> 等泛型类型
#[test]
fn test_typed_query_generic_types() {
    typed_query! {
        table typed_products {
            id: i64,
            name: String,
            price: Option<f64>,
        }
    }
    assert_eq!(<typed_products::col_id as TypedColumn>::NAME, "id");
    assert_eq!(<typed_products::col_price as TypedColumn>::NAME, "price");
}

/// 零大小标记类型不应占用内存
#[test]
fn test_typed_query_zero_sized() {
    typed_query! {
        table typed_zero {
            id: i64,
        }
    }
    assert_eq!(std::mem::size_of::<typed_zero::table>(), 0);
    assert_eq!(std::mem::size_of::<typed_zero::col_id>(), 0);
}

// ============================================================================
// SELECT 表达式测试
// ============================================================================

/// SELECT 表达式分支应生成 SQL 字符串字面量
#[test]
fn test_typed_query_select_basic() {
    let sql = typed_query!(SELECT id, name FROM typed_users WHERE id = ?);
    assert!(sql.contains("SELECT"));
    assert!(sql.contains("FROM"));
    assert!(sql.contains("typed_users"));
    assert!(sql.contains("?"));
}

/// SELECT * 应正常工作
#[test]
fn test_typed_query_select_star() {
    let sql = typed_query!(SELECT * FROM typed_users);
    assert!(sql.contains("SELECT"));
    assert!(sql.contains("*"));
    assert!(sql.contains("FROM"));
}

/// SELECT 带 ORDER BY 子句
#[test]
fn test_typed_query_select_order_by() {
    let sql = typed_query!(SELECT id, name FROM typed_users ORDER BY id DESC);
    assert!(sql.contains("ORDER BY"));
    assert!(sql.contains("DESC"));
}

/// SELECT 带 LIMIT 子句
#[test]
fn test_typed_query_select_limit() {
    let sql = typed_query!(SELECT id FROM typed_users LIMIT 10);
    assert!(sql.contains("LIMIT"));
    assert!(sql.contains("10"));
}

/// SELECT 带 JOIN
#[test]
fn test_typed_query_select_join() {
    let sql = typed_query!(
        SELECT u.id, o.order_id
        FROM typed_users u
        INNER JOIN typed_orders o ON u.id = o.user_id
    );
    assert!(sql.contains("INNER JOIN"));
    assert!(sql.contains("ON"));
}

/// SELECT 带多个 WHERE 条件
#[test]
fn test_typed_query_select_multiple_where() {
    let sql = typed_query!(
        SELECT id FROM typed_users WHERE id = ? AND name = ? OR email = ?
    );
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("AND"));
    assert!(sql.contains("OR"));
    assert_eq!(sql.matches('?').count(), 3);
}
