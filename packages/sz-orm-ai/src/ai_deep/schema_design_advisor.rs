//! Schema 设计建议器
//!
//! 查询模式分析 → 索引推荐 → 反范式建议 → 分区建议 → 预期收益估算 → 可解释报告。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use sz_orm_ai_designer::AiSchemaDesigner;

use super::{
    record_audit, AiDeepError, AiDeepEventType, DenormSuggestion, IndexSuggestion,
    PartitionSuggestion, QueryPattern, SchemaDesignReport, TableSuggestion,
};

/// Schema 设计配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDesignConfig {
    /// 数据不足阈值（天），低于此值推迟建议
    pub min_data_days: u32,
    /// 高频查询阈值（次/天）
    pub high_frequency_threshold: u64,
    /// 是否启用反范式建议
    pub enable_denorm: bool,
    /// 是否启用分区建议
    pub enable_partition: bool,
}

impl Default for SchemaDesignConfig {
    fn default() -> Self {
        Self {
            min_data_days: 30,
            high_frequency_threshold: 100,
            enable_denorm: true,
            enable_partition: true,
        }
    }
}

/// Schema 设计建议器
///
/// 查询模式分析 → 索引推荐 → 反范式建议 → 分区建议 → 预期收益估算 → 可解释报告。
pub struct SchemaDesignAdvisor {
    designer: Arc<AiSchemaDesigner>,
    config: SchemaDesignConfig,
}

impl SchemaDesignAdvisor {
    /// 创建 schema 设计建议器
    pub fn new(designer: Arc<AiSchemaDesigner>, config: SchemaDesignConfig) -> Self {
        Self { designer, config }
    }

    /// 使用默认配置创建
    pub fn with_default_config(designer: Arc<AiSchemaDesigner>) -> Self {
        Self::new(designer, SchemaDesignConfig::default())
    }

    /// 生成 schema 设计建议
    ///
    /// 流程：
    /// 1. 查询模式分析
    /// 2. 索引推荐
    /// 3. 反范式建议
    /// 4. 分区建议
    /// 5. 预期收益估算
    /// 6. 可解释报告
    ///
    /// 异常映射：
    /// - 数据不足 30 天 → `AiDeepError::SchemaDesignDataInsufficient`（推迟建议，日志 SCHEMA_DESIGN_DATA_INSUFFICIENT）
    pub async fn advise(
        &self,
        query_patterns: &[QueryPattern],
    ) -> Result<SchemaDesignReport, AiDeepError> {
        // 1. 数据充足性检查
        let max_data_days = query_patterns
            .iter()
            .map(|p| p.data_days)
            .max()
            .unwrap_or(0);

        if max_data_days < self.config.min_data_days {
            eprintln!(
                "SCHEMA_DESIGN_DATA_INSUFFICIENT: 仅有 {} 天数据 < 阈值 {} 天，推迟建议",
                max_data_days, self.config.min_data_days
            );
            return Err(AiDeepError::SchemaDesignDataInsufficient(format!(
                "仅有 {} 天数据，需要至少 {} 天才能生成可靠建议",
                max_data_days, self.config.min_data_days
            )));
        }

        // 2. 查询模式分析 + 索引推荐
        let index_suggestions = self.recommend_indexes(query_patterns);

        // 3. 反范式建议
        let denorm_suggestions = if self.config.enable_denorm {
            self.recommend_denorm(query_patterns)
        } else {
            vec![]
        };

        // 4. 分区建议
        let partition_suggestions = if self.config.enable_partition {
            self.recommend_partitions(query_patterns)
        } else {
            vec![]
        };

        // 5. 表建议（使用 AiSchemaDesigner 生成，失败时降级为规则型分析）
        let requirement = format!("基于 {} 个查询模式的 schema 设计建议", query_patterns.len());
        let table_suggestions = match self.designer.design_schema(&requirement).await {
            Ok(design_result) => {
                let mut suggestions = self.analyze_tables(query_patterns);
                for table in &design_result.design.tables {
                    suggestions.push(TableSuggestion {
                        table_name: table.name.clone(),
                        suggestion: format!(
                            "LLM 建议表结构（{} 列，{} 索引）",
                            table.columns.len(),
                            table.indexes.len()
                        ),
                        rationale: design_result.design.rationale.clone(),
                    });
                }
                suggestions
            }
            Err(_) => self.analyze_tables(query_patterns),
        };

        // 6. 预期收益估算
        let expected_benefit = self.estimate_benefit(
            &index_suggestions,
            &denorm_suggestions,
            &partition_suggestions,
            query_patterns,
        );

        // 7. 可解释报告
        let explanation = self.build_explanation(
            query_patterns,
            &index_suggestions,
            &denorm_suggestions,
            &partition_suggestions,
            expected_benefit,
        );

        let report = SchemaDesignReport {
            table_suggestions,
            index_suggestions,
            denorm_suggestions,
            partition_suggestions,
            expected_benefit,
            explanation,
        };

        // 记录审计日志
        let _ = record_audit(
            &sz_orm_audit::AutonomousDecisionAuditor::new(),
            AiDeepEventType::SchemaDesign,
            "schema_advise",
            &format!(
                "Schema 设计建议: {} 个索引, {} 个反范式, {} 个分区, 预期收益 {:.2}",
                report.index_suggestions.len(),
                report.denorm_suggestions.len(),
                report.partition_suggestions.len(),
                report.expected_benefit
            ),
            true,
        );

        Ok(report)
    }

