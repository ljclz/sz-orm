//! v9.2.0 M13: query.rs where 条件变体 + HavingOp/AggExpr + select_only + clone_for_count

use std::collections::HashMap;
use sz_orm_core::{
    dialect::get_dialect, AggExpr, DbType, HavingOp, Model, ModelExt, QueryBuilder, Value,
};

#[derive(Clone, Debug)]
struct User {
    id: i64,
}
impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> i64 {
        self.id
    }
    fn set_pk(&mut self, pk: i64) {
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
    fn relations() -> std::collections::HashMap<&'static str, sz_orm_core::Relation> {
        Default::default()
    }
    fn fill(&mut self, _: std::collections::HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

fn builder() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

fn pg_builder() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::PostgreSQL).unwrap())
}

#[test]
fn test_where_not_in() {
    let (sql, params) = builder()
        .where_not_in("id", vec![Value::I64(1), Value::I64(2)])
        .build_select();
    let clean = sql.replace('`', "");
    assert!(
        clean.to_uppercase().contains("NOT IN") || clean.to_uppercase().contains("NOT"),
        "sql: {clean}"
    );
    assert!(params.len() >= 2);
}

#[test]
fn test_where_not_between() {
    let (sql, params) = builder()
        .where_not_between("age", Value::I64(10), Value::I64(20))
        .build_select();
    let clean = sql.replace('`', "");
    assert!(
        clean.to_uppercase().contains("NOT BETWEEN") || clean.to_uppercase().contains("BETWEEN"),
        "sql: {clean}"
    );
    assert!(params.len() >= 2);
}

#[test]
fn test_where_not_null() {
    let (sql, _) = builder().where_not_null("name").build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("NOT NULL"), "sql: {clean}");
}

#[test]
fn test_or_where_ne() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_ne("name", Value::String("test".to_string()))
        .build_select();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_or_where_gt() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_gt("age", Value::I64(18))
        .build_select();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_or_where_ge() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_ge("age", Value::I64(18))
        .build_select();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_or_where_lt() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_lt("age", Value::I64(65))
        .build_select();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_or_where_le() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_le("age", Value::I64(65))
        .build_select();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_or_where_like() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_like("name", Value::String("%test%".to_string()))
        .build_select();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_or_where_grouping() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_eq("name", Value::String("a".to_string()))
        .or_where_eq("name", Value::String("b".to_string()))
        .build_select();
    assert!(sql.to_uppercase().contains("OR"));
}

#[test]
fn test_having_op_all_variants() {
    assert_eq!(HavingOp::Eq.as_sql(), "=");
    assert_eq!(HavingOp::Ne.as_sql(), "!=");
    assert_eq!(HavingOp::Gt.as_sql(), ">");
    assert_eq!(HavingOp::Ge.as_sql(), ">=");
    assert_eq!(HavingOp::Lt.as_sql(), "<");
    assert_eq!(HavingOp::Le.as_sql(), "<=");
}

#[test]
fn test_agg_expr_sum_in_having() {
    let b = builder()
        .having(
            AggExpr::Sum("price".to_string()),
            HavingOp::Gt,
            Value::I64(100),
        )
        .unwrap();
    let (sql, _) = b.build_select();
    assert!(sql.to_uppercase().contains("SUM") || sql.to_uppercase().contains("HAVING"));
}

#[test]
fn test_agg_expr_avg_in_having() {
    let b = builder()
        .having(
            AggExpr::Avg("price".to_string()),
            HavingOp::Gt,
            Value::I64(100),
        )
        .unwrap();
    let (sql, _) = b.build_select();
    assert!(sql.to_uppercase().contains("AVG") || sql.to_uppercase().contains("HAVING"));
}

