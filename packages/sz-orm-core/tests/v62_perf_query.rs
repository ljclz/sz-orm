#![allow(deprecated)]

//! v6.2 性能优化：build_select_with_params 等价性测试
//!
//! 验证 `build_select_with_params` 在零分配优化后，
//! SQL 输出与预期逐字节相等，params 向量与预期相等。
//! 覆盖 WHERE / JOIN / GROUP BY / ORDER BY / LIMIT 全场景。

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::dialect::SqliteDialect;
use sz_orm_core::relation_trait::{RelationDef, RelationKind, RelationTrait};
use sz_orm_core::{DbType, Model, QueryBuilder, Value};

// ===================== 测试模型 =====================

struct TestModel;
impl Model for TestModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        1
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

// ===================== 关联关系（用于 JOIN 测试） =====================

static ORDERS_RELATION: RelationDef = RelationDef::new(
    "orders",
    "users",
    "orders",
    "id",
    "user_id",
    RelationKind::HasMany,
);

struct OrdersRelation;
impl RelationTrait for OrdersRelation {
    fn def(&self) -> &'static RelationDef {
        &ORDERS_RELATION
    }
    fn all_relations() -> &'static [RelationDef] {
        std::slice::from_ref(&ORDERS_RELATION)
    }
}

static PROFILE_RELATION: RelationDef = RelationDef::new(
    "profile",
    "users",
    "profiles",
    "id",
    "user_id",
    RelationKind::BelongsTo,
);

struct ProfileRelation;
impl RelationTrait for ProfileRelation {
    fn def(&self) -> &'static RelationDef {
        &PROFILE_RELATION
    }
    fn all_relations() -> &'static [RelationDef] {
        std::slice::from_ref(&PROFILE_RELATION)
    }
}

// ===================== 辅助函数 =====================

fn make_builder() -> QueryBuilder<TestModel> {
    let dialect = get_dialect(DbType::Sqlite).expect("SQLite dialect");
    QueryBuilder::<TestModel>::new(dialect)
}

// ===================== 等价性测试 =====================

/// 场景 1：简单 SELECT（无 WHERE），验证基础 SQL 结构
#[test]
fn build_select_with_params_equivalence_simple() {
    let builder = make_builder()
        .table("users")
        .select(vec!["id", "name"])
        .expect("valid columns");
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT "id", "name" FROM "users""#);
    assert!(params.is_empty(), "params should be empty: {:?}", params);
}

/// 场景 2：SELECT *（无指定列）
#[test]
fn build_select_with_params_equivalence_select_star() {
    let builder = make_builder().table("users");
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users""#);
    assert!(params.is_empty());
}

/// 场景 3：WHERE eq 条件
#[test]
fn build_select_with_params_equivalence_where_eq() {
    let builder = make_builder()
        .table("users")
        .where_eq("status", Value::String("active".to_string()));
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users" WHERE "status" = ?"#);
    assert_eq!(params, vec![Value::String("active".to_string())]);
}

/// 场景 4：WHERE 多条件（eq + gt + in）
#[test]
fn build_select_with_params_equivalence_where_multi() {
    let builder = make_builder()
        .table("users")
        .where_eq("status", Value::String("active".to_string()))
        .where_gt("age", Value::I64(18))
        .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE "status" = ? AND "age" > ? AND "id" IN (?, ?, ?)"#
    );
    assert_eq!(
        params,
        vec![
            Value::String("active".to_string()),
            Value::I64(18),
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
        ]
    );
}

/// 场景 5：WHERE BETWEEN
#[test]
fn build_select_with_params_equivalence_where_between() {
    let builder =
        make_builder()
            .table("users")
            .where_between("age", Value::I32(18), Value::I32(65));
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users" WHERE "age" BETWEEN ? AND ?"#);
    assert_eq!(params, vec![Value::I32(18), Value::I32(65)]);
}

/// 场景 6：WHERE IS NULL
#[test]
fn build_select_with_params_equivalence_where_null() {
    let builder = make_builder().table("users").where_null("deleted_at");
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users" WHERE "deleted_at" IS NULL"#);
    assert!(params.is_empty());
}

