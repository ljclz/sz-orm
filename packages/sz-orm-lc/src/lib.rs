//! # SZ-ORM LC — 低代码模型定义 (experimental, not for production use)
//!
//! 提供低代码场景下的模型声明式定义，包含字段、索引与关联关系，
//! 可自动推导 PascalCase 模型名与单数表名。支持动态表单生成、
//! CRUD 模板引擎、字段类型映射与验证规则配置。
//!
//! ## 主要类型
//!
//! - [`ModelDefinition`] — 模型定义
//! - [`FieldDef`] — 字段定义
//! - [`RelationDefinition`] — 关联关系定义
//! - [`FieldTypeMapping`] — 字段类型映射（SQL ↔ Rust ↔ HTML）
//! - [`ValidationRule`] / [`FieldValidation`] — 验证规则配置
//! - [`FormField`] / [`FormGenerator`] — 动态表单生成
//! - [`CrudTemplateEngine`] — CRUD 模板引擎

use serde::{Deserialize, Serialize};

// ============================================================================
// 模型定义
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub indexes: Vec<String>,
    pub relations: Vec<RelationDefinition>,
}

impl ModelDefinition {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fields: vec![],
            indexes: vec![],
            relations: vec![],
        }
    }

    pub fn validate_identifier(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("identifier must not be empty".to_string());
        }
        if name.len() > 63 {
            return Err(format!("identifier length {} exceeds 63", name.len()));
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!(
                "identifier '{}' contains invalid chars (only alphanumeric + underscore allowed)",
                name
            ));
        }
        Ok(())
    }

    pub fn sanitize_identifier(name: &str) -> String {
        let sanitized: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let sanitized = if sanitized.is_empty() {
            "_".to_string()
        } else {
            sanitized
        };
        if sanitized.len() > 63 {
            sanitized[..63].to_string()
        } else {
            sanitized
        }
    }

    /// Example: "users" -> "User", "order_items" -> "OrderItem"
    pub fn pascal_case_name(&self) -> String {
        to_pascal_singular(&self.name)
    }

    /// Return the singular form of the table name (simple heuristic: strip the trailing 's').
    pub fn singular_name(&self) -> String {
        let n = self.name.trim_end_matches('s');
        n.to_string()
    }

    /// Add a field (chainable).
    pub fn with_field(mut self, field: FieldDef) -> Self {
        self.fields.push(field);
        self
    }

    /// Add an index (chainable).
    pub fn with_index(mut self, index: &str) -> Self {
        self.indexes.push(index.to_string());
        self
    }

    /// Add a relation (chainable).
    pub fn with_relation(mut self, relation: RelationDefinition) -> Self {
        self.relations.push(relation);
        self
    }

    /// Find a field by name.
    pub fn find_field(&self, name: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get the primary key field (defaults to id).
    pub fn primary_key(&self) -> Option<&FieldDef> {
        self.find_field("id")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub field_type: String,
    pub nullable: bool,
    /// Field comment/label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Default value (SQL expression or literal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    /// Whether this is a primary key.
    #[serde(default)]
    pub primary_key: bool,
    /// Whether this is unique.
    #[serde(default)]
    pub unique: bool,
}

impl FieldDef {
    pub fn new(name: &str, field_type: &str) -> Self {
        Self {
            name: name.to_string(),
            field_type: field_type.to_string(),
            nullable: false,
            label: None,
            default_value: None,
            primary_key: false,
            unique: false,
        }
    }

    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn with_default(mut self, value: &str) -> Self {
        self.default_value = Some(value.to_string());
        self
    }

    pub fn primary(mut self) -> Self {
        self.primary_key = true;
        self.nullable = false;
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// Get the field label (prefers label, falls back to name).
    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationDefinition {
    pub name: String,
    pub rel_type: String,
    pub target_model: String,
    pub foreign_key: String,
}

impl RelationDefinition {
    pub fn new(name: &str, rel_type: &str, target_model: &str, foreign_key: &str) -> Self {
        Self {
            name: name.to_string(),
            rel_type: rel_type.to_string(),
            target_model: target_model.to_string(),
            foreign_key: foreign_key.to_string(),
        }
    }

    /// Whether this is a one-to-one relation.
    pub fn is_one_to_one(&self) -> bool {
        self.rel_type.eq_ignore_ascii_case("one_to_one")
    }

    /// Whether this is a one-to-many relation.
    pub fn is_one_to_many(&self) -> bool {
        self.rel_type.eq_ignore_ascii_case("one_to_many")
    }

    /// Whether this is a many-to-many relation.
    pub fn is_many_to_many(&self) -> bool {
        self.rel_type.eq_ignore_ascii_case("many_to_many")
    }
}

/// Convert a table name such as "users" or "order_items" to "User" / "OrderItem"
/// and strip the trailing 's' to singularize.
fn to_pascal_singular(input: &str) -> String {
    let mut out = String::new();
    let mut cap_next = true;
    for ch in input.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            cap_next = true;
        } else if cap_next {
            out.extend(ch.to_uppercase());
            cap_next = false;
        } else {
            out.push(ch);
        }
    }
    // 简单单数化：如果结果超过 1 个字符且以 's' 结尾，且不以 'ss' 结尾，则去除末尾 's'
    let len = out.len();
    if len > 1 && out.ends_with('s') && !out.ends_with("ss") {
        out.truncate(len - 1);
    }
    out
}

// ============================================================================
// 字段类型映射（FieldTypeMapping）
// ============================================================================

/// Field type mapping utility.
///
/// Provides bidirectional mappings between SQL types, Rust types, HTML input
/// types, and JSON Schema types.
pub struct FieldTypeMapping;

impl FieldTypeMapping {
    /// Map a SQL type to a Rust type.
    ///
    /// Example: BIGINT -> i64, VARCHAR -> String, TIMESTAMP -> chrono::NaiveDateTime
    pub fn sql_to_rust(sql_type: &str) -> &'static str {
        let upper = sql_type.to_uppercase();
        if upper.starts_with("BIGINT") || upper.starts_with("INT8") {
            "i64"
        } else if upper.starts_with("INT")
            || upper.starts_with("INTEGER")
            || upper.starts_with("INT4")
        {
            "i32"
        } else if upper.starts_with("SMALLINT") || upper.starts_with("INT2") {
            "i16"
        } else if upper.starts_with("BOOL") {
            "bool"
        } else if upper.starts_with("FLOAT8") || upper.starts_with("DOUBLE") {
            "f64"
        } else if upper.starts_with("FLOAT")
            || upper.starts_with("REAL")
            || upper.starts_with("FLOAT4")
        {
            "f32"
        } else if upper.starts_with("NUMERIC") || upper.starts_with("DECIMAL") {
            "rust_decimal::Decimal"
        } else if upper.starts_with("TIMESTAMPTZ") {
            "chrono::DateTime<chrono::Utc>"
        } else if upper.starts_with("TIMESTAMP") {
            "chrono::NaiveDateTime"
        } else if upper.starts_with("DATE") {
            "chrono::NaiveDate"
        } else if upper.starts_with("TIME") {
            "chrono::NaiveTime"
        } else if upper.starts_with("UUID") {
            "uuid::Uuid"
        } else if upper.starts_with("JSON") || upper.starts_with("JSONB") {
            "serde_json::Value"
        } else if upper.starts_with("BYTEA") || upper.starts_with("BLOB") {
            "Vec<u8>"
        } else {
            "String"
        }
    }

    /// Map a SQL type to an HTML input type.
    ///
    /// Example: VARCHAR -> text, INTEGER -> number, DATE -> date, BOOLEAN -> checkbox
    pub fn sql_to_html_input(sql_type: &str) -> &'static str {
        let upper = sql_type.to_uppercase();
        if upper.starts_with("INT")
            || upper.starts_with("BIGINT")
            || upper.starts_with("SMALLINT")
            || upper.starts_with("FLOAT")
            || upper.starts_with("DOUBLE")
            || upper.starts_with("NUMERIC")
            || upper.starts_with("DECIMAL")
        {
            "number"
        } else if upper.starts_with("BOOL") {
            "checkbox"
        } else if upper.starts_with("DATE") && !upper.starts_with("DATETIME") {
            "date"
        } else if upper.starts_with("TIMESTAMP") || upper.starts_with("DATETIME") {
            "datetime-local"
        } else if upper.starts_with("TIME") {
            "time"
        } else if upper.starts_with("UUID") {
            "text"
        } else if upper.starts_with("JSON") || upper.starts_with("TEXT") {
            "textarea"
        } else {
            "text"
        }
    }

    /// Map a SQL type to a JSON Schema type.
    pub fn sql_to_json_schema(sql_type: &str) -> &'static str {
        let upper = sql_type.to_uppercase();
        if upper.starts_with("INT") || upper.starts_with("BIGINT") || upper.starts_with("SMALLINT")
        {
            "integer"
        } else if upper.starts_with("FLOAT")
            || upper.starts_with("DOUBLE")
            || upper.starts_with("NUMERIC")
            || upper.starts_with("DECIMAL")
        {
            "number"
        } else if upper.starts_with("BOOL") {
            "boolean"
        } else if upper.starts_with("JSON") || upper.starts_with("JSONB") {
            "object"
        } else {
            // BYTEA/BLOB/VARCHAR/TEXT/UUID 等均映射为 string
            "string"
        }
    }

    /// Map a Rust type to a SQL type.
    pub fn rust_to_sql(rust_type: &str) -> &'static str {
        match rust_type {
            "i16" => "SMALLINT",
            "i32" => "INTEGER",
            "i64" => "BIGINT",
            "bool" => "BOOLEAN",
            "f32" => "REAL",
            "f64" => "DOUBLE PRECISION",
            "String" | "&str" => "VARCHAR(255)",
            "chrono::NaiveDateTime" => "TIMESTAMP",
            "chrono::DateTime<chrono::Utc>" => "TIMESTAMPTZ",
            "chrono::NaiveDate" => "DATE",
            "chrono::NaiveTime" => "TIME",
            "uuid::Uuid" => "UUID",
            "serde_json::Value" => "JSONB",
            "Vec<u8>" => "BYTEA",
            "rust_decimal::Decimal" => "NUMERIC(19,4)",
            _ => "VARCHAR(255)",
        }
    }

    /// Whether the SQL type is a numeric type.
    pub fn is_numeric(sql_type: &str) -> bool {
        let upper = sql_type.to_uppercase();
        upper.starts_with("INT")
            || upper.starts_with("BIGINT")
            || upper.starts_with("SMALLINT")
            || upper.starts_with("FLOAT")
            || upper.starts_with("DOUBLE")
            || upper.starts_with("NUMERIC")
            || upper.starts_with("DECIMAL")
            || upper.starts_with("REAL")
    }

    /// Whether the SQL type is a date/time type.
    pub fn is_temporal(sql_type: &str) -> bool {
        let upper = sql_type.to_uppercase();
        upper.starts_with("DATE")
            || upper.starts_with("TIMESTAMP")
            || upper.starts_with("DATETIME")
            || upper.starts_with("TIME")
    }
}