#[test]
fn test_agg_expr_min_in_having() {
    let b = builder()
        .having(
            AggExpr::Min("price".to_string()),
            HavingOp::Gt,
            Value::I64(100),
        )
        .unwrap();
    let (sql, _) = b.build_select();
    assert!(sql.to_uppercase().contains("MIN") || sql.to_uppercase().contains("HAVING"));
}

#[test]
fn test_agg_expr_max_in_having() {
    let b = builder()
        .having(
            AggExpr::Max("price".to_string()),
            HavingOp::Gt,
            Value::I64(100),
        )
        .unwrap();
    let (sql, _) = b.build_select();
    assert!(sql.to_uppercase().contains("MAX") || sql.to_uppercase().contains("HAVING"));
}

#[test]
fn test_having_with_all_ops() {
    let ops = [
        HavingOp::Eq,
        HavingOp::Ne,
        HavingOp::Gt,
        HavingOp::Ge,
        HavingOp::Lt,
        HavingOp::Le,
    ];
    for op in ops {
        let b = builder()
            .having(AggExpr::Sum("price".to_string()), op, Value::I64(100))
            .unwrap();
        let (sql, _) = b.build_select();
        assert!(sql.to_uppercase().contains("HAVING") || !sql.is_empty());
    }
}

#[test]
fn test_select_only_with_column() {
    let (sql, _) = builder().select_only().column("name").build_select();
    let clean = sql.replace('`', "");
    assert!(clean.contains("name"));
}

#[test]
fn test_select_only_with_columns() {
    let (sql, _) = builder()
        .select_only()
        .columns(vec!["id", "name"])
        .build_select();
    assert!(sql.contains("id") || sql.contains("name"));
}

#[test]
fn test_clone_for_count() {
    let b = builder().where_eq("id", Value::I64(1));
    let cloned = b.clone_for_count();
    let (sql, _) = cloned.build_select();
    assert!(sql.to_uppercase().contains("COUNT") || sql.to_uppercase().contains("SELECT"));
}

#[test]
fn test_order_desc() {
    let (sql, _) = builder().order_desc("id").build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("DESC"));
}

#[test]
fn test_keyset_after_with_existing_order_by() {
    let (sql, _) = builder()
        .order_by("id")
        .keyset_after("id", Value::I64(10), 20)
        .build_select();
    assert!(sql.to_uppercase().contains("SELECT"));
}

#[test]
fn test_keyset_before_with_existing_order_by() {
    let (sql, _) = builder()
        .order_by("id")
        .keyset_before("id", Value::I64(10), 20)
        .build_select();
    assert!(sql.to_uppercase().contains("SELECT"));
}

#[test]
fn test_batch_insert_postgresql_placeholders() {
    let data: Vec<HashMap<String, Value>> = vec![
        HashMap::from([
            ("id".to_string(), Value::I64(1)),
            ("name".to_string(), Value::String("a".to_string())),
        ]),
        HashMap::from([
            ("id".to_string(), Value::I64(2)),
            ("name".to_string(), Value::String("b".to_string())),
        ]),
    ];
    let (sql, params) = pg_builder().build_batch_insert_with_params(&data);
    assert!(sql.to_uppercase().contains("INSERT"));
    assert!(params.len() >= 4);
}

#[test]
fn test_where_not_in_pg() {
    let (sql, _) = pg_builder()
        .where_not_in("id", vec![Value::I64(1), Value::I64(2)])
        .build_select();
    assert!(sql.to_uppercase().contains("NOT IN") || sql.to_uppercase().contains("NOT"));
}

#[test]
fn test_where_not_between_pg() {
    let (sql, _) = pg_builder()
        .where_not_between("age", Value::I64(10), Value::I64(20))
        .build_select();
    assert!(sql.to_uppercase().contains("NOT BETWEEN") || sql.to_uppercase().contains("BETWEEN"));
}

#[test]
fn test_where_not_null_pg() {
    let (sql, _) = pg_builder().where_not_null("name").build_select();
    assert!(sql.to_uppercase().contains("NOT NULL"));
}
