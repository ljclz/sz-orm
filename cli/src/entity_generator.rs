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

    /// 幂等生成：字段按字母序排列，格式固定
    ///
    /// 连续两次 generate_idempotent → 输出逐字节相同。
    pub fn generate_idempotent(&self, entity: &EntityDefinition) -> String {
        let mut sorted_entity = entity.clone();
        sorted_entity.fields.sort_by(|a, b| a.name.cmp(&b.name));
        sorted_entity.relations.sort_by(|a, b| {
            a.target_entity
                .cmp(&b.target_entity)
                .then_with(|| a.local_field.cmp(&b.local_field))
        });
        let mut code = self.generate(&sorted_entity);
        code = format!(
            "// @generated by sz-orm-cli\n// Do not edit manually\n\n{}",
            code
        );
        code
    }

    /// 幂等批量生成
    pub fn generate_all_idempotent(&self, entities: &[EntityDefinition]) -> String {
        let mut sorted_entities: Vec<EntityDefinition> = entities.to_vec();
        sorted_entities.sort_by(|a, b| a.name.cmp(&b.name));
        let mut code = String::new();
        for entity in &sorted_entities {
            code.push_str(&self.generate_idempotent(entity));
            code.push_str("\n\n");
        }
        code
    }

    /// 从数据库 schema 元数据生成全部实体
    ///
    /// `tables` 为数据库表 schema 元数据列表，由调用方从真实 DB 读取。
    pub fn generate_from_db(&self, tables: &[DbTableSchema]) -> String {
        let entities: Vec<EntityDefinition> = tables
            .iter()
            .map(|table| {
                let entity_name = to_pascal_case(&table.name) + "Model";
                let mut entity = EntityDefinition::new(entity_name, table.name.clone());
                let mut fields: Vec<EntityField> = table
                    .columns
                    .iter()
                    .map(|col| {
                        let mut field = EntityField::new(
                            col.name.clone(),
                            Self::infer_rust_type(&col.db_type),
                            col.db_type.clone(),
                        );
                        if col.is_nullable {
                            field = field.nullable();
                        }
                        if col.is_primary_key {
                            field = field.primary_key();
                        }
                        field
                    })
                    .collect();
                fields.sort_by(|a, b| a.name.cmp(&b.name));
                for field in fields {
                    entity = entity.with_field(field);
                }
                for fk in &table.foreign_keys {
                    entity = entity.with_relation(EntityRelation {
                        relation_type: RelationType::BelongsTo,
                        target_entity: to_pascal_case(&fk.target_table) + "Model",
                        local_field: fk.local_column.clone(),
                        target_field: fk.target_column.clone(),
                    });
                }
                entity
            })
            .collect();
        self.generate_all_idempotent(&entities)
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

/// 数据库表 schema 元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTableSchema {
    pub name: String,
    pub columns: Vec<DbColumnSchema>,
    pub foreign_keys: Vec<DbForeignKey>,
}

/// 数据库列 schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbColumnSchema {
    pub name: String,
    pub db_type: String,
    pub is_primary_key: bool,
    pub is_nullable: bool,
}

/// 外键关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbForeignKey {
    pub local_column: String,
    pub target_table: String,
    pub target_column: String,
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
