use sz_orm_core::migration::Migration;

use crate::design_ir::{select_ddl_generator, to_pascal_case, Dialect, SchemaDesign};
use crate::designer::DesignerError;

pub struct DesignerCodeGenerator {
    dialect: Dialect,
}

impl DesignerCodeGenerator {
    pub fn new(dialect: Dialect) -> Self {
        Self { dialect }
    }

    pub fn generate_migration(&self, design: &SchemaDesign) -> Result<Migration, DesignerError> {
        let diff = design.to_schema_diff();
        let generator = select_ddl_generator(self.dialect);
        let ddl = generator
            .generate(&diff)
            .map_err(|e| DesignerError::DdlGenerationPartial {
                dialect: self.dialect.as_str().to_string(),
                feature: e.to_string(),
            })?;

        let sql_up = ddl.join("\n");
        let sql_down = design
            .tables
            .iter()
            .rev()
            .map(|t| format!("DROP TABLE IF EXISTS {};", t.name))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(Migration::new(
            "1",
            "designer_generated",
            &sql_up,
            &sql_down,
        ))
    }

    pub fn generate_model_code(&self, design: &SchemaDesign) -> Result<String, DesignerError> {
        let _ = quote::quote! { struct _Marker; };

        let mut code = String::new();

        for table in &design.tables {
            let struct_name = to_pascal_case(&table.name);
            code.push_str("#[derive(Debug, Clone, sz_orm_core::Model)]\n");
            code.push_str(&format!("pub struct {} {{\n", struct_name));

            for col in &table.columns {
                let base_type = col.col_type.to_rust_type();
                let field_type = if col.nullable && !col.is_primary_key {
                    format!("Option<{}>", base_type)
                } else {
                    base_type.to_string()
                };
                code.push_str(&format!("    pub {}: {},\n", col.name, field_type));
            }

            code.push_str("}\n\n");
        }

        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_ir::*;

    fn sample_design() -> SchemaDesign {
        SchemaDesign::new(Dialect::MySql).with_table(DesignTable::new(
            "users",
            vec![
                DesignColumn::new("id", ColumnType::BigInt)
                    .primary_key()
                    .auto_increment(),
                DesignColumn::new("name", ColumnType::Varchar(Some(255))),
                DesignColumn::new("email", ColumnType::Varchar(Some(255))).nullable(),
            ],
        ))
    }

    #[test]
    fn test_generate_migration() {
        let design = sample_design();
        let gen = DesignerCodeGenerator::new(Dialect::MySql);
        let migration = gen.generate_migration(&design).unwrap();

        assert!(migration.sql_up.contains("users"));
        assert!(migration.sql_down.contains("DROP TABLE"));
    }

    #[test]
    fn test_generate_model_code() {
        let design = sample_design();
        let gen = DesignerCodeGenerator::new(Dialect::MySql);
        let code = gen.generate_model_code(&design).unwrap();

        assert!(code.contains("pub struct Users"));
        assert!(code.contains("pub id: i64"));
        assert!(code.contains("pub name: String"));
        assert!(code.contains("pub email: Option<String>"));
        assert!(code.contains("sz_orm_core::Model"));
    }

    #[test]
    fn test_generate_model_code_all_dialects() {
        for dialect in [
            Dialect::MySql,
            Dialect::PostgreSql,
            Dialect::Sqlite,
            Dialect::Oracle,
            Dialect::Mssql,
        ] {
            let design = sample_design();
            let gen = DesignerCodeGenerator::new(dialect);
            let migration = gen.generate_migration(&design);
            assert!(migration.is_ok(), "migration failed for {:?}", dialect);
        }
    }

    #[test]
    fn test_generate_migration_multi_table() {
        let design = sample_design().with_table(DesignTable::new(
            "orders",
            vec![DesignColumn::new("id", ColumnType::BigInt).primary_key()],
        ));
        let gen = DesignerCodeGenerator::new(Dialect::MySql);
        let migration = gen.generate_migration(&design).unwrap();
        assert!(migration.sql_up.contains("users"));
        assert!(migration.sql_up.contains("orders"));
        assert!(migration.sql_down.contains("DROP TABLE"));
    }
}
