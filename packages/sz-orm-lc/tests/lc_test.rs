use serde_json::json;
use sz_orm_lc::*;

// ============================================================================
// ModelDefinition
// ============================================================================

#[test]
fn test_model_definition_new() {
    let model = ModelDefinition::new("users");
    assert_eq!(model.name, "users");
    assert!(model.fields.is_empty());
    assert!(model.indexes.is_empty());
    assert!(model.relations.is_empty());
}

#[test]
fn test_pascal_case_name() {
    assert_eq!(ModelDefinition::new("users").pascal_case_name(), "User");
    assert_eq!(
        ModelDefinition::new("order_items").pascal_case_name(),
        "OrderItem"
    );
    assert_eq!(
        ModelDefinition::new("products").pascal_case_name(),
        "Product"
    );
    assert_eq!(
        ModelDefinition::new("address").pascal_case_name(),
        "Address"
    );
}

#[test]
fn test_singular_name() {
    assert_eq!(ModelDefinition::new("users").singular_name(), "user");
    assert_eq!(ModelDefinition::new("items").singular_name(), "item");
    assert_eq!(ModelDefinition::new("data").singular_name(), "data");
}

#[test]
fn test_model_with_field() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT").primary())
        .with_field(FieldDef::new("name", "VARCHAR(255)"));
    assert_eq!(model.fields.len(), 2);
    assert!(model.fields[0].primary_key);
    assert!(!model.fields[1].primary_key);
}

#[test]
fn test_model_with_index() {
    let model = ModelDefinition::new("users")
        .with_index("idx_email")
        .with_index("idx_created");
    assert_eq!(model.indexes.len(), 2);
}

#[test]
fn test_model_with_relation() {
    let rel = RelationDefinition::new("posts", "one_to_many", "posts", "user_id");
    let model = ModelDefinition::new("users").with_relation(rel);
    assert_eq!(model.relations.len(), 1);
    assert!(model.relations[0].is_one_to_many());
}

#[test]
fn test_find_field() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT"))
        .with_field(FieldDef::new("email", "VARCHAR(255)"));
    assert!(model.find_field("email").is_some());
    assert!(model.find_field("nonexistent").is_none());
}

#[test]
fn test_primary_key() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT").primary())
        .with_field(FieldDef::new("name", "VARCHAR(255)"));
    let pk = model.primary_key().unwrap();
    assert_eq!(pk.name, "id");
    assert!(pk.primary_key);
}

// ============================================================================
// FieldDef
// ============================================================================

#[test]
fn test_field_def_new() {
    let f = FieldDef::new("email", "VARCHAR(255)");
    assert_eq!(f.name, "email");
    assert_eq!(f.field_type, "VARCHAR(255)");
    assert!(!f.nullable);
    assert!(!f.primary_key);
    assert!(!f.unique);
}

#[test]
fn test_field_with_nullable() {
    let f = FieldDef::new("bio", "TEXT").with_nullable(true);
    assert!(f.nullable);
}

#[test]
fn test_field_with_label() {
    let f = FieldDef::new("email", "VARCHAR(255)").with_label("电子邮箱");
    assert_eq!(f.label.as_deref(), Some("电子邮箱"));
    assert_eq!(f.display_label(), "电子邮箱");
}

#[test]
fn test_field_display_label_fallback() {
    let f = FieldDef::new("email", "VARCHAR(255)");
    assert_eq!(f.display_label(), "email");
}

#[test]
fn test_field_with_default() {
    let f = FieldDef::new("active", "BOOLEAN").with_default("true");
    assert_eq!(f.default_value.as_deref(), Some("true"));
}

#[test]
fn test_field_primary() {
    let f = FieldDef::new("id", "BIGINT").primary();
    assert!(f.primary_key);
    assert!(!f.nullable);
}

#[test]
fn test_field_unique() {
    let f = FieldDef::new("email", "VARCHAR(255)").unique();
    assert!(f.unique);
}

