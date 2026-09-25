use sz_orm_ai_migration::ai_migration_generator::{
    AiMigrationGenerator, DataImpactReport, LlmMigrationProvider, MigrationError, MigrationScript,
};
use async_trait::async_trait;

struct MockProvider;

#[async_trait]
impl LlmMigrationProvider for MockProvider {
    async fn generate(&self, _change_description: &str) -> Result<MigrationScript, MigrationError> {
        Ok(MigrationScript {
            up_sql: "CREATE TABLE t (id INT);".into(),
            down_sql: "DROP TABLE t;".into(),
            description: "test".into(),
        })
    }
}

#[tokio::test]
async fn test_generate_migration() {
    let gen = AiMigrationGenerator::new(Box::new(MockProvider));
    let result = gen.generate_migration("add table").await.unwrap();
    assert!(!result.script.up_sql.is_empty());
    assert!(result.rollback_verified);
}

#[test]
fn test_analyze_data_impact_safe() {
    let gen = AiMigrationGenerator::new(Box::new(MockProvider));
    let report = gen.analyze_data_impact("add column name varchar");
    assert!(!report.data_loss_risk);
    assert!(report.suggested_actions.contains(&"可直接执行".to_string()));
}

#[test]
fn test_analyze_data_impact_drop() {
    let gen = AiMigrationGenerator::new(Box::new(MockProvider));
    let report = gen.analyze_data_impact("drop table users");
    assert!(report.data_loss_risk);
    assert!(report.suggested_actions.contains(&"备份数据后再执行".to_string()));
}

#[test]
fn test_analyze_data_impact_not_null() {
    let gen = AiMigrationGenerator::new(Box::new(MockProvider));
    let report = gen.analyze_data_impact("add column email not null");
    assert!(report.data_loss_risk);
    assert!(report.suggested_actions.contains(&"先处理现有空值数据".to_string()));
}

#[test]
fn test_migration_error_display() {
    let err = MigrationError::Llm("timeout".into());
    assert_eq!(err.to_string(), "LLM error: timeout");
    let err = MigrationError::RollbackFailed("mismatch".into());
    assert_eq!(err.to_string(), "Rollback verification failed: mismatch");
    let err = MigrationError::HighRisk("drop".into());
    assert_eq!(err.to_string(), "High risk migration requires confirmation: drop");
}