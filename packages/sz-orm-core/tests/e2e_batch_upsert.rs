//! P2-6：Batch Upsert L3 SQL 生成验证测试
//!
//! **注意**：本文件仅验证 `build_batch_insert_with_params` / `build_batch_upsert_with_params`
//! 生成的 SQL 字符串正确性（语法、参数占位符、方言差异），**不执行真实数据库操作**。
//!
//! 如需验证 upsert 在真实数据库上的执行语义（冲突检测、更新生效、数据一致性），
//! 请参阅 `integration_sqlite.rs` 中的 `test_sqlite_upsert_*` 系列测试。
//!
//! 验证目标：
//! - `build_batch_insert_with_params` 生成正确的多行 INSERT SQL
//! - `build_batch_upsert_with_params` 生成正确的 ON DUPLICATE KEY UPDATE / ON CONFLICT DO UPDATE SQL
//! - 参数化绑定正确（所有值通过 ? 占位符传递，不拼接用户值）
//! - 跨方言 SQL 生成正确（MySQL / PostgreSQL / SQLite）
//! - 不支持方言（Oracle / SQL Server）返回明确错误
//! - 空行列表返回错误
//! - 边界情况：单行、NULL 值、Unicode、大批量

#![cfg(test)]

use std::collections::HashMap;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, Model, ModelExt, QueryBuilder, Relation, Value};

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
    age: i32,
    email: String,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

impl ModelExt for User {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "age", "email"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name", "age", "email"]
    }
    fn guarded() -> Vec<&'static str> {
        vec!["id"]
    }
    fn hidden() -> Vec<&'static str> {
        vec![]
    }
    fn relations() -> HashMap<&'static str, Relation> {
        HashMap::new()
    }
    fn fill(&mut self, _data: HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

/// 构建指定方言的 QueryBuilder
fn make_builder(db_type: DbType) -> QueryBuilder<User> {
    let dialect = get_dialect(db_type).unwrap();
    QueryBuilder::<User>::new(dialect).table("users")
}

/// 构造一行测试数据
fn make_row(id: i64, name: &str, age: i32, email: &str) -> HashMap<String, Value> {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(id));
    row.insert("name".to_string(), Value::String(name.to_string()));
    row.insert("age".to_string(), Value::I32(age));
    row.insert("email".to_string(), Value::String(email.to_string()));
    row
}

/// 去除 SQL 中的方言引号（反引号、双引号），便于断言
fn strip_quotes(sql: &str) -> String {
    sql.replace('`', "").replace('"', "")
}

// ===== L3-1：MySQL 批量 INSERT 基本结构 =====

#[test]
fn test_l3_1_mysql_batch_insert_basic() {
    let builder = make_builder(DbType::MySQL);
    let rows = vec![
        make_row(1, "Alice", 30, "alice@test.com"),
        make_row(2, "Bob", 25, "bob@test.com"),
    ];
    let (sql, params) = builder.build_batch_insert_with_params(&rows);

    assert!(sql.to_uppercase().starts_with("INSERT INTO"), "应以 INSERT INTO 开头: {}", sql);
    assert!(sql.to_uppercase().contains("VALUES"), "应包含 VALUES: {}", sql);
    // 两行应有两组 VALUES 括号（列列表括号 + 2 行值括号 = 3）
    let values_pos = sql.to_uppercase().find("VALUES").unwrap();
    let values_part = &sql[values_pos..];
    let paren_count = values_part.matches('(').count();
    assert_eq!(paren_count, 2, "VALUES 后应有 2 组值括号: {}", sql);
    // 参数应为 8 个（4 列 × 2 行）
    assert_eq!(params.len(), 8, "应有 8 个参数: {:?}", params);
}

// ===== L3-2：MySQL 批量 INSERT 参数顺序 =====

#[test]
fn test_l3_2_mysql_batch_insert_param_order() {
    let builder = make_builder(DbType::MySQL);
    let rows = vec![make_row(10, "Alice", 30, "a@t.com")];
    let (sql, params) = builder.build_batch_insert_with_params(&rows);

    // 参数应按列顺序绑定
    assert_eq!(params.len(), 4, "单行应有 4 个参数: {:?}", params);
    // SQL 应有 4 个占位符
    let placeholder_count = sql.matches('?').count();
    assert_eq!(placeholder_count, 4, "应有 4 个 ? 占位符: {}", sql);
}

// ===== L3-3：MySQL 批量 Upsert — ON DUPLICATE KEY UPDATE =====

