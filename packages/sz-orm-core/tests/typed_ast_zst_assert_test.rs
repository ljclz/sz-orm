//! M1-T7.1: 编译期 ZST 断言测试
//!
//! 为所有 15 种新增表达式（CTE 3 + Window Frame 6 + JSON 6）
//! 添加运行时 ZST 断言，验证零成本抽象。

#![cfg(feature = "typed-dsl")]

use sz_orm_core::dialect::MySqlDialect;
use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::*;

// ---- 测试用 mock 类型 ----

struct UsersTable;
impl TypedTable for UsersTable {
    const NAME: &'static str = "users";
}

struct ColId;
impl TypedColumn for ColId {
    const NAME: &'static str = "id";
    type Table = UsersTable;
    type RustType = i64;
    type SqlType = BigInt;
}

struct ColName;
impl TypedColumn for ColName {
    const NAME: &'static str = "name";
    type Table = UsersTable;
    type RustType = String;
    type SqlType = Text;
}

// ---- CTE ZST 断言 ----

struct CteTestName;
impl CteName for CteTestName {
    const NAME: &'static str = "test_cte";
}

#[test]
fn test_cte_expressions_are_zst() {
    assert_eq!(
        std::mem::size_of::<With<CteTestName, ColumnExpr<ColId>>>(),
        0
    );
    assert_eq!(
        std::mem::size_of::<WithRecursive<CteTestName, ColumnExpr<ColId>, ColumnExpr<ColName>>>(),
        0
    );
    assert_eq!(std::mem::size_of::<CteRef<CteTestName>>(), 0);
}

// ---- Window Frame ZST 断言 ----

#[test]
fn test_window_frame_expressions_are_zst() {
    assert_eq!(std::mem::size_of::<RowsFrame>(), 0);
    assert_eq!(std::mem::size_of::<RangeFrame>(), 0);
    assert_eq!(std::mem::size_of::<GroupsFrame>(), 0);
    assert_eq!(std::mem::size_of::<FrameUnboundedPreceding>(), 0);
    assert_eq!(std::mem::size_of::<FrameCurrentRow>(), 0);
    assert_eq!(
        std::mem::size_of::<FrameBetween<FrameUnboundedPreceding, FrameCurrentRow>>(),
        0
    );
}

// ---- JSON 操作符 ZST 断言 ----

#[test]
fn test_json_operator_expressions_are_zst() {
    assert_eq!(std::mem::size_of::<JsonGet<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonGetText<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonPathGet<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonPathGetText<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonContains<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonExists<ColName, String>>(), 0);
}

// ---- 所有 15 种表达式 ZST 汇总 ----

#[test]
fn test_all_15_new_expressions_are_zst() {
    let cte_count = 3;
    let window_frame_count = 6;
    let json_op_count = 6;
    let total = cte_count + window_frame_count + json_op_count;
    assert_eq!(total, 15);

    // CTE (3)
    assert_eq!(
        std::mem::size_of::<With<CteTestName, ColumnExpr<ColId>>>(),
        0
    );
    assert_eq!(
        std::mem::size_of::<WithRecursive<CteTestName, ColumnExpr<ColId>, ColumnExpr<ColName>>>(),
        0
    );
    assert_eq!(std::mem::size_of::<CteRef<CteTestName>>(), 0);

    // Window Frame (6)
    assert_eq!(std::mem::size_of::<RowsFrame>(), 0);
    assert_eq!(std::mem::size_of::<RangeFrame>(), 0);
    assert_eq!(std::mem::size_of::<GroupsFrame>(), 0);
    assert_eq!(std::mem::size_of::<FrameUnboundedPreceding>(), 0);
    assert_eq!(std::mem::size_of::<FrameCurrentRow>(), 0);
    assert_eq!(
        std::mem::size_of::<FrameBetween<FrameUnboundedPreceding, FrameCurrentRow>>(),
        0
    );

    // JSON (6)
    assert_eq!(std::mem::size_of::<JsonGet<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonGetText<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonPathGet<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonPathGetText<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonContains<ColName, String>>(), 0);
    assert_eq!(std::mem::size_of::<JsonExists<ColName, String>>(), 0);
}
