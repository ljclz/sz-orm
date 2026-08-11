//! SchemaToModelMapper — OpenAPI Schema → Rust struct + derive Model 字段映射

use super::{to_pascal_case, ReverseGenError};
use crate::{ArrayType, ObjectType, PrimitiveSchema, Schema};
use quote::quote;
use std::collections::HashMap;

/// Rust 类型表示
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustType {
    /// String
    String,
    /// i64
    I64,
    /// i32
    I32,
    /// f64
    F64,
    /// bool
    Bool,
    /// chrono::DateTime<Utc>
    DateTime,
    /// uuid::Uuid
    Uuid,
    /// Vec<T>
    Vec(Box<RustType>),
    /// 嵌套 struct（引用名）
    StructRef(String),
    /// Option<T>
    Option(Box<RustType>),
}

impl RustType {
    /// 生成 Rust 类型代码字符串
    pub fn to_code(&self) -> String {
        match self {
            RustType::String => "String".to_string(),
            RustType::I64 => "i64".to_string(),
            RustType::I32 => "i32".to_string(),
            RustType::F64 => "f64".to_string(),
            RustType::Bool => "bool".to_string(),
            RustType::DateTime => "chrono::DateTime<chrono::Utc>".to_string(),
            RustType::Uuid => "uuid::Uuid".to_string(),
            RustType::Vec(inner) => format!("Vec<{}>", inner.to_code()),
            RustType::StructRef(name) => name.clone(),
            RustType::Option(inner) => format!("Option<{}>", inner.to_code()),
        }
    }
}

/// 字段约束
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// NOT NULL（required）
    NotNull,
    /// VARCHAR(max_length)
    MaxLength(u32),
    /// TIMESTAMP（format: date-time）
    DateTime,
    /// UUID（format: uuid）
    Uuid,
    /// UNIQUE（uniqueItems）
    Unique,
    /// CHECK 约束（pattern / range）
    Check(String),
}

/// 模型字段
#[derive(Debug, Clone)]
pub struct ModelField {
    /// 字段名
    pub name: String,
    /// Rust 类型
    pub rust_type: RustType,
    /// 约束列表
    pub constraints: Vec<Constraint>,
    /// 是否必填
    pub required: bool,
}

/// SchemaToModelMapper — OpenAPI Schema → Rust struct 字段映射
pub struct SchemaToModelMapper;

impl SchemaToModelMapper {
    /// 创建新的 mapper
    pub fn new() -> Self {
        Self
    }

    /// 类型映射：OpenAPI Schema → RustType
    pub fn map_type(schema: &Schema) -> RustType {
        match schema {
            Schema::Primitive(p) => Self::map_primitive_type(p),
            Schema::Array(arr) => Self::map_array_type(arr),
            Schema::Object(_) => RustType::StructRef("NestedStruct".to_string()),
            Schema::Ref { ref_path } => {
                let name = ref_path.rsplit('/').next().unwrap_or("Unknown").to_string();
                RustType::StructRef(to_pascal_case(&name))
            }
        }
    }

    /// 基本类型映射
    fn map_primitive_type(p: &PrimitiveSchema) -> RustType {
        match p.schema_type.as_str() {
            "string" => match p.format.as_deref() {
                Some("date-time") => RustType::DateTime,
                Some("uuid") => RustType::Uuid,
                _ => RustType::String,
            },
            "integer" => match p.format.as_deref() {
                Some("int32") => RustType::I32,
                _ => RustType::I64,
            },
            "number" => RustType::F64,
            "boolean" => RustType::Bool,
            _ => RustType::String,
        }
    }

    /// 数组类型映射
    fn map_array_type(arr: &ArrayType) -> RustType {
        let inner = Self::map_type(&arr.items);
        RustType::Vec(Box::new(inner))
    }

    /// 约束映射：OpenAPI Schema → Vec<Constraint>
    pub fn map_constraint(schema: &Schema, required: bool) -> Vec<Constraint> {
        let mut constraints = Vec::new();

        if required {
            constraints.push(Constraint::NotNull);
        }

        if let Schema::Primitive(p) = schema {
            if let Some(max_len) = p.max_length {
                constraints.push(Constraint::MaxLength(max_len));
            }
            if let Some(ref format) = p.format {
                if format == "date-time" {
                    constraints.push(Constraint::DateTime);
                }
                if format == "uuid" {
                    constraints.push(Constraint::Uuid);
                }
            }
            if let Some(ref pattern) = p.pattern {
                constraints.push(Constraint::Check(format!("pattern: {}", pattern)));
            }
            if p.minimum.is_some() || p.maximum.is_some() {
                let min = p.minimum.map(|v| v.to_string()).unwrap_or_default();
                let max = p.maximum.map(|v| v.to_string()).unwrap_or_default();
                constraints.push(Constraint::Check(format!("range: [{}, {}]", min, max)));
            }
        }

        if let Schema::Array(arr) = schema {
            if arr.unique_items == Some(true) {
                constraints.push(Constraint::Unique);
            }
        }

        constraints
    }