// ============================================================================
// RelationDefinition
// ============================================================================

#[test]
fn test_relation_one_to_one() {
    let rel = RelationDefinition::new("profile", "one_to_one", "profiles", "user_id");
    assert!(rel.is_one_to_one());
    assert!(!rel.is_one_to_many());
    assert!(!rel.is_many_to_many());
}

#[test]
fn test_relation_one_to_many() {
    let rel = RelationDefinition::new("posts", "one_to_many", "posts", "user_id");
    assert!(!rel.is_one_to_one());
    assert!(rel.is_one_to_many());
}

#[test]
fn test_relation_many_to_many() {
    let rel = RelationDefinition::new("roles", "many_to_many", "roles", "role_id");
    assert!(rel.is_many_to_many());
}

// ============================================================================
// FieldTypeMapping
// ============================================================================

#[test]
fn test_sql_to_rust() {
    assert_eq!(FieldTypeMapping::sql_to_rust("BIGINT"), "i64");
    assert_eq!(FieldTypeMapping::sql_to_rust("INTEGER"), "i32");
    assert_eq!(FieldTypeMapping::sql_to_rust("SMALLINT"), "i16");
    assert_eq!(FieldTypeMapping::sql_to_rust("BOOLEAN"), "bool");
    assert_eq!(FieldTypeMapping::sql_to_rust("DOUBLE PRECISION"), "f64");
    assert_eq!(FieldTypeMapping::sql_to_rust("REAL"), "f32");
    assert_eq!(FieldTypeMapping::sql_to_rust("VARCHAR(255)"), "String");
    assert_eq!(
        FieldTypeMapping::sql_to_rust("TIMESTAMP"),
        "chrono::NaiveDateTime"
    );
    assert_eq!(
        FieldTypeMapping::sql_to_rust("TIMESTAMPTZ"),
        "chrono::DateTime<chrono::Utc>"
    );
    assert_eq!(FieldTypeMapping::sql_to_rust("DATE"), "chrono::NaiveDate");
    assert_eq!(FieldTypeMapping::sql_to_rust("UUID"), "uuid::Uuid");
    assert_eq!(FieldTypeMapping::sql_to_rust("JSONB"), "serde_json::Value");
    assert_eq!(FieldTypeMapping::sql_to_rust("BYTEA"), "Vec<u8>");
    assert_eq!(
        FieldTypeMapping::sql_to_rust("NUMERIC(19,4)"),
        "rust_decimal::Decimal"
    );
}

#[test]
fn test_sql_to_html_input() {
    assert_eq!(FieldTypeMapping::sql_to_html_input("INTEGER"), "number");
    assert_eq!(FieldTypeMapping::sql_to_html_input("BIGINT"), "number");
    assert_eq!(FieldTypeMapping::sql_to_html_input("BOOLEAN"), "checkbox");
    assert_eq!(FieldTypeMapping::sql_to_html_input("DATE"), "date");
    assert_eq!(
        FieldTypeMapping::sql_to_html_input("TIMESTAMP"),
        "datetime-local"
    );
    assert_eq!(FieldTypeMapping::sql_to_html_input("TIME"), "time");
    assert_eq!(FieldTypeMapping::sql_to_html_input("VARCHAR(255)"), "text");
    assert_eq!(FieldTypeMapping::sql_to_html_input("TEXT"), "textarea");
    assert_eq!(FieldTypeMapping::sql_to_html_input("JSON"), "textarea");
}

#[test]
fn test_sql_to_json_schema() {
    assert_eq!(FieldTypeMapping::sql_to_json_schema("INTEGER"), "integer");
    assert_eq!(FieldTypeMapping::sql_to_json_schema("BIGINT"), "integer");
    assert_eq!(
        FieldTypeMapping::sql_to_json_schema("DOUBLE PRECISION"),
        "number"
    );
    assert_eq!(FieldTypeMapping::sql_to_json_schema("BOOLEAN"), "boolean");
    assert_eq!(FieldTypeMapping::sql_to_json_schema("JSONB"), "object");
    assert_eq!(
        FieldTypeMapping::sql_to_json_schema("VARCHAR(255)"),
        "string"
    );
    assert_eq!(FieldTypeMapping::sql_to_json_schema("UUID"), "string");
}