#[test]
fn test_l3_3_mysql_batch_upsert_on_duplicate_key() {
    let builder = make_builder(DbType::MySQL);
    let rows = vec![
        make_row(1, "Alice", 30, "alice@t.com"),
        make_row(2, "Bob", 25, "bob@t.com"),
    ];
    let result = builder.build_batch_upsert_with_params(&rows, &["id"], &[]);
    assert!(result.is_ok(), "MySQL upsert 应成功: {:?}", result);

    let (sql, params) = result.unwrap();
    let sql_upper = sql.to_uppercase();

    assert!(
        sql_upper.contains("ON DUPLICATE KEY UPDATE"),
        "应包含 ON DUPLICATE KEY UPDATE: {}",
        sql
    );
    assert!(
        sql_upper.contains("VALUES("),
        "应包含 VALUES() 函数引用: {}",
        sql
    );
    // 参数应为 8 个（4 列 × 2 行）
    assert_eq!(params.len(), 8, "应有 8 个参数: {:?}", params);
}

// ===== L3-4：MySQL Upsert 指定更新列 =====

#[test]
fn test_l3_4_mysql_upsert_specific_update_columns() {
    let builder = make_builder(DbType::MySQL);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let (sql, _) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &["name", "age"])
        .unwrap();

    let sql_clean = strip_quotes(&sql);
    assert!(sql_clean.contains("name=VALUES(name)"), "应更新 name 列: {}", sql);
    assert!(sql_clean.contains("age=VALUES(age)"), "应更新 age 列: {}", sql);
    assert!(
        !sql_clean.contains("email=VALUES(email)"),
        "不应更新 email 列（未指定）: {}",
        sql
    );
}

// ===== L3-5：MySQL Upsert 更新所有列（update_columns 为空）=====

#[test]
fn test_l3_5_mysql_upsert_update_all_columns() {
    let builder = make_builder(DbType::MySQL);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let (sql, _) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .unwrap();

    let sql_clean = strip_quotes(&sql);
    // MySQL 模式下 update_columns 为空 → 更新所有列（包括 id）
    assert!(sql_clean.contains("name=VALUES(name)"), "应更新 name: {}", sql);
    assert!(sql_clean.contains("age=VALUES(age)"), "应更新 age: {}", sql);
    assert!(sql_clean.contains("email=VALUES(email)"), "应更新 email: {}", sql);
}

// ===== L3-6：PostgreSQL 批量 Upsert — ON CONFLICT DO UPDATE =====

#[test]
fn test_l3_6_pg_batch_upsert_on_conflict() {
    let builder = make_builder(DbType::PostgreSQL);
    let rows = vec![
        make_row(1, "Alice", 30, "alice@t.com"),
        make_row(2, "Bob", 25, "bob@t.com"),
    ];
    let result = builder.build_batch_upsert_with_params(&rows, &["id"], &[]);
    assert!(result.is_ok(), "PG upsert 应成功: {:?}", result);

    let (sql, params) = result.unwrap();
    let sql_upper = sql.to_uppercase();

    assert!(
        sql_upper.contains("ON CONFLICT"),
        "应包含 ON CONFLICT: {}",
        sql
    );
    assert!(
        sql_upper.contains("DO UPDATE SET"),
        "应包含 DO UPDATE SET: {}",
        sql
    );
    assert!(
        sql_upper.contains("EXCLUDED"),
        "应包含 EXCLUDED 引用: {}",
        sql
    );
    assert_eq!(params.len(), 8, "应有 8 个参数: {:?}", params);
}

// ===== L3-7：PostgreSQL Upsert 冲突列出现在 ON CONFLICT 中 =====

#[test]
fn test_l3_7_pg_upsert_conflict_columns_in_clause() {
    let builder = make_builder(DbType::PostgreSQL);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let (sql, _) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .unwrap();

    let sql_clean = strip_quotes(&sql);
    assert!(
        sql_clean.to_uppercase().contains("ON CONFLICT (ID)"),
        "ON CONFLICT 应包含 id 列: {}",
        sql
    );
}

// ===== L3-8：PostgreSQL Upsert 更新所有非冲突列 =====

#[test]
fn test_l3_8_pg_upsert_update_non_conflict_columns() {
    let builder = make_builder(DbType::PostgreSQL);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let (sql, _) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .unwrap();

    let sql_clean = strip_quotes(&sql);
    // PG 模式下 update_columns 为空 → 更新所有非冲突列（不含 id）
    assert!(sql_clean.contains("name=EXCLUDED.name"), "应更新 name: {}", sql);
    assert!(sql_clean.contains("age=EXCLUDED.age"), "应更新 age: {}", sql);
    assert!(sql_clean.contains("email=EXCLUDED.email"), "应更新 email: {}", sql);
    // id 是冲突列，不应出现在 SET 中
    assert!(
        !sql_clean.contains("id=EXCLUDED.id"),
        "不应更新冲突列 id: {}",
        sql
    );
}

