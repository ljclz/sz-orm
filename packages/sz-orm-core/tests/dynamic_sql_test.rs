use sz_orm_core::dynamic_sql::{DynamicSqlParser, ParamValue, SqlParams};

#[test]
fn test_sql_params_new() {
    let p = SqlParams::new();
    assert!(!p.contains("name"));
}

#[test]
fn test_sql_params_set() {
    let mut p = SqlParams::new();
    p.set("name", "Alice");
    assert!(p.contains("name"));
    assert!(!p.is_null("name"));
    assert!(p.is_not_null("name"));
    match p.get("name") {
        Some(ParamValue::String(s)) => assert_eq!(s, "Alice"),
        _ => panic!("expected string"),
    }
}

#[test]
fn test_sql_params_set_int() {
    let mut p = SqlParams::new();
    p.set_int("age", 18);
    match p.get("age") {
        Some(ParamValue::Int(i)) => assert_eq!(*i, 18),
        _ => panic!("expected int"),
    }
}

#[test]
fn test_sql_params_set_float() {
    let mut p = SqlParams::new();
    p.set_float("score", 3.14);
    match p.get("score") {
        Some(ParamValue::Float(f)) => assert_eq!(*f, 3.14),
        _ => panic!("expected float"),
    }
}

#[test]
fn test_sql_params_set_bool() {
    let mut p = SqlParams::new();
    p.set_bool("active", true);
    match p.get("active") {
        Some(ParamValue::Bool(b)) => assert!(*b),
        _ => panic!("expected bool"),
    }
}

#[test]
fn test_sql_params_set_null() {
    let mut p = SqlParams::new();
    p.set_null("opt");
    assert!(p.is_null("opt"));
    assert!(!p.is_not_null("opt"));
}

#[test]
fn test_sql_params_set_array() {
    let mut p = SqlParams::new();
    p.set_array("ids", vec![ParamValue::Int(1), ParamValue::Int(2)]);
    match p.get("ids") {
        Some(ParamValue::Array(arr)) => assert_eq!(arr.len(), 2),
        _ => panic!("expected array"),
    }
}

#[test]
fn test_sql_params_is_null_missing() {
    let p = SqlParams::new();
    assert!(p.is_null("nonexistent"));
}

#[test]
fn test_sql_params_names() {
    let mut p = SqlParams::new();
    p.set("a", "1");
    p.set("b", "2");
    let names = p.names();
    assert_eq!(names.len(), 2);
}

#[test]
fn test_dynamic_sql_parser_new() {
    let parser = DynamicSqlParser::new();
    assert!(parser.statement_ids().is_empty());
}

#[test]
fn test_from_xml_simple_select() {
    let xml = r#"<select id="find_all">SELECT * FROM users</select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let ids = parser.statement_ids();
    assert_eq!(ids, vec!["find_all".to_string()]);
}

#[test]
fn test_build_simple_select() {
    let xml = r#"<select id="find_all">SELECT * FROM users</select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let params = SqlParams::new();
    let sql = parser.build("find_all", &params).unwrap();
    assert!(sql.contains("SELECT * FROM users"));
}

#[test]
fn test_build_statement_not_found() {
    let xml = r#"<select id="find_all">SELECT * FROM users</select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let params = SqlParams::new();
    let result = parser.build("nonexistent", &params);
    assert!(result.is_err());
}

#[test]
fn test_build_with_if_true() {
    let xml = r#"<select id="find">SELECT * FROM users <if test="name != null">WHERE name = #{name}</if></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Alice");
    let sql = parser.build("find", &params).unwrap();
    assert!(sql.contains("WHERE name = ?"));
}

#[test]
fn test_build_with_if_false() {
    let xml = r#"<select id="find">SELECT * FROM users <if test="name != null">WHERE name = #{name}</if></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let params = SqlParams::new();
    let sql = parser.build("find", &params).unwrap();
    assert!(!sql.contains("WHERE"));
}

#[test]
fn test_build_with_where_tag() {
    let xml = r#"<select id="find">SELECT * FROM users <where><if test="name != null">AND name = #{name}</if></where></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Alice");
    let sql = parser.build("find", &params).unwrap();
    assert!(sql.contains("WHERE name = ?"));
}

#[test]
fn test_build_with_where_strips_leading_and() {
    let xml = r#"<select id="find">SELECT * FROM users <where><if test="name != null">AND name = #{name}</if><if test="age != null">AND age = #{age}</if></where></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Alice");
    params.set_int("age", 18);
    let sql = parser.build("find", &params).unwrap();
    assert!(sql.contains("WHERE name = ?"));
    assert!(sql.contains("AND age = ?"));
}

