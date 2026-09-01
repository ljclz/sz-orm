//! TASK-026: EntityGenerator 单元测试

use sz_orm_cli::{EntityDefinition, EntityField, EntityGenerator, EntityRelation, RelationType};

#[test]
fn test_generate_basic_entity() {
    let generator = EntityGenerator::new();
    let entity = EntityDefinition::new("User", "users")
        .with_field(EntityField::new("id", "i64", "bigint").primary_key())
        .with_field(EntityField::new("name", "String", "varchar"))
        .with_field(EntityField::new("email", "String", "varchar").nullable());

    let code = generator.generate(&entity);

    assert!(code.contains("pub struct User"));
    assert!(code.contains("pub id: i64"));
    assert!(code.contains("pub name: String"));
    assert!(code.contains("pub email: Option<String>"));
}

#[test]
fn test_generate_with_relations() {
    let generator = EntityGenerator::new();
    let entity = EntityDefinition::new("User", "users")
        .with_field(EntityField::new("id", "i64", "bigint").primary_key())
        .with_relation(EntityRelation {
            relation_type: RelationType::HasMany,
            target_entity: "Order".to_string(),
            local_field: "id".to_string(),
            target_field: "user_id".to_string(),
        });

    let code = generator.generate(&entity);
    assert!(code.contains("impl User"));
    assert!(code.contains("Vec<Order>"));
}

#[test]
fn test_infer_rust_type() {
    assert_eq!(EntityGenerator::infer_rust_type("int"), "i64");
    assert_eq!(EntityGenerator::infer_rust_type("varchar"), "String");
    assert_eq!(EntityGenerator::infer_rust_type("boolean"), "bool");
    assert_eq!(EntityGenerator::infer_rust_type("float"), "f64");
    assert_eq!(
        EntityGenerator::infer_rust_type("json"),
        "serde_json::Value"
    );
}

#[test]
fn test_generate_all() {
    let generator = EntityGenerator::new();
    let entities = vec![
        EntityDefinition::new("User", "users")
            .with_field(EntityField::new("id", "i64", "bigint").primary_key()),
        EntityDefinition::new("Order", "orders")
            .with_field(EntityField::new("id", "i64", "bigint").primary_key()),
    ];
    let code = generator.generate_all(&entities);
    assert!(code.contains("pub struct User"));
    assert!(code.contains("pub struct Order"));
}
