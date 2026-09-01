//! Schema 文档生成器

use crate::entity_generator::{EntityDefinition, RelationType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocOutput {
    pub markdown: String,
    pub plantuml: String,
}

pub struct DocGenerator;

impl DocGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate_doc(&self, entities: &[EntityDefinition]) -> DocOutput {
        let markdown = self.generate_markdown(entities);
        let plantuml = self.generate_er_diagram(entities);
        DocOutput { markdown, plantuml }
    }

    fn generate_markdown(&self, entities: &[EntityDefinition]) -> String {
        let mut md = String::new();
        md.push_str("# Database Schema Documentation\n\n");
        md.push_str(&format!("Generated entities: {}\n\n", entities.len()));
        md.push_str("---\n\n");

        for entity in entities {
            md.push_str(&format!("## {}\n\n", entity.name));
            md.push_str(&format!("**Table**: `{}`\n\n", entity.table_name));
            md.push_str("### Fields\n\n");
            md.push_str("| Name | Type | PK | Nullable | DB Type |\n");
            md.push_str("|------|------|----|---------|--------|\n");
            for field in &entity.fields {
                md.push_str(&format!(
                    "| `{}` | `{}` | {} | {} | `{}` |\n",
                    field.name,
                    field.rust_type,
                    if field.is_primary_key { "✓" } else { "" },
                    if field.nullable { "✓" } else { "" },
                    field.db_type
                ));
            }
            md.push('\n');
            if !entity.relations.is_empty() {
                md.push_str("### Relations\n\n");
                for rel in &entity.relations {
                    let rel_type = match rel.relation_type {
                        RelationType::HasOne => "has one",
                        RelationType::HasMany => "has many",
                        RelationType::BelongsTo => "belongs to",
                    };
                    md.push_str(&format!(
                        "- **{}** `{}` ({} → {})\n",
                        rel_type, rel.target_entity, rel.local_field, rel.target_field
                    ));
                }
                md.push('\n');
            }
            md.push_str("---\n\n");
        }
        md
    }

    fn generate_er_diagram(&self, entities: &[EntityDefinition]) -> String {
        let mut puml = String::new();
        puml.push_str("@startuml\n\n");
        for entity in entities {
            puml.push_str(&format!("entity {} {{\n", entity.name));
            for field in &entity.fields {
                if field.is_primary_key {
                    puml.push_str(&format!("  * {} : {}\n", field.name, field.rust_type));
                } else {
                    puml.push_str(&format!("  {} : {}\n", field.name, field.rust_type));
                }
            }
            puml.push_str("}\n\n");
        }
        for entity in entities {
            for rel in &entity.relations {
                let (from, to, arrow) = match rel.relation_type {
                    RelationType::HasOne => (&entity.name, &rel.target_entity, "||--o|"),
                    RelationType::HasMany => (&entity.name, &rel.target_entity, "||--|{"),
                    RelationType::BelongsTo => (&rel.target_entity, &entity.name, "||--|{"),
                };
                puml.push_str(&format!(
                    "{} {} {} : {}\n",
                    from, arrow, to, rel.local_field
                ));
            }
        }
        puml.push_str("@enduml\n");
        puml
    }
}

impl Default for DocGenerator {
    fn default() -> Self {
        Self::new()
    }
}
