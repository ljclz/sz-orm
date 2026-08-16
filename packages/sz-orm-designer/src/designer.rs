use thiserror::Error;

use crate::design_ir::{select_ddl_generator, DesignRelation, DesignTable, SchemaDesign};

#[derive(Debug, Error)]
pub enum DesignerError {
    #[error("round-trip inconsistency: {field}")]
    RoundTripInconsistency { field: String },

    #[error("DDL generation partial: dialect={dialect}, feature={feature}")]
    DdlGenerationPartial { dialect: String, feature: String },

    #[error("web UI unavailable")]
    WebUiUnavailable,

    #[error("parse failed at line {line}: {reason}")]
    ParseFailed { line: usize, reason: String },

    #[error("masking rule not found: {field}")]
    MaskingRuleNotFound { field: String },
}

pub struct SchemaDesigner {
    pub(crate) design: SchemaDesign,
    ddl_generator: Box<dyn sz_orm_core::schema_sync::DdlGenerator>,
}

impl SchemaDesigner {
    pub fn new(design: SchemaDesign) -> Self {
        let dialect = design.dialect;
        Self {
            design,
            ddl_generator: select_ddl_generator(dialect),
        }
    }

    pub fn design(&self) -> &SchemaDesign {
        &self.design
    }

    pub fn add_table(&mut self, table: DesignTable) -> Result<(), DesignerError> {
        if self.design.tables.iter().any(|t| t.name == table.name) {
            return Err(DesignerError::RoundTripInconsistency {
                field: format!("table '{}' already exists", table.name),
            });
        }
        self.design.tables.push(table);
        Ok(())
    }

    pub fn modify_table(&mut self, name: &str, table: DesignTable) -> Result<(), DesignerError> {
        let pos = self
            .design
            .tables
            .iter()
            .position(|t| t.name == name)
            .ok_or_else(|| DesignerError::RoundTripInconsistency {
                field: format!("table '{}' not found", name),
            })?;
        self.design.tables[pos] = table;
        Ok(())
    }

    pub fn add_relation(&mut self, relation: DesignRelation) -> Result<(), DesignerError> {
        let from_exists = self
            .design
            .tables
            .iter()
            .any(|t| t.name == relation.from_table);
        let to_exists = self
            .design
            .tables
            .iter()
            .any(|t| t.name == relation.to_table);
        if !from_exists || !to_exists {
            return Err(DesignerError::RoundTripInconsistency {
                field: format!(
                    "relation references non-existent table: from={}, to={}",
                    relation.from_table, relation.to_table
                ),
            });
        }
        self.design.relations.push(relation);
        Ok(())
    }

    pub fn preview_ddl(&self) -> Result<Vec<String>, DesignerError> {
        let diff = self.design.to_schema_diff();
        self.ddl_generator
            .generate(&diff)
            .map_err(|e| DesignerError::DdlGenerationPartial {
                dialect: self.design.dialect.as_str().to_string(),
                feature: e.to_string(),
            })
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
    fn test_preview_ddl_all_dialects() {
        for dialect in [
            Dialect::MySql,
            Dialect::PostgreSql,
            Dialect::Sqlite,
            Dialect::Oracle,
            Dialect::Mssql,
        ] {
            let mut design = sample_design();
            design.dialect = dialect;
            let designer = SchemaDesigner::new(design);
            let ddl = designer
                .preview_ddl()
                .unwrap_or_else(|e| panic!("preview_ddl failed for {:?}: {}", dialect, e));
            assert!(!ddl.is_empty(), "DDL empty for {:?}", dialect);
            let combined = ddl.join(" ");
            assert!(
                combined.to_uppercase().contains("CREATE TABLE"),
                "no CREATE TABLE for {:?}: {}",
                dialect,
                combined
            );
        }
    }

    #[test]
    fn test_add_table_duplicate() {
        let design = sample_design();
        let mut designer = SchemaDesigner::new(design);
        let err = designer
            .add_table(DesignTable::new("users", vec![]))
            .unwrap_err();
        assert!(matches!(err, DesignerError::RoundTripInconsistency { .. }));
    }

    #[test]
    fn test_add_relation_validates_tables() {
        let design = sample_design();
        let mut designer = SchemaDesigner::new(design);
        let err = designer
            .add_relation(DesignRelation {
                from_table: "users".to_string(),
                to_table: "nonexistent".to_string(),
                from_column: "id".to_string(),
                to_column: "x".to_string(),
                cardinality: Cardinality::OneToMany,
            })
            .unwrap_err();
        assert!(matches!(err, DesignerError::RoundTripInconsistency { .. }));
    }

    #[test]
    fn test_design_accessor() {
        let design = sample_design();
        let designer = SchemaDesigner::new(design);
        assert_eq!(designer.design().tables.len(), 1);
        assert_eq!(designer.design().tables[0].name, "users");
    }

    #[test]
    fn test_add_table_success() {
        let design = sample_design();
        let mut designer = SchemaDesigner::new(design);
        designer
            .add_table(DesignTable::new(
                "orders",
                vec![DesignColumn::new("id", ColumnType::BigInt)],
            ))
            .unwrap();
        assert_eq!(designer.design().tables.len(), 2);
    }

    #[test]
    fn test_modify_table_success_and_not_found() {
        let design = sample_design();
        let mut designer = SchemaDesigner::new(design);
        let new_table = DesignTable::new("users", vec![DesignColumn::new("id", ColumnType::Int)]);
        designer.modify_table("users", new_table).unwrap();
        assert_eq!(designer.design().tables[0].columns.len(), 1);

        let err = designer
            .modify_table("nonexistent", DesignTable::new("x", vec![]))
            .unwrap_err();
        assert!(matches!(err, DesignerError::RoundTripInconsistency { .. }));
    }

    #[test]
    fn test_add_relation_success() {
        let design = sample_design().with_table(DesignTable::new(
            "orders",
            vec![DesignColumn::new("id", ColumnType::BigInt)],
        ));
        let mut designer = SchemaDesigner::new(design);
        designer
            .add_relation(DesignRelation {
                from_table: "users".to_string(),
                to_table: "orders".to_string(),
                from_column: "id".to_string(),
                to_column: "user_id".to_string(),
                cardinality: Cardinality::OneToMany,
            })
            .unwrap();
        assert_eq!(designer.design().relations.len(), 1);
    }
}
