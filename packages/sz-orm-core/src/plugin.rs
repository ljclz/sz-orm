//! 插件系统模块
//!
//! 提供 SzOrmPlugin trait 允许第三方扩展 AI 能力/方言/中间件。
//! 通过 PluginRegistry 管理插件注册 + 加载 + 调用。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

/// 插件元数据
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// 插件名称
    pub name: String,
    /// 插件版本
    pub version: String,
    /// 插件描述
    pub description: String,
    /// 插件作者
    pub author: String,
}

impl PluginMetadata {
    /// 创建插件元数据
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            author: String::new(),
        }
    }

    /// 设置作者
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }
}

/// AI 能力扩展点
pub trait AiExtension: Send + Sync {
    /// 扩展名称
    fn name(&self) -> &str;

    /// 执行 AI 扩展能力
    fn execute(&self, input: &str) -> Result<String, PluginError>;
}

/// 方言扩展点
pub trait DialectExtension: Send + Sync {
    /// 方言名称
    fn dialect_name(&self) -> &str;

    /// 将 SQL 转换为该方言
    fn translate(&self, sql: &str) -> Result<String, PluginError>;
}

/// 中间件扩展点
pub trait MiddlewareExtension: Send + Sync {
    /// 中间件名称
    fn name(&self) -> &str;

    /// 前置处理
    fn before_query(&self, sql: &str) -> Result<String, PluginError>;

    /// 后置处理
    fn after_query(&self, sql: &str, result: &str) -> Result<String, PluginError>;
}

/// 插件错误
#[derive(Debug, Clone)]
pub enum PluginError {
    /// 插件未找到
    NotFound(String),
    /// 执行失败
    ExecutionFailed(String),
    /// 注册失败
    RegistrationFailed(String),
    /// 中间件链过长
    ChainTooLong(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::NotFound(msg) => write!(f, "Plugin not found: {}", msg),
            PluginError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            PluginError::RegistrationFailed(msg) => write!(f, "Registration failed: {}", msg),
            PluginError::ChainTooLong(msg) => write!(f, "Chain too long: {}", msg),
        }
    }
}

impl std::error::Error for PluginError {}

/// SZ-ORM 插件 trait
///
/// 允许第三方扩展 AI 能力/方言/中间件。
pub trait SzOrmPlugin: Send + Sync {
    /// 插件元数据
    fn metadata(&self) -> &PluginMetadata;

    /// 初始化插件
    fn init(&self) -> Result<(), PluginError> {
        Ok(())
    }

    /// 获取 AI 扩展（可选）
    fn ai_extension(&self) -> Option<&dyn AiExtension> {
        None
    }

    /// 获取方言扩展（可选）
    fn dialect_extension(&self) -> Option<&dyn DialectExtension> {
        None
    }

    /// 获取中间件扩展（可选）
    fn middleware_extension(&self) -> Option<&dyn MiddlewareExtension> {
        None
    }
}

