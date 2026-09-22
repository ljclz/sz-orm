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