    /// 从 ObjectType 提取字段列表
    pub fn extract_fields(obj: &ObjectType) -> Vec<ModelField> {
        let mut fields = Vec::new();
        for (name, schema) in &obj.properties {
            let required = obj.required.contains(name);
            let rust_type = Self::map_type(schema);
            let constraints = Self::map_constraint(schema, required);
            let final_type = if !required {
                RustType::Option(Box::new(rust_type))
            } else {
                rust_type
            };
            fields.push(ModelField {
                name: name.clone(),
                rust_type: final_type,
                constraints,
                required,
            });
        }
        fields
    }

    /// 生成 Rust struct + derive Model 代码
    pub fn generate_model_code(
        schema_name: &str,
        schema: &Schema,
    ) -> Result<String, ReverseGenError> {
        let struct_name = to_pascal_case(schema_name);

        let obj = match schema {
            Schema::Object(o) => o,
            _ => {
                return Err(ReverseGenError::UnsupportedSchemaConstruct {
                    construct: "non-object schema".to_string(),
                    schema: schema_name.to_string(),
                });
            }
        };

        let fields = Self::extract_fields(obj);
        if fields.is_empty() {
            return Ok(format!(
                "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct {} {{}}\n",
                struct_name
            ));
        }

        let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
        let field_types: Vec<String> = fields.iter().map(|f| f.rust_type.to_code()).collect();

        let field_tokens: Vec<proc_macro2::TokenStream> = field_names
            .iter()
            .zip(field_types.iter())
            .map(|(name, ty)| {
                let name_ident = syn::Ident::new(name, proc_macro2::Span::call_site());
                let ty_tokens: syn::Type = syn::parse_str(ty).unwrap_or_else(|_| {
                    syn::parse_str("String").expect("String is always parseable")
                });
                quote! { pub #name_ident: #ty_tokens }
            })
            .collect();

        let struct_ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());

        let code = quote! {
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
            pub struct #struct_ident {
                #(#field_tokens),*
            }
        };

        Ok(code.to_string())
    }

    /// 批量生成 Model 代码
    pub fn generate_models(
        schemas: &HashMap<String, Schema>,
    ) -> Result<HashMap<String, String>, ReverseGenError> {
        let mut result = HashMap::new();
        for (name, schema) in schemas {
            let code = Self::generate_model_code(name, schema)?;
            result.insert(name.clone(), code);
        }
        Ok(result)
    }
}

