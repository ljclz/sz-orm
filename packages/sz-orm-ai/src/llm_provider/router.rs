//! LlmRouter — 运行时热切换 + 按能力路由 + fallback 链

use super::types::{LlmRequestConfig, LlmResponse};
use super::{
    ClaudeProvider, GeminiProvider, LlmCapability, LlmConfig, LlmError, LlmProvider,
    LocalLlamaProvider, OpenAIProvider,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// 根据 LlmConfig 构造对应的 provider 实例
fn build_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, LlmError> {
    match config.provider {
        super::LlmProviderKind::Claude => Ok(Arc::new(ClaudeProvider::new(config.clone())?)),
        super::LlmProviderKind::Gemini => Ok(Arc::new(GeminiProvider::new(config.clone())?)),
        super::LlmProviderKind::Ollama => Ok(Arc::new(LocalLlamaProvider::new(config.clone())?)),
        super::LlmProviderKind::OpenAI => Ok(Arc::new(OpenAIProvider::new(config.clone())?)),
    }
}

/// LLM 路由器：运行时热切换 + 按能力路由 + fallback 链
///
/// - `current`：当前活跃 provider（RwLock<Arc> 热切换）
/// - `capability_routes`：按能力路由表（如 NL2SQL→Claude，Embedding→OpenAI）
/// - `fallback_config`：fallback 配置链（当前 provider 失败时尝试）
pub struct LlmRouter {
    current: RwLock<Arc<dyn LlmProvider>>,
    capability_routes: RwLock<HashMap<LlmCapability, Arc<dyn LlmProvider>>>,
    fallback_config: RwLock<Option<LlmConfig>>,
}

impl LlmRouter {
    /// 构造 LlmRouter（根据 config.provider 构造对应 provider）
    pub fn new(config: &LlmConfig) -> Result<Self, LlmError> {
        let provider = build_provider(config)?;
        Ok(Self {
            current: RwLock::new(provider),
            capability_routes: RwLock::new(HashMap::new()),
            fallback_config: RwLock::new(config.fallback.as_ref().map(|c| *c.clone())),
        })
    }

    /// 运行时热切换 provider（无需重启）
    pub fn switch(&self, config: &LlmConfig) -> Result<(), LlmError> {
        let provider = build_provider(config)?;
        *self.current.write() = provider;
        *self.fallback_config.write() = config.fallback.as_ref().map(|c| *c.clone());
        Ok(())
    }

    /// 配置能力路由表（如 NL2SQL→Claude，Embedding→OpenAI）
    pub fn set_capability_route(
        &self,
        cap: LlmCapability,
        provider: Arc<dyn LlmProvider>,
    ) -> Result<(), LlmError> {
        self.capability_routes.write().insert(cap, provider);
        Ok(())
    }

    /// 文本生成（调用当前 provider，失败时 fallback）
    pub async fn complete(
        &self,
        prompt: &str,
        config: &LlmRequestConfig,
    ) -> Result<LlmResponse, LlmError> {
        let _span = tracing::info_span!(
            "llm_complete",
            provider = tracing::field::Empty,
            model = tracing::field::Empty,
        );
        let _enter = _span.enter();

        let provider = self.current.read().clone();
        tracing::Span::current().record("provider", provider.provider_name());
        tracing::Span::current().record("model", provider.model());
        match provider.complete(prompt, config).await {
            Ok(resp) => {
                tracing::info!(
                    prompt_tokens = resp.usage.prompt_tokens,
                    completion_tokens = resp.usage.completion_tokens,
                    "llm_complete_success"
                );
                Ok(resp)
            }
            Err(primary_err) => {
                let fb_config = self.fallback_config.read().clone();
                if let Some(fb) = fb_config.as_ref() {
                    tracing::warn!(
                        "fallback from {} to {}",
                        provider.provider_name(),
                        fb.provider.as_str()
                    );
                    let fb_provider = build_provider(fb)?;
                    match fb_provider.complete(prompt, config).await {
                        Ok(resp) => Ok(resp),
                        Err(_) => {
                            if let Some(fb2) = fb.fallback.as_ref() {
                                tracing::warn!(
                                    "fallback from {} to {}",
                                    fb.provider.as_str(),
                                    fb2.provider.as_str()
                                );
                                let fb2_provider = build_provider(fb2)?;
                                fb2_provider
                                    .complete(prompt, config)
                                    .await
                                    .map_err(|_| LlmError::FallbackExhausted)
                            } else {
                                Err(LlmError::FallbackExhausted)
                            }
                        }
                    }
                } else {
                    Err(primary_err)
                }
            }
        }
    }

    /// 按能力路由文本生成（如 NL2SQL→Claude）
    pub async fn complete_by_capability(
        &self,
        cap: LlmCapability,
        prompt: &str,
        config: &LlmRequestConfig,
    ) -> Result<LlmResponse, LlmError> {
        let route_provider = self.capability_routes.read().get(&cap).cloned();
        if let Some(provider) = route_provider {
            return provider.complete(prompt, config).await;
        }
        self.complete(prompt, config).await
    }

    /// 获取当前 provider 名称
    pub fn current_provider_name(&self) -> &'static str {
        self.current.read().provider_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_config(provider: super::super::LlmProviderKind, api_key: &str) -> LlmConfig {
        LlmConfig {
            provider,
            model: match provider {
                super::super::LlmProviderKind::Claude => "claude-sonnet-4-20250514".to_string(),
                super::super::LlmProviderKind::Gemini => "gemini-1.5-pro".to_string(),
                super::super::LlmProviderKind::Ollama => "llama3.1".to_string(),
                super::super::LlmProviderKind::OpenAI => "gpt-4o".to_string(),
            },
            api_key: Some(api_key.to_string()),
            api_base: LlmConfig::api_base_for(provider),
            timeout: Duration::from_secs(30),
            max_tokens: 2000,
            fallback: None,
        }
    }

    #[test]
    fn test_router_new_openai() {
        let config = make_config(super::super::LlmProviderKind::OpenAI, "test-key");
        let router = LlmRouter::new(&config).unwrap();
        assert_eq!(router.current_provider_name(), "openai");
    }

    #[test]
    fn test_router_switch() {
        let openai_config = make_config(super::super::LlmProviderKind::OpenAI, "test-key");
        let router = LlmRouter::new(&openai_config).unwrap();
        assert_eq!(router.current_provider_name(), "openai");

        let claude_config = make_config(super::super::LlmProviderKind::Claude, "test-key");
        router.switch(&claude_config).unwrap();
        assert_eq!(router.current_provider_name(), "claude");
    }

    #[test]
    fn test_router_capability_route() {
        let config = make_config(super::super::LlmProviderKind::OpenAI, "test-key");
        let router = LlmRouter::new(&config).unwrap();

        let claude_config = make_config(super::super::LlmProviderKind::Claude, "test-key");
        let claude_provider: Arc<dyn LlmProvider> =
            Arc::new(ClaudeProvider::new(claude_config).unwrap());
        router
            .set_capability_route(LlmCapability::Nl2Sql, claude_provider)
            .unwrap();

        let routes = router.capability_routes.read();
        assert!(routes.get(&LlmCapability::Nl2Sql).is_some());
        assert!(routes.get(&LlmCapability::Embedding).is_none());
    }

    #[test]
    fn test_router_fallback_config() {
        let mut claude_config = make_config(super::super::LlmProviderKind::Claude, "test-key");
        let openai_config = make_config(super::super::LlmProviderKind::OpenAI, "test-key");
        claude_config.fallback = Some(Box::new(openai_config));

        let router = LlmRouter::new(&claude_config).unwrap();
        assert_eq!(router.current_provider_name(), "claude");

        let fb = router.fallback_config.read();
        assert!(fb.is_some());
        assert_eq!(
            fb.as_ref().unwrap().provider,
            super::super::LlmProviderKind::OpenAI
        );
    }

    #[test]
    fn test_router_new_ollama_no_key() {
        let config = LlmConfig::for_provider(super::super::LlmProviderKind::Ollama);
        let router = LlmRouter::new(&config).unwrap();
        assert_eq!(router.current_provider_name(), "ollama");
    }
}
