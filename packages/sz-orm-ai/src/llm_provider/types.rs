//! LLM 请求/响应类型定义

use serde::{Deserialize, Serialize};

/// LLM 请求配置（单次请求参数）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequestConfig {
    /// 采样温度（0.0 = 确定性，1.0 = 高随机性）
    pub temperature: f32,
    /// 最大生成 token 数
    pub max_tokens: u32,
}

impl Default for LlmRequestConfig {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            max_tokens: 2000,
        }
    }
}

/// LLM 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// 生成的文本
    pub text: String,
    /// token 用量统计
    pub usage: LlmUsage,
}

/// LLM token 用量
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmUsage {
    /// 输入 prompt token 数
    pub prompt_tokens: u32,
    /// 输出 completion token 数
    pub completion_tokens: u32,
}

/// LLM 能力分类（用于按能力路由）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LlmCapability {
    /// 自然语言转 SQL
    Nl2Sql,
    /// 查询优化建议
    QueryOptimization,
    /// 索引建议
    IndexAdvice,
    /// 查询重写建议
    RewriteAdvice,
    /// 文本嵌入（embedding）
    Embedding,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_request_config_default() {
        let config = LlmRequestConfig::default();
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.max_tokens, 2000);
    }

    #[test]
    fn test_llm_usage_default() {
        let usage = LlmUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
    }

    #[test]
    fn test_llm_capability_equality() {
        assert_eq!(LlmCapability::Nl2Sql, LlmCapability::Nl2Sql);
        assert_ne!(LlmCapability::Nl2Sql, LlmCapability::Embedding);
    }
}
