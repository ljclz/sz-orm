//! AI Schema 设计器实现
//!
//! 输入业务需求描述，LLM 生成建议表结构/字段/关系/索引。
//! sqlparser 验证语法，不合法时重试最多 3 次。
//! 禁止自动执行 DDL，仅返回 DDL 文本。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Schema 设计错误
#[derive(Debug, Error)]
pub enum DesignError {
    /// LLM 调用错误
    #[error("LLM error: {0}")]
    Llm(String),
    /// DDL 语法错误
    #[error("DDL syntax error: {0}")]
    Syntax(String),
    /// 重试次数耗尽
    #[error("Max retries ({0}) exhausted")]
    MaxRetries(usize),
}

/// 列定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// 列名
    pub name: String,
    /// 数据类型
    pub data_type: String,
    /// 是否可空
    pub nullable: bool,
    /// 是否主键
    pub is_primary_key: bool,
    /// 是否唯一
    pub is_unique: bool,
    /// 外键引用（格式："table.column"）
    pub foreign_key: Option<String>,
    /// 默认值
    pub default_value: Option<String>,
    /// 注释
    pub comment: Option<String>,
}

/// 表定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    /// 表名
    pub name: String,
    /// 列定义
    pub columns: Vec<ColumnDefinition>,
    /// 索引建议（索引名 + 列列表）
    pub indexes: Vec<(String, Vec<String>)>,
    /// 表注释
    pub comment: Option<String>,
}

/// Schema 设计结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDesign {
    /// 表定义列表
    pub tables: Vec<TableDefinition>,
    /// DDL 文本列表（CREATE TABLE + CREATE INDEX 语句）
    pub ddl_texts: Vec<String>,
    /// 设计理由
    pub rationale: String,
}

/// 设计结果（含重试信息）
#[derive(Debug, Clone)]
pub struct DesignResult {
    /// 最终设计
    pub design: SchemaDesign,
    /// 重试次数
    pub retries: usize,
    /// 是否经过语法修复
    pub fixed: bool,
}

/// Schema 变更影响报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationImpactReport {
    /// 受影响的查询数
    pub affected_queries: usize,
    /// 受影响的索引数
    pub affected_indexes: usize,
    /// 受影响的外键数
    pub affected_foreign_keys: usize,
    /// 建议迁移步骤
    pub migration_steps: Vec<String>,
    /// 风险等级
    pub risk_level: MigrationRisk,
}

/// 迁移风险等级
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationRisk {
    /// 低风险
    Low,
    /// 中风险
    Medium,
    /// 高风险
    High,
}

/// 反范式化建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenormalizationAdvice {
    /// 建议冗余的列
    pub redundant_columns: Vec<RedundantColumn>,
    /// 减少的 JOIN 数
    pub joins_reduced: usize,
    /// 理由
    pub reason: String,
}

/// 冗余列建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedundantColumn {
    /// 目标表
    pub target_table: String,
    /// 冗余列名
    pub column_name: String,
    /// 来源表
    pub source_table: String,
    /// 来源列
    pub source_column: String,
    /// 通过哪个外键关联
    pub via_foreign_key: String,
}

// ==================== LLM Provider trait ====================

/// LLM Schema 设计 Provider trait
///
/// 抽象 LLM 调用，测试中用 Mock 实现。
#[async_trait::async_trait]
pub trait LlmSchemaProvider: Send + Sync {
    /// 请求 LLM 生成 Schema 设计
    ///
    /// # 参数
    /// - `requirement`: 业务需求描述（如"电商订单系统"）
    ///
    /// # 返回值
    /// - `Ok(SchemaDesign)`: LLM 生成的 Schema 设计
    /// - `Err(DesignError)`: LLM 调用失败
    async fn design(&self, requirement: &str) -> Result<SchemaDesign, DesignError>;
}

// ==================== DDL 验证 ====================