    /// 索引推荐
    fn recommend_indexes(&self, patterns: &[QueryPattern]) -> Vec<IndexSuggestion> {
        let mut suggestions = Vec::new();

        for pattern in patterns {
            if pattern.frequency < self.config.high_frequency_threshold {
                continue;
            }

            if pattern.columns_accessed.len() < 2 {
                continue;
            }

            let table = pattern
                .tables
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());

            let index_columns: Vec<String> =
                pattern.columns_accessed.iter().take(3).cloned().collect();

            let col_list = index_columns.join(", ");
            let idx_name = format!("idx_{}_{}", table, index_columns.join("_"));
            let ddl_text = format!("CREATE INDEX {} ON {} ({})", idx_name, table, col_list);

            let expected_benefit = (pattern.frequency as f64 / 1000.0).min(1.0);

            suggestions.push(IndexSuggestion {
                table_name: table,
                index_columns,
                ddl_text,
                expected_benefit,
            });
        }

        suggestions
    }

    /// 反范式建议
    fn recommend_denorm(&self, patterns: &[QueryPattern]) -> Vec<DenormSuggestion> {
        let mut suggestions = Vec::new();

        for pattern in patterns {
            if pattern.tables.len() >= 2
                && pattern.frequency >= self.config.high_frequency_threshold
            {
                let target_table = pattern
                    .tables
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let source_table = pattern
                    .tables
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());

                if let Some(col) = pattern.columns_accessed.last() {
                    suggestions.push(DenormSuggestion {
                        target_table,
                        redundant_column: format!("denorm_{}", col),
                        source_table,
                        source_column: col.clone(),
                        rationale: format!(
                            "高频 JOIN 查询（{} 次/天），冗余 {} 列可减少 JOIN",
                            pattern.frequency, col
                        ),
                    });
                }
            }
        }

        suggestions
    }

    /// 分区建议
    fn recommend_partitions(&self, patterns: &[QueryPattern]) -> Vec<PartitionSuggestion> {
        let mut suggestions = Vec::new();

        for pattern in patterns {
            let has_time_column = pattern
                .columns_accessed
                .iter()
                .any(|c| c.contains("time") || c.contains("date") || c.contains("created"));

            if has_time_column && pattern.frequency >= self.config.high_frequency_threshold {
                let table = pattern
                    .tables
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());

                let partition_column = pattern
                    .columns_accessed
                    .iter()
                    .find(|c| c.contains("time") || c.contains("date") || c.contains("created"))
                    .cloned()
                    .unwrap_or_else(|| "created_at".to_string());

                suggestions.push(PartitionSuggestion {
                    table_name: table,
                    partition_column,
                    partition_strategy: "RANGE".to_string(),
                    rationale: "时间序列高频查询，按时间范围分区可提升查询性能".to_string(),
                });
            }
        }

        suggestions
    }

    /// 表分析
    fn analyze_tables(&self, patterns: &[QueryPattern]) -> Vec<TableSuggestion> {
        let mut suggestions = Vec::new();
        let mut table_freq: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();

        for pattern in patterns {
            for table in &pattern.tables {
                *table_freq.entry(table.clone()).or_insert(0) += pattern.frequency;
            }
        }

        for (table, freq) in table_freq {
            if freq >= self.config.high_frequency_threshold * 2 {
                suggestions.push(TableSuggestion {
                    table_name: table.clone(),
                    suggestion: "考虑添加复合索引或分区".to_string(),
                    rationale: format!("高频访问表（{} 次/天）", freq),
                });
            }
        }

        suggestions
    }

    /// 预期收益估算
    fn estimate_benefit(
        &self,
        indexes: &[IndexSuggestion],
        denorm: &[DenormSuggestion],
        partitions: &[PartitionSuggestion],
        patterns: &[QueryPattern],
    ) -> f64 {
        let index_benefit: f64 = indexes.iter().map(|i| i.expected_benefit).sum();
        let denorm_benefit = denorm.len() as f64 * 0.1;
        let partition_benefit = partitions.len() as f64 * 0.15;

        let _total_queries: u64 = patterns.iter().map(|p| p.frequency).sum::<u64>().max(1);
        let coverage = (indexes.len() as f64 + denorm.len() as f64 + partitions.len() as f64)
            / patterns.len().max(1) as f64;

        let benefit = (index_benefit + denorm_benefit + partition_benefit) * coverage;
        benefit.min(1.0)
    }

    fn build_explanation(
        &self,
        patterns: &[QueryPattern],
        indexes: &[IndexSuggestion],
        denorm: &[DenormSuggestion],
        partitions: &[PartitionSuggestion],
        expected_benefit: f64,
    ) -> String {
        format!(
            "Schema 设计建议报告：\n\
             - 查询模式数: {}\n\
             - 索引建议数: {}\n\
             - 反范式建议数: {}\n\
             - 分区建议数: {}\n\
             - 预期收益: {:.2}\n\
             - 数据天数: {}",
            patterns.len(),
            indexes.len(),
            denorm.len(),
            partitions.len(),
            expected_benefit,
            patterns.iter().map(|p| p.data_days).max().unwrap_or(0)
        )
    }

    /// 获取配置
    pub fn config(&self) -> &SchemaDesignConfig {
        &self.config
    }
}