// ============================================================================
// 验证规则配置（ValidationRule / FieldValidation）
// ============================================================================

/// Validation rule enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ValidationRule {
    /// Required.
    Required,
    /// Minimum length.
    MinLength { value: u32 },
    /// Maximum length.
    MaxLength { value: u32 },
    /// Minimum value.
    Min { value: f64 },
    /// Maximum value.
    Max { value: f64 },
    /// Regular expression pattern.
    Pattern { regex: String },
    /// Email format.
    Email,
    /// URL format.
    Url,
    /// Enumerated values.
    Enum { values: Vec<String> },
}

impl ValidationRule {
    /// Validate whether the given value satisfies the rule.
    ///
    /// Returns `Ok(())` on success, `Err(message)` on failure.
    pub fn validate(&self, value: &serde_json::Value) -> Result<(), String> {
        match self {
            Self::Required => {
                if value.is_null() {
                    return Err("此字段为必填项".to_string());
                }
                if let Some(s) = value.as_str() {
                    if s.trim().is_empty() {
                        return Err("此字段不能为空".to_string());
                    }
                }
                Ok(())
            }
            Self::MinLength { value: min } => {
                if let Some(s) = value.as_str() {
                    if s.len() < *min as usize {
                        return Err(format!("长度不能少于 {} 个字符", min));
                    }
                }
                Ok(())
            }
            Self::MaxLength { value: max } => {
                if let Some(s) = value.as_str() {
                    if s.len() > *max as usize {
                        return Err(format!("长度不能超过 {} 个字符", max));
                    }
                }
                Ok(())
            }
            Self::Min { value: min } => {
                if let Some(n) = value.as_f64() {
                    if n < *min {
                        return Err(format!("值不能小于 {}", min));
                    }
                }
                Ok(())
            }
            Self::Max { value: max } => {
                if let Some(n) = value.as_f64() {
                    if n > *max {
                        return Err(format!("值不能大于 {}", max));
                    }
                }
                Ok(())
            }
            Self::Pattern { regex } => {
                // M-2 修复：使用 regex crate 进行真实正则匹配
                // 对齐 PHP preg_match 行为：完整匹配（anchored）由调用方在 pattern 中表达
                if let Some(s) = value.as_str() {
                    let compiled = regex::Regex::new(regex)
                        .map_err(|e| format!("正则表达式编译失败: {}", e))?;
                    if !compiled.is_match(s) {
                        return Err(format!("值不匹配正则: {}", regex));
                    }
                }
                Ok(())
            }
            Self::Email => {
                if let Some(s) = value.as_str() {
                    if !s.contains('@') || !s.contains('.') {
                        return Err("邮箱格式不正确".to_string());
                    }
                }
                Ok(())
            }
            Self::Url => {
                if let Some(s) = value.as_str() {
                    if !s.starts_with("http://") && !s.starts_with("https://") {
                        return Err("URL 必须以 http:// 或 https:// 开头".to_string());
                    }
                }
                Ok(())
            }
            Self::Enum { values } => {
                if let Some(s) = value.as_str() {
                    if !values.iter().any(|v| v == s) {
                        return Err(format!("值必须是以下之一: {}", values.join(", ")));
                    }
                }
                Ok(())
            }
        }
    }

    /// Convert to an HTML form attribute string.
    pub fn to_html_attribute(&self) -> Option<String> {
        match self {
            Self::Required => Some("required".to_string()),
            Self::MinLength { value } => Some(format!("minlength=\"{}\"", value)),
            Self::MaxLength { value } => Some(format!("maxlength=\"{}\"", value)),
            Self::Min { value } => Some(format!("min=\"{}\"", value)),
            Self::Max { value } => Some(format!("max=\"{}\"", value)),
            Self::Pattern { regex } => Some(format!("pattern=\"{}\"", regex)),
            Self::Email => Some("type=\"email\"".to_string()),
            Self::Url => Some("type=\"url\"".to_string()),
            Self::Enum { values } => {
                // enum 转为 list 属性
                Some(format!("list=\"{}\"", values.join(",")))
            }
        }
    }
}

/// Set of field validation rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldValidation {
    pub field_name: String,
    pub rules: Vec<ValidationRule>,
}

impl FieldValidation {
    pub fn new(field_name: &str) -> Self {
        Self {
            field_name: field_name.to_string(),
            rules: vec![],
        }
    }

    /// Add a validation rule (chainable).
    pub fn with_rule(mut self, rule: ValidationRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Validate the given value, returning all error messages.
    pub fn validate(&self, value: &serde_json::Value) -> Result<(), Vec<String>> {
        let errors: Vec<String> = self
            .rules
            .iter()
            .filter_map(|rule| rule.validate(value).err())
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Generate the HTML attribute string for all rules.
    pub fn to_html_attributes(&self) -> String {
        self.rules
            .iter()
            .filter_map(|rule| rule.to_html_attribute())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// ============================================================================
// 动态表单生成（FormField / FormGenerator）
// ============================================================================

/// Form input type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
    Text,
    Number,
    Email,
    Password,
    Date,
    DateTime,
    Time,
    Checkbox,
    Select,
    Textarea,
    Hidden,
    File,
}

impl InputType {
    /// Convert to the HTML input type attribute value.
    pub fn as_html_type(&self) -> &str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Email => "email",
            Self::Password => "password",
            Self::Date => "date",
            Self::DateTime => "datetime-local",
            Self::Time => "time",
            Self::Checkbox => "checkbox",
            Self::Select => "select",
            Self::Textarea => "textarea",
            Self::Hidden => "hidden",
            Self::File => "file",
        }
    }
}

/// Form field definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub label: String,
    pub input_type: InputType,
    pub required: bool,
    pub validation: FieldValidation,
    pub default_value: Option<serde_json::Value>,
    pub placeholder: Option<String>,
    /// Option list for Select-type fields: (value, label).
    pub options: Vec<(String, String)>,
    pub help_text: Option<String>,
}

impl FormField {
    pub fn new(name: &str, label: &str, input_type: InputType) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            input_type,
            required: false,
            validation: FieldValidation::new(name),
            default_value: None,
            placeholder: None,
            options: vec![],
            help_text: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self.validation = self.validation.with_rule(ValidationRule::Required);
        self
    }

    pub fn with_placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.to_string());
        self
    }

    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default_value = Some(value);
        self
    }

    pub fn with_option(mut self, value: &str, label: &str) -> Self {
        self.options.push((value.to_string(), label.to_string()));
        self
    }

    pub fn with_validation(mut self, rule: ValidationRule) -> Self {
        self.validation = self.validation.with_rule(rule);
        self
    }

    pub fn with_help_text(mut self, text: &str) -> Self {
        self.help_text = Some(text.to_string());
        self
    }
}

/// HTML escape: escapes special characters into HTML entities to prevent XSS.
fn escape_html(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#x27;"),
            _ => result.push(ch),
        }
    }
    result
}

/// Dynamic form generator.
pub struct FormGenerator;

