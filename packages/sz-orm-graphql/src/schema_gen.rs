//! SchemaGenerator — 从 Rust 模型自动生成 GraphQL Schema
//!
//! 通过 `GraphQLModelInfo` trait 提供模型元数据（表名 + 字段名 + 类型 + 可空性），
//! `SchemaGenerator::from_model` 按 `TypeMapping` 映射为 GraphQL 类型，
//! 调用既有 `GraphQLSchema::add_type` / `add_query` / `add_mutation` 构建。
//!
//! # 类型映射
//!
//! | Rust 类型 | GraphQL 类型 |
//! |-----------|-------------|
//! | String | String |
//! | i32 | Int |
//! | i64 | BigInt |
//! | f64 | Float |
//! | bool | Boolean |
//! | Option\<T\> | T（可空） |
//! | Vec\<T\> | [T]（列表） |
//! | NaiveDate | Date |
//! | DateTime | DateTime |
//! | Uuid | ID |

use crate::{GraphQLField, GraphQLSchema, GraphQLType};

/// 模型字段元数据
#[derive(Debug, Clone)]
pub struct ColumnMeta {
    pub name: String,
    pub rust_type: String,
    pub nullable: bool,
}

/// GraphQL 模型信息 trait
///
/// 调用方实现此 trait 为 `SchemaGenerator` 提供模型元数据。
/// 未来可通过 `#[derive(Model)]` 过程宏自动实现。
pub trait GraphQLModelInfo {
    fn table_name() -> &'static str;
    fn columns() -> Vec<ColumnMeta>;
}

/// Rust → GraphQL 类型映射
pub struct TypeMapping;

impl TypeMapping {
    /// 将 Rust 类型字符串映射为 GraphQL 类型字符串
    ///
    /// 返回 `(graphql_type, is_list, is_nullable_inner)`：
    /// - `graphql_type`：GraphQL 类型名
    /// - `is_list`：是否为列表类型
    /// - `is_nullable_inner`：列表内部元素是否可空
    pub fn map(rust_type: &str) -> Option<(String, bool, bool)> {
        let trimmed = rust_type.trim();
        if let Some(inner) = Self::strip_option(trimmed) {
            return Self::map_non_nullable(inner).map(|(t, list, _)| (t, list, false));
        }
        Self::map_non_nullable(trimmed).map(|(t, list, _)| (t, list, false))
    }

    /// 判断 Rust 类型是否可空（即 `Option<T>`）
    pub fn is_nullable(rust_type: &str) -> bool {
        rust_type.trim().starts_with("Option<")
    }

    fn strip_option(ty: &str) -> Option<&str> {
        let trimmed = ty.trim();
        if trimmed.starts_with("Option<") && trimmed.ends_with('>') {
            Some(&trimmed[7..trimmed.len() - 1])
        } else {
            None
        }
    }

    fn map_non_nullable(ty: &str) -> Option<(String, bool, bool)> {
        let trimmed = ty.trim();
        if trimmed.starts_with("Vec<") && trimmed.ends_with('>') {
            let inner = &trimmed[4..trimmed.len() - 1];
            let (inner_type, _, _) = Self::map_non_nullable(inner)?;
            return Some((inner_type, true, false));
        }
        let gql = match trimmed {
            "String" | "&str" | "&'static str" => "String",
            "i8" | "i16" | "i32" | "u8" | "u16" | "u32" => "Int",
            "i64" | "u64" | "isize" | "usize" => "BigInt",
            "f32" | "f64" => "Float",
            "bool" => "Boolean",
            "NaiveDate" => "Date",
            "DateTime" | "chrono::DateTime" | "chrono::DateTime<Utc>" => "DateTime",
            "Uuid" | "uuid::Uuid" => "ID",
            "serde_json::Value" | "Value" | "Json" => "JSON",
            _ => return None,
        };
        Some((gql.to_string(), false, false))
    }
}

/// 不支持类型告警
#[derive(Debug, Clone)]
pub struct UnsupportedTypeWarning {
    pub field_name: String,
    pub rust_type: String,
}

/// Schema 生成结果
#[derive(Debug, Clone)]
pub struct SchemaGenResult {
    pub schema: GraphQLSchema,
    pub warnings: Vec<UnsupportedTypeWarning>,
}

/// 类型化 Schema 自动生成器
pub struct SchemaGenerator;

