//! P2-5：Keyset 分页 L3 行为测试
//!
//! 验证目标：
//! - `keyset_after` 生成正确的 `WHERE field > ? ORDER BY field ASC LIMIT n` SQL
//! - `keyset_before` 生成正确的 `WHERE field < ? ORDER BY field DESC LIMIT n` SQL
//! - 参数化绑定正确（游标值通过 ? 占位符传递）
//! - 自动设置 ORDER BY（用户未手动设置时）
//! - 用户手动设置 ORDER BY 时不覆盖
//! - keyset 与 offset 互斥（设置 keyset 后清除 offset）
//! - keyset 与其他 WHERE 条件组合正确
//! - 跨方言 SQL 生成正确

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

fn make_builder() -> QueryBuilder<User> {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    QueryBuilder::<User>::new(dialect)
}

/// 去除 SQL 中的方言引号（反引号、双引号），便于断言
fn strip_quotes(sql: &str) -> String {
    sql.replace(['`', '"'], "")
}

// ===== L3-1：keyset_after 生成正确 SQL =====

#[test]
fn test_l3_1_keyset_after_generates_correct_sql() {
    let builder = make_builder()
        .table("users")
        .keyset_after("id", Value::I64(100), 20);
    let (sql, params) = builder.build_select_with_params();
    let sql_clean = strip_quotes(&sql);

    assert!(sql_clean.contains("id > ?"), "应包含 'id > ?': {}", sql);
    assert!(
        sql.to_uppercase().contains("ORDER BY"),
        "应包含 ORDER BY: {}",
        sql
    );
    assert!(sql.to_uppercase().contains("ASC"), "应包含 ASC: {}", sql);
    assert!(sql.contains("LIMIT 20"), "应包含 LIMIT 20: {}", sql);
    assert!(
        !sql.to_uppercase().contains("OFFSET"),
        "不应包含 OFFSET: {}",
        sql
    );
    assert_eq!(params, vec![Value::I64(100)]);
}

// ===== L3-2：keyset_before 生成正确 SQL =====

#[test]
fn test_l3_2_keyset_before_generates_correct_sql() {
    let builder = make_builder()
        .table("users")
        .keyset_before("id", Value::I64(100), 20);
    let (sql, params) = builder.build_select_with_params();
    let sql_clean = strip_quotes(&sql);

    assert!(sql_clean.contains("id < ?"), "应包含 'id < ?': {}", sql);
    assert!(
        sql.to_uppercase().contains("ORDER BY"),
        "应包含 ORDER BY: {}",
        sql
    );
    assert!(sql.to_uppercase().contains("DESC"), "应包含 DESC: {}", sql);
    assert!(sql.contains("LIMIT 20"), "应包含 LIMIT 20: {}", sql);
    assert!(
        !sql.to_uppercase().contains("OFFSET"),
        "不应包含 OFFSET: {}",
        sql
    );
    assert_eq!(params, vec![Value::I64(100)]);
}

// ===== L3-3：keyset_after 参数化绑定正确 =====

#[test]
fn test_l3_3_keyset_after_param_binding() {
    let builder = make_builder().table("users").keyset_after(
        "created_at",
        Value::String("2024-01-01".to_string()),
        10,
    );
    let (sql, params) = builder.build_select_with_params();
    let sql_clean = strip_quotes(&sql);

    assert!(
        sql_clean.contains("created_at > ?"),
        "应包含 'created_at > ?': {}",
        sql
    );
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], Value::String("2024-01-01".to_string()));
}

// ===== L3-4：自动设置 ORDER BY（用户未手动设置） =====

#[test]
fn test_l3_4_keyset_auto_sets_order_by() {
    let builder = make_builder()
        .table("users")
        .keyset_after("id", Value::I64(50), 10);
    let (sql, _) = builder.build_select_with_params();
    let sql_upper = sql.to_uppercase();

    assert!(
        sql_upper.contains("ORDER BY") && sql_upper.contains("ID") && sql_upper.contains("ASC"),
        "应自动设置 ORDER BY id ASC: {}",
        sql
    );
}

// ===== L3-5：用户手动设置 ORDER BY 时不覆盖 =====

#[test]
fn test_l3_5_keyset_respects_user_order_by() {
    let builder =
        make_builder()
            .table("users")
            .order_by("name")
            .keyset_after("id", Value::I64(50), 10);
    let (sql, _) = builder.build_select_with_params();
    let sql_clean = strip_quotes(&sql);

    assert!(
        sql.to_uppercase().contains("ORDER BY") && sql_clean.contains("name"),
        "应保留用户设置的 ORDER BY: {}",
        sql
    );
    assert!(sql_clean.contains("id"), "应包含 id 排序: {}", sql);
}

// ===== L3-6：keyset 与 offset 互斥 =====

#[test]
fn test_l3_6_keyset_clears_offset() {
    let builder = make_builder()
        .table("users")
        .offset(100)
        .keyset_after("id", Value::I64(50), 10);
    let (sql, _) = builder.build_select_with_params();

    assert!(
        !sql.to_uppercase().contains("OFFSET"),
        "keyset 应清除 OFFSET: {}",
        sql
    );
    assert!(sql.contains("LIMIT 10"), "应保留 LIMIT: {}", sql);
}

