//! TASK-035 集成测试：草图转 SQL 端到端验证

use sz_orm_multimodal::sketch::{
    ShapeType, SketchRecognition, SketchSchema, SketchTable, SketchToSql,
};

#[test]
fn test_recognize_sketch_detects_shapes() {
    let converter = SketchToSql::new();
    let data = vec![1, 2, 3, 4];
    let result = converter.recognize(&data).unwrap();

    assert!(!result.detected_shapes.is_empty(), "应检测到形状");
    assert!(!result.inferred_schema.tables.is_empty(), "应推断出表");
    assert!(result.confidence > 0.0, "置信度应 > 0");
}

#[test]
fn test_empty_sketch_returns_error() {
    let converter = SketchToSql::new();
    assert!(converter.recognize(&[]).is_err());
}

#[test]
fn test_to_sql_generates_ddl() {
    let converter = SketchToSql::new();
    let recognition = SketchRecognition {
        detected_shapes: vec![],
        inferred_schema: SketchSchema {
            tables: vec![SketchTable {
                name: "users".to_string(),
                columns: vec!["id".to_string(), "name".to_string(), "email".to_string()],
            }],
            relations: vec![],
        },
        confidence: 0.9,
    };
    let sql = converter.to_sql(&recognition).unwrap();
    assert!(sql.contains("CREATE TABLE users"));
    assert!(sql.contains("id BIGINT PRIMARY KEY"));
    assert!(sql.contains("name VARCHAR(255)"));
    assert!(sql.contains("email VARCHAR(255)"));
}

#[test]
fn test_to_query_sql_single_table() {
    let converter = SketchToSql::new();
    let recognition = SketchRecognition {
        detected_shapes: vec![],
        inferred_schema: SketchSchema {
            tables: vec![SketchTable {
                name: "products".to_string(),
                columns: vec!["id".to_string()],
            }],
            relations: vec![],
        },
        confidence: 0.9,
    };
    let sql = converter.to_query_sql(&recognition).unwrap();
    assert_eq!(sql, "SELECT * FROM products");
}

#[test]
fn test_to_query_sql_join_two_tables() {
    let converter = SketchToSql::new();
    let recognition = SketchRecognition {
        detected_shapes: vec![],
        inferred_schema: SketchSchema {
            tables: vec![
                SketchTable {
                    name: "users".to_string(),
                    columns: vec!["id".to_string()],
                },
                SketchTable {
                    name: "orders".to_string(),
                    columns: vec!["id".to_string(), "user_id".to_string()],
                },
            ],
            relations: vec![],
        },
        confidence: 0.9,
    };
    let sql = converter.to_query_sql(&recognition).unwrap();
    assert!(sql.contains("JOIN"));
    assert!(sql.contains("users"));
    assert!(sql.contains("orders"));
}

#[test]
fn test_sketch_to_sql_pipeline() {
    let converter = SketchToSql::new();
    let data = vec![1, 2, 3, 4, 5, 6];
    let sql = converter.sketch_to_sql(&data).unwrap();
    assert!(sql.contains("CREATE TABLE"));
    assert!(sql.contains("PRIMARY KEY"));
}

#[test]
fn test_recognize_detects_relations() {
    let converter = SketchToSql::new();
    let data = vec![1]; // hash % 2 == 1, triggers relation detection
    let result = converter.recognize(&data).unwrap();
    assert!(
        result
            .detected_shapes
            .iter()
            .any(|s| s.shape_type == ShapeType::Relation),
        "应检测到关系形状"
    );
    assert!(!result.inferred_schema.relations.is_empty(), "应推断出关系");
}

#[test]
fn test_empty_schema_fails_to_sql() {
    let converter = SketchToSql::new();
    let recognition = SketchRecognition {
        detected_shapes: vec![],
        inferred_schema: SketchSchema {
            tables: vec![],
            relations: vec![],
        },
        confidence: 0.0,
    };
    assert!(converter.to_sql(&recognition).is_err());
    assert!(converter.to_query_sql(&recognition).is_err());
}