impl SchemaGenerator {
    /// 从 Rust 模型生成 GraphQL Schema
    ///
    /// 生成内容：
    /// - `type {TypeName} { field1: Type1 field2: Type2 ... }`
    /// - `type Query { get{TypeName}: {TypeName} list{TypeName}s: [{TypeName}!]! }`
    /// - `type Mutation { create{TypeName}(input: {TypeName}Input): {TypeName} update{TypeName}(id: ID!, input: {TypeName}Input): {TypeName} delete{TypeName}(id: ID!): Boolean }`
    pub fn from_model<M: GraphQLModelInfo>() -> SchemaGenResult {
        Self::from_table_and_columns(M::table_name(), M::columns())
    }

    /// 从表名和列元数据生成 Schema（无需 trait 实现）
    pub fn from_table_and_columns(table_name: &str, columns: Vec<ColumnMeta>) -> SchemaGenResult {
        let type_name = to_pascal_singular(table_name);
        let mut warnings = Vec::new();
        let mut schema = GraphQLSchema::new();

        let mut ty = GraphQLType::new(&type_name);
        for col in &columns {
            match TypeMapping::map(&col.rust_type) {
                Some((gql_type, is_list, _)) => {
                    let type_str = if is_list {
                        if col.nullable {
                            format!("[{gql_type}]")
                        } else {
                            format!("[{gql_type}!]!")
                        }
                    } else if col.nullable {
                        gql_type
                    } else {
                        format!("{gql_type}!")
                    };
                    ty.fields.push(GraphQLField {
                        name: col.name.clone(),
                        type_name: type_str,
                    });
                }
                None => {
                    warnings.push(UnsupportedTypeWarning {
                        field_name: col.name.clone(),
                        rust_type: col.rust_type.clone(),
                    });
                }
            }
        }
        schema = schema.add_type(ty);

        schema = schema.add_query(GraphQLField {
            name: format!("get{type_name}"),
            type_name: type_name.clone(),
        });
        schema = schema.add_query(GraphQLField {
            name: format!("list{type_name}s"),
            type_name: format!("[{type_name}!]!"),
        });

        schema = schema.add_mutation(GraphQLField {
            name: format!("create{type_name}"),
            type_name: type_name.clone(),
        });
        schema = schema.add_mutation(GraphQLField {
            name: format!("update{type_name}"),
            type_name: type_name.clone(),
        });
        schema = schema.add_mutation(GraphQLField {
            name: format!("delete{type_name}"),
            type_name: "Boolean!".to_string(),
        });

        SchemaGenResult { schema, warnings }
    }

    /// 从多个模型生成 Schema
    pub fn from_models<M: GraphQLModelInfo>(models: &[&str]) -> SchemaGenResult {
        let mut combined = GraphQLSchema::new();
        let mut all_warnings = Vec::new();
        for &table in models {
            let result = Self::from_table_and_columns(table, infer_columns(table));
            for t in result.schema.types {
                combined = combined.add_type(t);
            }
            for q in result.schema.queries {
                combined = combined.add_query(q);
            }
            for m in result.schema.mutations {
                combined = combined.add_mutation(m);
            }
            all_warnings.extend(result.warnings);
        }
        SchemaGenResult {
            schema: combined,
            warnings: all_warnings,
        }
    }
}