#[test]
fn test_rust_to_sql() {
    assert_eq!(FieldTypeMapping::rust_to_sql("i16"), "SMALLINT");
    assert_eq!(FieldTypeMapping::rust_to_sql("i32"), "INTEGER");
    assert_eq!(FieldTypeMapping::rust_to_sql("i64"), "BIGINT");
    assert_eq!(FieldTypeMapping::rust_to_sql("bool"), "BOOLEAN");
    assert_eq!(FieldTypeMapping::rust_to_sql("f32"), "REAL");
    assert_eq!(FieldTypeMapping::rust_to_sql("f64"), "DOUBLE PRECISION");
    assert_eq!(FieldTypeMapping::rust_to_sql("String"), "VARCHAR(255)");
    assert_eq!(FieldTypeMapping::rust_to_sql("uuid::Uuid"), "UUID");
    assert_eq!(FieldTypeMapping::rust_to_sql("Vec<u8>"), "BYTEA");
}

#[test]
fn test_is_numeric() {
    assert!(FieldTypeMapping::is_numeric("INTEGER"));
    assert!(FieldTypeMapping::is_numeric("BIGINT"));
    assert!(FieldTypeMapping::is_numeric("FLOAT8"));
    assert!(FieldTypeMapping::is_numeric("NUMERIC(19,4)"));
    assert!(!FieldTypeMapping::is_numeric("VARCHAR(255)"));
    assert!(!FieldTypeMapping::is_numeric("BOOLEAN"));
}

#[test]
fn test_is_temporal() {
    assert!(FieldTypeMapping::is_temporal("DATE"));
    assert!(FieldTypeMapping::is_temporal("TIMESTAMP"));
    assert!(FieldTypeMapping::is_temporal("TIME"));
    assert!(!FieldTypeMapping::is_temporal("INTEGER"));
    assert!(!FieldTypeMapping::is_temporal("VARCHAR(255)"));
}

// ============================================================================
// ValidationRule
// ============================================================================

#[test]
fn test_validation_required() {
    assert!(ValidationRule::Required.validate(&json!("hello")).is_ok());
    assert!(ValidationRule::Required.validate(&json!(null)).is_err());
    assert!(ValidationRule::Required.validate(&json!("")).is_err());
    assert!(ValidationRule::Required.validate(&json!("  ")).is_err());
}

#[test]
fn test_validation_min_max_length() {
    let min = ValidationRule::MinLength { value: 3 };
    let max = ValidationRule::MaxLength { value: 5 };
    assert!(min.validate(&json!("abc")).is_ok());
    assert!(min.validate(&json!("ab")).is_err());
    assert!(max.validate(&json!("abcde")).is_ok());
    assert!(max.validate(&json!("abcdef")).is_err());
}

#[test]
fn test_validation_min_max_value() {
    let min = ValidationRule::Min { value: 10.0 };
    let max = ValidationRule::Max { value: 100.0 };
    assert!(min.validate(&json!(50)).is_ok());
    assert!(min.validate(&json!(5)).is_err());
    assert!(max.validate(&json!(50)).is_ok());
    assert!(max.validate(&json!(150)).is_err());
}

#[test]
fn test_validation_email() {
    assert!(ValidationRule::Email
        .validate(&json!("user@example.com"))
        .is_ok());
    assert!(ValidationRule::Email.validate(&json!("invalid")).is_err());
}

#[test]
fn test_validation_url() {
    assert!(ValidationRule::Url
        .validate(&json!("https://example.com"))
        .is_ok());
    assert!(ValidationRule::Url
        .validate(&json!("ftp://example.com"))
        .is_err());
}