/// 验证 DDL 语法（使用 sqlparser）
#[cfg(feature = "ai-schema-design")]
fn validate_ddl(ddl: &str) -> Result<(), DesignError> {
    let dialect = sqlparser::dialect::GenericDialect {};
    sqlparser::parser::Parser::parse_sql(&dialect, ddl)
        .map_err(|e| DesignError::Syntax(format!("{}", e)))?;
    Ok(())
}

/// 无 sqlparser 时跳过验证
#[cfg(not(feature = "ai-schema-design"))]
fn validate_ddl(_ddl: &str) -> Result<(), DesignError> {
    Ok(())
}

/// 生成 CREATE TABLE DDL
fn generate_create_table_ddl(table: &TableDefinition) -> String {
    let mut ddl = format!("CREATE TABLE {} (\n", table.name);
    let mut column_defs: Vec<String> = Vec::new();

    for col in &table.columns {
        let mut def = format!("    {} {}", col.name, col.data_type);
        if !col.nullable {
            def.push_str(" NOT NULL");
        }
        if col.is_primary_key {
            def.push_str(" PRIMARY KEY");
        }
        if col.is_unique && !col.is_primary_key {
            def.push_str(" UNIQUE");
        }
        if let Some(default) = &col.default_value {
            def.push_str(&format!(" DEFAULT {}", default));
        }
        if let Some(fk) = &col.foreign_key {
            let parts: Vec<&str> = fk.split('.').collect();
            if parts.len() == 2 {
                def.push_str(&format!(" REFERENCES {}({})", parts[0], parts[1]));
            }
        }
        column_defs.push(def);
    }

    ddl.push_str(&column_defs.join(",\n"));
    ddl.push_str("\n)");
    ddl
}

/// 生成 CREATE INDEX DDL
fn generate_create_index_ddl(table_name: &str, index_name: &str, columns: &[String]) -> String {
    format!(
        "CREATE INDEX {} ON {} ({})",
        index_name,
        table_name,
        columns.join(", ")
    )
}

// ==================== AiSchemaDesigner ====================

/// AI Schema 设计器
///
/// 输入业务需求描述，LLM 生成建议表结构/字段/关系/索引。
/// sqlparser 验证语法，不合法时重试最多 3 次。
/// 禁止自动执行 DDL，仅返回 DDL 文本。
pub struct AiSchemaDesigner {
    /// LLM Provider
    llm_provider: Box<dyn LlmSchemaProvider>,
    /// 最大重试次数
    max_retries: usize,
}