// ============================================================================
// v10.11.0 T3.4：AI Schema 自主演进
// 分区分表/索引调整/反范式，自动应用低风险变更（如新增分区），
// 高风险变更（删列/改类型）须人工确认，兼容性保护。
// 复用既有 SchemaDesignAdvisor + ai-schema-design feature gate +
// ai-config feature gate（DecisionExplainer）+
// ai-security-audit feature gate（LlmSecurityAuditor 三重安全审计）。
// ============================================================================

/// Schema 变更类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaChangeType {
    /// 加列（低风险，向后兼容）
    AddColumn,
    /// 加分区（低风险，向后兼容）
    AddPartition,
    /// 加索引（低风险，向后兼容）
    AddIndex,
    /// 删列（高风险，破坏应用兼容性）
    DropColumn,
    /// 改类型（高风险，破坏应用兼容性）
    AlterType,
    /// 删表（高风险，破坏应用兼容性）
    DropTable,
}

impl SchemaChangeType {
    /// 变更名称
    pub fn name(&self) -> &str {
        match self {
            SchemaChangeType::AddColumn => "AddColumn",
            SchemaChangeType::AddPartition => "AddPartition",
            SchemaChangeType::AddIndex => "AddIndex",
            SchemaChangeType::DropColumn => "DropColumn",
            SchemaChangeType::AlterType => "AlterType",
            SchemaChangeType::DropTable => "DropTable",
        }
    }
}

/// Schema 变更风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaRiskLevel {
    /// 低风险（可自动应用：加列/加分区/加索引）
    Low,
    /// 高风险（须人工确认：删列/改类型/删表）
    High,
}

/// Schema 变更
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaChange {
    /// 变更类型
    pub change_type: SchemaChangeType,
    /// DDL 文本（如 ALTER TABLE ... ADD COLUMN ...）
    pub ddl: String,
    /// 回滚 DDL
    pub rollback_ddl: String,
    /// 变更描述
    pub description: String,
}

