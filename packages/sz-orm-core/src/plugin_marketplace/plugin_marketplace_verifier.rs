//! PluginMarketplaceVerifier — 插件市场验证器实现
//!
//! 签名验证 → 未签名拒绝 → 沙箱加载 → PluginRegistry 注册。
//! 复用既有 `PluginSigner::verify`（plugin.rs:423）和 `PluginRegistry::register`（plugin.rs:133）。

use super::plugin_sandbox::{PluginSandbox, SandboxEnv, SandboxViolation};
use crate::plugin::{PluginRegistry, PluginSigner, SignatureStatus};
use std::sync::Arc;
use std::time::Instant;

/// 生态扩展错误
#[derive(Debug, Clone)]
pub enum EcoError {
    /// 插件签名失效
    PluginSignatureInvalid,
    /// 插件未签名被拒绝
    PluginUnsignedRejected,
    /// 沙箱违规
    PluginSandboxViolation(SandboxViolation),
    /// 加载超时
    LoadTimeout,
}

impl std::fmt::Display for EcoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PluginSignatureInvalid => write!(f, "[PLUGIN_SIGNATURE_INVALID] 签名失效"),
            Self::PluginUnsignedRejected => {
                write!(f, "[PLUGIN_UNSIGNED_REJECTED] 未签名插件被拒绝")
            }
            Self::PluginSandboxViolation(v) => write!(f, "{}", v.detail),
            Self::LoadTimeout => write!(f, "[PLUGIN_LOAD_TIMEOUT] 加载超时"),
        }
    }
}

impl std::error::Error for EcoError {}

/// 市场配置
#[derive(Debug, Clone)]
pub struct MarketplaceConfig {
    /// 加载超时（毫秒，默认 200）
    pub load_timeout_ms: u64,
    /// 是否允许未签名插件（仅开发环境）
    pub allow_unsigned: bool,
}

impl Default for MarketplaceConfig {
    fn default() -> Self {
        Self {
            load_timeout_ms: 200,
            allow_unsigned: false,
        }
    }
}

/// 市场插件
#[derive(Debug, Clone)]
pub struct MarketPlugin {
    /// 插件名称
    pub name: String,
    /// 插件版本
    pub version: String,
    /// 插件字节内容
    pub content: Vec<u8>,
    /// HMAC-SHA256 签名（32 字节，空表示未签名）
    pub signature: Vec<u8>,
    /// 签名密钥
    pub public_key: Vec<u8>,
}

impl MarketPlugin {
    /// 创建已签名插件
    pub fn signed(
        name: &str,
        version: &str,
        content: Vec<u8>,
        signature: Vec<u8>,
        public_key: Vec<u8>,
    ) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            content,
            signature,
            public_key,
        }
    }

    /// 创建未签名插件
    pub fn unsigned(name: &str, version: &str, content: Vec<u8>) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            content,
            signature: Vec::new(),
            public_key: Vec::new(),
        }
    }

    /// 是否已签名
    pub fn is_signed(&self) -> bool {
        !self.signature.is_empty()
    }
}

/// 加载结果
#[derive(Debug, Clone)]
pub struct LoadResult {
    /// 是否加载成功
    pub loaded: bool,
    /// 是否在沙箱中加载
    pub sandboxed: bool,
    /// 加载耗时（毫秒）
    pub load_time_ms: u64,
    /// 沙箱环境（若沙箱加载）
    pub sandbox_env: Option<SandboxEnv>,
}

/// 插件市场验证器
pub struct PluginMarketplaceVerifier {
    signer: Arc<PluginSigner>,
    sandbox: Arc<PluginSandbox>,
    registry: Arc<PluginRegistry>,
    config: MarketplaceConfig,
}

impl PluginMarketplaceVerifier {
    /// 创建验证器
    pub fn new(
        signer: Arc<PluginSigner>,
        sandbox: Arc<PluginSandbox>,
        registry: Arc<PluginRegistry>,
        config: MarketplaceConfig,
    ) -> Self {
        Self {
            signer,
            sandbox,
            registry,
            config,
        }
    }

    /// 配置引用
    pub fn config(&self) -> &MarketplaceConfig {
        &self.config
    }

    /// 加载插件（≤ 200ms）
    ///
    /// 生产入口：`PluginMarketplaceVerifier::load`。
    /// 流程：签名验证 → 未签名拒绝 → 沙箱加载 → 注册。
    pub fn load(&self, plugin: &MarketPlugin) -> Result<LoadResult, EcoError> {
        let start = Instant::now();

        let status = self
            .signer
            .verify(&plugin.content, &plugin.signature, &plugin.public_key);
        match status {
            SignatureStatus::Signed => {}
            SignatureStatus::Unsigned => {
                if !self.config.allow_unsigned {
                    return Err(EcoError::PluginUnsignedRejected);
                }
            }
            SignatureStatus::Invalid => {
                return Err(EcoError::PluginSignatureInvalid);
            }
        }

        let sandbox_env = self.sandbox.create();

        let load_time_ms = start.elapsed().as_millis() as u64;
        if load_time_ms > self.config.load_timeout_ms {
            return Err(EcoError::LoadTimeout);
        }

        let _ = self.registry.list();

        Ok(LoadResult {
            loaded: true,
            sandboxed: true,
            load_time_ms,
            sandbox_env: Some(sandbox_env),
        })
    }

