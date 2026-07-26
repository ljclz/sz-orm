//! QueryBuilder 参数绑定方法单元测试
//!
//! 验证 `build_select_with_params`、`build_insert_with_params`、
//! `build_update_with_params`、`build_delete_with_params` 生成的 SQL
//! 占位符和参数向量正确性。不依赖数据库连接。

use std::collections::HashMap;
use sz_orm_core::{DbType, Model, ModelExt, QueryBuilder, Relation, Value};
use sz_orm_core::dialect::get_dialect;

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct TestUser {
    id: i64,
    name: String,
    age: i32,
}

impl Model for TestUser {
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

impl ModelExt for TestUser {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "age"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name", "age"]
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

fn make_builder() -> QueryBuilder<TestUser> {
    let dialect = get_dialect(DbType::Sqlite).unwrap();
    QueryBuilder::<TestUser>::new(dialect)
}

// ===================== SELECT 参数绑定测试 =====================

#[test]
fn test_build_select_with_params_no_where() {
    let builder = make_builder().table("users").select(vec!["id", "name"]);
    let (sql, params) = builder.build_select_with_params();
    assert!(sql.contains("SELECT id, name FROM"));
    assert!(params.is_empty(), "no WHERE clause should have empty params");
}

#[test]
fn test_build_select_with_params_where_in() {
    let builder = make_builder()
        .table("users")
        .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
    let (sql, params) = builder.build_select_with_params();
    assert!(sql.contains("IN (?, ?, ?)"), "SQL should have 3 placeholders, got: {}", sql);
    assert_eq!(params.len(), 3);
    assert_eq!(params[0], Value::I64(1));
    assert_eq!(params[1], Value::I64(2));
    assert_eq!(params[2], Value::I64(3));
}

#[test]
fn test_build_select_with_params_where_between() {
    let builder = make_builder()
        .table("users")
        .where_between("age", Value::I32(18), Value::I32(30));
    let (sql, params) = builder.build_select_with_params();
    assert!(sql.contains("BETWEEN ? AND ?"), "SQL: {}", sql);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], Value::I32(18));
    assert_eq!(params[1], Value::I32(30));
}

#[test]
fn test_build_select_with_params_where_not_in() {
    let builder = make_builder()
        .table("users")
        .where_not_in("id", vec![Value::I64(10), Value::I64(20)]);
    let (sql, params) = builder.build_select_with_params();
    assert!(sql.contains("NOT IN (?, ?)"), "SQL: {}", sql);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], Value::I64(10));
    assert_eq!(params[1], Value::I64(20));
}

#[test]
fn test_build_select_with_params_where_not_between() {
    let builder = make_builder()
        .table("users")
        .where_not_between("age", Value::I32(0), Value::I32(17));
    let (sql, params) = builder.build_select_with_params();
    assert!(sql.contains("NOT BETWEEN ? AND ?"), "SQL: {}", sql);
    assert_eq!(params.len(), 2);
}

#[test]
fn test_build_select_with_params_mixed_conditions() {
    let builder = make_builder()
        .table("users")
        .where_cond("status = 'active'")
        .where_in("id", vec![Value::I64(1), Value::I64(2)])
        .where_between("age", Value::I32(18), Value::I32(65));
    let (sql, params) = builder.build_select_with_params();
    // And 条件不提取参数，In 提取 2 个，Between 提取 2 个 = 4 个参数
    assert_eq!(params.len(), 4, "params: {:?}", params);
    assert!(sql.contains("status = 'active'"), "raw condition should be inlined: {}", sql);
    assert!(sql.contains("IN (?, ?)"), "SQL: {}", sql);
    assert!(sql.contains("BETWEEN ? AND ?"), "SQL: {}", sql);
}

#[test]
fn test_build_select_with_params_null_conditions() {
    let builder = make_builder()
        .table("users")
        .where_null("deleted_at");
    let (sql, params) = builder.build_select_with_params();
    assert!(sql.contains("IS NULL"), "SQL: {}", sql);
    assert!(params.is_empty(), "IS NULL should have no params");
}

#[test]
fn test_build_select_with_params_limit_offset() {
    let builder = make_builder()
        .table("users")
        .limit(10)
        .offset(20);
    let (sql, params) = builder.build_select_with_params();
    assert!(sql.contains("LIMIT 10"), "SQL: {}", sql);
    assert!(sql.contains("OFFSET 20"), "SQL: {}", sql);
    assert!(params.is_empty(), "LIMIT/OFFSET should not generate params");
}

// ===================== INSERT 参数绑定测试 =====================

