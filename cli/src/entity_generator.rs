//! 实体代码生成器
//!
//! 从数据库 Schema 自动生成 Rust struct + 关系标注。

use serde::{Deserialize, Serialize};

/// 实体字段定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityField {
    pub name: String,
    pub rust_type: String,
    pub is_primary_key: bool,
    pub nullable: bool,
    pub db_type: String,
}

impl EntityField {
    pub fn new(
        name: impl Into<String>,
        rust_type: impl Into<String>,
        db_type: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            rust_type: rust_type.into(),
            is_primary_key: false,
            nullable: false,
            db_type: db_type.into(),
        }
    }

    pub fn primary_key(mut self) -> Self {
        self.is_primary_key = true;
        self
    }

    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRelation {
    pub relation_type: RelationType,
    pub target_entity: String,
    pub local_field: String,
    pub target_field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationType {
    HasOne,
    HasMany,
    BelongsTo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDefinition {
    pub name: String,
    pub table_name: String,
    pub fields: Vec<EntityField>,
    pub relations: Vec<EntityRelation>,
}

impl EntityDefinition {
    pub fn new(name: impl Into<String>, table_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            table_name: table_name.into(),
            fields: Vec::new(),
            relations: Vec::new(),
        }
    }

    pub fn with_field(mut self, field: EntityField) -> Self {
        self.fields.push(field);
        self
    }

    pub fn with_relation(mut self, relation: EntityRelation) -> Self {
        self.relations.push(relation);
        self
    }
}

/// 实体代码生成器
pub struct EntityGenerator;

impl EntityGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, entity: &EntityDefinition) -> String {
        let mut code = String::new();

        code.push_str("use serde::{Deserialize, Serialize};\n\n");
        code.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
        code.push_str(&format!("pub struct {} {{\n", entity.name));

        for field in &entity.fields {
            let rust_type = if field.nullable {
                format!("Option<{}>", field.rust_type)
            } else {
                field.rust_type.clone()
            };
            if field.is_primary_key {
                code.push_str("    /// Primary key\n");
            }
            code.push_str(&format!("    pub {}: {},\n", field.name, rust_type));
        }

        code.push_str("}\n\n");

        if !entity.relations.is_empty() {
            code.push_str(&self.generate_relations(entity));
        }

        code
    }

    fn generate_relations(&self, entity: &EntityDefinition) -> String {
        let mut code = String::new();
        code.push_str(&format!("impl {} {{\n", entity.name));
        for rel in &entity.relations {
            let method_name = rel.target_entity.to_lowercase();
            match rel.relation_type {
                RelationType::HasOne => {
                    code.push_str(&format!(
                        "    pub fn {}(&self) -> Option<&{}> {{\n",
                        method_name, rel.target_entity
                    ));
                    code.push_str("        None\n");
                    code.push_str("    }\n\n");
                }
                RelationType::HasMany => {
                    code.push_str(&format!(
                        "    pub fn {}s(&self) -> Vec<{}> {{\n",
                        method_name, rel.target_entity
                    ));
                    code.push_str("        Vec::new()\n");
                    code.push_str("    }\n\n");
                }
                RelationType::BelongsTo => {
                    code.push_str(&format!(
                        "    pub fn {}(&self) -> Option<&{}> {{\n",
                        method_name, rel.target_entity
                    ));
                    code.push_str("        None\n");
                    code.push_str("    }\n\n");
                }
            }
        }
        code.push_str("}\n");
        code
    }

    pub fn generate_all(&self, entities: &[EntityDefinition]) -> String {
        let mut code = String::new();
        for entity in entities {
            code.push_str(&self.generate(entity));
            code.push('\n');
        }
        code
    }

    pub fn infer_rust_type(db_type: &str) -> String {
        let lower = db_type.to_lowercase();
        if lower.contains("int") || lower.contains("serial") {
            "i64".to_string()
        } else if lower.contains("float")
            || lower.contains("double")
            || lower.contains("real")
            || lower.contains("decimal")
            || lower.contains("numeric")
        {
            "f64".to_string()
        } else if lower.contains("bool") {
            "bool".to_string()
        } else if lower.contains("json") {
            "serde_json::Value".to_string()
        } else {
            "String".to_string()
        }
    }
}

impl Default for EntityGenerator {
    fn default() -> Self {
        Self::new()
    }
}
