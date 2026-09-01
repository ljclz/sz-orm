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
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::NotFound(msg) => write!(f, "Plugin not found: {}", msg),
            PluginError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            PluginError::RegistrationFailed(msg) => write!(f, "Registration failed: {}", msg),
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
