//! TASK-029 集成测试：ER 图交互式建模端到端验证

use sz_orm_multimodal::er_diagram::{
    Cardinality, Entity, ErDiagram, ErDiagramInteractor, Field, Relationship,
};

#[test]
fn test_nl_to_er_diagram() {
    let interactor = ErDiagramInteractor::new();
    let diagram = interactor.from_natural_language("用户和订单系统").unwrap();

    assert!(diagram.entities.len() >= 2);
    assert!(diagram.entities.iter().any(|e| e.name == "users"));
    assert!(diagram.entities.iter().any(|e| e.name == "orders"));
}

#[test]
fn test_er_diagram_to_ddl() {
    let interactor = ErDiagramInteractor::new();
    let ddl = interactor.nl_to_ddl("用户和订单系统").unwrap();

    assert!(ddl.contains("CREATE TABLE users"));
    assert!(ddl.contains("CREATE TABLE orders"));
    assert!(ddl.contains("PRIMARY KEY"));
    assert!(ddl.contains("REFERENCES"));
}

#[test]
fn test_relationships_with_foreign_keys() {
    let interactor = ErDiagramInteractor::new();
    let diagram = interactor.from_natural_language("用户和订单系统").unwrap();

    assert!(!diagram.relationships.is_empty());
    let rel = &diagram.relationships[0];
    assert_eq!(rel.from_cardinality, Cardinality::Many);
    assert_eq!(rel.to_cardinality, Cardinality::One);
}

#[test]
fn test_interactive_add_entity() {
    let interactor = ErDiagramInteractor::new();
    let mut diagram = ErDiagram {
        entities: vec![],
        relationships: vec![],
    };

    interactor.add_entity(
        &mut diagram,
        Entity {
            name: "test_table".to_string(),
            fields: vec![Field {
                name: "id".to_string(),
                data_type: "BIGINT".to_string(),
                is_primary_key: true,
                is_foreign_key: false,
                references: None,
            }],
        },
    );

    assert_eq!(diagram.entities.len(), 1);
    let ddl = interactor.to_ddl(&diagram);
    assert!(ddl.contains("CREATE TABLE test_table"));
}

#[test]
fn test_interactive_add_relationship() {
    let interactor = ErDiagramInteractor::new();
    let mut diagram = ErDiagram {
        entities: vec![
            Entity {
                name: "a".to_string(),
                fields: vec![],
            },
            Entity {
                name: "b".to_string(),
                fields: vec![],
            },
        ],
        relationships: vec![],
    };

    interactor.add_relationship(
        &mut diagram,
        Relationship {
            from_entity: "a".to_string(),
            to_entity: "b".to_string(),
            from_cardinality: Cardinality::Many,
            to_cardinality: Cardinality::One,
        },
    );

    assert_eq!(diagram.relationships.len(), 1);
}

#[test]
fn test_empty_description_returns_error() {
    let interactor = ErDiagramInteractor::new();
    assert!(interactor.from_natural_language("").is_err());
}

#[test]
fn test_unknown_entity_fallback() {
    let interactor = ErDiagramInteractor::new();
    let diagram = interactor.from_natural_language("某个未知系统").unwrap();
    assert!(!diagram.entities.is_empty(), "应有兜底实体");
}

#[test]
fn test_product_entity() {
    let interactor = ErDiagramInteractor::new();
    let diagram = interactor.from_natural_language("产品管理系统").unwrap();
    assert!(diagram.entities.iter().any(|e| e.name == "products"));
}