// ===== L3-9：PostgreSQL Upsert 指定更新列 =====

#[test]
fn test_l3_9_pg_upsert_specific_update_columns() {
    let builder = make_builder(DbType::PostgreSQL);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let (sql, _) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &["name"])
        .unwrap();

    let sql_clean = strip_quotes(&sql);
    assert!(sql_clean.contains("name=EXCLUDED.name"), "应更新 name: {}", sql);
    assert!(
        !sql_clean.contains("age=EXCLUDED.age"),
        "不应更新 age（未指定）: {}",
        sql
    );
    assert!(
        !sql_clean.contains("email=EXCLUDED.email"),
        "不应更新 email（未指定）: {}",
        sql
    );
}

// ===== L3-10：SQLite 批量 Upsert 语法与 PG 一致 =====

#[test]
fn test_l3_10_sqlite_batch_upsert() {
    let builder = make_builder(DbType::Sqlite);
    let rows = vec![
        make_row(1, "Alice", 30, "alice@t.com"),
        make_row(2, "Bob", 25, "bob@t.com"),
    ];
    let result = builder.build_batch_upsert_with_params(&rows, &["id"], &[]);
    assert!(result.is_ok(), "SQLite upsert 应成功: {:?}", result);

    let (sql, params) = result.unwrap();
    let sql_upper = sql.to_uppercase();

    assert!(sql_upper.contains("ON CONFLICT"), "应包含 ON CONFLICT: {}", sql);
    assert!(sql_upper.contains("DO UPDATE SET"), "应包含 DO UPDATE SET: {}", sql);
    assert!(sql_upper.contains("EXCLUDED"), "应包含 EXCLUDED: {}", sql);
    assert_eq!(params.len(), 8, "应有 8 个参数: {:?}", params);
}

// ===== L3-11：空行列表返回错误 =====

#[test]
fn test_l3_11_empty_rows_returns_error() {
    let builder = make_builder(DbType::MySQL);
    let rows: Vec<HashMap<String, Value>> = vec![];
    let result = builder.build_batch_upsert_with_params(&rows, &["id"], &[]);

    assert!(result.is_err(), "空行应返回错误");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("empty") || err_msg.contains("不能为空"),
        "错误消息应说明空行问题: {}",
        err_msg
    );
}

// ===== L3-12：单行 Upsert（边界情况）=====

#[test]
fn test_l3_12_single_row_upsert() {
    let builder = make_builder(DbType::MySQL);
    let rows = vec![make_row(42, "Single", 99, "single@t.com")];
    let (sql, params) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .unwrap();

    assert!(sql.to_uppercase().contains("ON DUPLICATE KEY UPDATE"), "应包含 upsert 子句: {}", sql);
    assert_eq!(params.len(), 4, "单行应有 4 个参数: {:?}", params);
}

// ===== L3-13：Oracle 方言不支持 Upsert，返回明确错误 =====

#[test]
fn test_l3_13_oracle_unsupported_upsert() {
    let builder = make_builder(DbType::Oracle);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let result = builder.build_batch_upsert_with_params(&rows, &["id"], &[]);

    assert!(result.is_err(), "Oracle 应返回不支持错误");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support") || err_msg.contains("不支持"),
        "错误消息应说明不支持: {}",
        err_msg
    );
}

// ===== L3-14：SQL Server 方言不支持 Upsert =====

#[test]
fn test_l3_14_sqlserver_unsupported_upsert() {
    let builder = make_builder(DbType::SqlServer);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let result = builder.build_batch_upsert_with_params(&rows, &["id"], &[]);

    assert!(result.is_err(), "SQL Server 应返回不支持错误");
    let err_msg = result.unwrap_err().to_string();
    // 验证错误消息明确指出 SQL Server 方言不支持 upsert，
    // 而非其他原因（如 rows 为空、参数缺失）导致的错误。
    assert!(
        err_msg.contains("does not support") || err_msg.contains("不支持"),
        "错误消息应说明不支持: {}",
        err_msg
    );
    assert!(
        err_msg.contains("upsert"),
        "错误消息应包含 upsert 关键词: {}",
        err_msg
    );
    assert!(
        err_msg.contains("SqlServer") || err_msg.contains("SQL Server"),
        "错误消息应指明 SqlServer 方言: {}",
        err_msg
    );
}

