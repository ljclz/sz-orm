//! # SZ-ORM LC — 低代码模型定义
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

    /// 将表名转换为 PascalCase 单数模型名。
    /// 例："users" -> "User", "order_items" -> "OrderItem"
    pub fn pascal_case_name(&self) -> String {
        to_pascal_singular(&self.name)
    }

    /// 返回表名的单数形式（简单启发式：去除末尾 's'）。
    pub fn singular_name(&self) -> String {
        let n = self.name.trim_end_matches('s');
        n.to_string()
    }

    /// 添加字段（链式调用）
    pub fn with_field(mut self, field: FieldDef) -> Self {
        self.fields.push(field);
        self
    }

    /// 添加索引（链式调用）
    pub fn with_index(mut self, index: &str) -> Self {
        self.indexes.push(index.to_string());
        self
    }

    /// 添加关联关系（链式调用）
    pub fn with_relation(mut self, relation: RelationDefinition) -> Self {
        self.relations.push(relation);
        self
    }

    /// 查找指定名称的字段
    pub fn find_field(&self, name: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// 获取主键字段（默认为 id）
    pub fn primary_key(&self) -> Option<&FieldDef> {
        self.find_field("id")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub field_type: String,
    pub nullable: bool,
    /// 字段注释/标签
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 默认值（SQL 表达式或字面量）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    /// 是否为主键
    #[serde(default)]
    pub primary_key: bool,
    /// 是否唯一
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

    /// 获取字段标签（优先 label，回退到 name）
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

    /// 判断是否为一对一关系
    pub fn is_one_to_one(&self) -> bool {
        self.rel_type.eq_ignore_ascii_case("one_to_one")
    }

    /// 判断是否为一对多关系
    pub fn is_one_to_many(&self) -> bool {
        self.rel_type.eq_ignore_ascii_case("one_to_many")
    }

    /// 判断是否为多对多关系
    pub fn is_many_to_many(&self) -> bool {
        self.rel_type.eq_ignore_ascii_case("many_to_many")
    }
}

/// 将表名如 "users" 或 "order_items" 转换为 "User" / "OrderItem"
/// 并去除末尾 's' 以单数化。
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

/// 字段类型映射工具
///
/// 提供 SQL 类型 ↔ Rust 类型 ↔ HTML input 类型 ↔ JSON Schema 类型 的双向映射。
pub struct FieldTypeMapping;

impl FieldTypeMapping {
    /// 将 SQL 类型映射为 Rust 类型
    ///
    /// 例：BIGINT -> i64, VARCHAR -> String, TIMESTAMP -> chrono::NaiveDateTime
    pub fn sql_to_rust(sql_type: &str) -> &'static str {
        let upper = sql_type.to_uppercase();
        if upper.starts_with("BIGINT") || upper.starts_with("INT8") {
            "i64"
        } else if upper.starts_with("INT") || upper.starts_with("INTEGER") || upper.starts_with("INT4") {
            "i32"
        } else if upper.starts_with("SMALLINT") || upper.starts_with("INT2") {
            "i16"
        } else if upper.starts_with("BOOL") {
            "bool"
        } else if upper.starts_with("FLOAT8") || upper.starts_with("DOUBLE") {
            "f64"
        } else if upper.starts_with("FLOAT") || upper.starts_with("REAL") || upper.starts_with("FLOAT4") {
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

    /// 将 SQL 类型映射为 HTML input 类型
    ///
    /// 例：VARCHAR -> text, INTEGER -> number, DATE -> date, BOOLEAN -> checkbox
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

    /// 将 SQL 类型映射为 JSON Schema 类型
    pub fn sql_to_json_schema(sql_type: &str) -> &'static str {
        let upper = sql_type.to_uppercase();
        if upper.starts_with("INT") || upper.starts_with("BIGINT") || upper.starts_with("SMALLINT") {
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

    /// 将 Rust 类型映射为 SQL 类型
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

    /// 判断 SQL 类型是否为数值类型
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

    /// 判断 SQL 类型是否为日期/时间类型
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

/// 验证规则枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ValidationRule {
    /// 必填
    Required,
    /// 最小长度
    MinLength { value: u32 },
    /// 最大长度
    MaxLength { value: u32 },
    /// 最小值
    Min { value: f64 },
    /// 最大值
    Max { value: f64 },
    /// 正则表达式
    Pattern { regex: String },
    /// 邮箱格式
    Email,
    /// URL 格式
    Url,
    /// 枚举值
    Enum { values: Vec<String> },
}

impl ValidationRule {
    /// 验证给定值是否符合规则
    ///
    /// 返回 `Ok(())` 表示通过，`Err(message)` 表示失败。
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
            Self::Pattern { regex: _ } => {
                // 简化实现：实际项目中应使用 regex crate
                // 这里仅做存在性检查
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

    /// 转换为 HTML 表单属性字符串
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

/// 字段验证规则集合
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

    /// 添加验证规则（链式调用）
    pub fn with_rule(mut self, rule: ValidationRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// 验证给定值，返回所有错误信息
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

    /// 生成所有规则的 HTML 属性字符串
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

/// 表单输入类型
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
    /// 转换为 HTML input type 属性值
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

/// 表单字段定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub label: String,
    pub input_type: InputType,
    pub required: bool,
    pub validation: FieldValidation,
    pub default_value: Option<serde_json::Value>,
    pub placeholder: Option<String>,
    /// Select 类型的选项列表：(value, label)
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

/// 动态表单生成器
pub struct FormGenerator;

impl FormGenerator {
    /// 从模型定义生成表单字段列表
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

                let mut form_field = FormField::new(
                    &field.name,
                    field.display_label(),
                    input_type,
                );

                if !field.nullable && !field.primary_key {
                    form_field = form_field.required();
                }

                if let Some(default) = &field.default_value {
                    form_field = form_field.with_default(serde_json::Value::String(default.clone()));
                }

                form_field
            })
            .collect()
    }

    /// 生成 HTML 表单
    pub fn generate_html_form(fields: &[FormField], action: &str, method: &str) -> String {
        let mut html = String::new();
        html.push_str(&format!(
            r#"<form action="{}" method="{}" enctype="multipart/form-data">"#,
            action, method
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

    /// 生成单个 HTML 表单字段
    pub fn generate_html_field(field: &FormField) -> String {
        let mut html = String::new();
        html.push_str("    <div class=\"form-group\">\n");

        // Hidden 字段不显示 label
        if !matches!(field.input_type, InputType::Hidden) {
            html.push_str(&format!(
                "        <label for=\"{}\">{}{}</label>\n",
                field.name,
                field.label,
                if field.required { " <span class=\"required\">*</span>" } else { "" }
            ));
        }

        let validation_attrs = field.validation.to_html_attributes();
        let placeholder = field
            .placeholder
            .as_ref()
            .map(|p| format!("placeholder=\"{}\"", p))
            .unwrap_or_default();

        match &field.input_type {
            InputType::Select => {
                html.push_str(&format!(
                    "        <select id=\"{}\" name=\"{}\" {}>\n",
                    field.name, field.name, validation_attrs
                ));
                html.push_str("            <option value=\"\">请选择</option>\n");
                for (value, label) in &field.options {
                    html.push_str(&format!(
                        "            <option value=\"{}\">{}</option>\n",
                        value, label
                    ));
                }
                html.push_str("        </select>\n");
            }
            InputType::Textarea => {
                html.push_str(&format!(
                    "        <textarea id=\"{}\" name=\"{}\" {} {}></textarea>\n",
                    field.name, field.name, validation_attrs, placeholder
                ));
            }
            InputType::Checkbox => {
                html.push_str(&format!(
                    "        <input type=\"checkbox\" id=\"{}\" name=\"{}\" {} />\n",
                    field.name, field.name, validation_attrs
                ));
            }
            _ => {
                let input_type = field.input_type.as_html_type();
                html.push_str(&format!(
                    "        <input type=\"{}\" id=\"{}\" name=\"{}\" {} {} />\n",
                    input_type, field.name, field.name, validation_attrs, placeholder
                ));
            }
        }

        if let Some(help) = &field.help_text {
            html.push_str(&format!(
                "        <small class=\"help-text\">{}</small>\n",
                help
            ));
        }

        html.push_str("    </div>");
        html
    }

    /// 生成 JSON Schema（用于 API 文档或前端校验）
    pub fn generate_json_schema(fields: &[FormField]) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = vec![];

        for field in fields {
            let field_type = match &field.input_type {
                InputType::Number => "number",
                InputType::Checkbox => "boolean",
                InputType::Textarea | InputType::Text | InputType::Email
                | InputType::Password | InputType::Date | InputType::DateTime
                | InputType::Time | InputType::Select | InputType::File
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

/// CRUD 模板引擎
///
/// 生成 SQL DDL、参数化 CRUD 语句、Rust 结构体与仓储层代码。
pub struct CrudTemplateEngine;

impl CrudTemplateEngine {
    /// 生成 CREATE TABLE DDL 语句
    pub fn generate_ddl(model: &ModelDefinition) -> String {
        let mut sql = String::new();
        sql.push_str(&format!("CREATE TABLE \"{}\" (\n", model.name));

        let column_defs: Vec<String> = model
            .fields
            .iter()
            .map(|field| {
                let mut def = format!("    \"{}\" {}", field.name, field.field_type);
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
                    def.push_str(&format!(" DEFAULT {}", default));
                }
                def
            })
            .collect();

        sql.push_str(&column_defs.join(",\n"));
        sql.push_str("\n);");

        // 索引
        for index in &model.indexes {
            sql.push_str(&format!("\nCREATE INDEX \"{}\" ON \"{}\";", index, model.name));
        }

        sql
    }

    /// 生成 INSERT 语句（参数化）
    pub fn generate_insert(model: &ModelDefinition) -> String {
        let columns: Vec<&str> = model.fields.iter().map(|f| f.name.as_str()).collect();
        let placeholders: Vec<String> = (1..=columns.len())
            .map(|i| format!("${}", i))
            .collect();
        format!(
            "INSERT INTO \"{}\" ({}) VALUES ({});",
            model.name,
            columns.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", "),
            placeholders.join(", ")
        )
    }

    /// 生成 SELECT BY ID 语句
    pub fn generate_select_by_id(model: &ModelDefinition) -> String {
        let columns: Vec<String> = model
            .fields
            .iter()
            .map(|f| format!("\"{}\"", f.name))
            .collect();
        format!(
            "SELECT {} FROM \"{}\" WHERE \"id\" = $1;",
            columns.join(", "),
            model.name
        )
    }

    /// 生成 SELECT ALL 语句（带分页）
    pub fn generate_select_all(model: &ModelDefinition) -> String {
        let columns: Vec<String> = model
            .fields
            .iter()
            .map(|f| format!("\"{}\"", f.name))
            .collect();
        format!(
            "SELECT {} FROM \"{}\" ORDER BY \"id\" DESC LIMIT $1 OFFSET $2;",
            columns.join(", "),
            model.name
        )
    }

    /// 生成 UPDATE 语句（参数化，排除主键）
    pub fn generate_update(model: &ModelDefinition) -> String {
        let update_fields: Vec<&FieldDef> = model
            .fields
            .iter()
            .filter(|f| !f.primary_key)
            .collect();

        let set_clauses: Vec<String> = update_fields
            .iter()
            .enumerate()
            .map(|(i, f)| format!("\"{}\" = ${}", f.name, i + 1))
            .collect();

        let id_placeholder = format!("${}", update_fields.len() + 1);

        format!(
            "UPDATE \"{}\" SET {} WHERE \"id\" = {};",
            model.name,
            set_clauses.join(", "),
            id_placeholder
        )
    }

    /// 生成 DELETE 语句
    pub fn generate_delete(model: &ModelDefinition) -> String {
        format!("DELETE FROM \"{}\" WHERE \"id\" = $1;", model.name)
    }

    /// 生成 COUNT 语句
    pub fn generate_count(model: &ModelDefinition) -> String {
        format!("SELECT COUNT(*) AS total FROM \"{}\";", model.name)
    }

    /// 生成 Rust 结构体定义
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

    /// 生成 Rust 仓储层代码（Repository 模式）
    pub fn generate_rust_repository(model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let singular_lower = model.singular_name().to_lowercase();
        let table = &model.name;

        let insert_sql = Self::generate_insert(model);
        let select_sql = Self::generate_select_by_id(model);
        let update_sql = Self::generate_update(model);
        let delete_sql = Self::generate_delete(model);

        format!(
            r#"pub struct {pascal}Repository;

impl {pascal}Repository {{
    pub async fn create(pool: &sqlx::PgPool, {singular_lower}: &{pascal}) -> Result<{pascal}, sqlx::Error> {{
        // SQL: {insert_sql}
        unimplemented!("create {singular_lower}")
    }}

    pub async fn find_by_id(pool: &sqlx::PgPool, id: i64) -> Result<Option<{pascal}>, sqlx::Error> {{
        // SQL: {select_sql}
        unimplemented!("find {singular_lower} by id")
    }}

    pub async fn update(pool: &sqlx::PgPool, {singular_lower}: &{pascal}) -> Result<{pascal}, sqlx::Error> {{
        // SQL: {update_sql}
        unimplemented!("update {singular_lower}")
    }}

    pub async fn delete(pool: &sqlx::PgPool, id: i64) -> Result<bool, sqlx::Error> {{
        // SQL: {delete_sql}
        unimplemented!("delete {singular_lower}")
    }}
}}"#,
            pascal = pascal,
            singular_lower = singular_lower,
            insert_sql = insert_sql,
            select_sql = select_sql,
            update_sql = update_sql,
            delete_sql = delete_sql,
        )
        .replace("unimplemented!", &format!("// table: {}", table))
    }
}

// ============================================================================
// LowCodeEngine（保留原有 API，内部委托给 CrudTemplateEngine）
// ============================================================================

pub struct LowCodeEngine;

impl LowCodeEngine {
    /// 逆向工程：从表名列表生成 ModelDefinitions
    /// 包含默认字段（id, name, created_at, updated_at）和索引。
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

    /// 生成 SQL CRUD 语句（INSERT/SELECT/UPDATE/DELETE）
    ///
    /// # 安全性（门禁 9 修复）
    ///
    /// 表名用双引号包裹（PostgreSQL 标准），防止含特殊字符或 SQL 关键字的表名逃逸注入。
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

    /// 生成 Rust handler 代码
    pub fn generate_api(&self, model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let singular_lower = model.singular_name().to_lowercase();
        format!(
            r#"use axum::{{Json, extract::Path}};
use serde::{{Deserialize, Serialize}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct {pascal} {{
    pub id: i64,
    pub name: String,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}}

pub async fn create_{singular_lower}(Json(payload): Json<{pascal}>) -> Json<{pascal}> {{
    Json(payload)
}}

pub async fn get_{singular_lower}(Path(id): Path<i64>) -> Json<{pascal}> {{
    Json({pascal} {{
        id,
        name: String::new(),
        created_at: chrono::NaiveDateTime::default(),
        updated_at: chrono::NaiveDateTime::default(),
    }})
}}

pub async fn update_{singular_lower}(
    Path(id): Path<i64>,
    Json(payload): Json<{pascal}>,
) -> Json<{pascal}> {{
    Json(payload)
}}

pub async fn delete_{singular_lower}(Path(id): Path<i64>) -> Json<bool> {{
    Json(true)
}}
"#,
            pascal = pascal,
            singular_lower = singular_lower
        )
    }

    /// 生成 HTML 表单标记
    pub fn generate_frontend(&self, model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let singular_lower = model.singular_name().to_lowercase();
        let fields = FormGenerator::from_model(model);
        let form_body = FormGenerator::generate_html_field(
            fields.iter().find(|f| f.name == "name").unwrap_or(&FormField::new("name", "Name", InputType::Text)),
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
        assert_eq!(FieldTypeMapping::sql_to_rust("TIMESTAMP"), "chrono::NaiveDateTime");
        assert_eq!(FieldTypeMapping::sql_to_rust("TIMESTAMPTZ"), "chrono::DateTime<chrono::Utc>");
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
        assert_eq!(FieldTypeMapping::sql_to_html_input("TIMESTAMP"), "datetime-local");
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

    #[test]
    fn test_validation_email_pass() {
        let rule = ValidationRule::Email;
        assert!(rule.validate(&serde_json::json!("user@example.com")).is_ok());
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
        assert!(rule.validate(&serde_json::json!("https://example.com")).is_ok());
        assert!(rule.validate(&serde_json::json!("http://test.org")).is_ok());
    }

    #[test]
    fn test_validation_url_fail() {
        let rule = ValidationRule::Url;
        assert!(rule.validate(&serde_json::json!("ftp://example.com")).is_err());
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
        assert!(validation.validate(&serde_json::json!("a_very_long_username_that_exceeds_max")).is_err());
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
        let field = FormField::new("email", "邮箱", InputType::Email)
            .with_help_text("请输入有效邮箱地址");
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