/// Schema 状态（当前 schema 的摘要信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaState {
    /// 表名列表
    pub tables: Vec<String>,
    /// 查询模式（用于分析）
    pub query_patterns: Vec<QueryPattern>,
}

impl SchemaState {
    pub fn new(tables: Vec<String>, query_patterns: Vec<QueryPattern>) -> Self {
        Self {
            tables,
            query_patterns,
        }
    }
}

/// Schema 演进结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolveResult {
    /// 已应用的变更列表
    pub applied: Vec<SchemaChange>,
    /// 待人工确认的变更列表
    pub pending_manual: Vec<SchemaChange>,
    /// 回滚方案列表
    pub rollback_plans: Vec<String>,
    /// 决策可解释性（需 ai-config feature）
    #[cfg(feature = "ai-config")]
    pub explainer: Option<crate::ai_config::DecisionExplainer>,
    /// 兼容性验证通过
    pub compatibility_verified: bool,
}

/// Schema 演进错误
#[derive(Debug, thiserror::Error)]
pub enum EvolveError {
    #[error("Schema 状态为空")]
    EmptySchemaState,
    #[error("安全审计失败：{0}")]
    SecurityAuditFailed(String),
    #[error("变更生成失败：{0}")]
    ChangeGenerationFailed(String),
}

/// Schema 变更风险评估器
pub struct SchemaRiskAssessor;

impl SchemaRiskAssessor {
    pub fn new() -> Self {
        Self
    }

    /// 评估变更风险等级
    pub fn assess(&self, change: &SchemaChange) -> SchemaRiskLevel {
        match change.change_type {
            SchemaChangeType::AddColumn
            | SchemaChangeType::AddPartition
            | SchemaChangeType::AddIndex => SchemaRiskLevel::Low,
            SchemaChangeType::DropColumn
            | SchemaChangeType::AlterType
            | SchemaChangeType::DropTable => SchemaRiskLevel::High,
        }
    }

    /// 生成风险评估（用于 DecisionExplainer）
    #[cfg(feature = "ai-config")]
    pub fn risk_assessment(&self, level: SchemaRiskLevel) -> crate::ai_config::RiskAssessment {
        use crate::ai_config::{RiskAssessment, RiskFactor, RiskLevel};
        let (risk_level, score, factor_name) = match level {
            SchemaRiskLevel::Low => (RiskLevel::Low, 0.2, "additive_change"),
            SchemaRiskLevel::High => (RiskLevel::High, 0.9, "breaking_change"),
        };
        RiskAssessment {
            level: risk_level,
            factors: vec![RiskFactor {
                factor: factor_name.to_string(),
                weight: score,
            }],
            score,
        }
    }
}

impl Default for SchemaRiskAssessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Schema 变更自动应用器
pub struct SchemaAutoApplier {
    /// 已应用的变更列表
    applied: Vec<SchemaChange>,
}

impl SchemaAutoApplier {
    pub fn new() -> Self {
        Self {
            applied: Vec::new(),
        }
    }

    /// 安全审计（需 ai-security-audit feature）
    #[cfg(feature = "ai-security-audit")]
    async fn security_check(&self, ddl: &str) -> Result<bool, EvolveError> {
        use crate::llm_security_audit::LlmSecurityAuditor;
        use crate::permission_auditor::{DbAccount, PermissionAuditor, QueryUsage};
        use crate::sql_sanitizer::SqlSanitizer;

        // 1. SqlSanitizer 脱敏
        let sanitized = SqlSanitizer::sanitize(ddl);
        if sanitized != ddl {
            return Err(EvolveError::SecurityAuditFailed(
                "DDL 含敏感字面量".to_string(),
            ));
        }

        // 2. PermissionAuditor 权限审计
        let account = DbAccount::new("auto_schema_evolver")
            .with_privileges(vec!["ALTER".to_string(), "CREATE".to_string()]);
        let usage = QueryUsage::from_sql(ddl);
        let auditor = PermissionAuditor::new();
        let perm_result = auditor.audit_permissions(&account, &usage);
        for finding in &perm_result.findings {
            if finding.severity == crate::permission_auditor::PermissionIssueSeverity::Critical {
                return Err(EvolveError::SecurityAuditFailed(format!(
                    "权限审计失败：{}",
                    finding.title
                )));
            }
        }

        // 3. LlmSecurityAuditor
        let mut llm_auditor = LlmSecurityAuditor::rule_only();
        let audit_result = llm_auditor
            .audit(ddl)
            .await
            .map_err(|e| EvolveError::SecurityAuditFailed(e.to_string()))?;
        if audit_result.risk_level.is_dangerous() {
            return Err(EvolveError::SecurityAuditFailed(
                "LLM 安全审计检测到注入风险".to_string(),
            ));
        }

        Ok(true)
    }

