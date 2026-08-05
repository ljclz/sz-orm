//! 编译期类型验证集成测试（P0-2）
//!
//! 仅当 sz-orm-core 启用 `db-verify` feature 时编译（默认 `cargo test` 跳过）。
//!
//! 运行方式（需本机 MySQL，表 `sz_orm_typecheck` 已建）：
//! ```bash
//! DATABASE_URL=mysql://root:test123@127.0.0.1:3306/sz_orm_test \
//! SZ_ORM_QUERY_VERIFY=1 \
//! cargo test -p sz-orm-core --features db-verify --test compile_time_verify
//! ```
//!
//! 宏在编译期连真 DB 获取 SELECT 列的实际类型，与结构体 `__sz_orm_column_types()`
//! 在 const 上下文中对比：列名/列数/类型不匹配 → const panic → **编译失败**
//! （真正的编译期拦截，非运行时）。

#![cfg(feature = "db-verify")]

use sz_orm_core::FromQueryResult;

#[derive(FromQueryResult, Debug, PartialEq)]
#[allow(dead_code)]
struct TypeCheckRow {
    id: i64,
    name: String,
}

#[test]
fn query_as_compile_time_type_check_passes() {
    // 编译期验证通过：id(BIGINT↔i64)、name(VARCHAR↔TEXT) 类型兼容
    let q = sz_orm_core::query_as!(
        TypeCheckRow,
        "SELECT id, name FROM sz_orm_typecheck WHERE id = ?"
    );
    assert!(q.sql().contains("sz_orm_typecheck"));
}

#[test]
fn query_with_type_param_compile_time_check_passes() {
    // query!(T, "SQL") 同样附加编译期类型验证（P0-1 + P0-2 组合路径）
    let q = sz_orm_core::query!(
        TypeCheckRow,
        "SELECT id, name FROM sz_orm_typecheck WHERE id = ?"
    );
    assert!(q.sql().contains("sz_orm_typecheck"));
}

#[test]
fn compile_time_check_const_metadata_matches_db() {
    // 验证编译期嵌入的 DB 类型与结构体元数据一致（const 上下文可调用）
    const TYPES: &[(&str, &str)] = TypeCheckRow::__sz_orm_column_types();
    assert_eq!(TYPES, vec![("id", "BIGINT"), ("name", "TEXT")]);
}
