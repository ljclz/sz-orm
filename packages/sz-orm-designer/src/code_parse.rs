use quote::ToTokens;

use crate::design_ir::{ColumnType, DesignColumn, DesignTable, Dialect, SchemaDesign};
use crate::designer::DesignerError;

pub struct CodeReverseParser;

impl CodeReverseParser {
    pub fn parse_migration(sql: &str, dialect: Dialect) -> Result<SchemaDesign, DesignerError> {
        let mut tables = Vec::new();

        for part in sql.split(';') {
            let part = part.trim();
            if part.to_uppercase().starts_with("CREATE TABLE") {
                tables.push(Self::parse_create_table(part)?);
            }
        }

        Ok(SchemaDesign {
            tables,
            relations: vec![],
            dialect,
        })
    }

    fn parse_create_table(sql: &str) -> Result<DesignTable, DesignerError> {
        let after_create = sql
            .trim()
            .trim_start_matches(|c: char| c.is_ascii_whitespace())
            .strip_prefix("CREATE TABLE")
            .ok_or_else(|| DesignerError::ParseFailed {
                line: 0,
                reason: "not a CREATE TABLE".to_string(),
            })?
            .trim();

        let after_create = after_create
            .strip_prefix("IF NOT EXISTS")
            .unwrap_or(after_create)
            .trim();

        let paren_pos = after_create
            .find('(')
            .ok_or_else(|| DesignerError::ParseFailed {
                line: 0,
                reason: "no opening paren".to_string(),
            })?;

        let table_name = after_create[..paren_pos]
            .trim()
            .trim_matches('"')
            .trim_matches('`')
            .to_string();

        let close_paren = after_create.rfind(')').unwrap_or(after_create.len());
        let columns_str = &after_create[paren_pos + 1..close_paren];

        let mut columns = Vec::new();
        for col_def in columns_str.split(',') {
            let col_def = col_def.trim();
            let upper = col_def.to_uppercase();
            if col_def.is_empty()
                || upper.starts_with("PRIMARY KEY")
                || upper.starts_with("FOREIGN KEY")
                || upper.starts_with("CONSTRAINT")
                || upper.starts_with("INDEX")
                || upper.starts_with("UNIQUE")
                || upper.starts_with("KEY")
            {
                continue;
            }

            let parts: Vec<&str> = col_def.splitn(2, char::is_whitespace).collect();
            if parts.len() < 2 {
                continue;
            }

            let col_name = parts[0].trim_matches('"').trim_matches('`').to_string();
            let rest = parts[1].trim();

            let type_part: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            let col_type = ColumnType::from_sql_type(&type_part);

            let is_not_null = upper.contains("NOT NULL");
            let is_primary = upper.contains("PRIMARY KEY");
            let is_auto_inc = upper.contains("AUTO_INCREMENT")
                || upper.contains("AUTOINCREMENT")
                || upper.contains("IDENTITY")
                || upper.contains("SERIAL");

            columns.push(DesignColumn {
                name: col_name,
                col_type,
                nullable: !is_not_null && !is_primary,
                default: None,
                comment: None,
                is_primary_key: is_primary,
                is_auto_increment: is_auto_inc,
            });
        }

        Ok(DesignTable {
            name: table_name,
            columns,
            indexes: vec![],
            comment: None,
        })
    }

    pub fn parse_model_code(
        rust_code: &str,
        dialect: Dialect,
    ) -> Result<SchemaDesign, DesignerError> {
        let file = syn::parse_file(rust_code).map_err(|e| DesignerError::ParseFailed {
            line: 0,
            reason: e.to_string(),
        })?;

        let mut tables = Vec::new();
        for item in &file.items {
            if let syn::Item::Struct(s) = item {
                if Self::has_model_derive(&s.attrs) {
                    tables.push(Self::parse_struct_to_table(s));
                }
            }
        }

        Ok(SchemaDesign {
            tables,
            relations: vec![],
            dialect,
        })
    }

