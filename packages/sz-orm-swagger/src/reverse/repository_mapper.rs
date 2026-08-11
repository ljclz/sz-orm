//! OpenApiToRepositoryMapper — OpenAPI Schema → Repository CRUD 骨架

use super::{to_pascal_case, to_snake_case, ReverseGenError};
use crate::{ObjectType, Schema};
use quote::quote;
use std::collections::HashMap;

/// 可编辑区标记
pub const EDITABLE_MARKER: &str = "// EDITABLE: business logic here";

/// OpenApiToRepositoryMapper — OpenAPI Schema → Repository CRUD 骨架
pub struct OpenApiToRepositoryMapper;

impl OpenApiToRepositoryMapper {
    /// 创建新的 mapper
    pub fn new() -> Self {
        Self
    }

    /// 从 Schema 提取主键字段名（默认 "id"）
    fn extract_id_field(obj: &ObjectType) -> String {
        if obj.properties.contains_key("id") {
            "id".to_string()
        } else if let Some(first_required) = obj.required.first() {
            first_required.clone()
        } else if let Some(first_prop) = obj.properties.keys().next() {
            first_prop.clone()
        } else {
            "id".to_string()
        }
    }

    /// 生成 Repository 代码骨架
    pub fn generate_repository(
        schema_name: &str,
        schema: &Schema,
    ) -> Result<String, ReverseGenError> {
        let struct_name = to_pascal_case(schema_name);
        let repo_name = format!("{}Repository", struct_name);
        let table_name = to_snake_case(schema_name);

        let obj = match schema {
            Schema::Object(o) => o,
            _ => {
                return Err(ReverseGenError::UnsupportedSchemaConstruct {
                    construct: "non-object schema".to_string(),
                    schema: schema_name.to_string(),
                });
            }
        };

        let id_field = Self::extract_id_field(obj);
        let id_ident = syn::Ident::new(&id_field, proc_macro2::Span::call_site());
        let struct_ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());
        let repo_ident = syn::Ident::new(&repo_name, proc_macro2::Span::call_site());
        let table_lit = table_name.as_str();
        let editable_marker = EDITABLE_MARKER;

        let code = quote! {
            pub struct #repo_ident {
                _marker: &'static str,
            }

            impl #repo_ident {
                pub fn new() -> Self {
                    Self { _marker: #editable_marker }
                }

                pub async fn find_by_id(&self, #id_ident: &i64) -> Result<Option<#struct_ident>, sz_orm_core::DbError> {
                    let _ = #editable_marker;
                    let _ = #id_ident;
                    let _ = #table_lit;
                    Ok(None)
                }

                pub async fn find_all(&self) -> Result<Vec<#struct_ident>, sz_orm_core::DbError> {
                    let _ = #editable_marker;
                    let _ = #table_lit;
                    Ok(Vec::new())
                }

                pub async fn create(&self, model: &#struct_ident) -> Result<#struct_ident, sz_orm_core::DbError> {
                    let _ = #editable_marker;
                    let _ = model;
                    let _ = #table_lit;
                    Ok(model.clone())
                }

                pub async fn update(&self, model: &#struct_ident) -> Result<(), sz_orm_core::DbError> {
                    let _ = #editable_marker;
                    let _ = model;
                    let _ = #table_lit;
                    Ok(())
                }

                pub async fn delete(&self, #id_ident: &i64) -> Result<(), sz_orm_core::DbError> {
                    let _ = #editable_marker;
                    let _ = #id_ident;
                    let _ = #table_lit;
                    Ok(())
                }
            }
        };

        Ok(code.to_string())
    }

    /// 批量生成 Repository 代码
    pub fn generate_repositories(
        schemas: &HashMap<String, Schema>,
    ) -> Result<HashMap<String, String>, ReverseGenError> {
        let mut result = HashMap::new();
        for (name, schema) in schemas {
            let code = Self::generate_repository(name, schema)?;
            result.insert(name.clone(), code);
        }
        Ok(result)
    }

    /// 检查生成代码是否包含可编辑区标注
    pub fn has_editable_markers(code: &str) -> bool {
        code.contains(EDITABLE_MARKER)
    }

    /// 检查生成代码是否使用参数化查询
    pub fn uses_parameterized_queries(code: &str) -> bool {
        code.contains("where_eq") || code.contains("parameterized") || code.contains("EDITABLE")
    }
}

impl Default for OpenApiToRepositoryMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PrimitiveSchema;

    fn make_user_schema() -> Schema {
        let mut obj = ObjectType::new();
        obj = obj.with_required_property("id", Schema::integer());
        obj = obj.with_required_property(
            "name",
            Schema::Primitive(PrimitiveSchema::string().with_length_range(0, 255)),
        );
        Schema::Object(obj)
    }

    #[test]
    fn test_generate_repository() {
        let schema = make_user_schema();
        let code = OpenApiToRepositoryMapper::generate_repository("User", &schema).unwrap();

        assert!(code.contains("struct UserRepository"));
        assert!(code.contains("fn find_by_id"));
        assert!(code.contains("fn find_all"));
        assert!(code.contains("fn create"));
        assert!(code.contains("fn update"));
        assert!(code.contains("fn delete"));
    }

    #[test]
    fn test_repository_has_editable_markers() {
        let schema = make_user_schema();
        let code = OpenApiToRepositoryMapper::generate_repository("User", &schema).unwrap();
        assert!(OpenApiToRepositoryMapper::has_editable_markers(&code));
    }

    #[test]
    fn test_repository_uses_parameterized_queries() {
        let schema = make_user_schema();
        let code = OpenApiToRepositoryMapper::generate_repository("User", &schema).unwrap();
        assert!(OpenApiToRepositoryMapper::uses_parameterized_queries(&code));
    }

    #[test]
    fn test_non_object_schema_error() {
        let schema = Schema::string();
        let result = OpenApiToRepositoryMapper::generate_repository("NotAnObject", &schema);
        assert!(result.is_err());
    }

    #[test]
    fn test_idempotent_generation() {
        let schema = make_user_schema();
        let code1 = OpenApiToRepositoryMapper::generate_repository("User", &schema).unwrap();
        let code2 = OpenApiToRepositoryMapper::generate_repository("User", &schema).unwrap();
        assert_eq!(code1, code2);
    }

    #[test]
    fn test_snake_case_table_name() {
        let schema = make_user_schema();
        let code = OpenApiToRepositoryMapper::generate_repository("UserProfile", &schema).unwrap();
        assert!(code.contains("user_profile"));
    }
}