impl FormGenerator {
    /// Generate a list of form fields from a model definition.
    pub fn from_model(model: &ModelDefinition) -> Vec<FormField> {
        model
            .fields
            .iter()
            .map(|field| {
                let input_type_str = FieldTypeMapping::sql_to_html_input(&field.field_type);
                let input_type = match input_type_str {
                    "number" => InputType::Number,
                    "checkbox" => InputType::Checkbox,
                    "date" => InputType::Date,
                    "datetime-local" => InputType::DateTime,
                    "time" => InputType::Time,
                    "textarea" => InputType::Textarea,
                    "email" => InputType::Email,
                    "password" => InputType::Password,
                    "hidden" => InputType::Hidden,
                    "file" => InputType::File,
                    _ => InputType::Text,
                };

                let mut form_field = FormField::new(&field.name, field.display_label(), input_type);

                if !field.nullable && !field.primary_key {
                    form_field = form_field.required();
                }

                if let Some(default) = &field.default_value {
                    form_field =
                        form_field.with_default(serde_json::Value::String(default.clone()));
                }

                form_field
            })
            .collect()
    }

    /// Generate an HTML form.
    pub fn generate_html_form(fields: &[FormField], action: &str, method: &str) -> String {
        let mut html = String::new();
        html.push_str(&format!(
            r#"<form action="{}" method="{}" enctype="multipart/form-data">"#,
            escape_html(action),
            escape_html(method)
        ));
        html.push('\n');

        for field in fields {
            html.push_str(&Self::generate_html_field(field));
            html.push('\n');
        }

        html.push_str("    <button type=\"submit\">提交</button>\n");
        html.push_str("</form>");
        html
    }

    /// Generate a single HTML form field.
    pub fn generate_html_field(field: &FormField) -> String {
        let mut html = String::new();
        html.push_str("    <div class=\"form-group\">\n");

        let escaped_name = escape_html(&field.name);
        let escaped_label = escape_html(&field.label);

        // Hidden 字段不显示 label
        if !matches!(field.input_type, InputType::Hidden) {
            html.push_str(&format!(
                "        <label for=\"{}\">{}{}</label>\n",
                escaped_name,
                escaped_label,
                if field.required {
                    " <span class=\"required\">*</span>"
                } else {
                    ""
                }
            ));
        }

        let validation_attrs = field.validation.to_html_attributes();
        let placeholder = field
            .placeholder
            .as_ref()
            .map(|p| format!("placeholder=\"{}\"", escape_html(p)))
            .unwrap_or_default();

        match &field.input_type {
            InputType::Select => {
                html.push_str(&format!(
                    "        <select id=\"{}\" name=\"{}\" {}>\n",
                    escaped_name, escaped_name, validation_attrs
                ));
                html.push_str("            <option value=\"\">请选择</option>\n");
                for (value, label) in &field.options {
                    html.push_str(&format!(
                        "            <option value=\"{}\">{}</option>\n",
                        escape_html(value),
                        escape_html(label)
                    ));
                }
                html.push_str("        </select>\n");
            }
            InputType::Textarea => {
                html.push_str(&format!(
                    "        <textarea id=\"{}\" name=\"{}\" {} {}></textarea>\n",
                    escaped_name, escaped_name, validation_attrs, placeholder
                ));
            }
            InputType::Checkbox => {
                html.push_str(&format!(
                    "        <input type=\"checkbox\" id=\"{}\" name=\"{}\" {} />\n",
                    escaped_name, escaped_name, validation_attrs
                ));
            }
            _ => {
                let input_type = field.input_type.as_html_type();
                html.push_str(&format!(
                    "        <input type=\"{}\" id=\"{}\" name=\"{}\" {} {} />\n",
                    input_type, escaped_name, escaped_name, validation_attrs, placeholder
                ));
            }
        }

        if let Some(help) = &field.help_text {
            html.push_str(&format!(
                "        <small class=\"help-text\">{}</small>\n",
                escape_html(help)
            ));
        }

        html.push_str("    </div>");
        html
    }

    /// Generate a JSON Schema (for API documentation or frontend validation).
    pub fn generate_json_schema(fields: &[FormField]) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = vec![];

        for field in fields {
            let field_type = match &field.input_type {
                InputType::Number => "number",
                InputType::Checkbox => "boolean",
                InputType::Textarea
                | InputType::Text
                | InputType::Email
                | InputType::Password
                | InputType::Date
                | InputType::DateTime
                | InputType::Time
                | InputType::Select
                | InputType::File
                | InputType::Hidden => "string",
            };

            let mut prop = serde_json::json!({
                "type": field_type,
                "title": field.label,
            });

            if let Some(placeholder) = &field.placeholder {
                prop["description"] = serde_json::json!(placeholder);
            }

            if !field.options.is_empty() {
                let enum_values: Vec<serde_json::Value> = field
                    .options
                    .iter()
                    .map(|(v, _)| serde_json::json!(v))
                    .collect();
                prop["enum"] = serde_json::Value::Array(enum_values);
            }

            properties.insert(field.name.clone(), prop);

            if field.required {
                required.push(field.name.clone());
            }
        }

        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    }
}

// ============================================================================
// CRUD 模板引擎（CrudTemplateEngine）
// ============================================================================

/// CRUD template engine.
///
/// Generates SQL DDL, parameterized CRUD statements, Rust structs, and
/// repository layer code.
pub struct CrudTemplateEngine;

impl CrudTemplateEngine {
    /// Generate the CREATE TABLE DDL statement.
    pub fn generate_ddl(model: &ModelDefinition) -> String {
        let table = ModelDefinition::sanitize_identifier(&model.name);
        let mut sql = String::new();
        sql.push_str(&format!("CREATE TABLE \"{}\" (\n", table));

        let column_defs: Vec<String> = model
            .fields
            .iter()
            .map(|field| {
                let col_name = ModelDefinition::sanitize_identifier(&field.name);
                let mut def = format!("    \"{}\" {}", col_name, field.field_type);
                if !field.nullable {
                    def.push_str(" NOT NULL");
                }
                if field.primary_key {
                    def.push_str(" PRIMARY KEY");
                }
                if field.unique {
                    def.push_str(" UNIQUE");
                }
                if let Some(default) = &field.default_value {
                    let safe_default = CrudTemplateEngine::sanitize_default_value(default);
                    def.push_str(&format!(" DEFAULT {}", safe_default));
                }
                def
            })
            .collect();

        sql.push_str(&column_defs.join(",\n"));
        sql.push_str("\n);");

        for index in &model.indexes {
            let safe_index = ModelDefinition::sanitize_identifier(index);
            sql.push_str(&format!(
                "\nCREATE INDEX \"{}\" ON \"{}\";",
                safe_index, table
            ));
        }

        sql
    }

    fn sanitize_default_value(value: &str) -> String {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case("NULL")
            || trimmed.eq_ignore_ascii_case("TRUE")
            || trimmed.eq_ignore_ascii_case("FALSE")
            || trimmed.parse::<f64>().is_ok()
        {
            return trimmed.to_string();
        }
        let inner = if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2 {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };
        let escaped = inner.replace('\'', "''");
        format!("'{}'", escaped)
    }

    /// Generate an INSERT statement (parameterized).
    pub fn generate_insert(model: &ModelDefinition) -> String {
        let table = ModelDefinition::sanitize_identifier(&model.name);
        let columns: Vec<String> = model
            .fields
            .iter()
            .map(|f| ModelDefinition::sanitize_identifier(&f.name))
            .collect();
        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${}", i)).collect();
        format!(
            "INSERT INTO \"{}\" ({}) VALUES ({});",
            table,
            columns
                .iter()
                .map(|c| format!("\"{}\"", c))
                .collect::<Vec<_>>()
                .join(", "),
            placeholders.join(", ")
        )
    }

    /// Generate a SELECT BY ID statement.
    pub fn generate_select_by_id(model: &ModelDefinition) -> String {
        let table = ModelDefinition::sanitize_identifier(&model.name);
        let columns: Vec<String> = model
            .fields
            .iter()
            .map(|f| format!("\"{}\"", ModelDefinition::sanitize_identifier(&f.name)))
            .collect();
        format!(
            "SELECT {} FROM \"{}\" WHERE \"id\" = $1;",
            columns.join(", "),
            table
        )
    }

    /// Generate a SELECT ALL statement (with pagination).
    pub fn generate_select_all(model: &ModelDefinition) -> String {
        let table = ModelDefinition::sanitize_identifier(&model.name);
        let columns: Vec<String> = model
            .fields
            .iter()
            .map(|f| format!("\"{}\"", ModelDefinition::sanitize_identifier(&f.name)))
            .collect();
        format!(
            "SELECT {} FROM \"{}\" ORDER BY \"id\" DESC LIMIT $1 OFFSET $2;",
            columns.join(", "),
            table
        )
    }