/// 场景 7：JOIN（HasMany → LEFT JOIN）
#[test]
fn build_select_with_params_equivalence_join_has_many() {
    let builder = make_builder().table("users").join(&OrdersRelation);
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" LEFT JOIN "orders" ON "users"."id" = "orders"."user_id""#
    );
    assert!(params.is_empty());
}

/// 场景 8：JOIN（BelongsTo → INNER JOIN）
#[test]
fn build_select_with_params_equivalence_join_belongs_to() {
    let builder = make_builder().table("users").join(&ProfileRelation);
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" INNER JOIN "profiles" ON "users"."id" = "profiles"."user_id""#
    );
    assert!(params.is_empty());
}

/// 场景 9：GROUP BY
#[test]
fn build_select_with_params_equivalence_group_by() {
    let builder = make_builder().table("users").group_by("status");
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users" GROUP BY "status""#);
    assert!(params.is_empty());
}

/// 场景 10：ORDER BY（ASC + DESC）
#[test]
fn build_select_with_params_equivalence_order_by() {
    let builder = make_builder()
        .table("users")
        .order_by("created_at")
        .order_desc("id");
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" ORDER BY "created_at" ASC, "id" DESC"#
    );
    assert!(params.is_empty());
}

/// 场景 11：LIMIT + OFFSET
#[test]
fn build_select_with_params_equivalence_limit_offset() {
    let builder = make_builder().table("users").limit(10).offset(20);
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users" LIMIT 10 OFFSET 20"#);
    assert!(params.is_empty());
}

/// 场景 12：全组合（WHERE + JOIN + GROUP BY + ORDER BY + LIMIT）
#[test]
fn build_select_with_params_equivalence_full_combination() {
    let builder = make_builder()
        .table("users")
        .select(vec!["id", "name", "status"])
        .expect("valid columns")
        .join(&OrdersRelation)
        .where_eq("status", Value::String("active".to_string()))
        .where_gt("age", Value::I64(18))
        .group_by("status")
        .order_by("created_at")
        .limit(50)
        .offset(100);
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT "id", "name", "status" FROM "users" LEFT JOIN "orders" ON "users"."id" = "orders"."user_id" WHERE "status" = ? AND "age" > ? GROUP BY "status" ORDER BY "created_at" ASC LIMIT 50 OFFSET 100"#
    );
    assert_eq!(
        params,
        vec![Value::String("active".to_string()), Value::I64(18)]
    );
}

/// 场景 13：WHERE NOT IN
#[test]
fn build_select_with_params_equivalence_where_not_in() {
    let builder = make_builder()
        .table("users")
        .where_not_in("id", vec![Value::I64(10), Value::I64(20)]);
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users" WHERE "id" NOT IN (?, ?)"#);
    assert_eq!(params, vec![Value::I64(10), Value::I64(20)]);
}

/// 场景 14：WHERE NOT BETWEEN
#[test]
fn build_select_with_params_equivalence_where_not_between() {
    let builder =
        make_builder()
            .table("users")
            .where_not_between("age", Value::I32(0), Value::I32(17));
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE "age" NOT BETWEEN ? AND ?"#
    );
    assert_eq!(params, vec![Value::I32(0), Value::I32(17)]);
}

/// 场景 15：多个 GROUP BY 列
#[test]
fn build_select_with_params_equivalence_multi_group_by() {
    let builder = make_builder()
        .table("users")
        .group_by("status")
        .group_by("department");
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" GROUP BY "status", "department""#
    );
    assert!(params.is_empty());
}

/// 场景 15：WHERE IS NOT NULL
#[test]
fn build_select_with_params_equivalence_where_not_null() {
    let builder = make_builder().table("users").where_not_null("email");
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users" WHERE "email" IS NOT NULL"#);
    assert!(params.is_empty());
}

/// 场景 16：多 JOIN（HasMany + BelongsTo）
#[test]
fn build_select_with_params_equivalence_multi_join() {
    let builder = make_builder()
        .table("users")
        .join(&OrdersRelation)
        .join(&ProfileRelation);
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" LEFT JOIN "orders" ON "users"."id" = "orders"."user_id" INNER JOIN "profiles" ON "users"."id" = "profiles"."user_id""#
    );
    assert!(params.is_empty());
}