    fn has_model_derive(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| {
            if attr.path().is_ident("derive") {
                let tokens = attr
                    .meta
                    .require_list()
                    .map(|l| l.tokens.to_string())
                    .unwrap_or_default();
                tokens.contains("Model")
            } else {
                false
            }
        })
    }

    fn parse_struct_to_table(s: &syn::ItemStruct) -> DesignTable {
        let name = s.ident.to_string();
        let mut columns = Vec::new();

        if let syn::Fields::Named(named) = &s.fields {
            for field in &named.named {
                let field_name = field
                    .ident
                    .as_ref()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                let field_type = field.ty.to_token_stream().to_string();
                let col_type = Self::rust_type_to_column_type(&field_type);
                let nullable = field_type.replace(' ', "").starts_with("Option<");
                columns.push(DesignColumn {
                    name: field_name,
                    col_type,
                    nullable,
                    default: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                });
            }
        }

        DesignTable {
            name,
            columns,
            indexes: vec![],
            comment: None,
        }
    }

    fn rust_type_to_column_type(rust_type: &str) -> ColumnType {
        let trimmed = rust_type.replace(' ', "");
        let base = if trimmed.starts_with("Option<") && trimmed.ends_with('>') {
            &trimmed[7..trimmed.len() - 1]
        } else {
            &trimmed
        };

        match base {
            "i8" => ColumnType::TinyInt,
            "i16" => ColumnType::SmallInt,
            "i32" => ColumnType::Int,
            "i64" => ColumnType::BigInt,
            "bool" => ColumnType::Boolean,
            "f32" => ColumnType::Float,
            "f64" => ColumnType::Double,
            "String" => ColumnType::Varchar(None),
            "Vec<u8>" => ColumnType::Binary,
            _ if base.contains("DateTime") => ColumnType::DateTime,
            _ if base.contains("Uuid") => ColumnType::Uuid,
            _ => ColumnType::Custom(base.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_ir::*;

    #[test]
    fn test_parse_migration() {
        let sql = "CREATE TABLE users (id BIGINT NOT NULL PRIMARY KEY, name VARCHAR(255) NOT NULL, email VARCHAR(255));";
        let design = CodeReverseParser::parse_migration(sql, Dialect::MySql).unwrap();

        assert_eq!(design.tables.len(), 1);
        assert_eq!(design.tables[0].name, "users");
        assert_eq!(design.tables[0].columns.len(), 3);
        assert_eq!(design.tables[0].columns[0].name, "id");
        assert!(design.tables[0].columns[0].is_primary_key);
        assert!(!design.tables[0].columns[0].nullable);
        assert!(design.tables[0].columns[2].nullable);
    }

    #[test]
    fn test_parse_model_code() {
        let code = r#"
#[derive(Debug, Clone, sz_orm_core::Model)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: Option<String>,
}
"#;
        let design = CodeReverseParser::parse_model_code(code, Dialect::MySql).unwrap();

        assert_eq!(design.tables.len(), 1);
        assert_eq!(design.tables[0].name, "User");
        assert_eq!(design.tables[0].columns.len(), 3);
        assert_eq!(design.tables[0].columns[0].col_type, ColumnType::BigInt);
        assert_eq!(
            design.tables[0].columns[1].col_type,
            ColumnType::Varchar(None)
        );
        assert!(design.tables[0].columns[2].nullable);
    }

    #[test]
    fn test_roundtrip_design_to_code_to_design() {
        let design = SchemaDesign::new(Dialect::MySql).with_table(DesignTable::new(
            "users",
            vec![
                DesignColumn::new("id", ColumnType::BigInt).primary_key(),
                DesignColumn::new("name", ColumnType::Varchar(Some(255))),
            ],
        ));

        let gen = crate::code_gen::DesignerCodeGenerator::new(Dialect::MySql);
        let code = gen.generate_model_code(&design).unwrap();
        let parsed = CodeReverseParser::parse_model_code(&code, Dialect::MySql).unwrap();

        assert_eq!(parsed.tables.len(), 1);
        assert_eq!(parsed.tables[0].name, "Users");
        assert_eq!(parsed.tables[0].columns.len(), 2);
    }
}