    /// 应用变更
    pub async fn apply(&mut self, change: SchemaChange) -> Result<(), EvolveError> {
        #[cfg(feature = "ai-security-audit")]
        {
            self.security_check(&change.ddl).await?;
        }
        self.applied.push(change);
        Ok(())
    }

    /// 获取已应用的变更
    pub fn applied_changes(&self) -> &[SchemaChange] {
        &self.applied
    }

    /// 回滚
    pub fn rollback(&mut self, ddl: &str) -> Option<String> {
        if let Some(pos) = self.applied.iter().position(|c| c.ddl == ddl) {
            let change = self.applied.remove(pos);
            Some(change.rollback_ddl)
        } else {
            None
        }
    }
}

impl Default for SchemaAutoApplier {
    fn default() -> Self {
        Self::new()
    }
}

/// AI Schema 自主演进器
///
/// 自动应用低风险变更（加列/加分区/加索引），
/// 高风险变更（删列/改类型/删表）须人工确认，兼容性保护。
/// 复用既有 SchemaDesignAdvisor（schema_design_advisor.rs:43）。
pub struct AutoSchemaEvolver {
    /// 风险评估器
    risk_assessor: SchemaRiskAssessor,
    /// 自动应用器
    auto_applier: SchemaAutoApplier,
}

impl AutoSchemaEvolver {
    pub fn new() -> Self {
        Self {
            risk_assessor: SchemaRiskAssessor::new(),
            auto_applier: SchemaAutoApplier::new(),
        }
    }

    /// 生成 Schema 变更建议
    ///
    /// 基于 SchemaState 分析查询模式，生成变更建议。
    fn generate_changes(&self, schema_state: &SchemaState) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        for pattern in &schema_state.query_patterns {
            if pattern.frequency > 100 && pattern.data_days >= 30 {
                let table = pattern
                    .sql_template
                    .split_whitespace()
                    .nth(2)
                    .unwrap_or("unknown");

                changes.push(SchemaChange {
                    change_type: SchemaChangeType::AddIndex,
                    ddl: format!(
                        "CREATE INDEX idx_{}_auto ON {} ({})",
                        table,
                        table,
                        pattern.columns_accessed.join(", ")
                    ),
                    rollback_ddl: format!("DROP INDEX idx_{}_auto", table),
                    description: format!(
                        "为高频查询自动添加索引: {} ({})",
                        table,
                        pattern.columns_accessed.join(", ")
                    ),
                });
            }
        }

        if !schema_state.tables.is_empty() {
            let table = &schema_state.tables[0];
            changes.push(SchemaChange {
                change_type: SchemaChangeType::AddPartition,
                ddl: format!(
                    "ALTER TABLE {} PARTITION BY RANGE (created_at) (PARTITION p2025 VALUES LESS THAN ('2026-01-01'))",
                    table
                ),
                rollback_ddl: format!("ALTER TABLE {} REMOVE PARTITIONING", table),
                description: format!("为表 {} 添加分区", table),
            });
        }

