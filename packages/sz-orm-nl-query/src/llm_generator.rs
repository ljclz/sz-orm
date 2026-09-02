//! LLM 驱动的 NL2SQL 生成器（真实 provider 接入）

use crate::pipeline::Nl2SqlGenerator;
use crate::types::NlQueryError;
use std::sync::Arc;
use sz_orm_ai::llm_provider::{LlmProvider, LlmRequestConfig};

/// 基于 LLM Provider 的 NL2SQL 生成器
///
/// 包装 `sz_orm_ai::LlmProvider`（OpenAI/Ollama/Gemini/Claude），
/// 实现 `Nl2SqlGenerator` trait，使 `NlQueryPipeline` 可通过 LLM 生成 SQL。
pub struct LlmNl2SqlGenerator {
    provider: Arc<dyn LlmProvider>,
    config: LlmRequestConfig,
}

impl LlmNl2SqlGenerator {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            config: LlmRequestConfig {
                temperature: 0.0,
                max_tokens: 500,
            },
        }
    }

    pub fn with_config(mut self, config: LlmRequestConfig) -> Self {
        self.config = config;
        self
    }

    fn build_prompt(nl: &str) -> String {
        format!(
            "将以下自然语言转换为 SQL 查询语句。只返回 SQL 语句本身，不要包含解释或 markdown 标记。\n自然语言: {}",
            nl
        )
    }

    fn extract_sql(text: &str) -> String {
        let trimmed = text.trim();
        let without_code_block = trimmed
            .strip_prefix("```sql")
            .or_else(|| trimmed.strip_prefix("```"))
            .map(|s| s.trim())
            .unwrap_or(trimmed)
            .trim_end_matches("```")
            .trim();
        without_code_block.to_string()
    }
}

#[async_trait::async_trait]
impl Nl2SqlGenerator for LlmNl2SqlGenerator {
    async fn generate(&self, nl: &str) -> Result<String, NlQueryError> {
        let prompt = Self::build_prompt(nl);
        let response = self
            .provider
            .complete(&prompt, &self.config)
            .await
            .map_err(|e| NlQueryError::Nl2SqlFailed(format!("LLM 调用失败: {}", e)))?;
        Ok(Self::extract_sql(&response.text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_ai::llm_provider::{LlmError, LlmResponse, LlmUsage};

    struct MockProvider;

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            _prompt: &str,
            _config: &LlmRequestConfig,
        ) -> Result<LlmResponse, LlmError> {
            Ok(LlmResponse {
                text: "SELECT * FROM users".to_string(),
                usage: LlmUsage::default(),
            })
        }

        async fn embed(&self, _text: &str) -> Result<Vec<f32>, LlmError> {
            Ok(vec![])
        }

        fn provider_name(&self) -> &'static str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-model"
        }
    }

    #[tokio::test]
    async fn test_llm_generator_produces_sql() {
        let generator = LlmNl2SqlGenerator::new(Arc::new(MockProvider));
        let sql = generator.generate("查询所有用户").await.unwrap();
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn test_extract_sql_strips_code_block() {
        let sql = LlmNl2SqlGenerator::extract_sql("```sql\nSELECT 1\n```");
        assert_eq!(sql, "SELECT 1");
    }

    #[test]
    fn test_extract_sql_plain_text() {
        let sql = LlmNl2SqlGenerator::extract_sql("SELECT 1");
        assert_eq!(sql, "SELECT 1");
    }

    #[test]
    fn test_build_prompt_contains_nl() {
        let prompt = LlmNl2SqlGenerator::build_prompt("查询订单");
        assert!(prompt.contains("查询订单"));
        assert!(prompt.contains("SQL"));
    }
}