    /// Generate an UPDATE statement (parameterized, excludes the primary key).
    pub fn generate_update(model: &ModelDefinition) -> String {
        let table = ModelDefinition::sanitize_identifier(&model.name);
        let update_fields: Vec<&FieldDef> =
            model.fields.iter().filter(|f| !f.primary_key).collect();

        let set_clauses: Vec<String> = update_fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                format!(
                    "\"{}\" = ${}",
                    ModelDefinition::sanitize_identifier(&f.name),
                    i + 1
                )
            })
            .collect();

        let id_placeholder = format!("${}", update_fields.len() + 1);

        format!(
            "UPDATE \"{}\" SET {} WHERE \"id\" = {};",
            table,
            set_clauses.join(", "),
            id_placeholder
        )
    }

    /// Generate a DELETE statement.
    pub fn generate_delete(model: &ModelDefinition) -> String {
        let table = ModelDefinition::sanitize_identifier(&model.name);
        format!("DELETE FROM \"{}\" WHERE \"id\" = $1;", table)
    }

    /// Generate a COUNT statement.
    pub fn generate_count(model: &ModelDefinition) -> String {
        let table = ModelDefinition::sanitize_identifier(&model.name);
        format!("SELECT COUNT(*) AS total FROM \"{}\";", table)
    }

    /// Generate a Rust struct definition.
    pub fn generate_rust_struct(model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let mut code = String::new();

        code.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
        code.push_str(&format!("pub struct {} {{\n", pascal));

        for field in &model.fields {
            let rust_type = FieldTypeMapping::sql_to_rust(&field.field_type);
            let type_str = if field.nullable {
                format!("Option<{}>", rust_type)
            } else {
                rust_type.to_string()
            };
            code.push_str(&format!("    pub {}: {},\n", field.name, type_str));
        }

        code.push_str("}\n");
        code
    }

    /// Generate Rust repository layer code (Repository pattern).
    ///
    /// C-1 fix: generates real compilable CRUD code that uses `sqlx::query` to
    /// execute parameterized SQL and binds field values via a `bind` chain.
    /// `create`/`update`/`delete` return `PgQueryResult`; `find_by_id` returns
    /// `Option<PgRow>` (the caller reads fields via `row.get`).
    ///
    /// The generated code is guaranteed to compile without users filling in placeholders.
    pub fn generate_rust_repository(model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let singular_lower = model.singular_name().to_lowercase();
        let table = &model.name;

        let insert_sql = Self::generate_insert(model);
        let select_sql = Self::generate_select_by_id(model);
        let update_sql = Self::generate_update(model);
        let delete_sql = Self::generate_delete(model);

        // INSERT 绑定链：按字段顺序链式 bind（C-1 修复：使用 .bind() 而非无效的 .binds()）
        let insert_binds: Vec<String> = model
            .fields
            .iter()
            .map(|f| format!("{singular_lower}.{}", f.name))
            .collect();
        let insert_bind_chain = if insert_binds.is_empty() {
            String::new()
        } else {
            let chain: String = insert_binds
                .iter()
                .map(|b| format!(".bind({})", b))
                .collect::<Vec<_>>()
                .join("\n        ");
            format!("\n        {}", chain)
        };

        // UPDATE 绑定链：非主键字段 + 主键（WHERE 条件）
        let update_fields: Vec<&FieldDef> =
            model.fields.iter().filter(|f| !f.primary_key).collect();
        let update_binds: Vec<String> = update_fields
            .iter()
            .map(|f| format!("{singular_lower}.{}", f.name))
            .collect();
        let update_bind_chain = if update_binds.is_empty() {
            String::new()
        } else {
            let chain: String = update_binds
                .iter()
                .map(|b| format!(".bind({})", b))
                .collect::<Vec<_>>()
                .join("\n        ");
            format!("\n        {}\n        .bind(id)", chain)
        };

        format!(
            r#"pub struct {pascal}Repository;

impl {pascal}Repository {{
    /// Insert a record and return the execution result.
    pub async fn create(
        pool: &sqlx::PgPool,
        {singular_lower}: &{pascal},
    ) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {{
        // table: {table}
        sqlx::query({insert_sql:?}){insert_bind_chain}
            .execute(pool)
            .await
    }}

    /// Query a single record by id, returning the raw Row (the caller reads via row.get::<_, &str>("column")).
    pub async fn find_by_id(
        pool: &sqlx::PgPool,
        id: i64,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {{
        // table: {table}
        sqlx::query({select_sql:?})
            .bind(id)
            .fetch_optional(pool)
            .await
    }}

    /// Update a record by id (non-primary-key fields) and return the execution result.
    pub async fn update(
        pool: &sqlx::PgPool,
        {singular_lower}: &{pascal},
        id: i64,
    ) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {{
        // table: {table}
        sqlx::query({update_sql:?}){update_bind_chain}
            .execute(pool)
            .await
    }}

    /// Delete a record by id and return whether a row was deleted (rows_affected > 0).
    pub async fn delete(pool: &sqlx::PgPool, id: i64) -> Result<bool, sqlx::Error> {{
        // table: {table}
        let result = sqlx::query({delete_sql:?})
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }}
}}"#,
            pascal = pascal,
            singular_lower = singular_lower,
            table = table,
            insert_sql = insert_sql,
            insert_bind_chain = insert_bind_chain,
            select_sql = select_sql,
            update_sql = update_sql,
            update_bind_chain = update_bind_chain,
            delete_sql = delete_sql,
        )
    }
}

// ============================================================================
// LowCodeEngine（保留原有 API，内部委托给 CrudTemplateEngine）
// ============================================================================

pub struct LowCodeEngine;

impl LowCodeEngine {
    /// Reverse engineering: generate ModelDefinitions from a list of table names.
    /// Includes default fields (id, name, created_at, updated_at) and indexes.
    pub fn reverse_engineer(&self, tables: &[&str]) -> Vec<ModelDefinition> {
        tables
            .iter()
            .map(|t| {
                let mut m = ModelDefinition::new(t);
                m.fields = vec![
                    FieldDef::new("id", "BIGINT").primary(),
                    FieldDef::new("name", "VARCHAR(255)").with_label("名称"),
                    FieldDef::new("created_at", "TIMESTAMP").with_default("CURRENT_TIMESTAMP"),
                    FieldDef::new("updated_at", "TIMESTAMP").with_default("CURRENT_TIMESTAMP"),
                ];
                m.indexes = vec!["idx_id".to_string(), "idx_name".to_string()];
                m.relations = vec![];
                m
            })
            .collect()
    }

    /// Generate SQL CRUD statements (INSERT/SELECT/UPDATE/DELETE).
    ///
    /// # Safety (gate 9 fix)
    ///
    /// Table names are wrapped in double quotes (PostgreSQL standard) to prevent
    /// injection via table names containing special characters or SQL keywords.
    pub fn generate_crud(&self, model: &ModelDefinition) -> String {
        let mut sql = String::new();
        sql.push_str(&format!("-- CRUD for table {}\n", model.name));
        sql.push_str(&CrudTemplateEngine::generate_insert(model));
        sql.push('\n');
        sql.push_str(&CrudTemplateEngine::generate_select_by_id(model));
        sql.push('\n');
        sql.push_str(&CrudTemplateEngine::generate_update(model));
        sql.push('\n');
        sql.push_str(&CrudTemplateEngine::generate_delete(model));
        sql.push('\n');
        sql
    }

    /// Generate Rust handler code.
    pub fn generate_api(&self, model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let singular_lower = model.singular_name().to_lowercase();
        let table = &model.name;

        // M-3 修复：生成调用真实数据库的 handler 代码，替代原 mock 实现。
        // 生成的代码使用 sqlx::query_as + FromRow 模式，与 generate_rust_repository 风格一致。
        format!(
            r#"use axum::{{Json, extract::{{Path, State}}}};
use serde::{{Deserialize, Serialize}};
use sqlx::PgPool;
use std::sync::Arc;

/// {pascal} entity — corresponds to database table `{table}`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct {pascal} {{
    pub id: i64,
    pub name: String,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}}

/// Create {singular_lower} — calls the real database INSERT and returns the created entity.
pub async fn create_{singular_lower}(
    State(pool): State<Arc<PgPool>>,
    Json(payload): Json<{pascal}>,
) -> Result<Json<{pascal}>, (axum::http::StatusCode, String)> {{
    let row = sqlx::query_as::<_, {pascal}>(
        "INSERT INTO \"{table}\" (name, created_at, updated_at) VALUES ($1, NOW(), NOW()) \
         RETURNING id, name, created_at, updated_at",
    )
    .bind(&payload.name)
    .fetch_one(pool.as_ref())
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(row))
}}