/// 插件注册表
///
/// 管理插件注册 + 加载 + 调用。
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, Arc<dyn SzOrmPlugin>>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
        }
    }

    /// 注册插件
    pub fn register(&self, plugin: Arc<dyn SzOrmPlugin>) -> Result<(), PluginError> {
        let metadata = plugin.metadata();
        let name = metadata.name.clone();

        plugin.init()?;

        let mut plugins = self.plugins.write();
        if plugins.contains_key(&name) {
            return Err(PluginError::RegistrationFailed(format!(
                "插件 {} 已存在",
                name
            )));
        }
        plugins.insert(name, plugin);
        Ok(())
    }

    /// 注销插件
    pub fn unregister(&self, name: &str) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write();
        plugins
            .remove(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        Ok(())
    }

    /// 获取插件
    pub fn get(&self, name: &str) -> Option<Arc<dyn SzOrmPlugin>> {
        self.plugins.read().get(name).cloned()
    }

    /// 列出所有插件名
    pub fn list(&self) -> Vec<String> {
        self.plugins.read().keys().cloned().collect()
    }

    /// 插件数量
    pub fn len(&self) -> usize {
        self.plugins.read().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.plugins.read().is_empty()
    }

    /// 调用 AI 扩展
    pub fn execute_ai(&self, plugin_name: &str, input: &str) -> Result<String, PluginError> {
        let plugin = self
            .get(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?;
        let ext = plugin
            .ai_extension()
            .ok_or_else(|| PluginError::ExecutionFailed("插件无 AI 扩展".to_string()))?;
        ext.execute(input)
    }

    /// 调用方言扩展
    pub fn translate_dialect(&self, plugin_name: &str, sql: &str) -> Result<String, PluginError> {
        let plugin = self
            .get(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?;
        let ext = plugin
            .dialect_extension()
            .ok_or_else(|| PluginError::ExecutionFailed("插件无方言扩展".to_string()))?;
        ext.translate(sql)
    }

    /// 调用中间件前置处理
    pub fn before_query(&self, plugin_name: &str, sql: &str) -> Result<String, PluginError> {
        let plugin = self
            .get(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?;
        let ext = plugin
            .middleware_extension()
            .ok_or_else(|| PluginError::ExecutionFailed("插件无中间件扩展".to_string()))?;
        ext.before_query(sql)
    }

    /// 调用中间件后置处理
    pub fn after_query(
        &self,
        plugin_name: &str,
        sql: &str,
        result: &str,
    ) -> Result<String, PluginError> {
        let plugin = self
            .get(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?;
        let ext = plugin
            .middleware_extension()
            .ok_or_else(|| PluginError::ExecutionFailed("插件无中间件扩展".to_string()))?;
        ext.after_query(sql, result)
    }
}
// =====================================================================
// v7.0.0 composable-plugin：PanicSafeRegistry / PluginSigner / MiddlewareChain
// =====================================================================

#[cfg(feature = "composable-plugin")]
mod composable {
    use std::collections::HashMap;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use parking_lot::RwLock;

    use super::{MiddlewareExtension, PluginError, SzOrmPlugin};

    /// 插件运行状态
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PluginState {
        /// 正常启用
        Enabled,
        /// 手动禁用
        Disabled,
        /// 因 panic 自动禁用
        AutoDisabled,
    }

    /// panic 安全的插件注册表
    ///
    /// 包装插件调用以 `std::panic::catch_unwind`，panic 后自动将插件置为
    /// [`PluginState::AutoDisabled`] 并通过 `tracing::warn!` 告警，宿主继续运行。
    pub struct PanicSafeRegistry {
        plugins: RwLock<HashMap<String, (Arc<dyn SzOrmPlugin>, PluginState)>>,
    }

    impl Default for PanicSafeRegistry {
        fn default() -> Self {
            Self::new()
        }
    }

    impl PanicSafeRegistry {
        /// 创建空注册表
        pub fn new() -> Self {
            Self {
                plugins: RwLock::new(HashMap::new()),
            }
        }

        /// 安全注册插件（验证元数据后置为 Enabled）
        pub fn register_safe(&self, plugin: Arc<dyn SzOrmPlugin>) -> Result<(), PluginError> {
            let metadata = plugin.metadata();
            if metadata.name.is_empty() {
                return Err(PluginError::RegistrationFailed(
                    "插件名称不能为空".to_string(),
                ));
            }
            plugin.init()?;
            let name = metadata.name.clone();
            let mut plugins = self.plugins.write();
            if plugins.contains_key(&name) {
                return Err(PluginError::RegistrationFailed(format!(
                    "插件 {} 已存在",
                    name
                )));
            }
            plugins.insert(name, (plugin, PluginState::Enabled));
            Ok(())
        }

        /// 安全调用插件，panic 时自动禁用
        ///
        /// `f` 接收插件引用并返回结果。若插件 panic，则置为
        /// `AutoDisabled` 并返回 `PluginError::ExecutionFailed`。
        pub fn invoke_safe<F, R>(&self, plugin_name: &str, f: F) -> Result<R, PluginError>
        where
            F: FnOnce(&dyn SzOrmPlugin) -> Result<R, PluginError>,
        {
            let (plugin, state) = {
                let plugins = self.plugins.read();
                plugins
                    .get(plugin_name)
                    .map(|(p, s)| (p.clone(), *s))
                    .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?
            };
            if state != PluginState::Enabled {
                return Err(PluginError::ExecutionFailed(format!(
                    "插件 {} 未启用（当前状态: {:?}）",
                    plugin_name, state
                )));
            }
            let result = catch_unwind(AssertUnwindSafe(|| f(plugin.as_ref())));
            match result {
                Ok(r) => r,
                Err(panic_payload) => {
                    let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "未知 panic".to_string()
                    };
                    tracing::warn!(
                        plugin = plugin_name,
                        panic_msg = %msg,
                        "插件 panic，自动禁用"
                    );
                    let mut plugins = self.plugins.write();
                    if let Some(entry) = plugins.get_mut(plugin_name) {
                        entry.1 = PluginState::AutoDisabled;
                    }
                    Err(PluginError::ExecutionFailed(format!(
                        "插件 {} panic: {}",
                        plugin_name, msg
                    )))
                }
            }
        }

        /// 查询插件状态
        pub fn state(&self, name: &str) -> Option<PluginState> {
            self.plugins.read().get(name).map(|(_, s)| *s)
        }

        /// 手动恢复 AutoDisabled 插件为 Enabled
        pub fn enable(&self, name: &str) -> Result<(), PluginError> {
            let mut plugins = self.plugins.write();
            let entry = plugins
                .get_mut(name)
                .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
            entry.1 = PluginState::Enabled;
            Ok(())
        }

        /// 列出所有插件名
        pub fn list(&self) -> Vec<String> {
            self.plugins.read().keys().cloned().collect()
        }
    }

    /// 插件签名状态
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SignatureStatus {
        /// 签名有效
        Signed,
        /// 未签名（无签名数据）
        Unsigned,
        /// 签名无效
        Invalid,
    }

    /// 插件签名验证器（HMAC-SHA256）
    ///
    /// 使用 `sz-orm-crypto` 的 HMAC-SHA256 原语验证插件完整性。
    /// 生产环境仅允许 [`SignatureStatus::Signed`]，`Unsigned` 在
    /// `allow_unsigned = true` 时放行（仅限开发环境）。
    pub struct PluginSigner {
        allow_unsigned: bool,
    }

    impl PluginSigner {
        /// 创建签名验证器
        ///
        /// `allow_unsigned` 为 true 时允许未签名插件通过（开发模式）。
        pub fn new(allow_unsigned: bool) -> Self {
            Self { allow_unsigned }
        }

        /// 验证插件签名
        ///
        /// - `plugin`：插件字节内容
        /// - `signature`：HMAC-SHA256 签名（32 字节）
        /// - `public_key`：HMAC 密钥
        ///
        /// 返回 [`SignatureStatus::Signed`] 表示签名有效，
        /// [`SignatureStatus::Invalid`] 表示签名不匹配，
        /// [`SignatureStatus::Unsigned`] 表示无签名数据。
        pub fn verify(
            &self,
            plugin: &[u8],
            signature: &[u8],
            public_key: &[u8],
        ) -> SignatureStatus {
            if signature.is_empty() {
                return SignatureStatus::Unsigned;
            }
            let expected = sz_orm_crypto::hmac_sha256(public_key, plugin);
            if expected.as_slice() == signature {
                SignatureStatus::Signed
            } else {
                SignatureStatus::Invalid
            }
        }

        /// 检查签名状态是否允许加载
        pub fn is_allowed(&self, status: SignatureStatus) -> bool {
            match status {
                SignatureStatus::Signed => true,
                SignatureStatus::Unsigned => self.allow_unsigned,
                SignatureStatus::Invalid => false,
            }
        }

        /// 对插件内容签名（用于签名生成）
        pub fn sign(&self, plugin: &[u8], secret_key: &[u8]) -> Vec<u8> {
            sz_orm_crypto::hmac_sha256(secret_key, plugin).to_vec()
        }
    }

    /// 中间件链最大长度
    const MAX_CHAIN_LEN: usize = 10;

    /// 有序中间件链
    ///
    /// 按 `order` 升序执行前置处理，降序执行后置处理。
    /// 链长上限 [`MAX_CHAIN_LEN`]，超过返回 `PluginError::ChainTooLong`。
    pub struct MiddlewareChain {
        chain: Vec<(i32, Arc<dyn MiddlewareExtension>)>,
        last_latency: RwLock<Option<Duration>>,
    }

    impl Default for MiddlewareChain {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MiddlewareChain {
        /// 创建空中间件链
        pub fn new() -> Self {
            Self {
                chain: Vec::new(),
                last_latency: RwLock::new(None),
            }
        }

        /// 添加中间件，按 order 排序插入
        pub fn add(
            &mut self,
            order: i32,
            middleware: Arc<dyn MiddlewareExtension>,
        ) -> Result<(), PluginError> {
            if self.chain.len() >= MAX_CHAIN_LEN {
                return Err(PluginError::ChainTooLong(format!(
                    "中间件链长度超过上限 {}",
                    MAX_CHAIN_LEN
                )));
            }
            let pos = self.chain.partition_point(|(o, _)| *o < order);
            self.chain.insert(pos, (order, middleware));
            Ok(())
        }

        /// 前置处理：按 order 升序执行
        pub fn before_query(&self, sql: &str) -> Result<String, PluginError> {
            let start = Instant::now();
            let mut current = sql.to_string();
            for (_, mw) in &self.chain {
                current = mw.before_query(&current)?;
            }
            let latency = start.elapsed();
            *self.last_latency.write() = Some(latency);
            Ok(current)
        }

        /// 后置处理：按 order 降序执行
        pub fn after_query(&self, sql: &str, result: &str) -> Result<String, PluginError> {
            let start = Instant::now();
            let mut current = result.to_string();
            for (_, mw) in self.chain.iter().rev() {
                current = mw.after_query(sql, &current)?;
            }
            let latency = start.elapsed();
            *self.last_latency.write() = Some(latency);
            Ok(current)
        }

        /// 最近一次链执行开销
        pub fn last_chain_latency(&self) -> Duration {
            self.last_latency.read().unwrap_or_default()
        }

        /// 链长度
        pub fn len(&self) -> usize {
            self.chain.len()
        }

        /// 是否为空
        pub fn is_empty(&self) -> bool {
            self.chain.is_empty()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::super::PluginMetadata;
        use super::*;

        struct GoodPlugin;
        impl SzOrmPlugin for GoodPlugin {
            fn metadata(&self) -> &PluginMetadata {
                use std::sync::OnceLock;
                static META: OnceLock<PluginMetadata> = OnceLock::new();
                META.get_or_init(|| PluginMetadata::new("good", "1.0.0", "test plugin"))
            }
        }

        struct PanicPlugin;
        impl SzOrmPlugin for PanicPlugin {
            fn metadata(&self) -> &PluginMetadata {
                use std::sync::OnceLock;
                static META: OnceLock<PluginMetadata> = OnceLock::new();
                META.get_or_init(|| PluginMetadata::new("panic", "1.0.0", "panic plugin"))
            }
            fn ai_extension(&self) -> Option<&dyn super::super::AiExtension> {
                struct PanicAi;
                impl super::super::AiExtension for PanicAi {
                    fn name(&self) -> &str {
                        "panic_ai"
                    }
                    fn execute(&self, _input: &str) -> Result<String, PluginError> {
                        panic!("故意 panic")
                    }
                }
                Some(&PanicAi)
            }
        }

        #[test]
        fn panic_safe_registry_normal() {
            let reg = PanicSafeRegistry::new();
            reg.register_safe(Arc::new(GoodPlugin)).unwrap();
            let result = reg
                .invoke_safe("good", |p| {
                    p.metadata();
                    Ok(42i32)
                })
                .unwrap();
            assert_eq!(result, 42);
            assert_eq!(reg.state("good"), Some(PluginState::Enabled));
        }

        #[test]
        fn panic_safe_registry_catches_panic() {
            let reg = PanicSafeRegistry::new();
            reg.register_safe(Arc::new(PanicPlugin)).unwrap();
            let result = reg.invoke_safe("panic", |p| {
                if let Some(ext) = p.ai_extension() {
                    ext.execute("x")?;
                }
                Ok(())
            });
            assert!(result.is_err());
            assert_eq!(reg.state("panic"), Some(PluginState::AutoDisabled));
        }

        #[test]
        fn panic_safe_registry_recover() {
            let reg = PanicSafeRegistry::new();
            reg.register_safe(Arc::new(PanicPlugin)).unwrap();
            let _ = reg.invoke_safe("panic", |p| {
                if let Some(ext) = p.ai_extension() {
                    ext.execute("x")?;
                }
                Ok(())
            });
            assert_eq!(reg.state("panic"), Some(PluginState::AutoDisabled));
            reg.enable("panic").unwrap();
            assert_eq!(reg.state("panic"), Some(PluginState::Enabled));
        }

        #[test]
        fn plugin_signer_signed() {
            let signer = PluginSigner::new(false);
            let plugin = b"plugin bytes";
            let key = b"secret key";
            let sig = signer.sign(plugin, key);
            let status = signer.verify(plugin, &sig, key);
            assert_eq!(status, SignatureStatus::Signed);
            assert!(signer.is_allowed(status));
        }

        #[test]
        fn plugin_signer_invalid() {
            let signer = PluginSigner::new(false);
            let plugin = b"plugin bytes";
            let key = b"secret key";
            let bad_sig = vec![0u8; 32];
            let status = signer.verify(plugin, &bad_sig, key);
            assert_eq!(status, SignatureStatus::Invalid);
            assert!(!signer.is_allowed(status));
        }

        #[test]
        fn plugin_signer_unsigned_rejected() {
            let signer = PluginSigner::new(false);
            let status = signer.verify(b"plugin", &[], b"key");
            assert_eq!(status, SignatureStatus::Unsigned);
            assert!(!signer.is_allowed(status));
        }

        #[test]
        fn plugin_signer_unsigned_allowed() {
            let signer = PluginSigner::new(true);
            let status = signer.verify(b"plugin", &[], b"key");
            assert_eq!(status, SignatureStatus::Unsigned);
            assert!(signer.is_allowed(status));
        }

        struct NoopMiddleware {
            name: String,
        }
        impl MiddlewareExtension for NoopMiddleware {
            fn name(&self) -> &str {
                &self.name
            }
            fn before_query(&self, sql: &str) -> Result<String, PluginError> {
                Ok(sql.to_string())
            }
            fn after_query(&self, _sql: &str, result: &str) -> Result<String, PluginError> {
                Ok(result.to_string())
            }
        }

        #[test]
        fn middleware_chain_order() {
            let mut chain = MiddlewareChain::new();
            chain
                .add(10, Arc::new(NoopMiddleware { name: "a".into() }))
                .unwrap();
            chain
                .add(1, Arc::new(NoopMiddleware { name: "b".into() }))
                .unwrap();
            chain
                .add(5, Arc::new(NoopMiddleware { name: "c".into() }))
                .unwrap();
            assert_eq!(chain.len(), 3);
            let result = chain.before_query("SELECT 1").unwrap();
            assert_eq!(result, "SELECT 1");
        }

        #[test]
        fn middleware_chain_max_len() {
            let mut chain = MiddlewareChain::new();
            for i in 0..10 {
                chain
                    .add(
                        i,
                        Arc::new(NoopMiddleware {
                            name: format!("m{}", i),
                        }),
                    )
                    .unwrap();
            }
            assert_eq!(chain.len(), 10);
            let err = chain
                .add(
                    100,
                    Arc::new(NoopMiddleware {
                        name: "overflow".into(),
                    }),
                )
                .unwrap_err();
            assert!(matches!(err, PluginError::ChainTooLong(_)));
        }

        #[test]
        fn middleware_chain_latency() {
            let mut chain = MiddlewareChain::new();
            for i in 0..10 {
                chain
                    .add(
                        i,
                        Arc::new(NoopMiddleware {
                            name: format!("m{}", i),
                        }),
                    )
                    .unwrap();
            }
            chain.before_query("SELECT 1").unwrap();
            let latency = chain.last_chain_latency();
            assert!(latency <= Duration::from_millis(1));
        }

        #[test]
        fn middleware_chain_empty() {
            let chain = MiddlewareChain::new();
            assert!(chain.is_empty());
            let result = chain.before_query("SELECT 1").unwrap();
            assert_eq!(result, "SELECT 1");
        }
    }
}

#[cfg(feature = "composable-plugin")]
pub use composable::{
    MiddlewareChain, PanicSafeRegistry, PluginSigner, PluginState, SignatureStatus,
};
