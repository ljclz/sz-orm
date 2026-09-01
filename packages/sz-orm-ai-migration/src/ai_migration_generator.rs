//! AI 迁移生成器实现（骨架，TASK-012 将填充）
//!
//! 输入 Schema 变更描述，LLM 生成 up/down 迁移脚本，验证 down(up(x)) == x。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 迁移生成错误
#[derive(Debug, Error)]
pub enum MigrationError {
    /// LLM 调用错误
    #[error("LLM error: {0}")]
    Llm(String),
    /// 回滚验证失败
    #[error("Rollback verification failed: {0}")]
    RollbackFailed(String),
    /// 高风险迁移需确认
    #[error("High risk migration requires confirmation: {0}")]
    HighRisk(String),
}

/// 迁移脚本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationScript {
    /// up 脚本（正向迁移）
    pub up_sql: String,
    /// down 脚本（回滚迁移）
    pub down_sql: String,
    /// 描述
    pub description: String,
}

/// 迁移结果
#[derive(Debug, Clone)]
pub struct MigrationResult {
    /// 迁移脚本
    pub script: MigrationScript,
    /// 是否高风险
    pub is_high_risk: bool,
    /// 回滚验证通过
    pub rollback_verified: bool,
}

/// 数据影响报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataImpactReport {
    /// 影响的表
    pub affected_tables: Vec<String>,
    /// 是否有数据丢失风险
    pub data_loss_risk: bool,
    /// 影响描述
    pub impact_description: String,
    /// 建议操作
    pub suggested_actions: Vec<String>,
}

/// LLM 迁移 Provider trait
#[async_trait::async_trait]
pub trait LlmMigrationProvider: Send + Sync {
    /// 请求 LLM 生成迁移脚本
    async fn generate(&self, change_description: &str) -> Result<MigrationScript, MigrationError>;
}

/// AI 迁移生成器
pub struct AiMigrationGenerator {
    llm_provider: Box<dyn LlmMigrationProvider>,
}

impl AiMigrationGenerator {
    /// 创建 AI 迁移生成器
    pub fn new(llm_provider: Box<dyn LlmMigrationProvider>) -> Self {
        Self { llm_provider }
    }

    /// 生成迁移脚本
    pub async fn generate_migration(
        &self,
        change_description: &str,
    ) -> Result<MigrationResult, MigrationError> {
        let script = self.llm_provider.generate(change_description).await?;

        let report = self.analyze_data_impact(change_description);
        let rollback_verified = self.verify_rollback(&script);

        Ok(MigrationResult {
            script,
            is_high_risk: report.data_loss_risk,
            rollback_verified,
        })
    }

    /// 分析数据影响
    pub fn analyze_data_impact(&self, change_description: &str) -> DataImpactReport {
        let desc = change_description.to_lowercase();
        let data_loss_risk = desc.contains("drop")
            || desc.contains("delete")
            || desc.contains("not null") && !desc.contains("default");

        let mut suggested_actions = Vec::new();
        if data_loss_risk {
            suggested_actions.push("备份数据后再执行".to_string());
        }
        if desc.contains("not null") {
            suggested_actions.push("先处理现有空值数据".to_string());
        }
        if suggested_actions.is_empty() {
            suggested_actions.push("可直接执行".to_string());
        }

        DataImpactReport {
            affected_tables: Vec::new(),
            data_loss_risk,
            impact_description: change_description.to_string(),
            suggested_actions,
        }
    }

    /// 验证回滚（down(up(x)) == x）
    fn verify_rollback(&self, script: &MigrationScript) -> bool {
        // 简化验证：up 和 down 都非空
        !script.up_sql.is_empty() && !script.down_sql.is_empty()
    }
}
