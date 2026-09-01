//! 白帽安全验证测试 — 从防御者视角验证安全防护机制有效工作。
//!
//! 与黑帽测试（`blackhat_sql_injection.rs` + OWASP 测试）互补：
//! - 黑帽：从攻击者角度构造恶意输入，验证防御拦截
//! - 白帽：从防御者角度验证防护机制正确生效，合法输入正常工作
//!
//! 5 项验证：参数化查询有效性、类型安全列验证、权限边界检查、
//! 输入验证完整性、安全默认配置验证。
//!
//! 运行：`cargo test -p sz-orm-core --test whitehat_security_validation`

use sz_orm_core::{get_dialect, DbType, Model, QueryBuilder, Value};

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
    fn tenant_field() -> Option<&'static str> {
        Some("tenant_id")
    }
}

fn mysql_builder() -> QueryBuilder<Order> {
    QueryBuilder::<Order>::new(get_dialect(DbType::MySQL).unwrap())
}

// ============================================================================
// WH-02：参数化查询有效性
// ============================================================================

#[test]
fn test_whitehat_parameterized_query_effective() {
    let injection_vectors = [
        "' OR 1=1 --",
        "1; DROP TABLE users; --",
        "' UNION SELECT * FROM admins --",
    ];

    for injection in &injection_vectors {
        let qb = mysql_builder()
            .table("orders")
            .where_eq("name", Value::String(injection.to_string()));
        let (sql, params) = qb.build_select_with_params();

        assert!(sql.contains('?'), "注入值必须参数化为占位符，SQL: {sql}");
        assert!(
            !sql.contains("OR 1=1") && !sql.contains("DROP TABLE") && !sql.contains("UNION SELECT"),
            "注入语义不得进入 SQL 文本，SQL: {sql}"
        );
        assert!(
            params.contains(&Value::String(injection.to_string())),
            "注入字符串原值必须在 params 中"
        );
    }
}

// ============================================================================
// WH-03：类型安全列验证
// ============================================================================

#[test]
fn test_whitehat_type_safe_column_accepts_registered() {
    let result = mysql_builder()
        .table("orders")
        .select(vec!["id", "name", "status"]);
    assert!(result.is_ok(), "合法列名应被接受");

    let qb = result.unwrap();
    let (sql, _) = qb.build_select_with_params();
    assert!(sql.contains("id"), "SQL 应包含 id 列: {sql}");
    assert!(sql.contains("name"), "SQL 应包含 name 列: {sql}");
    assert!(sql.contains("status"), "SQL 应包含 status 列: {sql}");
}

#[test]
fn test_whitehat_type_safe_column_rejects_unregistered() {
    // select() 校验标识符格式（ASCII 字母数字+下划线，长度1-63），不校验列是否在模型中注册
    // 以下列名格式非法，应被构建期拒绝
    let invalid_cols = ["", "col;DROP--", "1col", "col name"];

    for &col in &invalid_cols {
        let result = mysql_builder().table("orders").select(vec![col]);
        assert!(result.is_err(), "格式非法列名应被拒绝: {col:?}");
    }
}

// ============================================================================
// WH-04：权限边界检查（多租户隔离）
// ============================================================================

#[test]
fn test_whitehat_tenant_boundary_isolated() {
    let qb = mysql_builder().table("orders").with_tenant_id(100);
    let (sql, params) = qb.build_select_with_params();

    assert!(
        sql.contains("tenant_id"),
        "多租户查询应自动附加 tenant_id 条件: {sql}"
    );
    assert!(sql.contains('?'), "租户值应参数化: {sql}");
    assert!(
        params.contains(&Value::I64(100)),
        "租户 ID 100 应在 params 中"
    );
}

#[test]
fn test_whitehat_tenant_boundary_without_tenant() {
    let qb = mysql_builder()
        .table("orders")
        .with_tenant_id(100)
        .without_tenant();
    let (sql, _) = qb.build_select_with_params();

    assert!(
        !sql.contains("tenant_id"),
        "without_tenant() 后不应附加租户条件: {sql}"
    );
}

// ============================================================================
// WH-05：输入验证完整性
// ============================================================================

#[test]
fn test_whitehat_input_validation_rejects_invalid() {
    let too_long = "a".repeat(64);
    let control_char = "col\x00null";
    let unicode_col = "列名";
    let space_col = "col name";

    let invalid_inputs = [&too_long as &str, control_char, unicode_col, space_col];
    for &input in &invalid_inputs {
        let result = mysql_builder().table("orders").select(vec![input]);
        assert!(result.is_err(), "非法输入应被拒绝: {input:?}");
    }
}

// ============================================================================
// WH-06：安全默认配置验证
// ============================================================================

#[test]
fn test_whitehat_default_config_safe() {
    // 默认禁止 SELECT *（select() 校验标识符，* 不是合法标识符）
    let result = mysql_builder().table("orders").select(vec!["*"]);
    assert!(
        result.is_err(),
        "默认配置应禁止 SELECT *（必须走 select_expr）"
    );

    // 默认参数化
    let qb = mysql_builder()
        .table("orders")
        .where_eq("name", Value::String("test".to_string()));
    let (sql, params) = qb.build_select_with_params();
    assert!(sql.contains('?'), "默认应参数化: {sql}");
    assert!(
        params.contains(&Value::String("test".to_string())),
        "参数值应在 params 中"
    );

    // 默认租户隔离
    let qb = mysql_builder().table("orders").with_tenant_id(1);
    let (sql, _) = qb.build_select_with_params();
    assert!(sql.contains("tenant_id"), "默认应自动附加租户条件: {sql}");
}

// ============================================================================
// WH-07：边界条件 — 极端输入安全
// ============================================================================

#[test]
fn test_whitehat_boundary_extreme_inputs() {
    // 空字符串列名
    assert!(mysql_builder().table("orders").select(vec![""]).is_err());

    // 最大合法长度（63 字符）
    let max_len = "a".repeat(63);
    assert!(
        mysql_builder()
            .table("orders")
            .select(vec![max_len.as_str()])
            .is_ok(),
        "63 字符列名应被接受"
    );

    // 超长列名（64 字符）
    let too_long = "a".repeat(64);
    assert!(
        mysql_builder()
            .table("orders")
            .select(vec![too_long.as_str()])
            .is_err(),
        "64 字符列名应被拒绝"
    );

    // 数字开头列名
    assert!(mysql_builder()
        .table("orders")
        .select(vec!["1col"])
        .is_err());

    // 下划线开头列名（合法）
    assert!(mysql_builder().table("orders").select(vec!["_col"]).is_ok());

    // 含空格列名
    assert!(mysql_builder()
        .table("orders")
        .select(vec!["col name"])
        .is_err());

    // 含分号列名
    assert!(mysql_builder()
        .table("orders")
        .select(vec!["col;DROP--"])
        .is_err());
}
