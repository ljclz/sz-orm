//! TASK-027: DocGenerator 单元测试

use sz_orm_cli::{DocGenerator, EntityDefinition, EntityField, EntityRelation, RelationType};

fn make_entities() -> Vec<EntityDefinition> {
    vec![
        EntityDefinition::new("User", "users")
            .with_field(EntityField::new("id", "i64", "bigint").primary_key())
            .with_field(EntityField::new("name", "String", "varchar"))
            .with_relation(EntityRelation {
                relation_type: RelationType::HasMany,
                target_entity: "Order".to_string(),
                local_field: "id".to_string(),
                target_field: "user_id".to_string(),
            }),
        EntityDefinition::new("Order", "orders")
            .with_field(EntityField::new("id", "i64", "bigint").primary_key())
            .with_relation(EntityRelation {
                relation_type: RelationType::BelongsTo,
                target_entity: "User".to_string(),
                local_field: "user_id".to_string(),
                target_field: "id".to_string(),
            }),
    ]
}

#[test]
fn test_generate_markdown() {
    let generator = DocGenerator::new();
    let doc = generator.generate_doc(&make_entities());

    assert!(doc.markdown.contains("# Database Schema Documentation"));
    assert!(doc.markdown.contains("## User"));
    assert!(doc.markdown.contains("## Order"));
}

#[test]
fn test_markdown_fields_table() {
    let generator = DocGenerator::new();
    let doc = generator.generate_doc(&make_entities());

    assert!(doc
        .markdown
        .contains("| Name | Type | PK | Nullable | DB Type |"));
    assert!(doc.markdown.contains("| `id` | `i64` | ✓ |  | `bigint` |"));
}

#[test]
fn test_markdown_relations() {
    let generator = DocGenerator::new();
    let doc = generator.generate_doc(&make_entities());

    assert!(doc.markdown.contains("### Relations"));
    assert!(doc.markdown.contains("has many"));
    assert!(doc.markdown.contains("belongs to"));
}

#[test]
fn test_generate_er_diagram() {
    let generator = DocGenerator::new();
    let doc = generator.generate_doc(&make_entities());

    assert!(doc.plantuml.contains("@startuml"));
    assert!(doc.plantuml.contains("@enduml"));
    assert!(doc.plantuml.contains("entity User"));
    assert!(doc.plantuml.contains("entity Order"));
}

#[test]
fn test_er_diagram_primary_key() {
    let generator = DocGenerator::new();
    let doc = generator.generate_doc(&make_entities());

    assert!(doc.plantuml.contains("* id : i64"));
}

#[test]
fn test_empty_entities() {
    let generator = DocGenerator::new();
    let doc = generator.generate_doc(&[]);

    assert!(doc.markdown.contains("Generated entities: 0"));
    assert!(doc.plantuml.contains("@startuml"));
}
