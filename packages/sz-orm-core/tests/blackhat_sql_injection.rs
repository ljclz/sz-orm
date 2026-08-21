//! 黑盒 SQL 注入回归测试（审计 M-5 HAVING 参数化 / M-6 SELECT 列名校验）
//!
//! PoC 反转断言：把注入向量作为输入，断言防御机制生效——
//!   1. 恶意聚合列名 → `having()` 返回 Err（构建期拦截）
//!   2. 恶意 SELECT 列名 → `select()` 返回 Err（构建期拦截）
//!   3. 恶意值 → 渲染为 `?` 绑定参数，绝不内联进 SQL 文本
//!
//! 运行：`cargo test -p sz-orm-core --test blackhat_sql_injection`

use sz_orm_core::{get_dialect, AggExpr, DbType, HavingOp, Model, QueryBuilder, Value};

#[derive(Debug, Clone, Default)]
struct Order {
    id: i64,
}

impl Model for Order {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "orders"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

fn mysql_builder() -> QueryBuilder<Order> {
    QueryBuilder::<Order>::new(get_dialect(DbType::MySQL).unwrap())
}

// ============================================================================
// M-5：HAVING 参数化
// ============================================================================

#[test]
fn m5_having_invalid_aggregate_column_rejected() {
    // PoC：聚合列名注入向量（闭合引号 + 拼接 UNION）
    let qb = mysql_builder().table("orders").group_by("user_id");
    let result = qb.having(
        AggExpr::Sum("total`; DROP TABLE orders; --".to_string()),
        HavingOp::Gt,
        Value::I64(5),
    );
    assert!(
        result.is_err(),
        "注入列名必须被 having() 构建期拦截，实际为 Ok"
    );
}

#[test]
fn m5_having_function_whitelist_only() {
    // PoC：白名单外"聚合函数"（任意函数调用注入）
    let result = mysql_builder().having(
        AggExpr::Sum("total".to_string()),
        HavingOp::Gt,
        Value::I64(5),
    );
    assert!(result.is_ok());

    // CountStar 正常
    assert!(mysql_builder()
        .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))
        .is_ok());
}

#[test]
fn m5_having_value_bound_as_param_not_inlined() {
    // PoC：恶意值若内联进 HAVING 文本可被注入（1 OR 1=1）
    let qb = mysql_builder()
        .table("orders")
        .group_by("user_id")
        .having(
            AggExpr::CountStar,
            HavingOp::Gt,
            Value::String("5 OR 1=1 --".to_string()),
        )
        .expect("valid aggregate");
    let (sql, params) = qb.build_select_with_params();

    // 值必须以 ? 占位符渲染，绝不出现于 SQL 文本
    assert!(
        sql.contains("HAVING COUNT(*) > ?"),
        "SQL 必须使用参数占位符，实际: {}",
        sql
    );
    assert!(
        !sql.contains("OR 1=1"),
        "注入值不得内联进 SQL 文本，实际: {}",
        sql
    );
    assert_eq!(params, vec![Value::String("5 OR 1=1 --".to_string())]);
}

#[test]
fn m5_having_valid_count_renders() {
    let qb = mysql_builder()
        .table("orders")
        .group_by("user_id")
        .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))
        .expect("valid aggregate");

    // 无参数版本：值经方言转义内联
    let (sql, _params) = qb.build_select();
    assert!(sql.contains("HAVING COUNT(*) > 5"), "实际: {}", sql);

    // 参数版本：? 占位 + params
    let (sql, params) = qb.build_select_with_params();
    assert!(sql.contains("HAVING COUNT(*) > ?"), "实际: {}", sql);
    assert_eq!(params, vec![Value::I64(5)]);
}

#[test]
fn m5_having_sum_quoted_column() {
    let qb = mysql_builder()
        .table("orders")
        .group_by("user_id")
        .having(
            AggExpr::Sum("total".to_string()),
            HavingOp::Ge,
            Value::I64(100),
        )
        .expect("valid aggregate");
    let (sql, params) = qb.build_select_with_params();
    assert!(sql.contains("HAVING SUM(`total`) >= ?"), "实际: {}", sql);
    assert_eq!(params, vec![Value::I64(100)]);
}

#[test]
fn m5_having_multiple_conditions_and_joined() {
    let qb = mysql_builder()
        .table("orders")
        .group_by("user_id")
        .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))
        .expect("valid aggregate")
        .having(
            AggExpr::Sum("total".to_string()),
            HavingOp::Lt,
            Value::I64(1000),
        )
        .expect("valid aggregate");
    let (sql, _params) = qb.build_select();
    assert!(
        sql.contains("HAVING COUNT(*) > 5 AND SUM(`total`) < 1000"),
        "实际: {}",
        sql
    );
}

#[test]
fn m5_quick_query_having_parametized() {
    // QuickQuery wrapper 同步走参数化
    use sz_orm_core::quick_query::Db;
    let d = get_dialect(DbType::MySQL).unwrap();
    let db = Db::new(d)
        .name("orders")
        .group_by("user_id")
        .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))
        .expect("valid aggregate");
    let (sql, _params) = db.build_select();
    assert!(sql.contains("HAVING COUNT(*) > 5"), "实际: {}", sql);
}

// ============================================================================
// M-6：SELECT 列名校验
// ============================================================================

#[test]
fn m6_select_invalid_column_rejected() {
    // PoC：列名注入向量（闭合引号 + DROP 拼接）
    let result = mysql_builder()
        .table("users")
        .select(vec!["id", "name`; DROP TABLE users; --"]);
    assert!(
        result.is_err(),
        "注入列名必须被 select() 构建期拦截，实际为 Ok"
    );
}

#[test]
fn m6_select_star_must_use_expr() {
    // `*` 不是合法标识符，必须走 select_expr 逃生口
    assert!(mysql_builder().table("users").select(vec!["*"]).is_err());
    let qb = mysql_builder().table("users").select_expr(vec!["*"]);
    assert!(qb.build_select().0.contains("SELECT * FROM"));
}

#[test]
fn m6_select_valid_columns_quoted() {
    // 合法列名经校验并 quote（与 ORDER BY/GROUP BY 行为一致）
    let qb = mysql_builder()
        .table("users")
        .select(vec!["id", "name"])
        .expect("valid columns");
    let (sql, _params) = qb.build_select();
    assert!(sql.contains("SELECT `id`, `name` FROM"), "实际: {}", sql);
}

#[test]
fn m6_select_expr_escape_hatch_raw() {
    // 逃生口：复杂表达式原样输出（调用方自担注入风险，仅限可信来源）
    let qb = mysql_builder()
        .table("users")
        .select_expr(vec!["user_id", "COUNT(*) as cnt"]);
    let (sql, _params) = qb.build_select();
    assert!(
        sql.contains("SELECT user_id, COUNT(*) as cnt FROM"),
        "实际: {}",
        sql
    );
}

#[test]
fn m6_quick_query_select_validated() {
    use sz_orm_core::quick_query::Db;
    let d = get_dialect(DbType::MySQL).unwrap();
    assert!(Db::new(d).name("users").select(vec!["id"]).is_ok());

    let d2 = get_dialect(DbType::MySQL).unwrap();
    assert!(Db::new(d2).name("users").select(vec!["id`; --"]).is_err());
}