#[test]
fn test_validation_enum() {
    let rule = ValidationRule::Enum {
        values: vec!["active".into(), "inactive".into()],
    };
    assert!(rule.validate(&json!("active")).is_ok());
    assert!(rule.validate(&json!("pending")).is_err());
}

#[test]
fn test_validation_pattern() {
    let rule = ValidationRule::Pattern {
        regex: r"^\d{3}$".into(),
    };
    assert!(rule.validate(&json!("123")).is_ok());
    assert!(rule.validate(&json!("12")).is_err());
}

// ============================================================================
// FieldValidation
// ============================================================================

#[test]
fn test_field_validation() {
    let v = FieldValidation::new("email")
        .with_rule(ValidationRule::Required)
        .with_rule(ValidationRule::Email);
    assert!(v.validate(&json!("user@example.com")).is_ok());
    assert!(v.validate(&json!("")).is_err());
    assert!(v.validate(&json!("invalid")).is_err());
}

#[test]
fn test_field_validation_html_attributes() {
    let v = FieldValidation::new("name")
        .with_rule(ValidationRule::Required)
        .with_rule(ValidationRule::MinLength { value: 3 });
    let attrs = v.to_html_attributes();
    assert!(attrs.contains("required"));
    assert!(attrs.contains("minlength=\"3\""));
}

// ============================================================================
// FormField & FormGenerator
// ============================================================================

#[test]
fn test_form_field_new() {
    let f = FormField::new("email", "邮箱", InputType::Email);
    assert_eq!(f.name, "email");
    assert_eq!(f.label, "邮箱");
    assert!(!f.required);
}

#[test]
fn test_form_field_required() {
    let f = FormField::new("name", "姓名", InputType::Text).required();
    assert!(f.required);
}

#[test]
fn test_form_field_with_placeholder() {
    let f = FormField::new("name", "姓名", InputType::Text).with_placeholder("请输入姓名");
    assert_eq!(f.placeholder.as_deref(), Some("请输入姓名"));
}

#[test]
fn test_form_field_with_option() {
    let f = FormField::new("status", "状态", InputType::Select)
        .with_option("active", "激活")
        .with_option("inactive", "停用");
    assert_eq!(f.options.len(), 2);
}

#[test]
fn test_form_field_with_help_text() {
    let f = FormField::new("email", "邮箱", InputType::Email).with_help_text("请输入有效邮箱地址");
    assert_eq!(f.help_text.as_deref(), Some("请输入有效邮箱地址"));
}

#[test]
fn test_input_type_as_html() {
    assert_eq!(InputType::Text.as_html_type(), "text");
    assert_eq!(InputType::Number.as_html_type(), "number");
    assert_eq!(InputType::Email.as_html_type(), "email");
    assert_eq!(InputType::Password.as_html_type(), "password");
    assert_eq!(InputType::Date.as_html_type(), "date");
    assert_eq!(InputType::DateTime.as_html_type(), "datetime-local");
    assert_eq!(InputType::Checkbox.as_html_type(), "checkbox");
    assert_eq!(InputType::Select.as_html_type(), "select");
    assert_eq!(InputType::Textarea.as_html_type(), "textarea");
    assert_eq!(InputType::Hidden.as_html_type(), "hidden");
    assert_eq!(InputType::File.as_html_type(), "file");
}

#[test]
fn test_form_generator_from_model() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT").primary())
        .with_field(FieldDef::new("name", "VARCHAR(255)"))
        .with_field(FieldDef::new("age", "INTEGER"));
    let fields = FormGenerator::from_model(&model);
    assert_eq!(fields.len(), 3);
    assert!(!fields[0].required);
    assert!(fields[1].required);
    assert!(fields[2].required);
}

#[test]
fn test_form_generator_html_form() {
    let fields = vec![FormField::new("name", "姓名", InputType::Text).required()];
    let html = FormGenerator::generate_html_form(&fields, "/submit", "post");
    assert!(html.contains("<form"));
    assert!(html.contains("action=\"/submit\""));
    assert!(html.contains("method=\"post\""));
    assert!(html.contains("<button"));
}

