//! ER 图交互式建模（TASK-029）

use crate::types::MultimodalError;
use serde::{Deserialize, Serialize};

/// ER 图实体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entity {
    pub name: String,
    pub fields: Vec<Field>,
}

/// 实体字段
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub data_type: String,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
    pub references: Option<String>,
}

/// ER 图关系
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Relationship {
    pub from_entity: String,
    pub to_entity: String,
    pub from_cardinality: Cardinality,
    pub to_cardinality: Cardinality,
}

/// 基数
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Cardinality {
    One,
    Many,
}

/// ER 图
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErDiagram {
    pub entities: Vec<Entity>,
    pub relationships: Vec<Relationship>,
}

/// ER 图交互器
pub struct ErDiagramInteractor;

impl ErDiagramInteractor {
    pub fn new() -> Self {
        Self
    }

    /// 从自然语言描述生成 ER 图
    pub fn from_natural_language(&self, description: &str) -> Result<ErDiagram, MultimodalError> {
        if description.is_empty() {
            return Err(MultimodalError::RenderFallback);
        }

        let entities = Self::parse_entities(description);
        let relationships = Self::parse_relationships(description, &entities);

        Ok(ErDiagram {
            entities,
            relationships,
        })
    }

    /// 从 ER 图生成 DDL
    pub fn to_ddl(&self, diagram: &ErDiagram) -> String {
        let mut ddl = String::new();

        for entity in &diagram.entities {
            ddl.push_str(&format!("CREATE TABLE {} (\n", entity.name));
            for (i, field) in entity.fields.iter().enumerate() {
                if i > 0 {
                    ddl.push_str(",\n");
                }
                ddl.push_str(&format!("    {} {}", field.name, field.data_type));
                if field.is_primary_key {
                    ddl.push_str(" PRIMARY KEY");
                }
                if field.is_foreign_key {
                    if let Some(ref_table) = &field.references {
                        ddl.push_str(&format!(" REFERENCES {}(id)", ref_table));
                    }
                }
            }
            ddl.push_str("\n);\n\n");
        }

        ddl.trim_end().to_string()
    }

    /// 自然语言 → ER 图 → DDL 一站式
    pub fn nl_to_ddl(&self, description: &str) -> Result<String, MultimodalError> {
        let diagram = self.from_natural_language(description)?;
        Ok(self.to_ddl(&diagram))
    }

    /// 交互式添加实体
    pub fn add_entity(&self, diagram: &mut ErDiagram, entity: Entity) {
        diagram.entities.push(entity);
    }

    /// 交互式添加关系
    pub fn add_relationship(&self, diagram: &mut ErDiagram, relationship: Relationship) {
        diagram.relationships.push(relationship);
    }

    fn parse_entities(description: &str) -> Vec<Entity> {
        let mut entities = Vec::new();
        let lower = description.to_lowercase();

        let known_entities = [
            (
                ["user", "用户"],
                "users",
                vec![
                    ("id", "BIGINT", true),
                    ("name", "VARCHAR(255)", false),
                    ("email", "VARCHAR(255)", false),
                ],
            ),
            (
                ["order", "订单"],
                "orders",
                vec![
                    ("id", "BIGINT", true),
                    ("user_id", "BIGINT", false),
                    ("amount", "DECIMAL(10,2)", false),
                ],
            ),
            (
                ["product", "产品"],
                "products",
                vec![
                    ("id", "BIGINT", true),
                    ("name", "VARCHAR(255)", false),
                    ("price", "DECIMAL(10,2)", false),
                ],
            ),
        ];

        for (keywords, table_name, fields) in &known_entities {
            let matched = keywords.iter().any(|kw| lower.contains(kw));
            if matched {
                let entity = Entity {
                    name: table_name.to_string(),
                    fields: fields
                        .iter()
                        .map(|(name, dtype, pk)| Field {
                            name: name.to_string(),
                            data_type: dtype.to_string(),
                            is_primary_key: *pk,
                            is_foreign_key: name.ends_with("_id") && !*pk,
                            references: if name.ends_with("_id") && !*pk {
                                Some(name.trim_end_matches("_id").to_string() + "s")
                            } else {
                                None
                            },
                        })
                        .collect(),
                };
                entities.push(entity);
            }
        }

        if entities.is_empty() {
            entities.push(Entity {
                name: "data".to_string(),
                fields: vec![Field {
                    name: "id".to_string(),
                    data_type: "BIGINT".to_string(),
                    is_primary_key: true,
                    is_foreign_key: false,
                    references: None,
                }],
            });
        }

        entities
    }

    fn parse_relationships(description: &str, entities: &[Entity]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        let lower = description.to_lowercase();

        for entity in entities {
            for field in &entity.fields {
                if field.is_foreign_key {
                    if let Some(ref_table) = &field.references {
                        if entities.iter().any(|e| e.name == *ref_table) {
                            relationships.push(Relationship {
                                from_entity: entity.name.clone(),
                                to_entity: ref_table.clone(),
                                from_cardinality: Cardinality::Many,
                                to_cardinality: Cardinality::One,
                            });
                        }
                    }
                }
            }
        }

        if (lower.contains("many-to-many") || lower.contains("多对多")) && entities.len() >= 2 {
            relationships.push(Relationship {
                from_entity: entities[0].name.clone(),
                to_entity: entities[1].name.clone(),
                from_cardinality: Cardinality::Many,
                to_cardinality: Cardinality::Many,
            });
        }

        relationships
    }
}

impl Default for ErDiagramInteractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_nl_user_order() {
        let interactor = ErDiagramInteractor::new();
        let diagram = interactor.from_natural_language("用户和订单系统").unwrap();

        assert!(diagram.entities.len() >= 2);
        assert!(diagram.entities.iter().any(|e| e.name == "users"));
        assert!(diagram.entities.iter().any(|e| e.name == "orders"));
    }

    #[test]
    fn test_to_ddl() {
        let interactor = ErDiagramInteractor::new();
        let diagram = interactor.from_natural_language("用户系统").unwrap();
        let ddl = interactor.to_ddl(&diagram);

        assert!(ddl.contains("CREATE TABLE"));
        assert!(ddl.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_nl_to_ddl_pipeline() {
        let interactor = ErDiagramInteractor::new();
        let ddl = interactor.nl_to_ddl("用户和订单系统").unwrap();
        assert!(ddl.contains("CREATE TABLE users"));
        assert!(ddl.contains("CREATE TABLE orders"));
    }

    #[test]
    fn test_relationships_detected() {
        let interactor = ErDiagramInteractor::new();
        let diagram = interactor.from_natural_language("用户和订单系统").unwrap();
        assert!(!diagram.relationships.is_empty(), "应检测到外键关系");
    }

    #[test]
    fn test_empty_description_fails() {
        let interactor = ErDiagramInteractor::new();
        assert!(interactor.from_natural_language("").is_err());
    }

    #[test]
    fn test_add_entity_interactive() {
        let interactor = ErDiagramInteractor::new();
        let mut diagram = ErDiagram {
            entities: vec![],
            relationships: vec![],
        };
        interactor.add_entity(
            &mut diagram,
            Entity {
                name: "test".to_string(),
                fields: vec![],
            },
        );
        assert_eq!(diagram.entities.len(), 1);
    }
}
