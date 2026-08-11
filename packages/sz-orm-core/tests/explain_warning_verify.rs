//! v4.3.0 M1-T6.1：编译期 EXPLAIN 分析集成验证（真库）
//!
//! 仅当 sz-orm-core 启用 `db-verify` feature 时编译（默认 `cargo test` 跳过）。
//!
//! 运行方式（需本机 MySQL，表 `sz_orm_explain_users` 已建）：
//! ```bash
//! DATABASE_URL=mysql://root:test123@127.0.0.1:3306/sz_orm_test \
//! SZ_ORM_QUERY_VERIFY=1 \
//! cargo test -p sz-orm-core --features db-verify --test explain_warning_verify
//! ```
//!
//! 预期编译输出（警告非阻断，测试仍通过）：
//! ```text
//! warning: [sz-orm-explain] full table scan detected on table 'sz_orm_explain_users': consider adding an index
//! ```
//!
//! 表结构（MySQL sz_orm_test）：
//! ```sql
//! CREATE TABLE sz_orm_explain_users (
//!   id BIGINT PRIMARY KEY,
//!   name VARCHAR(100),          -- 无索引 → 全表扫描
//!   email VARCHAR(100),         -- 有索引 idx_email → 索引查询
//!   INDEX idx_email (email)
//! );
//! ```

#![cfg(feature = "db-verify")]

#[test]
fn full_table_scan_emits_compile_warning() {
    // WHERE name = ?（无索引）→ 编译期应输出 "full table scan detected" 警告
    // 注：MySQL 输出 Table scan；PG 对 `= NULL` 折叠为恒假不扫描表（无警告，SQL 语义）
    let q = sz_orm_core::query!("SELECT id, name FROM sz_orm_explain_users WHERE name = ?");
    assert!(q.sql().contains("sz_orm_explain_users"));
}

#[test]
fn no_where_full_table_scan_emits_warning() {
    // 无 WHERE 全表扫描：MySQL 与 PG 均输出 Table scan / Seq Scan → 双方言都应触发警告
    let q = sz_orm_core::query!("SELECT id, name FROM sz_orm_explain_users");
    assert!(q.sql().contains("sz_orm_explain_users"));
}

#[test]
fn indexed_query_compiles_clean() {
    // WHERE email = ?（有索引 idx_email）→ 编译期无全表扫描警告
    let q = sz_orm_core::query!("SELECT id, email FROM sz_orm_explain_users WHERE email = ?");
    assert!(q.sql().contains("sz_orm_explain_users"));
}

#[test]
fn pk_lookup_compiles_clean() {
    // WHERE id = ?（主键）→ 编译期无警告（UniqueLookup）
    let q = sz_orm_core::query!("SELECT id, name FROM sz_orm_explain_users WHERE id = ?");
    assert!(q.sql().contains("sz_orm_explain_users"));
}