#[test]
fn test_form_generator_json_schema() {
    let fields = vec![
        FormField::new("name", "姓名", InputType::Text).required(),
        FormField::new("age", "年龄", InputType::Number),
    ];
    let schema = FormGenerator::generate_json_schema(&fields);
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["name"].is_object());
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("name")));
}

// ============================================================================
// CrudTemplateEngine
// ============================================================================

#[test]
fn test_crud_generate_ddl() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT").primary())
        .with_field(FieldDef::new("name", "VARCHAR(255)"))
        .with_field(FieldDef::new("email", "VARCHAR(255)").unique());
    let ddl = CrudTemplateEngine::generate_ddl(&model);
    assert!(ddl.contains("CREATE TABLE \"users\""));
    assert!(ddl.contains("\"id\" BIGINT NOT NULL PRIMARY KEY"));
    assert!(ddl.contains("\"name\" VARCHAR(255) NOT NULL"));
    assert!(ddl.contains("\"email\" VARCHAR(255) NOT NULL UNIQUE"));
}

#[test]
fn test_crud_generate_insert() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT"))
        .with_field(FieldDef::new("name", "VARCHAR(255)"));
    let sql = CrudTemplateEngine::generate_insert(&model);
    assert!(sql.contains("INSERT INTO \"users\""));
    assert!(sql.contains("\"id\""));
    assert!(sql.contains("\"name\""));
    assert!(sql.contains("$1"));
    assert!(sql.contains("$2"));
}

#[test]
fn test_crud_generate_select_by_id() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT"))
        .with_field(FieldDef::new("name", "VARCHAR(255)"));
    let sql = CrudTemplateEngine::generate_select_by_id(&model);
    assert!(sql.contains("SELECT"));
    assert!(sql.contains("FROM \"users\""));
    assert!(sql.contains("WHERE \"id\" = $1"));
}

#[test]
fn test_crud_generate_select_all() {
    let model = ModelDefinition::new("users").with_field(FieldDef::new("id", "BIGINT"));
    let sql = CrudTemplateEngine::generate_select_all(&model);
    assert!(sql.contains("SELECT"));
    assert!(sql.contains("LIMIT $1 OFFSET $2"));
}

#[test]
fn test_crud_generate_update() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT").primary())
        .with_field(FieldDef::new("name", "VARCHAR(255)"));
    let sql = CrudTemplateEngine::generate_update(&model);
    assert!(sql.contains("UPDATE \"users\""));
    assert!(sql.contains("SET"));
    assert!(sql.contains("\"name\" = $1"));
    assert!(sql.contains("WHERE \"id\" = $2"));
}

#[test]
fn test_crud_generate_delete() {
    let model = ModelDefinition::new("users");
    let sql = CrudTemplateEngine::generate_delete(&model);
    assert_eq!(sql, "DELETE FROM \"users\" WHERE \"id\" = $1;");
}

#[test]
fn test_crud_generate_count() {
    let model = ModelDefinition::new("users");
    let sql = CrudTemplateEngine::generate_count(&model);
    assert_eq!(sql, "SELECT COUNT(*) AS total FROM \"users\";");
}

#[test]
fn test_crud_generate_rust_struct() {
    let model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT").primary())
        .with_field(FieldDef::new("name", "VARCHAR(255)"));
    let code = CrudTemplateEngine::generate_rust_struct(&model);
    assert!(code.contains("pub struct User {"));
    assert!(code.contains("pub id: i64,"));
    assert!(code.contains("pub name: String,"));
}

#[test]
fn test_model_definition_serialization() {
    let model = ModelDefinition::new("users").with_field(FieldDef::new("id", "BIGINT").primary());
    let json_str = serde_json::to_string(&model).unwrap();
    let deserialized: ModelDefinition = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.name, "users");
    assert_eq!(deserialized.fields.len(), 1);
}