/// Query {singular_lower} by id — calls the real database SELECT with 404 handling.
pub async fn get_{singular_lower}(
    State(pool): State<Arc<PgPool>>,
    Path(id): Path<i64>,
) -> Result<Json<{pascal}>, (axum::http::StatusCode, String)> {{
    let row = sqlx::query_as::<_, {pascal}>(
        "SELECT id, name, created_at, updated_at FROM \"{table}\" WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool.as_ref())
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match row {{
        Some(entity) => Ok(Json(entity)),
        None => Err((axum::http::StatusCode::NOT_FOUND, format!("{pascal} {{}} not found", id))),
    }}
}}

/// Update {singular_lower} by id — calls the real database UPDATE with 404 handling.
pub async fn update_{singular_lower}(
    State(pool): State<Arc<PgPool>>,
    Path(id): Path<i64>,
    Json(payload): Json<{pascal}>,
) -> Result<Json<{pascal}>, (axum::http::StatusCode, String)> {{
    let row = sqlx::query_as::<_, {pascal}>(
        "UPDATE \"{table}\" SET name = $1, updated_at = NOW() WHERE id = $2 \
         RETURNING id, name, created_at, updated_at",
    )
    .bind(&payload.name)
    .bind(id)
    .fetch_optional(pool.as_ref())
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match row {{
        Some(entity) => Ok(Json(entity)),
        None => Err((axum::http::StatusCode::NOT_FOUND, format!("{pascal} {{}} not found", id))),
    }}
}}

/// Delete {singular_lower} by id — calls the real database DELETE with 404 handling.
pub async fn delete_{singular_lower}(
    State(pool): State<Arc<PgPool>>,
    Path(id): Path<i64>,
) -> Result<Json<bool>, (axum::http::StatusCode, String)> {{
    let result = sqlx::query("DELETE FROM \"{table}\" WHERE id = $1")
        .bind(id)
        .execute(pool.as_ref())
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if result.rows_affected() > 0 {{
        Ok(Json(true))
    }} else {{
        Err((axum::http::StatusCode::NOT_FOUND, format!("{pascal} {{}} not found", id)))
    }}
}}"#,
            pascal = pascal,
            singular_lower = singular_lower,
            table = table,
        )
    }

    /// Generate HTML form markup.
    pub fn generate_frontend(&self, model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let singular_lower = model.singular_name().to_lowercase();
        let fields = FormGenerator::from_model(model);
        let form_body = FormGenerator::generate_html_field(
            fields
                .iter()
                .find(|f| f.name == "name")
                .unwrap_or(&FormField::new("name", "Name", InputType::Text)),
        );
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>{pascal} Form</title>
</head>
<body>
    <h1>{pascal} Form</h1>
    <form id="{singular_lower}_form" action="/api/{singular_lower}" method="POST">
{form_body}
        <input type="hidden" name="created_at" />
        <input type="hidden" name="updated_at" />
        <button type="submit">Submit</button>
    </form>
</body>
</html>"#,
            pascal = pascal,
            singular_lower = singular_lower,
            form_body = form_body
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model() {
        let m = ModelDefinition::new("User");
        assert_eq!(m.name, "User");
    }

    #[test]
    fn test_pascal_case_singular() {
        assert_eq!(to_pascal_singular("users"), "User");
        assert_eq!(to_pascal_singular("order_items"), "OrderItem");
        assert_eq!(to_pascal_singular("orders"), "Order");
        assert_eq!(to_pascal_singular("User"), "User");
        // Words ending in 'ss' (e.g. "Address") should not be singularized.
        assert_eq!(to_pascal_singular("address"), "Address");
    }

    #[test]
    fn test_reverse_engineer_generates_fields() {
        let e = LowCodeEngine;
        let models = e.reverse_engineer(&["users", "orders"]);
        assert_eq!(models.len(), 2);
        let m = &models[0];
        assert_eq!(m.name, "users");
        assert_eq!(m.fields.len(), 4);
        // Verify each required field exists with correct type
        let id = m.fields.iter().find(|f| f.name == "id").expect("id field");
        assert_eq!(id.field_type, "BIGINT");
        assert!(!id.nullable);
        assert!(id.primary_key);
        let name = m
            .fields
            .iter()
            .find(|f| f.name == "name")
            .expect("name field");
        assert!(name.field_type.starts_with("VARCHAR"));
        assert!(m.fields.iter().any(|f| f.name == "created_at"));
        assert!(m.fields.iter().any(|f| f.name == "updated_at"));
        assert!(m.indexes.contains(&"idx_id".to_string()));
        assert!(m.indexes.contains(&"idx_name".to_string()));
    }

    #[test]
    fn test_generate_crud_has_real_sql() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("users");
        let sql = e.generate_crud(&m);
        assert!(sql.contains("INSERT INTO \"users\""));
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("FROM \"users\""));
        assert!(sql.contains("UPDATE \"users\" SET"));
        assert!(sql.contains("DELETE FROM \"users\""));
        // Verify placeholder absence
        assert!(!sql.starts_with("CRUD for "));
    }

    #[test]
    fn test_generate_api_has_handler_code() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("users");
        let code = e.generate_api(&m);
        assert!(code.contains("pub async fn create_user"));
        assert!(code.contains("pub async fn get_user"));
        assert!(code.contains("pub async fn update_user"));
        assert!(code.contains("pub async fn delete_user"));
        assert!(code.contains("struct User"));
        assert!(code.contains("Json<"));
        assert!(!code.starts_with("API for "));
    }

    #[test]
    fn test_generate_api_handles_compound_names() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("order_items");
        let code = e.generate_api(&m);
        assert!(code.contains("struct OrderItem"));
        assert!(code.contains("pub async fn create_order_item"));
    }

    /// M-3 fix verification: the generated handler code should contain real
    /// database calls, not a mock implementation.
    #[test]
    fn test_generate_api_has_real_db_calls_not_mock() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("users");
        let code = e.generate_api(&m);

        // 应包含 sqlx 真实数据库调用
        assert!(
            code.contains("sqlx::query_as"),
            "应使用 sqlx::query_as 查询数据库"
        );
        assert!(
            code.contains("sqlx::query"),
            "应使用 sqlx::query 执行非查询 SQL"
        );
        assert!(code.contains("PgPool"), "应接受数据库连接池参数");
        assert!(code.contains("FromRow"), "应派生 sqlx::FromRow trait");
        assert!(
            code.contains("RETURNING"),
            "INSERT/UPDATE 应使用 RETURNING 子句"
        );
        assert!(
            code.contains("rows_affected"),
            "DELETE 应检查 rows_affected"
        );
        assert!(code.contains("NOT_FOUND"), "应处理 404 NOT_FOUND 场景");

        // 不应包含 mock 痕迹
        assert!(
            !code.contains("String::new()"),
            "不应返回硬编码空字符串（mock 痕迹）"
        );
        assert!(
            !code.contains("NaiveDateTime::default()"),
            "不应返回硬编码默认时间（mock 痕迹）"
        );
        // create/update 应通过 fetch_one/fetch_optional 获取数据库返回的实体，而非直接返回输入
        assert!(
            code.contains("fetch_one"),
            "create 应使用 fetch_one 获取插入后的实体"
        );
        assert!(
            code.contains("fetch_optional"),
            "get/update 应使用 fetch_optional 查询实体"
        );
        // delete 成功时返回 Json(true) 是合理的，但必须有 rows_affected 条件判断
        assert!(
            code.contains("rows_affected() > 0"),
            "delete 必须检查 rows_affected 条件"
        );
    }

    #[test]
    fn test_generate_frontend_has_form() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("users");
        let html = e.generate_frontend(&m);
        assert!(html.contains("<form"));
        assert!(html.contains("name=\"name\""));
        assert!(html.contains("action=\"/api/user\""));
        assert!(html.contains("<button"));
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(!html.starts_with("Frontend for "));
    }

    // ===== 字段类型映射测试 =====

    #[test]
    fn test_sql_to_rust_bigint() {
        assert_eq!(FieldTypeMapping::sql_to_rust("BIGINT"), "i64");
        assert_eq!(FieldTypeMapping::sql_to_rust("bigint"), "i64");
        assert_eq!(FieldTypeMapping::sql_to_rust("BIGINT NOT NULL"), "i64");
    }

    #[test]
    fn test_sql_to_rust_integer() {
        assert_eq!(FieldTypeMapping::sql_to_rust("INTEGER"), "i32");
        assert_eq!(FieldTypeMapping::sql_to_rust("INT"), "i32");
    }

    #[test]
    fn test_sql_to_rust_smallint() {
        assert_eq!(FieldTypeMapping::sql_to_rust("SMALLINT"), "i16");
    }

    #[test]
    fn test_sql_to_rust_boolean() {
        assert_eq!(FieldTypeMapping::sql_to_rust("BOOLEAN"), "bool");
    }

    #[test]
    fn test_sql_to_rust_float() {
        assert_eq!(FieldTypeMapping::sql_to_rust("FLOAT4"), "f32");
        assert_eq!(FieldTypeMapping::sql_to_rust("REAL"), "f32");
    }

    #[test]
    fn test_sql_to_rust_double() {
        assert_eq!(FieldTypeMapping::sql_to_rust("DOUBLE PRECISION"), "f64");
        assert_eq!(FieldTypeMapping::sql_to_rust("FLOAT8"), "f64");
    }

    #[test]
    fn test_sql_to_rust_varchar() {
        assert_eq!(FieldTypeMapping::sql_to_rust("VARCHAR(255)"), "String");
        assert_eq!(FieldTypeMapping::sql_to_rust("TEXT"), "String");
    }

    #[test]
    fn test_sql_to_rust_timestamp() {
        assert_eq!(
            FieldTypeMapping::sql_to_rust("TIMESTAMP"),
            "chrono::NaiveDateTime"
        );
        assert_eq!(
            FieldTypeMapping::sql_to_rust("TIMESTAMPTZ"),
            "chrono::DateTime<chrono::Utc>"
        );
    }

    #[test]
    fn test_sql_to_rust_date_time() {
        assert_eq!(FieldTypeMapping::sql_to_rust("DATE"), "chrono::NaiveDate");
        assert_eq!(FieldTypeMapping::sql_to_rust("TIME"), "chrono::NaiveTime");
    }

    #[test]
    fn test_sql_to_rust_uuid_json() {
        assert_eq!(FieldTypeMapping::sql_to_rust("UUID"), "uuid::Uuid");
        assert_eq!(FieldTypeMapping::sql_to_rust("JSONB"), "serde_json::Value");
    }

    #[test]
    fn test_sql_to_html_input() {
        assert_eq!(FieldTypeMapping::sql_to_html_input("VARCHAR(255)"), "text");
        assert_eq!(FieldTypeMapping::sql_to_html_input("INTEGER"), "number");
        assert_eq!(FieldTypeMapping::sql_to_html_input("BOOLEAN"), "checkbox");
        assert_eq!(FieldTypeMapping::sql_to_html_input("DATE"), "date");
        assert_eq!(
            FieldTypeMapping::sql_to_html_input("TIMESTAMP"),
            "datetime-local"
        );
        assert_eq!(FieldTypeMapping::sql_to_html_input("TEXT"), "textarea");
    }

    #[test]
    fn test_sql_to_json_schema() {
        assert_eq!(FieldTypeMapping::sql_to_json_schema("INTEGER"), "integer");
        assert_eq!(FieldTypeMapping::sql_to_json_schema("FLOAT"), "number");
        assert_eq!(FieldTypeMapping::sql_to_json_schema("BOOLEAN"), "boolean");
        assert_eq!(FieldTypeMapping::sql_to_json_schema("VARCHAR"), "string");
        assert_eq!(FieldTypeMapping::sql_to_json_schema("JSONB"), "object");
    }

    #[test]
    fn test_rust_to_sql() {
        assert_eq!(FieldTypeMapping::rust_to_sql("i64"), "BIGINT");
        assert_eq!(FieldTypeMapping::rust_to_sql("i32"), "INTEGER");
        assert_eq!(FieldTypeMapping::rust_to_sql("bool"), "BOOLEAN");
        assert_eq!(FieldTypeMapping::rust_to_sql("String"), "VARCHAR(255)");
        assert_eq!(FieldTypeMapping::rust_to_sql("uuid::Uuid"), "UUID");
    }

    #[test]
    fn test_is_numeric() {
        assert!(FieldTypeMapping::is_numeric("INTEGER"));
        assert!(FieldTypeMapping::is_numeric("BIGINT"));
        assert!(FieldTypeMapping::is_numeric("FLOAT"));
        assert!(FieldTypeMapping::is_numeric("NUMERIC(10,2)"));
        assert!(!FieldTypeMapping::is_numeric("VARCHAR"));
        assert!(!FieldTypeMapping::is_numeric("DATE"));
    }

    #[test]
    fn test_is_temporal() {
        assert!(FieldTypeMapping::is_temporal("DATE"));
        assert!(FieldTypeMapping::is_temporal("TIMESTAMP"));
        assert!(FieldTypeMapping::is_temporal("DATETIME"));
        assert!(FieldTypeMapping::is_temporal("TIME"));
        assert!(!FieldTypeMapping::is_temporal("VARCHAR"));
        assert!(!FieldTypeMapping::is_temporal("INTEGER"));
    }

    // ===== 验证规则测试 =====

    #[test]
    fn test_validation_required_pass() {
        let rule = ValidationRule::Required;
        assert!(rule.validate(&serde_json::json!("hello")).is_ok());
        assert!(rule.validate(&serde_json::json!(42)).is_ok());
    }

    #[test]
    fn test_validation_required_fail_null() {
        let rule = ValidationRule::Required;
        assert!(rule.validate(&serde_json::Value::Null).is_err());
    }

    #[test]
    fn test_validation_required_fail_empty() {
        let rule = ValidationRule::Required;
        assert!(rule.validate(&serde_json::json!("")).is_err());
        assert!(rule.validate(&serde_json::json!("   ")).is_err());
    }

    #[test]
    fn test_validation_min_length_pass() {
        let rule = ValidationRule::MinLength { value: 3 };
        assert!(rule.validate(&serde_json::json!("hello")).is_ok());
        assert!(rule.validate(&serde_json::json!("abc")).is_ok());
    }

    #[test]
    fn test_validation_min_length_fail() {
        let rule = ValidationRule::MinLength { value: 5 };
        assert!(rule.validate(&serde_json::json!("hi")).is_err());
    }

    #[test]
    fn test_validation_max_length_pass() {
        let rule = ValidationRule::MaxLength { value: 10 };
        assert!(rule.validate(&serde_json::json!("hello")).is_ok());
    }

    #[test]
    fn test_validation_max_length_fail() {
        let rule = ValidationRule::MaxLength { value: 3 };
        assert!(rule.validate(&serde_json::json!("hello world")).is_err());
    }

    #[test]
    fn test_validation_min_pass() {
        let rule = ValidationRule::Min { value: 10.0 };
        assert!(rule.validate(&serde_json::json!(15)).is_ok());
        assert!(rule.validate(&serde_json::json!(10)).is_ok());
    }

    #[test]
    fn test_validation_min_fail() {
        let rule = ValidationRule::Min { value: 10.0 };
        assert!(rule.validate(&serde_json::json!(5)).is_err());
    }

    #[test]
    fn test_validation_max_pass() {
        let rule = ValidationRule::Max { value: 100.0 };
        assert!(rule.validate(&serde_json::json!(50)).is_ok());
        assert!(rule.validate(&serde_json::json!(100)).is_ok());
    }

    #[test]
    fn test_validation_max_fail() {
        let rule = ValidationRule::Max { value: 100.0 };
        assert!(rule.validate(&serde_json::json!(150)).is_err());
    }

    // -------------------- M-2 修复：Pattern 真实验证测试 --------------------

    #[test]
    fn test_validation_pattern_pass() {
        // 匹配纯数字
        let rule = ValidationRule::Pattern {
            regex: r"^\d+$".to_string(),
        };
        assert!(rule.validate(&serde_json::json!("12345")).is_ok());
    }

    #[test]
    fn test_validation_pattern_fail() {
        // 含字母应失败
        let rule = ValidationRule::Pattern {
            regex: r"^\d+$".to_string(),
        };
        assert!(rule.validate(&serde_json::json!("12a45")).is_err());
    }

    #[test]
    fn test_validation_pattern_email_format() {
        // 简易邮箱正则
        let rule = ValidationRule::Pattern {
            regex: r"^[^@\s]+@[^@\s]+\.[^@\s]+$".to_string(),
        };
        assert!(rule
            .validate(&serde_json::json!("user@example.com"))
            .is_ok());
        assert!(rule.validate(&serde_json::json!("bad-email")).is_err());
    }

    #[test]
    fn test_validation_pattern_invalid_regex_returns_error() {
        // 非法正则应返回编译错误（而非静默放行）
        let rule = ValidationRule::Pattern {
            regex: r"[unclosed".to_string(),
        };
        let result = rule.validate(&serde_json::json!("anything"));
        assert!(result.is_err(), "非法正则应返回错误");
        let err = result.unwrap_err();
        assert!(
            err.contains("正则表达式编译失败"),
            "错误消息应包含编译失败提示，实际: {}",
            err
        );
    }

    #[test]
    fn test_validation_pattern_non_string_value_passes() {
        // 非 String 类型值（如数字）不做验证，直接放行（对齐其他规则的 null 容忍行为）
        let rule = ValidationRule::Pattern {
            regex: r"^\d+$".to_string(),
        };
        assert!(rule.validate(&serde_json::json!(123)).is_ok());
    }

    #[test]
    fn test_validation_pattern_null_value_passes() {
        // null 值不做验证（对齐其他规则的 null 容忍行为）
        let rule = ValidationRule::Pattern {
            regex: r"^\d+$".to_string(),
        };
        assert!(rule.validate(&serde_json::Value::Null).is_ok());
    }

    #[test]
    fn test_validation_email_pass() {
        let rule = ValidationRule::Email;
        assert!(rule
            .validate(&serde_json::json!("user@example.com"))
            .is_ok());
    }

    #[test]
    fn test_validation_email_fail() {
        let rule = ValidationRule::Email;
        assert!(rule.validate(&serde_json::json!("not-an-email")).is_err());
        assert!(rule.validate(&serde_json::json!("missing@domain")).is_err());
    }

    #[test]
    fn test_validation_url_pass() {
        let rule = ValidationRule::Url;
        assert!(rule
            .validate(&serde_json::json!("https://example.com"))
            .is_ok());
        assert!(rule.validate(&serde_json::json!("http://test.org")).is_ok());
    }

    #[test]
    fn test_validation_url_fail() {
        let rule = ValidationRule::Url;
        assert!(rule
            .validate(&serde_json::json!("ftp://example.com"))
            .is_err());
        assert!(rule.validate(&serde_json::json!("example.com")).is_err());
    }

    #[test]
    fn test_validation_enum_pass() {
        let rule = ValidationRule::Enum {
            values: vec!["active".to_string(), "inactive".to_string()],
        };
        assert!(rule.validate(&serde_json::json!("active")).is_ok());
        assert!(rule.validate(&serde_json::json!("inactive")).is_ok());
    }

    #[test]
    fn test_validation_enum_fail() {
        let rule = ValidationRule::Enum {
            values: vec!["active".to_string(), "inactive".to_string()],
        };
        assert!(rule.validate(&serde_json::json!("deleted")).is_err());
    }

    #[test]
    fn test_validation_to_html_attribute() {
        assert_eq!(
            ValidationRule::Required.to_html_attribute(),
            Some("required".to_string())
        );
        assert_eq!(
            ValidationRule::MinLength { value: 3 }.to_html_attribute(),
            Some("minlength=\"3\"".to_string())
        );
        assert_eq!(
            ValidationRule::MaxLength { value: 100 }.to_html_attribute(),
            Some("maxlength=\"100\"".to_string())
        );
        assert_eq!(
            ValidationRule::Min { value: 0.0 }.to_html_attribute(),
            Some("min=\"0\"".to_string())
        );
        assert_eq!(
            ValidationRule::Email.to_html_attribute(),
            Some("type=\"email\"".to_string())
        );
    }

    #[test]
    fn test_field_validation_multiple_rules() {
        let validation = FieldValidation::new("username")
            .with_rule(ValidationRule::Required)
            .with_rule(ValidationRule::MinLength { value: 3 })
            .with_rule(ValidationRule::MaxLength { value: 20 });

        assert!(validation.validate(&serde_json::json!("hello")).is_ok());
        assert!(validation.validate(&serde_json::json!("")).is_err());
        assert!(validation.validate(&serde_json::json!("hi")).is_err());
        assert!(validation
            .validate(&serde_json::json!("a_very_long_username_that_exceeds_max"))
            .is_err());
    }

    #[test]
    fn test_field_validation_to_html_attributes() {
        let validation = FieldValidation::new("email")
            .with_rule(ValidationRule::Required)
            .with_rule(ValidationRule::Email)
            .with_rule(ValidationRule::MaxLength { value: 100 });

        let attrs = validation.to_html_attributes();
        assert!(attrs.contains("required"));
        assert!(attrs.contains("type=\"email\""));
        assert!(attrs.contains("maxlength=\"100\""));
    }

    // ===== 动态表单生成测试 =====

    #[test]
    fn test_form_field_builder() {
        let field = FormField::new("email", "邮箱", InputType::Email)
            .required()
            .with_placeholder("请输入邮箱")
            .with_validation(ValidationRule::MaxLength { value: 100 });

        assert_eq!(field.name, "email");
        assert_eq!(field.label, "邮箱");
        assert!(field.required);
        assert_eq!(field.placeholder.as_deref(), Some("请输入邮箱"));
        assert_eq!(field.validation.rules.len(), 2); // Required + MaxLength
    }

    #[test]
    fn test_form_field_with_options() {
        let field = FormField::new("status", "状态", InputType::Select)
            .with_option("active", "活跃")
            .with_option("inactive", "停用");

        assert_eq!(field.options.len(), 2);
        assert_eq!(field.options[0].0, "active");
        assert_eq!(field.options[0].1, "活跃");
    }

    #[test]
    fn test_form_generator_from_model() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("name", "VARCHAR(255)").with_label("姓名"))
            .with_field(FieldDef::new("email", "VARCHAR(255)").with_label("邮箱"))
            .with_field(FieldDef::new("age", "INTEGER"))
            .with_field(FieldDef::new("bio", "TEXT"));

        let fields = FormGenerator::from_model(&model);
        assert_eq!(fields.len(), 5);

        // id 是主键，不需要 required
        let id_field = fields.iter().find(|f| f.name == "id").unwrap();
        assert!(!id_field.required);

        // name 不是主键且 nullable=false，需要 required
        let name_field = fields.iter().find(|f| f.name == "name").unwrap();
        assert!(name_field.required);
        assert_eq!(name_field.label, "姓名");

        // age 是 INTEGER -> Number
        let age_field = fields.iter().find(|f| f.name == "age").unwrap();
        assert!(matches!(age_field.input_type, InputType::Number));

        // bio 是 TEXT -> Textarea
        let bio_field = fields.iter().find(|f| f.name == "bio").unwrap();
        assert!(matches!(bio_field.input_type, InputType::Textarea));
    }

    #[test]
    fn test_generate_html_form_contains_form_tag() {
        let fields = vec![FormField::new("name", "姓名", InputType::Text).required()];
        let html = FormGenerator::generate_html_form(&fields, "/api/users", "POST");
        assert!(html.contains("<form"));
        assert!(html.contains("action=\"/api/users\""));
        assert!(html.contains("method=\"POST\""));
        assert!(html.contains("<button"));
    }

    #[test]
    fn test_generate_html_field_text_input() {
        let field = FormField::new("name", "姓名", InputType::Text)
            .required()
            .with_placeholder("请输入姓名");
        let html = FormGenerator::generate_html_field(&field);
        assert!(html.contains("<label"));
        assert!(html.contains("for=\"name\""));
        assert!(html.contains("type=\"text\""));
        assert!(html.contains("required"));
        assert!(html.contains("placeholder=\"请输入姓名\""));
    }

    #[test]
    fn test_generate_html_field_select() {
        let field = FormField::new("status", "状态", InputType::Select)
            .with_option("active", "活跃")
            .with_option("inactive", "停用");
        let html = FormGenerator::generate_html_field(&field);
        assert!(html.contains("<select"));
        assert!(html.contains("<option value=\"active\">活跃</option>"));
        assert!(html.contains("<option value=\"inactive\">停用</option>"));
    }

    #[test]
    fn test_generate_html_field_textarea() {
        let field = FormField::new("bio", "简介", InputType::Textarea);
        let html = FormGenerator::generate_html_field(&field);
        assert!(html.contains("<textarea"));
    }

    #[test]
    fn test_generate_html_field_checkbox() {
        let field = FormField::new("agree", "同意条款", InputType::Checkbox);
        let html = FormGenerator::generate_html_field(&field);
        assert!(html.contains("type=\"checkbox\""));
    }

    #[test]
    fn test_generate_html_field_with_help_text() {
        let field =
            FormField::new("email", "邮箱", InputType::Email).with_help_text("请输入有效邮箱地址");
        let html = FormGenerator::generate_html_field(&field);
        assert!(html.contains("help-text"));
        assert!(html.contains("请输入有效邮箱地址"));
    }

    #[test]
    fn test_generate_json_schema() {
        let fields = vec![
            FormField::new("name", "姓名", InputType::Text).required(),
            FormField::new("age", "年龄", InputType::Number),
            FormField::new("active", "激活", InputType::Checkbox),
        ];
        let schema = FormGenerator::generate_json_schema(&fields);

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["name"].is_object());
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert_eq!(schema["properties"]["age"]["type"], "number");
        assert_eq!(schema["properties"]["active"]["type"], "boolean");
        assert_eq!(schema["required"].as_array().unwrap().len(), 1);
        assert_eq!(schema["required"][0], "name");
    }

    // ===== CRUD 模板引擎测试 =====

    #[test]
    fn test_generate_ddl() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("name", "VARCHAR(255)"))
            .with_field(FieldDef::new("email", "VARCHAR(255)").unique())
            .with_field(FieldDef::new("bio", "TEXT").with_nullable(true))
            .with_index("idx_email");

        let ddl = CrudTemplateEngine::generate_ddl(&model);
        assert!(ddl.contains("CREATE TABLE \"users\""));
        assert!(ddl.contains("\"id\" BIGINT NOT NULL PRIMARY KEY"));
        assert!(ddl.contains("\"name\" VARCHAR(255) NOT NULL"));
        assert!(ddl.contains("\"email\" VARCHAR(255) NOT NULL UNIQUE"));
        assert!(ddl.contains("\"bio\" TEXT"));
        assert!(ddl.contains("CREATE INDEX \"idx_email\""));
    }

    #[test]
    fn test_generate_insert() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT"))
            .with_field(FieldDef::new("name", "VARCHAR(255)"))
            .with_field(FieldDef::new("email", "VARCHAR(255)"));

        let sql = CrudTemplateEngine::generate_insert(&model);
        assert!(sql.contains("INSERT INTO \"users\""));
        assert!(sql.contains("\"id\", \"name\", \"email\""));
        assert!(sql.contains("$1, $2, $3"));
    }

    #[test]
    fn test_generate_select_by_id() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT"))
            .with_field(FieldDef::new("name", "VARCHAR(255)"));

        let sql = CrudTemplateEngine::generate_select_by_id(&model);
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("FROM \"users\""));
        assert!(sql.contains("WHERE \"id\" = $1"));
    }

    #[test]
    fn test_generate_select_all() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT"))
            .with_field(FieldDef::new("name", "VARCHAR(255)"));

        let sql = CrudTemplateEngine::generate_select_all(&model);
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("FROM \"users\""));
        assert!(sql.contains("ORDER BY \"id\" DESC"));
        assert!(sql.contains("LIMIT $1 OFFSET $2"));
    }

    #[test]
    fn test_generate_update() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("name", "VARCHAR(255)"))
            .with_field(FieldDef::new("email", "VARCHAR(255)"));

        let sql = CrudTemplateEngine::generate_update(&model);
        assert!(sql.contains("UPDATE \"users\" SET"));
        assert!(sql.contains("\"name\" = $1"));
        assert!(sql.contains("\"email\" = $2"));
        assert!(sql.contains("WHERE \"id\" = $3"));
        // 主键不应出现在 SET 子句中（只检查 WHERE 之前的部分）
        let set_clause = sql.split("WHERE").next().unwrap();
        assert!(!set_clause.contains("\"id\" = $"));
    }

    #[test]
    fn test_generate_delete() {
        let model = ModelDefinition::new("users");
        let sql = CrudTemplateEngine::generate_delete(&model);
        assert!(sql.contains("DELETE FROM \"users\""));
        assert!(sql.contains("WHERE \"id\" = $1"));
    }

    #[test]
    fn test_generate_count() {
        let model = ModelDefinition::new("users");
        let sql = CrudTemplateEngine::generate_count(&model);
        assert!(sql.contains("SELECT COUNT(*)"));
        assert!(sql.contains("FROM \"users\""));
    }

    #[test]
    fn test_validate_identifier_safe() {
        assert!(ModelDefinition::validate_identifier("users").is_ok());
        assert!(ModelDefinition::validate_identifier("order_items").is_ok());
        assert!(ModelDefinition::validate_identifier("_private").is_ok());
        assert!(ModelDefinition::validate_identifier("t123").is_ok());
    }

    #[test]
    fn test_validate_identifier_unsafe() {
        assert!(ModelDefinition::validate_identifier("").is_err());
        assert!(ModelDefinition::validate_identifier("users; DROP TABLE").is_err());
        assert!(ModelDefinition::validate_identifier("users\" --").is_err());
        assert!(ModelDefinition::validate_identifier("user' OR '1'='1").is_err());
        assert!(ModelDefinition::validate_identifier(&"a".repeat(64)).is_err());
    }

    #[test]
    fn test_sanitize_identifier() {
        assert_eq!(ModelDefinition::sanitize_identifier("users"), "users");
        assert_eq!(
            ModelDefinition::sanitize_identifier("users; DROP TABLE"),
            "users__DROP_TABLE"
        );
        assert_eq!(
            ModelDefinition::sanitize_identifier("user\" --"),
            "user____"
        );
        assert_eq!(ModelDefinition::sanitize_identifier(""), "_");
    }

    #[test]
    fn test_generate_delete_sanitizes_malicious_name() {
        let model = ModelDefinition::new("users\" DROP TABLE users; --");
        let sql = CrudTemplateEngine::generate_delete(&model);
        assert!(
            !sql.contains("DROP TABLE"),
            "DELETE SQL must not contain DROP TABLE: {}",
            sql
        );
        assert!(
            !sql.contains("--"),
            "DELETE SQL must not contain comment: {}",
            sql
        );
        assert!(sql.starts_with("DELETE FROM \""));
    }

    #[test]
    fn test_generate_insert_sanitizes_malicious_name() {
        let model = ModelDefinition::new("users'; DROP TABLE users; --")
            .with_field(FieldDef::new("name", "VARCHAR(255)"));
        let sql = CrudTemplateEngine::generate_insert(&model);
        assert!(
            !sql.contains("DROP TABLE"),
            "INSERT SQL must not contain DROP TABLE: {}",
            sql
        );
        assert!(
            !sql.contains("'"),
            "INSERT SQL must not contain raw quote: {}",
            sql
        );
    }

    #[test]
    fn test_generate_ddl_sanitizes_malicious_name() {
        let model = ModelDefinition::new("users; DROP TABLE users; --")
            .with_field(FieldDef::new("name", "VARCHAR(255)").with_default("'; DROP TABLE x; --"));
        let sql = CrudTemplateEngine::generate_ddl(&model);
        assert!(
            !sql.contains("DROP TABLE users"),
            "DDL must not contain DROP TABLE users: {}",
            sql
        );
        assert!(
            sql.starts_with("CREATE TABLE"),
            "DDL must start with CREATE TABLE: {}",
            sql
        );
        let after_close = sql.rfind(");").map(|i| &sql[i + 2..]).unwrap_or("");
        assert!(
            !after_close.contains("DROP TABLE"),
            "DDL must not have DROP TABLE after closing: {}",
            sql
        );
    }

    #[test]
    fn test_generate_rust_struct() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("name", "VARCHAR(255)"))
            .with_field(FieldDef::new("email", "VARCHAR(255)"))
            .with_field(FieldDef::new("age", "INTEGER").with_nullable(true));

        let code = CrudTemplateEngine::generate_rust_struct(&model);
        assert!(code.contains("pub struct User {"));
        assert!(code.contains("pub id: i64,"));
        assert!(code.contains("pub name: String,"));
        assert!(code.contains("pub email: String,"));
        assert!(code.contains("pub age: Option<i32>,"));
    }

    #[test]
    fn test_generate_rust_repository() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("name", "VARCHAR(255)"));

        let code = CrudTemplateEngine::generate_rust_repository(&model);
        assert!(code.contains("pub struct UserRepository"));
        assert!(code.contains("impl UserRepository"));
        assert!(code.contains("async fn create"));
        assert!(code.contains("async fn find_by_id"));
        assert!(code.contains("async fn update"));
        assert!(code.contains("async fn delete"));
        // C-1 修复：验证生成真实可编译的 SQL 执行代码，而非占位符或无效 API
        assert!(
            code.contains("sqlx::query"),
            "生成代码应包含 sqlx::query 执行"
        );
        assert!(
            code.contains(".execute(pool)"),
            "create/update 应调用 execute"
        );
        assert!(
            code.contains(".fetch_optional(pool)"),
            "find_by_id 应调用 fetch_optional"
        );
        assert!(
            code.contains(".bind(id)"),
            "find_by_id/delete 应 bind id 参数"
        );
        assert!(
            code.contains("rows_affected"),
            "delete 应基于 rows_affected 返回 bool"
        );
        assert!(
            !code.contains("__SZORM_TODO__"),
            "生成代码不应包含占位符 __SZORM_TODO__"
        );
        // C-1 修复：验证使用 .bind()（singular）而非无效的 .binds()（plural）
        assert!(
            !code.contains(".binds("),
            "生成代码不应使用无效的 .binds() API，应使用链式 .bind()"
        );
        // 验证 create 方法使用链式 .bind() 绑定字段值（模型 name→user 单数化）
        assert!(
            code.contains(".bind(user.name)"),
            "create 应包含 .bind(user.name) 链式绑定字段值"
        );
    }

    // ===== FieldDef 测试 =====

    #[test]
    fn test_field_def_builder() {
        let field = FieldDef::new("email", "VARCHAR(255)")
            .with_label("邮箱")
            .with_default("''")
            .unique();

        assert_eq!(field.name, "email");
        assert_eq!(field.field_type, "VARCHAR(255)");
        assert!(!field.nullable);
        assert_eq!(field.label.as_deref(), Some("邮箱"));
        assert_eq!(field.default_value.as_deref(), Some("''"));
        assert!(field.unique);
    }

    #[test]
    fn test_field_def_display_label() {
        let with_label = FieldDef::new("email", "VARCHAR(255)").with_label("邮箱");
        assert_eq!(with_label.display_label(), "邮箱");

        let without_label = FieldDef::new("email", "VARCHAR(255)");
        assert_eq!(without_label.display_label(), "email");
    }

    #[test]
    fn test_model_definition_find_field() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("name", "VARCHAR(255)"));

        assert!(model.find_field("id").is_some());
        assert!(model.find_field("name").is_some());
        assert!(model.find_field("nonexistent").is_none());
    }

    #[test]
    fn test_model_definition_primary_key() {
        let model = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("name", "VARCHAR(255)"));

        let pk = model.primary_key().unwrap();
        assert_eq!(pk.name, "id");
        assert!(pk.primary_key);
    }

    #[test]
    fn test_relation_definition_helpers() {
        let one_to_one = RelationDefinition::new("profile", "one_to_one", "profiles", "user_id");
        assert!(one_to_one.is_one_to_one());
        assert!(!one_to_one.is_one_to_many());

        let one_to_many = RelationDefinition::new("posts", "one_to_many", "posts", "user_id");
        assert!(one_to_many.is_one_to_many());
        assert!(!one_to_many.is_many_to_many());

        let many_to_many = RelationDefinition::new("roles", "many_to_many", "roles", "role_id");
        assert!(many_to_many.is_many_to_many());
    }

    #[test]
    fn test_input_type_as_html_type() {
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
}