impl AiSchemaDesigner {
    /// 创建 AI Schema 设计器
    pub fn new(llm_provider: Box<dyn LlmSchemaProvider>) -> Self {
        Self {
            llm_provider,
            max_retries: 3,
        }
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 设计 Schema
    ///
    /// 输入业务需求描述（如"电商订单系统"），LLM 生成建议表结构/字段/关系/索引。
    /// sqlparser 验证语法，不合法时重试最多 max_retries 次。
    /// 禁止自动执行 DDL，仅返回 DDL 文本。
    pub async fn design_schema(&self, requirement: &str) -> Result<DesignResult, DesignError> {
        let mut last_error = DesignError::Llm("未执行任何尝试".to_string());

        for attempt in 0..=self.max_retries {
            // 1. LLM 生成 Schema
            let mut design = self.llm_provider.design(requirement).await?;

            // 2. 生成 DDL 文本
            let mut ddl_texts = Vec::new();
            for table in &design.tables {
                ddl_texts.push(generate_create_table_ddl(table));
                for (idx_name, idx_cols) in &table.indexes {
                    ddl_texts.push(generate_create_index_ddl(&table.name, idx_name, idx_cols));
                }
            }

            // 3. 验证 DDL 语法
            let mut all_valid = true;
            for ddl in &ddl_texts {
                if let Err(e) = validate_ddl(ddl) {
                    all_valid = false;
                    last_error = e;
                    break;
                }
            }

            if all_valid {
                design.ddl_texts = ddl_texts;
                return Ok(DesignResult {
                    design,
                    retries: attempt,
                    fixed: attempt > 0,
                });
            }
        }

        Err(last_error)
    }

    /// 分析 Schema 变更影响
    ///
    /// 分析 Schema 变更对现有查询/索引/外键的影响，输出影响报告。
    pub fn analyze_migration_impact(
        &self,
        old_schema: &SchemaDesign,
        new_schema: &SchemaDesign,
    ) -> MigrationImpactReport {
        let old_tables: std::collections::HashSet<&str> =
            old_schema.tables.iter().map(|t| t.name.as_str()).collect();
        let new_tables: std::collections::HashSet<&str> =
            new_schema.tables.iter().map(|t| t.name.as_str()).collect();

        let added_tables = new_tables.difference(&old_tables).count();
        let removed_tables = old_tables.difference(&new_tables).count();

        let mut affected_indexes = 0;
        let mut affected_foreign_keys = 0;

        for new_table in &new_schema.tables {
            if let Some(old_table) = old_schema.tables.iter().find(|t| t.name == new_table.name) {
                affected_indexes += new_table.indexes.len().abs_diff(old_table.indexes.len());

                for col in &new_table.columns {
                    if let Some(old_col) = old_table.columns.iter().find(|c| c.name == col.name) {
                        if old_col.foreign_key != col.foreign_key {
                            affected_foreign_keys += 1;
                        }
                    } else {
                        if col.foreign_key.is_some() {
                            affected_foreign_keys += 1;
                        }
                    }
                }
            }
        }

        let affected_queries = added_tables + removed_tables;
        let risk_level = if removed_tables > 0 || affected_foreign_keys > 2 {
            MigrationRisk::High
        } else if affected_indexes > 3 || affected_foreign_keys > 0 {
            MigrationRisk::Medium
        } else {
            MigrationRisk::Low
        };

        let mut migration_steps = Vec::new();
        if added_tables > 0 {
            migration_steps.push(format!("新增 {} 个表", added_tables));
        }
        if removed_tables > 0 {
            migration_steps.push(format!("删除 {} 个表（需先备份数据）", removed_tables));
        }
        if affected_indexes > 0 {
            migration_steps.push(format!("更新 {} 个索引", affected_indexes));
        }
        if affected_foreign_keys > 0 {
            migration_steps.push(format!("更新 {} 个外键关系", affected_foreign_keys));
        }
        if migration_steps.is_empty() {
            migration_steps.push("无变更".to_string());
        }

        MigrationImpactReport {
            affected_queries,
            affected_indexes,
            affected_foreign_keys,
            migration_steps,
            risk_level,
        }
    }

    /// 反范式化建议
    ///
    /// 基于查询日志识别频繁 JOIN，提供冗余字段建议以减少 JOIN。
    pub fn denormalization_advice(&self, frequent_joins: &[JoinPattern]) -> DenormalizationAdvice {
        let mut redundant_columns = Vec::new();
        let joins_reduced = frequent_joins.len();

        for join in frequent_joins {
            for col in &join.frequently_accessed_columns {
                redundant_columns.push(RedundantColumn {
                    target_table: join.from_table.clone(),
                    column_name: format!("{}_{}", join.to_table, col),
                    source_table: join.to_table.clone(),
                    source_column: col.clone(),
                    via_foreign_key: join.foreign_key.clone(),
                });
            }
        }

        let reason = if redundant_columns.is_empty() {
            "无频繁 JOIN，不建议反范式化".to_string()
        } else {
            format!(
                "识别到 {} 个频繁 JOIN 模式，建议冗余 {} 个列以减少 JOIN",
                joins_reduced,
                redundant_columns.len()
            )
        };

        DenormalizationAdvice {
            redundant_columns,
            joins_reduced,
            reason,
        }
    }
}

/// JOIN 查询模式（用于反范式化分析）
#[derive(Debug, Clone)]
pub struct JoinPattern {
    /// FROM 表
    pub from_table: String,
    /// JOIN 表
    pub to_table: String,
    /// 外键
    pub foreign_key: String,
    /// 频繁访问的列（JOIN 表的列）
    pub frequently_accessed_columns: Vec<String>,
    /// 频率（次/天）
    pub frequency: u64,
}