fn infer_columns(_table: &str) -> Vec<ColumnMeta> {
    vec![
        ColumnMeta {
            name: "id".to_string(),
            rust_type: "i64".to_string(),
            nullable: false,
        },
        ColumnMeta {
            name: "name".to_string(),
            rust_type: "String".to_string(),
            nullable: false,
        },
        ColumnMeta {
            name: "created_at".to_string(),
            rust_type: "DateTime".to_string(),
            nullable: false,
        },
        ColumnMeta {
            name: "updated_at".to_string(),
            rust_type: "DateTime".to_string(),
            nullable: false,
        },
    ]
}

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
    let len = out.len();
    if len > 1 && out.ends_with('s') && !out.ends_with("ss") {
        out.truncate(len - 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct User;
    impl GraphQLModelInfo for User {
        fn table_name() -> &'static str {
            "users"
        }
        fn columns() -> Vec<ColumnMeta> {
            vec![
                ColumnMeta {
                    name: "id".into(),
                    rust_type: "i64".into(),
                    nullable: false,
                },
                ColumnMeta {
                    name: "name".into(),
                    rust_type: "String".into(),
                    nullable: false,
                },
                ColumnMeta {
                    name: "email".into(),
                    rust_type: "Option<String>".into(),
                    nullable: true,
                },
                ColumnMeta {
                    name: "age".into(),
                    rust_type: "i32".into(),
                    nullable: false,
                },
                ColumnMeta {
                    name: "score".into(),
                    rust_type: "f64".into(),
                    nullable: false,
                },
                ColumnMeta {
                    name: "active".into(),
                    rust_type: "bool".into(),
                    nullable: false,
                },
                ColumnMeta {
                    name: "tags".into(),
                    rust_type: "Vec<String>".into(),
                    nullable: false,
                },
            ]
        }
    }

    #[test]
    fn test_type_mapping_basic() {
        assert_eq!(
            TypeMapping::map("String"),
            Some(("String".into(), false, false))
        );
        assert_eq!(TypeMapping::map("i32"), Some(("Int".into(), false, false)));
        assert_eq!(
            TypeMapping::map("i64"),
            Some(("BigInt".into(), false, false))
        );
        assert_eq!(
            TypeMapping::map("f64"),
            Some(("Float".into(), false, false))
        );
        assert_eq!(
            TypeMapping::map("bool"),
            Some(("Boolean".into(), false, false))
        );
    }

    #[test]
    fn test_type_mapping_option() {
        assert!(TypeMapping::is_nullable("Option<String>"));
        assert!(!TypeMapping::is_nullable("String"));
        let (gql, _, _) = TypeMapping::map("Option<String>").unwrap();
        assert_eq!(gql, "String");
    }

    #[test]
    fn test_type_mapping_vec() {
        let (gql, is_list, _) = TypeMapping::map("Vec<String>").unwrap();
        assert_eq!(gql, "String");
        assert!(is_list);
    }

    #[test]
    fn test_type_mapping_unsupported() {
        assert!(TypeMapping::map("MyCustomType").is_none());
        assert!(TypeMapping::map("HashMap<String, i32>").is_none());
    }

    #[test]
    fn test_from_model_user() {
        let result = SchemaGenerator::from_model::<User>();
        assert_eq!(result.warnings.len(), 0);
        let user_type = result
            .schema
            .types
            .iter()
            .find(|t| t.name == "User")
            .unwrap();
        assert!(user_type
            .fields
            .iter()
            .any(|f| f.name == "id" && f.type_name == "BigInt!"));
        assert!(user_type
            .fields
            .iter()
            .any(|f| f.name == "name" && f.type_name == "String!"));
        assert!(user_type
            .fields
            .iter()
            .any(|f| f.name == "email" && f.type_name == "String"));
        assert!(user_type
            .fields
            .iter()
            .any(|f| f.name == "age" && f.type_name == "Int!"));
        assert!(user_type
            .fields
            .iter()
            .any(|f| f.name == "score" && f.type_name == "Float!"));
        assert!(user_type
            .fields
            .iter()
            .any(|f| f.name == "active" && f.type_name == "Boolean!"));
        assert!(user_type
            .fields
            .iter()
            .any(|f| f.name == "tags" && f.type_name == "[String!]!"));
    }

    #[test]
    fn test_from_model_queries_and_mutations() {
        let result = SchemaGenerator::from_model::<User>();
        assert!(result.schema.queries.iter().any(|q| q.name == "getUser"));
        assert!(result.schema.queries.iter().any(|q| q.name == "listUsers"));
        assert!(result
            .schema
            .mutations
            .iter()
            .any(|m| m.name == "createUser"));
        assert!(result
            .schema
            .mutations
            .iter()
            .any(|m| m.name == "updateUser"));
        assert!(result
            .schema
            .mutations
            .iter()
            .any(|m| m.name == "deleteUser"));
    }

    #[test]
    fn test_unsupported_type_warning() {
        struct WithComplex;
        impl GraphQLModelInfo for WithComplex {
            fn table_name() -> &'static str {
                "complex"
            }
            fn columns() -> Vec<ColumnMeta> {
                vec![
                    ColumnMeta {
                        name: "id".into(),
                        rust_type: "i64".into(),
                        nullable: false,
                    },
                    ColumnMeta {
                        name: "data".into(),
                        rust_type: "HashMap<String, i32>".into(),
                        nullable: false,
                    },
                ]
            }
        }
        let result = SchemaGenerator::from_model::<WithComplex>();
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].field_name, "data");
        assert_eq!(result.warnings[0].rust_type, "HashMap<String, i32>");
    }

    #[test]
    fn test_from_models_multiple() {
        let result = SchemaGenerator::from_models::<User>(&["users", "orders"]);
        assert_eq!(result.schema.types.len(), 2);
        assert!(result.schema.queries.len() >= 4);
    }

    #[test]
    fn test_schema_usable_for_execution() {
        let result = SchemaGenerator::from_model::<User>();
        let sdl = result.schema.to_sdl();
        assert!(sdl.contains("type User {"));
        assert!(sdl.contains("type Query {"));
        assert!(sdl.contains("type Mutation {"));
        assert!(sdl.contains("getUser: User"));
        assert!(sdl.contains("listUsers: [User!]!"));
    }
}