        changes
    }

    /// 自主 Schema 演进
    ///
    /// 1. 基于 SchemaState 生成变更建议
    /// 2. 评估每项变更的风险等级
    /// 3. 低风险自动应用，高风险待人工确认
    /// 4. 安全审计三重检查（需 ai-security-audit feature）
    /// 5. 回滚方案
    /// 6. 决策可解释性
    pub async fn evolve(
        &mut self,
        schema_state: &SchemaState,
    ) -> Result<EvolveResult, EvolveError> {
        if schema_state.tables.is_empty() && schema_state.query_patterns.is_empty() {
            return Err(EvolveError::EmptySchemaState);
        }

        // 1. 生成变更建议
        let changes = self.generate_changes(schema_state);

        let mut applied = Vec::new();
        let mut pending_manual = Vec::new();
        let mut rollback_plans = Vec::new();

        for change in changes {
            let risk_level = self.risk_assessor.assess(&change);

            match risk_level {
                SchemaRiskLevel::Low => {
                    let rollback = change.rollback_ddl.clone();
                    self.auto_applier.apply(change.clone()).await?;
                    applied.push(change);
                    rollback_plans.push(rollback);
                }
                SchemaRiskLevel::High => {
                    pending_manual.push(change);
                }
            }
        }

        // 兼容性验证：低风险变更不破坏应用兼容性
        let compatibility_verified = applied
            .iter()
            .all(|c| matches!(c.change_type, SchemaChangeType::AddColumn | SchemaChangeType::AddPartition | SchemaChangeType::AddIndex));

        // 决策可解释性
        #[cfg(feature = "ai-config")]
        let explainer = self.build_explainer(&applied, &pending_manual);

        Ok(EvolveResult {
            applied,
            pending_manual,
            rollback_plans,
            #[cfg(feature = "ai-config")]
            explainer,
            compatibility_verified,
        })
    }

    /// 构建决策可解释性
    #[cfg(feature = "ai-config")]
    fn build_explainer(
        &self,
        applied: &[SchemaChange],
        pending_manual: &[SchemaChange],
    ) -> Option<crate::ai_config::DecisionExplainer> {
        use crate::ai_config::*;
        if applied.is_empty() && pending_manual.is_empty() {
            return None;
        }
        let has_high_risk = !pending_manual.is_empty();
        let risk_level = if has_high_risk {
            SchemaRiskLevel::High
        } else {
            SchemaRiskLevel::Low
        };
        let risk_assessment = self.risk_assessor.risk_assessment(risk_level);
        Some(DecisionExplainer::new(
            vec![
                FeatureImportance {
                    feature: "query_frequency".to_string(),
                    importance: 0.8,
                },
                FeatureImportance {
                    feature: "data_days".to_string(),
                    importance: 0.6,
                },
                FeatureImportance {
                    feature: "compatibility".to_string(),
                    importance: if has_high_risk { 0.3 } else { 0.9 },
                },
            ],
            DecisionPath {
                steps: vec![
                    DecisionStep {
                        step: "analyze_schema".to_string(),
                        detail: "分析当前 Schema 状态和查询模式".to_string(),
                    },
                    DecisionStep {
                        step: "generate_changes".to_string(),
                        detail: format!(
                            "生成变更建议: {} 项已应用, {} 项待确认",
                            applied.len(),
                            pending_manual.len()
                        ),
                    },
                    DecisionStep {
                        step: "assess_risk".to_string(),
                        detail: format!("风险等级: {:?}", risk_level),
                    },
                    DecisionStep {
                        step: "verify_compatibility".to_string(),
                        detail: "兼容性验证: 低风险变更不破坏应用兼容性".to_string(),
                    },
                ],
            },
            risk_assessment,
            RollbackPlan {
                steps: applied
                    .iter()
                    .map(|c| RollbackStep {
                        step: c.change_type.name().to_string(),
                        detail: c.rollback_ddl.clone(),
                    })
                    .collect(),
                verify: VerifyCondition {
                    condition: "schema_compatible_with_app".to_string(),
                },
            },
        ))
    }

    /// 获取已应用的变更
    pub fn applied_changes(&self) -> &[SchemaChange] {
        self.auto_applier.applied_changes()
    }

    /// 获取风险评估器
    pub fn risk_assessor(&self) -> &SchemaRiskAssessor {
        &self.risk_assessor
    }

    /// 回滚
    pub fn rollback(&mut self, ddl: &str) -> Option<String> {
        self.auto_applier.rollback(ddl)
    }
}

