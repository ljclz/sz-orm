//! Dynamic SQL 模块契约测试 — 对应 `docs/api-contracts.md` §12
//!
//! 锁定 SqlParams、DynamicSqlParser 契约，特别是已知陷阱：
//! - `is_null("x")` 对不存在的 key 返回 true
//! - `contains("x")` 对存在 key 返回 true（不检查是否为 Null）

use sz_orm_core::dynamic_sql::{DynamicSqlParser, ParamValue, SqlParams};

// ===== §12.2 SqlParams 契约 =====

#[test]
fn test_sql_params_new_is_empty_contract() {
    let params = SqlParams::new();
    assert!(params.names().is_empty());
}

#[test]
fn test_sql_params_set_str_contract() {
    let mut p = SqlParams::new();
    p.set("name", "Alice");
    match p.get("name") {
        Some(ParamValue::String(s)) => assert_eq!(s, "Alice"),
        other => panic!("期望 ParamValue::String，实际: {:?}", other),
    }
}

#[test]
fn test_sql_params_set_int_contract() {
    let mut p = SqlParams::new();
    p.set_int("age", 18);
    match p.get("age") {
        Some(ParamValue::Int(18)) => {}
        other => panic!("期望 ParamValue::Int(18)，实际: {:?}", other),
    }
}

#[test]
fn test_sql_params_set_float_contract() {
    let mut p = SqlParams::new();
    p.set_float("score", 3.15);
    match p.get("score") {
        Some(ParamValue::Float(f)) => assert!((f - 3.15).abs() < 1e-6),
        other => panic!("期望 ParamValue::Float，实际: {:?}", other),
    }
}

#[test]
fn test_sql_params_set_bool_contract() {
    let mut p = SqlParams::new();
    p.set_bool("active", true);
    match p.get("active") {
        Some(ParamValue::Bool(true)) => {}
        other => panic!("期望 ParamValue::Bool(true)，实际: {:?}", other),
    }
}

#[test]
fn test_sql_params_set_null_contract() {
    let mut p = SqlParams::new();
    p.set_null("deleted_at");
    match p.get("deleted_at") {
        Some(ParamValue::Null) => {}
        other => panic!("期望 ParamValue::Null，实际: {:?}", other),
    }
}

#[test]
fn test_sql_params_set_array_contract() {
    let mut p = SqlParams::new();
    p.set_array(
        "ids",
        vec![ParamValue::Int(1), ParamValue::Int(2), ParamValue::Int(3)],
    );
    match p.get("ids") {
        Some(ParamValue::Array(items)) => assert_eq!(items.len(), 3),
        other => panic!("期望 ParamValue::Array，实际: {:?}", other),
    }
}

#[test]
fn test_sql_params_get_missing_returns_none_contract() {
    let p = SqlParams::new();
    assert!(p.get("missing").is_none());
}

// ===== §12.2 contains 契约（不检查 Null） =====

#[test]
fn test_sql_params_contains_existing_key_contract() {
    let mut p = SqlParams::new();
    p.set("name", "Alice");
    assert!(p.contains("name"));
}

#[test]
fn test_sql_params_contains_missing_key_contract() {
    let p = SqlParams::new();
    assert!(!p.contains("missing"));
}

#[test]
fn test_sql_params_contains_returns_true_for_null_value_contract() {
    // 陷阱：contains 不检查是否为 Null
    let mut p = SqlParams::new();
    p.set_null("deleted_at");
    assert!(
        p.contains("deleted_at"),
        "contains 对 Null 值应返回 true（不检查 Null）"
    );
}

// ===== §12.2 is_null 契约（对不存在 key 也返回 true） =====

#[test]
fn test_sql_params_is_null_for_missing_key_contract() {
    // 陷阱：is_null 对不存在的 key 返回 true
    let p = SqlParams::new();
    assert!(p.is_null("missing"), "is_null 对不存在的 key 应返回 true");
}