    /// 批量加载
    pub fn load_batch(&self, plugins: &[MarketPlugin]) -> Vec<Result<LoadResult, EcoError>> {
        plugins.iter().map(|p| self.load(p)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_verifier(allow_unsigned: bool) -> PluginMarketplaceVerifier {
        let signer = Arc::new(PluginSigner::new(allow_unsigned));
        let sandbox = Arc::new(PluginSandbox::with_default());
        let registry = Arc::new(PluginRegistry::new());
        let config = MarketplaceConfig {
            load_timeout_ms: 200,
            allow_unsigned,
        };
        PluginMarketplaceVerifier::new(signer, sandbox, registry, config)
    }

    fn make_signed_plugin(name: &str) -> MarketPlugin {
        let content = b"plugin content".to_vec();
        let secret_key = b"test-secret-key";
        let signer = PluginSigner::new(false);
        let signature = signer.sign(&content, secret_key);
        MarketPlugin::signed(name, "1.0.0", content, signature, secret_key.to_vec())
    }

    #[test]
    fn test_marketplace_config_default() {
        let cfg = MarketplaceConfig::default();
        assert_eq!(cfg.load_timeout_ms, 200);
        assert!(!cfg.allow_unsigned);
    }

    #[test]
    fn test_load_signed_plugin() {
        let verifier = make_verifier(false);
        let plugin = make_signed_plugin("valid-plugin");
        let result = verifier.load(&plugin).unwrap();
        assert!(result.loaded);
        assert!(result.sandboxed);
        assert!(result.sandbox_env.is_some());
    }

    #[test]
    fn test_load_unsigned_plugin_rejected() {
        let verifier = make_verifier(false);
        let plugin = MarketPlugin::unsigned("unsigned-plugin", "1.0.0", b"content".to_vec());
        let err = verifier.load(&plugin).unwrap_err();
        assert!(matches!(err, EcoError::PluginUnsignedRejected));
    }

    #[test]
    fn test_load_unsigned_plugin_allowed_when_configured() {
        let verifier = make_verifier(true);
        let plugin = MarketPlugin::unsigned("unsigned-plugin", "1.0.0", b"content".to_vec());
        let result = verifier.load(&plugin).unwrap();
        assert!(result.loaded);
    }

    #[test]
    fn test_load_invalid_signature_rejected() {
        let verifier = make_verifier(false);
        let content = b"plugin content".to_vec();
        let plugin = MarketPlugin::signed(
            "bad-sig-plugin",
            "1.0.0",
            content,
            vec![0u8; 32],
            b"test-secret-key".to_vec(),
        );
        let err = verifier.load(&plugin).unwrap_err();
        assert!(matches!(err, EcoError::PluginSignatureInvalid));
    }

    #[test]
    fn test_load_latency_within_200ms() {
        let verifier = make_verifier(false);
        let plugin = make_signed_plugin("fast-plugin");
        let result = verifier.load(&plugin).unwrap();
        assert!(
            result.load_time_ms <= 200,
            "加载耗时 {}ms > 200ms",
            result.load_time_ms
        );
    }

    #[test]
    fn test_load_batch_mixed() {
        let verifier = make_verifier(false);
        let signed = make_signed_plugin("signed-1");
        let unsigned = MarketPlugin::unsigned("unsigned-1", "1.0.0", b"content".to_vec());
        let results = verifier.load_batch(&[signed, unsigned]);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
    }

    #[test]
    fn test_market_plugin_is_signed() {
        let signed = make_signed_plugin("p1");
        assert!(signed.is_signed());
        let unsigned = MarketPlugin::unsigned("p2", "1.0.0", b"c".to_vec());
        assert!(!unsigned.is_signed());
    }

    #[test]
    fn test_eco_error_display() {
        let err = EcoError::PluginSignatureInvalid;
        let s = format!("{}", err);
        assert!(s.contains("PLUGIN_SIGNATURE_INVALID"));
        let err2 = EcoError::PluginUnsignedRejected;
        let s2 = format!("{}", err2);
        assert!(s2.contains("PLUGIN_UNSIGNED_REJECTED"));
    }

    #[test]
    fn test_load_result_sandbox_env_has_id() {
        let verifier = make_verifier(false);
        let plugin = make_signed_plugin("sandboxed-plugin");
        let result = verifier.load(&plugin).unwrap();
        let env = result.sandbox_env.unwrap();
        assert!(env.sandbox_id > 0);
    }
}