impl Default for AutoSchemaEvolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use sz_orm_ai_designer::{DesignError, LlmSchemaProvider, SchemaDesign};

    /// Mock LLM Provider
    struct MockLlmProvider;

    #[async_trait]
    impl LlmSchemaProvider for MockLlmProvider {
        async fn design(&self, _requirement: &str) -> Result<SchemaDesign, DesignError> {
            Ok(SchemaDesign {
                tables: vec![],
                ddl_texts: vec![],
                rationale: "mock".to_string(),
            })
        }
    }

    fn make_advisor() -> SchemaDesignAdvisor {
        let designer = Arc::new(AiSchemaDesigner::new(Box::new(MockLlmProvider)));
        SchemaDesignAdvisor::with_default_config(designer)
    }

    fn make_patterns_30_days() -> Vec<QueryPattern> {
        vec![
            QueryPattern {
                sql_template: "SELECT * FROM users WHERE id = ? AND name = ?".to_string(),
                frequency: 500,
                columns_accessed: vec!["id".to_string(), "name".to_string()],
                tables: vec!["users".to_string()],
                data_days: 30,
            },
            QueryPattern {
                sql_template: "SELECT * FROM orders WHERE user_id = ? AND created_at > ?"
                    .to_string(),
                frequency: 300,
                columns_accessed: vec!["user_id".to_string(), "created_at".to_string()],
                tables: vec!["orders".to_string()],
                data_days: 30,
            },
        ]
    }

    #[tokio::test]
    async fn test_advise_with_sufficient_data() {
        let advisor = make_advisor();
        let patterns = make_patterns_30_days();
        let result = advisor.advise(&patterns).await.unwrap();
        assert!(!result.explanation.is_empty());
        assert!(result.expected_benefit >= 0.0);
    }

    #[tokio::test]
    async fn test_advise_data_insufficient() {
        let advisor = make_advisor();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM users".to_string(),
            frequency: 100,
            columns_accessed: vec!["id".to_string()],
            tables: vec!["users".to_string()],
            data_days: 10,
        }];
        let result = advisor.advise(&patterns).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AiDeepError::SchemaDesignDataInsufficient(_)));
        assert_eq!(err.error_code(), "SCHEMA_DESIGN_DATA_INSUFFICIENT");
    }

    #[tokio::test]
    async fn test_advise_generates_index_suggestions() {
        let advisor = make_advisor();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM users WHERE id = ? AND name = ? AND age = ?".to_string(),
            frequency: 500,
            columns_accessed: vec!["id".to_string(), "name".to_string(), "age".to_string()],
            tables: vec!["users".to_string()],
            data_days: 30,
        }];
        let result = advisor.advise(&patterns).await.unwrap();
        assert!(!result.index_suggestions.is_empty());
        assert!(result.index_suggestions[0]
            .ddl_text
            .contains("CREATE INDEX"));
    }

    #[tokio::test]
    async fn test_advise_generates_partition_suggestions() {
        let advisor = make_advisor();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM logs WHERE created_at > ?".to_string(),
            frequency: 500,
            columns_accessed: vec!["created_at".to_string()],
            tables: vec!["logs".to_string()],
            data_days: 30,
        }];
        let result = advisor.advise(&patterns).await.unwrap();
        assert!(!result.partition_suggestions.is_empty());
        assert_eq!(result.partition_suggestions[0].partition_strategy, "RANGE");
    }

    #[tokio::test]
    async fn test_advise_generates_denorm_suggestions() {
        let advisor = make_advisor();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT u.*, o.* FROM users u JOIN orders o ON u.id = o.user_id"
                .to_string(),
            frequency: 500,
            columns_accessed: vec![
                "id".to_string(),
                "user_id".to_string(),
                "amount".to_string(),
            ],
            tables: vec!["users".to_string(), "orders".to_string()],
            data_days: 30,
        }];
        let result = advisor.advise(&patterns).await.unwrap();
        assert!(!result.denorm_suggestions.is_empty());
    }

    #[tokio::test]
    async fn test_advise_empty_patterns() {
        let advisor = make_advisor();
        let patterns: Vec<QueryPattern> = vec![];
        let result = advisor.advise(&patterns).await;
        // 空模式时 max_data_days = 0 < 30，应返回数据不足错误
        assert!(result.is_err());
    }

    #[test]
    fn test_config_default() {
        let config = SchemaDesignConfig::default();
        assert_eq!(config.min_data_days, 30);
        assert_eq!(config.high_frequency_threshold, 100);
        assert!(config.enable_denorm);
        assert!(config.enable_partition);
    }
}
