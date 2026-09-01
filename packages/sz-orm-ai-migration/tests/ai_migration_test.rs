//! TASK-012: AI 迁移生成测试

use async_trait::async_trait;
use sz_orm_ai_migration::{
    AiMigrationGenerator, LlmMigrationProvider, MigrationError, MigrationScript,
};

struct MockLlmMigrationProvider {
    script: MigrationScript,
}

#[async_trait]
impl LlmMigrationProvider for MockLlmMigrationProvider {
    async fn generate(&self, _change_description: &str) -> Result<MigrationScript, MigrationError> {
        Ok(self.script.clone())
    }
}

#[tokio::test]
async fn test_generate_migration_unique_index() {
    let llm = MockLlmMigrationProvider {
        script: MigrationScript {
            up_sql: "CREATE UNIQUE INDEX idx_users_email ON users(email)".to_string(),
            down_sql: "DROP INDEX idx_users_email".to_string(),
            description: "给 users 表增加 email 唯一索引".to_string(),
        },
    };

    let generator = AiMigrationGenerator::new(Box::new(llm));
    let result = generator
        .generate_migration("给 users 表增加 email 唯一索引")
        .await
        .unwrap();

    assert!(result.script.up_sql.contains("CREATE UNIQUE INDEX"));
    assert!(result.script.down_sql.contains("DROP INDEX"));
    assert!(result.rollback_verified);
    assert!(!result.is_high_risk);
}

#[tokio::test]
async fn test_generate_migration_high_risk_drop() {
    let llm = MockLlmMigrationProvider {
        script: MigrationScript {
            up_sql: "DROP TABLE users".to_string(),
            down_sql: "".to_string(),
            description: "删除 users 表".to_string(),
        },
    };

    let generator = AiMigrationGenerator::new(Box::new(llm));
    let result = generator
        .generate_migration("drop users table")
        .await
        .unwrap();

    assert!(result.is_high_risk);
    assert!(!result.rollback_verified);
}

#[tokio::test]
async fn test_analyze_data_impact_not_null() {
    let llm = MockLlmMigrationProvider {
        script: MigrationScript {
            up_sql: "ALTER TABLE users ADD COLUMN email VARCHAR(200) NOT NULL".to_string(),
            down_sql: "ALTER TABLE users DROP COLUMN email".to_string(),
            description: "add not null column".to_string(),
        },
    };

    let generator = AiMigrationGenerator::new(Box::new(llm));
    let report = generator.analyze_data_impact("add NOT NULL column email");

    assert!(report.data_loss_risk);
    assert!(report.suggested_actions.iter().any(|a| a.contains("空值")));
}

#[tokio::test]
async fn test_analyze_data_impact_safe() {
    let llm = MockLlmMigrationProvider {
        script: MigrationScript {
            up_sql: "CREATE INDEX idx ON t(a)".to_string(),
            down_sql: "DROP INDEX idx".to_string(),
            description: "safe".to_string(),
        },
    };

    let generator = AiMigrationGenerator::new(Box::new(llm));
    let report = generator.analyze_data_impact("add index");

    assert!(!report.data_loss_risk);
}