// ===== L3-7：keyset 与其他 WHERE 条件组合 =====

#[test]
fn test_l3_7_keyset_with_other_where_conditions() {
    let builder = make_builder()
        .table("users")
        .where_eq("status", Value::String("active".to_string()))
        .keyset_after("id", Value::I64(100), 20);
    let (sql, params) = builder.build_select_with_params();
    let sql_clean = strip_quotes(&sql);

    assert!(
        sql_clean.contains("status = ?"),
        "应包含 status 条件: {}",
        sql
    );
    assert!(sql_clean.contains("id > ?"), "应包含 keyset 条件: {}", sql);
    assert_eq!(params.len(), 2, "应有 2 个参数: {:?}", params);
    assert_eq!(params[0], Value::String("active".to_string()));
    assert_eq!(params[1], Value::I64(100));
}

// ===== L3-8：keyset_after 使用不同值类型 =====

#[test]
fn test_l3_8_keyset_after_different_value_types() {
    let builder = make_builder().table("users").keyset_after(
        "email",
        Value::String("user@example.com".to_string()),
        10,
    );
    let (sql, params) = builder.build_select_with_params();
    let sql_clean = strip_quotes(&sql);
    assert!(sql_clean.contains("email > ?"), "应包含 email > ?: {}", sql);
    assert_eq!(params.len(), 1);

    let builder = make_builder()
        .table("users")
        .keyset_after("age", Value::I32(25), 10);
    let (sql, params) = builder.build_select_with_params();
    let sql_clean = strip_quotes(&sql);
    assert!(sql_clean.contains("age > ?"), "应包含 age > ?: {}", sql);
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], Value::I32(25));
}

// ===== L3-9：keyset_before 参数化绑定正确 =====

#[test]
fn test_l3_9_keyset_before_param_binding() {
    let builder = make_builder()
        .table("users")
        .keyset_before("id", Value::I64(50), 10);
    let (sql, params) = builder.build_select_with_params();
    let sql_clean = strip_quotes(&sql);

    assert!(sql_clean.contains("id < ?"), "应包含 'id < ?': {}", sql);
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], Value::I64(50));
}

// ===== L3-10：跨方言 SQL 生成 =====

#[test]
fn test_l3_10_keyset_cross_dialect() {
    let dialects = vec![DbType::MySQL, DbType::PostgreSQL, DbType::Sqlite];

    for db_type in dialects {
        let dialect = get_dialect(db_type).unwrap();
        let builder = QueryBuilder::<User>::new(dialect)
            .table("users")
            .keyset_after("id", Value::I64(100), 20);
        let (sql, params) = builder.build_select_with_params();

        assert!(
            sql.contains("> ?"),
            "方言 {:?} 应包含 '> ?': {}",
            db_type,
            sql
        );
        assert!(
            sql.to_uppercase().contains("ORDER BY"),
            "方言 {:?} 应包含 ORDER BY: {}",
            db_type,
            sql
        );
        assert!(
            sql.contains("LIMIT 20"),
            "方言 {:?} 应包含 LIMIT 20: {}",
            db_type,
            sql
        );
        assert_eq!(params, vec![Value::I64(100)]);
    }
}

// ===== L3-11：keyset 与 where_in 组合 =====

#[test]
fn test_l3_11_keyset_with_where_in() {
    let builder = make_builder()
        .table("users")
        .where_in(
            "status",
            vec![
                Value::String("active".to_string()),
                Value::String("pending".to_string()),
            ],
        )
        .keyset_after("id", Value::I64(100), 20);
    let (sql, params) = builder.build_select_with_params();

    assert!(sql.contains("IN (?, ?)"), "应包含 IN 子句: {}", sql);
    assert!(sql.contains("> ?"), "应包含 keyset 条件: {}", sql);
    assert_eq!(params.len(), 3, "应有 3 个参数: {:?}", params);
}

// ===== L3-12：keyset 与 where_between 组合 =====

#[test]
fn test_l3_12_keyset_with_where_between() {
    let builder = make_builder()
        .table("users")
        .where_between("age", Value::I32(18), Value::I32(65))
        .keyset_after("id", Value::I64(100), 20);
    let (sql, params) = builder.build_select_with_params();

    assert!(
        sql.to_uppercase().contains("BETWEEN ? AND ?"),
        "应包含 BETWEEN: {}",
        sql
    );
    assert!(sql.contains("> ?"), "应包含 keyset: {}", sql);
    assert_eq!(params.len(), 3, "应有 3 个参数: {:?}", params);
}

// ===== L3-13：keyset_after 无参数版本 SQL 正确 =====