/// 场景 17：GROUP BY + HAVING
#[test]
fn build_select_with_params_equivalence_having() {
    let builder = make_builder()
        .table("users")
        .group_by("status")
        .having(
            sz_orm_core::AggExpr::CountStar,
            sz_orm_core::HavingOp::Gt,
            Value::I64(1),
        )
        .expect("having");
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" GROUP BY "status" HAVING COUNT(*) > ?"#
    );
    assert_eq!(params, vec![Value::I64(1)]);
}

/// 场景 18：WHERE OR eq 条件
#[test]
fn build_select_with_params_equivalence_where_or_eq() {
    let builder = make_builder()
        .table("users")
        .where_eq("status", Value::String("active".to_string()))
        .or_where_eq("role", Value::String("admin".to_string()));
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE ("status" = ? OR "role" = ?)"#
    );
    assert_eq!(
        params,
        vec![
            Value::String("active".to_string()),
            Value::String("admin".to_string()),
        ]
    );
}

/// 场景 19：WHERE LIKE 条件
#[test]
fn build_select_with_params_equivalence_where_like() {
    let builder = make_builder()
        .table("users")
        .where_like("name", Value::String("%john%".to_string()));
    let (sql, params) = builder.build_select_with_params();

    assert_eq!(sql, r#"SELECT * FROM "users" WHERE "name" LIKE ?"#);
    assert_eq!(params, vec![Value::String("%john%".to_string())]);
}

// ===================== v6.2 查询性能集成测试 =====================

use std::collections::HashMap;
use std::time::Instant;

/// 验证 build_select_with_params 吞吐量（v6.3 目标：release ≥ 2M ops/s）
#[test]
fn sql_build_throughput_real() {
    let iterations = 500_000;

    let builder = QueryBuilder::<TestModel>::new(Box::new(SqliteDialect))
        .table("users")
        .where_eq("status", Value::String("active".to_string()))
        .where_gt("age", Value::I64(18));

    let start = Instant::now();
    for _ in 0..iterations {
        let (sql, params) = builder.build_select_with_params();
        std::hint::black_box((sql, params));
    }
    let elapsed = start.elapsed();

    let elapsed_secs = elapsed.as_secs_f64();
    let throughput = (iterations as f64 / elapsed_secs) as u64;
    let threshold = if cfg!(debug_assertions) {
        200_000
    } else {
        2_000_000
    };
    assert!(
        throughput >= threshold,
        "吞吐量应 ≥ {threshold} ops/s，实际: {throughput} ops/s（{elapsed_secs:.3}s）"
    );
}

/// 验证批量插入 1000 行生成 1 条多值 INSERT（非 1000 条）
#[test]
fn batch_insert_single_sql() {
    let dialect = get_dialect(DbType::MySQL).expect("dialect");
    let rows: Vec<HashMap<String, Value>> = (0..1000)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("name".to_string(), Value::String(format!("user_{i}")));
            row.insert("age".to_string(), Value::I32(i % 100));
            row
        })
        .collect();

    let builder = QueryBuilder::<TestModel>::new(dialect).table("users");
    let (sql, params) = builder.build_batch_insert_with_params(&rows);

    let values_count = sql.matches("VALUES").count();
    assert_eq!(
        values_count, 1,
        "应生成 1 条多值 INSERT（含 1 个 VALUES），实际 VALUES 数: {values_count}"
    );
    assert_eq!(
        params.len(),
        2000,
        "应有 2000 个参数（1000 行 × 2 列），实际: {}",
        params.len()
    );
}

/// 验证 build_select_with_params 占位符数 == params 长度
#[test]
fn param_placeholder_count_match() {
    let dialect = get_dialect(DbType::MySQL).expect("dialect");
    let builder = QueryBuilder::<TestModel>::new(dialect)
        .table("users")
        .where_eq("status", Value::String("active".to_string()))
        .where_eq("role", Value::String("admin".to_string()))
        .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        .where_between("age", Value::I32(18), Value::I32(65));

    let (sql, params) = builder.build_select_with_params();
    let placeholder_count = sql.matches('?').count();

    assert_eq!(
        placeholder_count,
        params.len(),
        "占位符数 ({placeholder_count}) 应等于 params 长度 ({})，SQL: {sql}",
        params.len()
    );
}