#[test]
fn test_sql_params_is_null_for_null_value_contract() {
    let mut p = SqlParams::new();
    p.set_null("deleted_at");
    assert!(p.is_null("deleted_at"));
}

#[test]
fn test_sql_params_is_null_for_non_null_value_contract() {
    let mut p = SqlParams::new();
    p.set("name", "Alice");
    assert!(!p.is_null("name"));
}

#[test]
fn test_sql_params_is_not_null_is_opposite_of_is_null_contract() {
    let mut p = SqlParams::new();
    p.set("name", "Alice");
    p.set_null("deleted_at");

    assert!(!p.is_not_null("missing")); // missing → is_null → true → is_not_null → false
    assert!(p.is_not_null("name"));
    assert!(!p.is_not_null("deleted_at"));
}

// ===== §12.2 names 契约 =====

#[test]
fn test_sql_params_names_contract() {
    let mut p = SqlParams::new();
    p.set("a", "1");
    p.set_int("b", 2);
    p.set_bool("c", true);

    let names = p.names();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"a".to_string()));
    assert!(names.contains(&"b".to_string()));
    assert!(names.contains(&"c".to_string()));
}

// ===== §12.1 DynamicSqlParser 契约 =====

#[test]
fn test_dynamic_sql_parser_from_xml_contract() {
    let xml = r#"
<select id="find_users">
    SELECT * FROM users
</select>
"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    // build 应成功生成 SQL
    let params = SqlParams::new();
    let sql = parser.build("find_users", &params).unwrap();
    assert!(sql.contains("SELECT"));
    assert!(sql.contains("users"));
}

#[test]
fn test_dynamic_sql_parser_missing_id_returns_err_contract() {
    let xml = r#"
<select>
    SELECT * FROM users
</select>
"#;
    let result = DynamicSqlParser::from_xml(xml);
    // 缺少 id 属性应返回错误
    assert!(result.is_err());
}

#[test]
fn test_dynamic_sql_parser_build_missing_statement_returns_err_contract() {
    let xml = r#"
<select id="find_users">
    SELECT * FROM users
</select>
"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let params = SqlParams::new();
    // build 不存在的 id 应返回错误
    let result = parser.build("missing_id", &params);
    assert!(result.is_err());
}

// ===== §12.1 <if test="..."> 标签契约 =====

#[test]
fn test_dynamic_sql_if_tag_includes_block_when_param_present_contract() {
    let xml = r#"
<select id="find">
    SELECT * FROM users
    <where>
        <if test="name != null">AND name = #{name}</if>
    </where>
</select>
"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Alice");

    let sql = parser.build("find", &params).unwrap();
    // 当 name 参数存在时，if 块应被包含
    assert!(sql.contains("name"), "if 块应被包含: {}", sql);
}

#[test]
fn test_dynamic_sql_if_tag_skips_block_when_param_missing_contract() {
    let xml = r#"
<select id="find">
    SELECT * FROM users
    <where>
        <if test="name != null">AND name = #{name}</if>
    </where>
</select>
"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    // 不设置 name 参数
    let params = SqlParams::new();

    let sql = parser.build("find", &params).unwrap();
    // 当 name 参数缺失时，if 块应被跳过
    assert!(
        !sql.contains("AND name"),
        "if 块应被跳过（参数缺失）: {}",
        sql
    );
}

// ===== §12.1 <where> 标签自动处理首个 AND/OR 契约 =====

#[test]
fn test_dynamic_sql_where_tag_strips_leading_and_contract() {
    let xml = r#"
<select id="find">
    SELECT * FROM users
    <where>
        <if test="name != null">AND name = #{name}</if>
    </where>
</select>
"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Alice");

    let sql = parser.build("find", &params).unwrap();
    // <where> 应自动移除首个 AND
    assert!(
        !sql.contains("WHERE AND"),
        "<where> 应自动移除首个 AND: {}",
        sql
    );
}