#[test]
fn test_l3_13_keyset_after_no_params_version() {
    let builder = make_builder()
        .table("users")
        .keyset_after("id", Value::I64(100), 20);
    let sql = builder.build_select();
    let sql_clean = strip_quotes(&sql);

    assert!(sql_clean.contains("id >"), "应包含 'id >': {}", sql);
    assert!(
        sql.to_uppercase().contains("ORDER BY"),
        "应包含 ORDER BY: {}",
        sql
    );
    assert!(sql.to_uppercase().contains("ASC"), "应包含 ASC: {}", sql);
    assert!(sql.contains("LIMIT 20"), "应包含 LIMIT 20: {}", sql);
}

// ===== L3-14：keyset_before 无参数版本 SQL 正确 =====

#[test]
fn test_l3_14_keyset_before_no_params_version() {
    let builder = make_builder()
        .table("users")
        .keyset_before("id", Value::I64(100), 20);
    let sql = builder.build_select();
    let sql_clean = strip_quotes(&sql);

    assert!(sql_clean.contains("id <"), "应包含 'id <': {}", sql);
    assert!(
        sql.to_uppercase().contains("ORDER BY"),
        "应包含 ORDER BY: {}",
        sql
    );
    assert!(sql.to_uppercase().contains("DESC"), "应包含 DESC: {}", sql);
    assert!(sql.contains("LIMIT 20"), "应包含 LIMIT 20: {}", sql);
}

// ===== L3-15：连续 keyset 调用（模拟翻页） =====

#[test]
fn test_l3_15_keyset_pagination_sequence() {
    let builder1 = make_builder()
        .table("users")
        .keyset_after("id", Value::I64(0), 10);
    let (sql1, params1) = builder1.build_select_with_params();
    assert!(sql1.contains("> ?"), "第一页应包含 > ?: {}", sql1);
    assert_eq!(params1, vec![Value::I64(0)]);

    let builder2 = make_builder()
        .table("users")
        .keyset_after("id", Value::I64(10), 10);
    let (sql2, params2) = builder2.build_select_with_params();
    assert!(sql2.contains("> ?"), "第二页应包含 > ?: {}", sql2);
    assert_eq!(params2, vec![Value::I64(10)]);

    let builder3 = make_builder()
        .table("users")
        .keyset_after("id", Value::I64(20), 10);
    let (sql3, params3) = builder3.build_select_with_params();
    assert!(sql3.contains("> ?"), "第三页应包含 > ?: {}", sql3);
    assert_eq!(params3, vec![Value::I64(20)]);
}

// ===== L3-16：keyset 使用 NULL 值游标 =====

#[test]
fn test_l3_16_keyset_with_null_cursor() {
    let builder = make_builder()
        .table("users")
        .keyset_after("id", Value::Null, 10);
    let (sql, params) = builder.build_select_with_params();

    assert!(sql.contains("> ?"), "应包含 '> ?': {}", sql);
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], Value::Null);
}

// ===== L3-17：keyset 与 group_by 组合 =====

#[test]
fn test_l3_17_keyset_with_group_by() {
    let builder = make_builder()
        .table("orders")
        .select(vec!["user_id", "COUNT(*) as cnt"])
        .group_by("user_id")
        .keyset_after("user_id", Value::I64(100), 20);
    let (sql, params) = builder.build_select_with_params();

    assert!(
        sql.to_uppercase().contains("GROUP BY"),
        "应包含 GROUP BY: {}",
        sql
    );
    assert!(sql.contains("> ?"), "应包含 keyset: {}", sql);
    assert_eq!(params.len(), 1);
}

// ===== L3-18：keyset 与 join 组合 =====

#[test]
fn test_l3_18_keyset_with_join() {
    let builder = make_builder()
        .table("orders")
        .join_inner("users", "orders.user_id", "users.id")
        .keyset_after("orders.id", Value::I64(100), 20);
    let (sql, params) = builder.build_select_with_params();

    assert!(
        sql.to_uppercase().contains("INNER JOIN"),
        "应包含 JOIN: {}",
        sql
    );
    assert!(sql.contains("> ?"), "应包含 keyset: {}", sql);
    assert_eq!(params.len(), 1);
}

// ===== L3-19：page_size = 0 边界情况 =====

#[test]
fn test_l3_19_keyset_zero_page_size() {
    let builder = make_builder()
        .table("users")
        .keyset_after("id", Value::I64(100), 0);
    let (sql, _) = builder.build_select_with_params();

    assert!(sql.contains("LIMIT 0"), "应包含 LIMIT 0: {}", sql);
}

// ===== L3-20：keyset 多次调用覆盖前一次 =====

#[test]
fn test_l3_20_keyset_overrides_previous() {
    let builder = make_builder()
        .table("users")
        .keyset_after("id", Value::I64(100), 10)
        .keyset_before("id", Value::I64(200), 10);
    let (sql, params) = builder.build_select_with_params();

    assert!(
        sql.contains("< ?"),
        "应使用最后一次 keyset (before): {}",
        sql
    );
    assert!(
        !sql.contains("> ?"),
        "不应包含前一次 keyset (after): {}",
        sql
    );
    assert!(sql.to_uppercase().contains("DESC"), "应使用 DESC: {}", sql);
    assert_eq!(params, vec![Value::I64(200)]);
}