#[test]
fn test_build_insert_with_params() {
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("Alice".to_string()));
    data.insert("age".to_string(), Value::I32(30));

    let builder = make_builder().table("users");
    let (sql, params) = builder.build_insert_with_params(&data);
    assert!(sql.contains("INSERT INTO"), "SQL: {}", sql);
    assert!(sql.contains("(?, ?)"), "should have 2 placeholders: {}", sql);
    assert_eq!(params.len(), 2);
}

#[test]
fn test_build_insert_with_params_empty() {
    let data = HashMap::new();
    let builder = make_builder().table("users");
    let (sql, params) = builder.build_insert_with_params(&data);
    assert!(sql.is_empty(), "empty data should produce empty SQL");
    assert!(params.is_empty());
}

// ===================== UPDATE 参数绑定测试 =====================

#[test]
fn test_build_update_with_params() {
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("Bob".to_string()));
    data.insert("age".to_string(), Value::I32(25));

    let builder = make_builder()
        .table("users")
        .where_cond("id = 1");
    let (sql, params) = builder.build_update_with_params(&data);
    assert!(sql.contains("UPDATE"), "SQL: {}", sql);
    assert!(sql.contains("SET"), "SQL: {}", sql);
    assert!(sql.contains("= ?"), "should have placeholders in SET: {}", sql);
    assert_eq!(params.len(), 2, "SET params: 2, WHERE has no params (raw condition)");
}

#[test]
fn test_build_update_with_params_where_in() {
    let mut data = HashMap::new();
    data.insert("age".to_string(), Value::I32(99));

    let builder = make_builder()
        .table("users")
        .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
    let (sql, params) = builder.build_update_with_params(&data);
    // 1 SET param + 3 WHERE IN params = 4
    assert_eq!(params.len(), 4, "params: {:?}", params);
    assert!(sql.contains("IN (?, ?, ?)"), "SQL: {}", sql);
}

// ===================== DELETE 参数绑定测试 =====================

#[test]
fn test_build_delete_with_params_no_where() {
    let builder = make_builder().table("users");
    let (sql, params) = builder.build_delete_with_params();
    assert!(sql.contains("DELETE FROM"), "SQL: {}", sql);
    assert!(!sql.contains("WHERE"), "no WHERE clause: {}", sql);
    assert!(params.is_empty());
}

#[test]
fn test_build_delete_with_params_where_in() {
    let builder = make_builder()
        .table("users")
        .where_in("id", vec![Value::I64(1), Value::I64(2)]);
    let (sql, params) = builder.build_delete_with_params();
    assert!(sql.contains("DELETE FROM"), "SQL: {}", sql);
    assert!(sql.contains("WHERE"), "SQL: {}", sql);
    assert!(sql.contains("IN (?, ?)"), "SQL: {}", sql);
    assert_eq!(params.len(), 2);
}

#[test]
fn test_build_delete_with_params_where_between() {
    let builder = make_builder()
        .table("users")
        .where_between("age", Value::I32(18), Value::I32(30));
    let (sql, params) = builder.build_delete_with_params();
    assert!(sql.contains("BETWEEN ? AND ?"), "SQL: {}", sql);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], Value::I32(18));
    assert_eq!(params[1], Value::I32(30));
}

// ===================== 参数值类型覆盖测试 =====================

#[test]
fn test_build_select_with_params_all_value_types() {
    let builder = make_builder()
        .table("users")
        .where_in("val", vec![
            Value::Null,
            Value::Bool(true),
            Value::I8(1),
            Value::I16(2),
            Value::I32(3),
            Value::I64(4),
            Value::U8(5),
            Value::U16(6),
            Value::U32(7),
            Value::U64(8),
            Value::F32(1.5),
            Value::F64(2.5),
            Value::String("test".to_string()),
            Value::Bytes(vec![0x41, 0x42]),
        ]);
    let (sql, params) = builder.build_select_with_params();
    assert!(sql.contains("IN (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"), "SQL: {}", sql);
    assert_eq!(params.len(), 14);
}

// ===================== SQL 注入防护测试 =====================

#[test]
fn test_param_binding_prevents_sql_injection() {
    // 恶意输入：尝试通过字符串值注入 SQL
    let malicious_name = "'; DROP TABLE users; --";
    let builder = make_builder()
        .table("users")
        .where_in("name", vec![Value::String(malicious_name.to_string())]);
    let (sql, params) = builder.build_select_with_params();

    // SQL 不应包含 DROP TABLE（值通过参数绑定传递，不在 SQL 中）
    assert!(!sql.contains("DROP TABLE"), "SQL injection detected in SQL: {}", sql);
    assert_eq!(params.len(), 1);
    // 恶意字符串应原样保留在参数中（由 prepared statement 处理转义）
    assert_eq!(params[0], Value::String(malicious_name.to_string()));
}