// ===== L3-15：PG 无冲突列返回错误 =====

#[test]
fn test_l3_15_pg_no_conflict_columns_error() {
    let builder = make_builder(DbType::PostgreSQL);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let result = builder.build_batch_upsert_with_params(&rows, &[], &[]);

    // PG 方言 conflict_columns 为空 → build_upsert_on_conflict 返回 None → 错误
    assert!(result.is_err(), "PG 无冲突列应返回错误");
}

// ===== L3-16：MySQL 无冲突列仍可工作（自动检测唯一键）=====

#[test]
fn test_l3_16_mysql_no_conflict_columns_ok() {
    let builder = make_builder(DbType::MySQL);
    let rows = vec![make_row(1, "Alice", 30, "alice@t.com")];
    let result = builder.build_batch_upsert_with_params(&rows, &[], &[]);

    assert!(result.is_ok(), "MySQL 无冲突列应成功（自动检测唯一键）: {:?}", result);
    let (sql, _) = result.unwrap();
    assert!(sql.to_uppercase().contains("ON DUPLICATE KEY UPDATE"), "应包含 upsert: {}", sql);
}

// ===== L3-17：NULL 值处理 =====

#[test]
fn test_l3_17_null_value_handling() {
    let builder = make_builder(DbType::MySQL);
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("name".to_string(), Value::Null);
    row.insert("age".to_string(), Value::I32(30));
    row.insert("email".to_string(), Value::String("a@t.com".to_string()));

    let (sql, params) = builder.build_batch_insert_with_params(&[row]);

    // Value::Null 应通过 ? 占位符参数化（与 SeaORM/Diesel 一致，不 inline NULL）
    // 4 个参数（id, name=Null, age, email）
    assert_eq!(params.len(), 4, "应有 4 个参数（Null 也占参数）: {:?}", params);
    let placeholder_count = sql.matches('?').count();
    assert_eq!(placeholder_count, 4, "应有 4 个 ? 占位符: {}", sql);
}

// ===== L3-18：Unicode/中文值 =====

#[test]
fn test_l3_18_unicode_values() {
    let builder = make_builder(DbType::MySQL);
    let rows = vec![make_row(1, "张三", 25, "张三@测试.com")];
    let (sql, params) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .unwrap();

    // 中文值应通过参数绑定，不直接拼接到 SQL
    assert!(!sql.contains("张三"), "中文值不应出现在 SQL 中（应参数化）: {}", sql);
    assert_eq!(params.len(), 4, "应有 4 个参数: {:?}", params);
}

// ===== L3-19：SQL 注入防护 — 值通过参数绑定 =====

#[test]
fn test_l3_19_sql_injection_prevention() {
    let builder = make_builder(DbType::MySQL);
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert(
        "name".to_string(),
        Value::String("'; DROP TABLE users; --".to_string()),
    );
    row.insert("age".to_string(), Value::I32(30));
    row.insert("email".to_string(), Value::String("safe@t.com".to_string()));

    let (sql, params) = builder
        .build_batch_upsert_with_params(&[row], &["id"], &[])
        .unwrap();

    // SQL 注入字符串不应出现在 SQL 中（应通过 ? 参数化）
    assert!(
        !sql.contains("DROP TABLE"),
        "SQL 注入字符串不应出现在 SQL 中: {}",
        sql
    );
    assert!(
        !sql.contains("--"),
        "SQL 注释不应出现在 SQL 中: {}",
        sql
    );
    // 注入字符串应作为参数值安全传递
    assert_eq!(params.len(), 4, "应有 4 个参数: {:?}", params);
}

// ===== L3-20：大批量（100 行）参数绑定正确 =====

#[test]
fn test_l3_20_large_batch_100_rows() {
    let builder = make_builder(DbType::PostgreSQL);
    let rows: Vec<HashMap<String, Value>> = (1..=100)
        .map(|i| make_row(i, &format!("user{}", i), (i % 80) as i32, &format!("u{}@t.com", i)))
        .collect();

    let (sql, params) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .unwrap();

    // 100 行 × 4 列 = 400 个参数
    assert_eq!(params.len(), 400, "100 行应有 400 个参数: {:?}", params.len());
    // SQL 应有 400 个 ? 占位符
    let placeholder_count = sql.matches('?').count();
    assert_eq!(placeholder_count, 400, "应有 400 个 ? 占位符: {}", placeholder_count);
    // 应包含 ON CONFLICT
    assert!(sql.to_uppercase().contains("ON CONFLICT"), "应包含 ON CONFLICT: {}", sql);
}
