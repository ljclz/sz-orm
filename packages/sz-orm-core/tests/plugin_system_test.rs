//! TASK-029: 插件系统单元测试
//!
//! 第三方插件注册 → 验证扩展 AI 能力 / 方言 / 中间件。

use std::sync::Arc;

use sz_orm_core::plugin::{
    AiExtension, DialectExtension, MiddlewareExtension, PluginError, PluginMetadata,
    PluginRegistry, SzOrmPlugin,
};

struct TestAiPlugin {
    metadata: PluginMetadata,
}

impl TestAiPlugin {
    fn new() -> Self {
        Self {
            metadata: PluginMetadata::new("test-ai-plugin", "1.0.0", "Test AI extension plugin"),
        }
    }
}

impl AiExtension for TestAiPlugin {
    fn name(&self) -> &str {
        "test-ai"
    }

    fn execute(&self, input: &str) -> Result<String, PluginError> {
        Ok(format!("AI processed: {}", input))
    }
}

impl SzOrmPlugin for TestAiPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn ai_extension(&self) -> Option<&dyn AiExtension> {
        Some(self)
    }
}

struct TestDialectPlugin {
    metadata: PluginMetadata,
}

impl TestDialectPlugin {
    fn new() -> Self {
        Self {
            metadata: PluginMetadata::new(
                "test-dialect-plugin",
                "1.0.0",
                "Test dialect extension plugin",
            ),
        }
    }
}

impl DialectExtension for TestDialectPlugin {
    fn dialect_name(&self) -> &str {
        "test-dialect"
    }

    fn translate(&self, sql: &str) -> Result<String, PluginError> {
        Ok(sql.replace("LIMIT", "FETCH FIRST").replace("LIMIT", "ROWS"))
    }
}

impl SzOrmPlugin for TestDialectPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn dialect_extension(&self) -> Option<&dyn DialectExtension> {
        Some(self)
    }
}

struct TestMiddlewarePlugin {
    metadata: PluginMetadata,
}

impl TestMiddlewarePlugin {
    fn new() -> Self {
        Self {
            metadata: PluginMetadata::new(
                "test-middleware-plugin",
                "1.0.0",
                "Test middleware extension plugin",
            ),
        }
    }
}

impl MiddlewareExtension for TestMiddlewarePlugin {
    fn name(&self) -> &str {
        "test-middleware"
    }

    fn before_query(&self, sql: &str) -> Result<String, PluginError> {
        Ok(format!("/* before */ {}", sql))
    }

    fn after_query(&self, sql: &str, result: &str) -> Result<String, PluginError> {
        Ok(format!("{} /* after: {} */", result, sql))
    }
}

impl SzOrmPlugin for TestMiddlewarePlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn middleware_extension(&self) -> Option<&dyn MiddlewareExtension> {
        Some(self)
    }
}

#[test]
fn test_register_and_get_plugin() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestAiPlugin::new());

    registry.register(plugin).unwrap();

    assert_eq!(registry.len(), 1);
    assert!(registry.get("test-ai-plugin").is_some());
}

#[test]
fn test_unregister_plugin() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestAiPlugin::new());

    registry.register(plugin).unwrap();
    assert_eq!(registry.len(), 1);

    registry.unregister("test-ai-plugin").unwrap();
    assert_eq!(registry.len(), 0);
}

#[test]
fn test_duplicate_registration() {
    let registry = PluginRegistry::new();
    let plugin1 = Arc::new(TestAiPlugin::new());
    let plugin2 = Arc::new(TestAiPlugin::new());

    registry.register(plugin1).unwrap();
    let result = registry.register(plugin2);
    assert!(result.is_err());
}

#[test]
fn test_execute_ai_extension() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestAiPlugin::new());
    registry.register(plugin).unwrap();

    let result = registry.execute_ai("test-ai-plugin", "test input").unwrap();
    assert!(result.contains("AI processed"));
    assert!(result.contains("test input"));
}

#[test]
fn test_dialect_extension() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestDialectPlugin::new());
    registry.register(plugin).unwrap();

    let result = registry
        .translate_dialect("test-dialect-plugin", "SELECT * FROM users LIMIT 10")
        .unwrap();
    assert!(result.contains("FETCH FIRST") || result.contains("ROWS"));
}

#[test]
fn test_middleware_before_query() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestMiddlewarePlugin::new());
    registry.register(plugin).unwrap();

    let result = registry
        .before_query("test-middleware-plugin", "SELECT * FROM users")
        .unwrap();
    assert!(result.contains("/* before */"));
}

#[test]
fn test_middleware_after_query() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestMiddlewarePlugin::new());
    registry.register(plugin).unwrap();

    let result = registry
        .after_query(
            "test-middleware-plugin",
            "SELECT * FROM users",
            "result data",
        )
        .unwrap();
    assert!(result.contains("/* after"));
}

#[test]
fn test_list_plugins() {
    let registry = PluginRegistry::new();
    registry.register(Arc::new(TestAiPlugin::new())).unwrap();
    registry
        .register(Arc::new(TestDialectPlugin::new()))
        .unwrap();
    registry
        .register(Arc::new(TestMiddlewarePlugin::new()))
        .unwrap();

    let list = registry.list();
    assert_eq!(list.len(), 3);
}

#[test]
fn test_plugin_not_found() {
    let registry = PluginRegistry::new();
    let result = registry.execute_ai("nonexistent", "input");
    assert!(result.is_err());
}

#[test]
fn test_ai_extension_not_available() {
    let registry = PluginRegistry::new();
    registry
        .register(Arc::new(TestDialectPlugin::new()))
        .unwrap();

    let result = registry.execute_ai("test-dialect-plugin", "input");
    assert!(result.is_err());
}

#[test]
fn test_empty_registry() {
    let registry = PluginRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(registry.list().is_empty());
}

#[test]
fn test_plugin_metadata() {
    let plugin = TestAiPlugin::new();
    let metadata = plugin.metadata();
    assert_eq!(metadata.name, "test-ai-plugin");
    assert_eq!(metadata.version, "1.0.0");
}

#[test]
fn test_plugin_metadata_with_author() {
    let metadata = PluginMetadata::new("test", "1.0", "desc").with_author("test author");
    assert_eq!(metadata.author, "test author");
}