impl Default for SchemaToModelMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_schema() -> Schema {
        let mut obj = ObjectType::new();
        obj = obj.with_required_property("id", Schema::integer());
        obj = obj.with_required_property(
            "name",
            Schema::Primitive(PrimitiveSchema::string().with_length_range(0, 255)),
        );
        obj = obj.with_property("email", Schema::string());
        Schema::Object(obj)
    }

    #[test]
    fn test_map_type_string() {
        let schema = Schema::string();
        assert_eq!(SchemaToModelMapper::map_type(&schema), RustType::String);
    }

    #[test]
    fn test_map_type_integer() {
        let schema = Schema::integer();
        assert_eq!(SchemaToModelMapper::map_type(&schema), RustType::I64);
    }

    #[test]
    fn test_map_type_int32() {
        let schema = Schema::Primitive(PrimitiveSchema::integer().with_format("int32"));
        assert_eq!(SchemaToModelMapper::map_type(&schema), RustType::I32);
    }

    #[test]
    fn test_map_type_number() {
        let schema = Schema::number();
        assert_eq!(SchemaToModelMapper::map_type(&schema), RustType::F64);
    }

    #[test]
    fn test_map_type_boolean() {
        let schema = Schema::boolean();
        assert_eq!(SchemaToModelMapper::map_type(&schema), RustType::Bool);
    }

    #[test]
    fn test_map_type_date_time() {
        let schema = Schema::Primitive(PrimitiveSchema::string().with_format("date-time"));
        assert_eq!(SchemaToModelMapper::map_type(&schema), RustType::DateTime);
    }

    #[test]
    fn test_map_type_uuid() {
        let schema = Schema::Primitive(PrimitiveSchema::string().with_format("uuid"));
        assert_eq!(SchemaToModelMapper::map_type(&schema), RustType::Uuid);
    }

    #[test]
    fn test_map_type_array() {
        let schema = Schema::Array(ArrayType::new(Schema::string()));
        assert_eq!(
            SchemaToModelMapper::map_type(&schema),
            RustType::Vec(Box::new(RustType::String))
        );
    }

    #[test]
    fn test_map_type_ref() {
        let schema = Schema::ref_to("UserProfile");
        assert_eq!(
            SchemaToModelMapper::map_type(&schema),
            RustType::StructRef("UserProfile".to_string())
        );
    }

    #[test]
    fn test_map_constraint_required() {
        let schema = Schema::string();
        let constraints = SchemaToModelMapper::map_constraint(&schema, true);
        assert!(constraints.contains(&Constraint::NotNull));
    }

    #[test]
    fn test_map_constraint_max_length() {
        let schema = Schema::Primitive(PrimitiveSchema::string().with_length_range(0, 255));
        let constraints = SchemaToModelMapper::map_constraint(&schema, true);
        assert!(constraints.contains(&Constraint::MaxLength(255)));
    }

    #[test]
    fn test_map_constraint_date_time() {
        let schema = Schema::Primitive(PrimitiveSchema::string().with_format("date-time"));
        let constraints = SchemaToModelMapper::map_constraint(&schema, true);
        assert!(constraints.contains(&Constraint::DateTime));
    }

    #[test]
    fn test_map_constraint_unique_items() {
        let schema = Schema::Array(ArrayType::new(Schema::string()).unique_items());
        let constraints = SchemaToModelMapper::map_constraint(&schema, true);
        assert!(constraints.contains(&Constraint::Unique));
    }

    #[test]
    fn test_extract_fields() {
        let schema = make_user_schema();
        let obj = match &schema {
            Schema::Object(o) => o,
            _ => unreachable!(),
        };
        let fields = SchemaToModelMapper::extract_fields(obj);
        assert_eq!(fields.len(), 3);

        let id_field = fields.iter().find(|f| f.name == "id").unwrap();
        assert!(id_field.required);
        assert_eq!(id_field.rust_type, RustType::I64);

        let name_field = fields.iter().find(|f| f.name == "name").unwrap();
        assert!(name_field.required);
        assert_eq!(name_field.rust_type, RustType::String);

        let email_field = fields.iter().find(|f| f.name == "email").unwrap();
        assert!(!email_field.required);
        assert_eq!(
            email_field.rust_type,
            RustType::Option(Box::new(RustType::String))
        );
    }

    #[test]
    fn test_generate_model_code() {
        let schema = make_user_schema();
        let code = SchemaToModelMapper::generate_model_code("User", &schema).unwrap();
        assert!(code.contains("struct User"));
        assert!(code.contains("id : i64"));
        assert!(code.contains("name : String"));
        assert!(code.contains("email : Option < String >"));
    }

    #[test]
    fn test_generate_model_code_non_object_error() {
        let schema = Schema::string();
        let result = SchemaToModelMapper::generate_model_code("NotAnObject", &schema);
        assert!(result.is_err());
        match result.unwrap_err() {
            ReverseGenError::UnsupportedSchemaConstruct { .. } => {}
            _ => panic!("expected UnsupportedSchemaConstruct"),
        }
    }

    #[test]
    fn test_rust_type_to_code() {
        assert_eq!(RustType::String.to_code(), "String");
        assert_eq!(RustType::I64.to_code(), "i64");
        assert_eq!(RustType::I32.to_code(), "i32");
        assert_eq!(RustType::F64.to_code(), "f64");
        assert_eq!(RustType::Bool.to_code(), "bool");
        assert_eq!(
            RustType::DateTime.to_code(),
            "chrono::DateTime<chrono::Utc>"
        );
        assert_eq!(RustType::Uuid.to_code(), "uuid::Uuid");
        assert_eq!(
            RustType::Vec(Box::new(RustType::String)).to_code(),
            "Vec<String>"
        );
        assert_eq!(
            RustType::Option(Box::new(RustType::I64)).to_code(),
            "Option<i64>"
        );
    }
}