#[test]
fn test_build_with_where_empty() {
    let xml = r#"<select id="find">SELECT * FROM users <where><if test="name != null">AND name = #{name}</if></where></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let params = SqlParams::new();
    let sql = parser.build("find", &params).unwrap();
    assert!(!sql.contains("WHERE"));
}

#[test]
fn test_build_with_set_tag() {
    let xml = r#"<update id="upd">UPDATE users <set><if test="name != null">name = #{name},</if></set> WHERE id = #{id}</update>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Bob");
    params.set_int("id", 1);
    let sql = parser.build("upd", &params).unwrap();
    assert!(sql.contains("SET name = ?"));
    assert!(sql.contains("WHERE id = ?"));
}

#[test]
fn test_build_with_foreach() {
    let xml = r#"<select id="find_in">SELECT * FROM users WHERE id IN <foreach collection="ids" item="i" open="(" separator="," close=")">#{i}</foreach></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set_array("ids", vec![ParamValue::Int(1), ParamValue::Int(2), ParamValue::Int(3)]);
    let sql = parser.build("find_in", &params).unwrap();
    assert!(sql.contains("(?,?,?)"));
}

#[test]
fn test_build_with_foreach_empty() {
    let xml = r#"<select id="find_in">SELECT * FROM users WHERE id IN <foreach collection="ids" item="i" open="(" separator="," close=")">#{i}</foreach></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let params = SqlParams::new();
    let sql = parser.build("find_in", &params).unwrap();
    assert!(sql.contains("IN"));
}

#[test]
fn test_build_with_choose_when() {
    let xml = r#"<select id="find">SELECT * FROM users <choose><when test="type == 'admin'">WHERE is_admin = 1</when><otherwise>WHERE is_admin = 0</otherwise></choose></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("type", "admin");
    let sql = parser.build("find", &params).unwrap();
    assert!(sql.contains("is_admin = 1"));
}

#[test]
fn test_build_with_choose_otherwise() {
    let xml = r#"<select id="find">SELECT * FROM users <choose><when test="type == 'admin'">WHERE is_admin = 1</when><otherwise>WHERE is_admin = 0</otherwise></choose></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let params = SqlParams::new();
    let sql = parser.build("find", &params).unwrap();
    assert!(sql.contains("is_admin = 0"));
}

#[test]
fn test_build_with_trim() {
    let xml = r#"<select id="find">SELECT * FROM users <trim prefix="WHERE" prefixOverrides="AND"><if test="name != null">AND name = #{name}</if></trim></select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Alice");
    let sql = parser.build("find", &params).unwrap();
    assert!(sql.contains("WHERE name = ?"));
}

#[test]
fn test_build_with_binds() {
    let xml = r#"<select id="find">SELECT * FROM users WHERE name = #{name} AND age = #{age}</select>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Alice");
    params.set_int("age", 18);
    let (sql, binds) = parser.build_with_binds("find", &params).unwrap();
    assert!(sql.contains("?"));
    assert_eq!(binds.len(), 2);
}

#[test]
fn test_multiple_statements() {
    let xml = r#"
    <select id="find_all">SELECT * FROM users</select>
    <select id="find_one">SELECT * FROM users LIMIT 1</select>
    "#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let ids = parser.statement_ids();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"find_all".to_string()));
    assert!(ids.contains(&"find_one".to_string()));
}

#[test]
fn test_from_xml_missing_id() {
    let xml = r#"<select>SELECT * FROM users</select>"#;
    let result = DynamicSqlParser::from_xml(xml);
    assert!(result.is_err());
}

#[test]
fn test_build_insert_statement() {
    let xml = r#"<insert id="ins">INSERT INTO users (name) VALUES (#{name})</insert>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Alice");
    let sql = parser.build("ins", &params).unwrap();
    assert!(sql.contains("INSERT INTO users"));
    assert!(sql.contains("?"));
}

#[test]
fn test_build_update_statement() {
    let xml = r#"<update id="upd">UPDATE users SET name = #{name} WHERE id = #{id}</update>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set("name", "Bob");
    params.set_int("id", 1);
    let sql = parser.build("upd", &params).unwrap();
    assert!(sql.contains("UPDATE users"));
}

#[test]
fn test_build_delete_statement() {
    let xml = r#"<delete id="del">DELETE FROM users WHERE id = #{id}</delete>"#;
    let parser = DynamicSqlParser::from_xml(xml).unwrap();
    let mut params = SqlParams::new();
    params.set_int("id", 1);
    let sql = parser.build("del", &params).unwrap();
    assert!(sql.contains("DELETE FROM users"));
}

