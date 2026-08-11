use sz_orm_masking::{DataMasker, MaskingRule};

use crate::design_ir::SchemaDesign;

pub struct DesignerMasking {
    rules: Vec<(String, MaskingRule)>,
}

impl DesignerMasking {
    pub fn new() -> Self {
        Self { rules: vec![] }
    }

    pub fn with_rule(mut self, field_name: &str, rule: MaskingRule) -> Self {
        self.rules.push((field_name.to_string(), rule));
        self
    }

    pub fn apply(&self, design: &mut SchemaDesign) {
        for table in &mut design.tables {
            for col in &mut table.columns {
                for (field_name, rule) in &self.rules {
                    if col.name == *field_name {
                        col.name = DataMasker::apply(rule, field_name);
                    }
                }
            }
        }
    }
}

impl Default for DesignerMasking {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_ir::*;

    #[test]
    fn test_masking_password() {
        let mut design = SchemaDesign::new(Dialect::MySql).with_table(DesignTable::new(
            "users",
            vec![
                DesignColumn::new("id", ColumnType::BigInt).primary_key(),
                DesignColumn::new("name", ColumnType::Varchar(Some(255))),
                DesignColumn::new("password", ColumnType::Varchar(Some(255))),
            ],
        ));

        let masking = DesignerMasking::new().with_rule("password", MaskingRule::Password);
        masking.apply(&mut design);

        let password_col = &design.tables[0].columns[2];
        assert_ne!(password_col.name, "password");
        assert!(password_col.name.contains('*'));
    }

    #[test]
    fn test_masking_email() {
        let mut design = SchemaDesign::new(Dialect::MySql).with_table(DesignTable::new(
            "users",
            vec![
                DesignColumn::new("id", ColumnType::BigInt).primary_key(),
                DesignColumn::new("email", ColumnType::Varchar(Some(255))),
            ],
        ));

        let masking = DesignerMasking::new().with_rule("email", MaskingRule::Email);
        masking.apply(&mut design);

        let email_col = &design.tables[0].columns[1];
        assert_ne!(email_col.name, "email");
    }

    #[test]
    fn test_masking_no_match() {
        let mut design = SchemaDesign::new(Dialect::MySql).with_table(DesignTable::new(
            "users",
            vec![DesignColumn::new("id", ColumnType::BigInt).primary_key()],
        ));

        let masking = DesignerMasking::new().with_rule("password", MaskingRule::Password);
        masking.apply(&mut design);

        assert_eq!(design.tables[0].columns[0].name, "id");
    }
}
