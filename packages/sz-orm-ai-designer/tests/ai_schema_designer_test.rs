use sz_orm_ai_designer::ai_schema_designer::{
    AiSchemaDesigner, ColumnDefinition, DesignError, JoinPattern, LlmSchemaProvider,
    MigrationRisk, SchemaDesign, TableDefinition,
};
use async_trait::async_trait;

struct MockProvider;

#[async_trait]
impl LlmSchemaProvider for MockProvider {
    async fn design(&self, _requirement: &str) -> Result<SchemaDesign, DesignError> {
        Ok(SchemaDesign {
            tables: vec![TableDefinition {
                name: "users".into(),
                columns: vec![ColumnDefinition {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    is_primary_key: true,
                    is_unique: false,
                    foreign_key: None,
                    default_value: None,
                    comment: None,
                }],
                indexes: vec![("idx_email".into(), vec!["email".into()])],
                comment: None,
            }],
            ddl_texts: Vec::new(),
            rationale: "test".into(),
        })
    }
}

#[tokio::test]
async fn test_design_schema() {
    let designer = AiSchemaDesigner::new(Box::new(MockProvider));
    let result = designer.design_schema("user system").await.unwrap();
    assert!(!result.design.tables.is_empty());
    assert!(!result.design.ddl_texts.is_empty());
    assert_eq!(result.retries, 0);
}

#[test]
fn test_analyze_migration_impact_no_change() {
    let designer = AiSchemaDesigner::new(Box::new(MockProvider));
    let schema = SchemaDesign { tables: vec![], ddl_texts: vec![], rationale: "".into() };
    let report = designer.analyze_migration_impact(&schema, &schema);
    assert_eq!(report.risk_level, MigrationRisk::Low);
    assert!(report.migration_steps.contains(&"无变更".to_string()));
}

#[test]
fn test_analyze_migration_impact_add_table() {
    let designer = AiSchemaDesigner::new(Box::new(MockProvider));
    let old = SchemaDesign { tables: vec![], ddl_texts: vec![], rationale: "".into() };
    let new = SchemaDesign {
        tables: vec![TableDefinition {
            name: "orders".into(), columns: vec![], indexes: vec![], comment: None,
        }],
        ddl_texts: vec![], rationale: "".into(),
    };
    let report = designer.analyze_migration_impact(&old, &new);
    assert_eq!(report.affected_queries, 1);
    assert_eq!(report.risk_level, MigrationRisk::Low);
}

#[test]
fn test_analyze_migration_impact_remove_table() {
    let designer = AiSchemaDesigner::new(Box::new(MockProvider));
    let old = SchemaDesign {
        tables: vec![TableDefinition {
            name: "old".into(), columns: vec![], indexes: vec![], comment: None,
        }],
        ddl_texts: vec![], rationale: "".into(),
    };
    let new = SchemaDesign { tables: vec![], ddl_texts: vec![], rationale: "".into() };
    let report = designer.analyze_migration_impact(&old, &new);
    assert_eq!(report.affected_queries, 1);
    assert_eq!(report.risk_level, MigrationRisk::High);
}

#[test]
fn test_denormalization_advice_empty() {
    let designer = AiSchemaDesigner::new(Box::new(MockProvider));
    let advice = designer.denormalization_advice(&[]);
    assert_eq!(advice.joins_reduced, 0);
    assert!(advice.reason.contains("不建议"));
}

#[test]
fn test_denormalization_advice_with_joins() {
    let designer = AiSchemaDesigner::new(Box::new(MockProvider));
    let joins = vec![JoinPattern {
        from_table: "orders".into(),
        to_table: "users".into(),
        foreign_key: "user_id".into(),
        frequently_accessed_columns: vec!["name".into(), "email".into()],
        frequency: 1000,
    }];
    let advice = designer.denormalization_advice(&joins);
    assert_eq!(advice.joins_reduced, 1);
    assert_eq!(advice.redundant_columns.len(), 2);
}

#[test]
fn test_design_error_display() {
    assert_eq!(DesignError::Llm("x".into()).to_string(), "LLM error: x");
    assert_eq!(DesignError::Syntax("x".into()).to_string(), "DDL syntax error: x");
    assert_eq!(DesignError::MaxRetries(3).to_string(), "Max retries (3) exhausted");
}