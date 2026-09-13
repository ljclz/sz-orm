//! v7.0.0 可组合性插件系统完整使用示例
//!
//! 演示：注册带签名的中间件插件 → MiddlewareChain 前置/后置处理 →
//! 插件 panic 后宿主继续运行 → 扩展点触发

#![cfg(feature = "composable-plugin")]

use std::sync::Arc;

use sz_orm_core::hooks::{ExtensionHandler, ExtensionPoint, ExtensionPointRegistry, HookContext};
use sz_orm_core::plugin::{
    MiddlewareChain, MiddlewareExtension, PanicSafeRegistry, PluginError, PluginMetadata,
    PluginSigner, SzOrmPlugin,
};

struct LoggingMiddleware;
impl MiddlewareExtension for LoggingMiddleware {
    fn name(&self) -> &str {
        "logging"
    }
    fn before_query(&self, sql: &str) -> Result<String, PluginError> {
        println!("[before] {}", sql);
        Ok(sql.to_string())
    }
    fn after_query(&self, sql: &str, result: &str) -> Result<String, PluginError> {
        println!("[after] {} → {}", sql, result);
        Ok(result.to_string())
    }
}

struct AuditMiddleware;
impl MiddlewareExtension for AuditMiddleware {
    fn name(&self) -> &str {
        "audit"
    }
    fn before_query(&self, sql: &str) -> Result<String, PluginError> {
        Ok(format!("/* audited */ {}", sql))
    }
    fn after_query(&self, _sql: &str, result: &str) -> Result<String, PluginError> {
        Ok(result.to_string())
    }
}

struct DemoPlugin;
impl SzOrmPlugin for DemoPlugin {
    fn metadata(&self) -> &PluginMetadata {
        use std::sync::OnceLock;
        static META: OnceLock<PluginMetadata> = OnceLock::new();
        META.get_or_init(|| PluginMetadata::new("demo", "1.0.0", "示例插件"))
    }
}

struct ConnectHandler;
impl ExtensionHandler for ConnectHandler {
    fn name(&self) -> &str {
        "connect_handler"
    }
    fn handle(&self, ctx: &mut HookContext) -> Result<(), sz_orm_core::DbError> {
        ctx.set_meta("connected", "true");
        println!("6. 扩展点 BeforeConnect 触发");
        Ok(())
    }
}

fn main() {
    println!("=== sz-orm v7.0.0 可组合性插件系统示例 ===\n");

    let signer = PluginSigner::new(false);
    let plugin_bytes = b"demo-plugin-v1";
    let key = b"signing-secret";
    let sig = signer.sign(plugin_bytes, key);
    let status = signer.verify(plugin_bytes, &sig, key);
    println!("1. 插件签名验证: {:?}", status);
    assert!(signer.is_allowed(status));

    let registry = PanicSafeRegistry::new();
    registry.register_safe(Arc::new(DemoPlugin)).unwrap();
    println!("2. 插件注册成功: {:?}", registry.list());

    registry
        .invoke_safe("demo", |p| {
            println!("3. 插件调用: {}", p.metadata().name);
            Ok(())
        })
        .unwrap();

    let mut chain = MiddlewareChain::new();
    chain.add(1, Arc::new(LoggingMiddleware)).unwrap();
    chain.add(10, Arc::new(AuditMiddleware)).unwrap();
    let sql = chain.before_query("SELECT * FROM users").unwrap();
    println!("4. 中间件链前置处理: {}", sql);
    let result = chain.after_query("SELECT * FROM users", "rows").unwrap();
    println!("5. 中间件链后置处理: {}", result);
    println!("   链执行开销: {:?}", chain.last_chain_latency());

    let ext_reg = ExtensionPointRegistry::new();
    ext_reg.register(ExtensionPoint::BeforeConnect, Arc::new(ConnectHandler));
    let mut ctx = HookContext::new();
    ext_reg
        .trigger(ExtensionPoint::BeforeConnect, &mut ctx)
        .unwrap();
    println!(
        "   上下文元数据: connected = {:?}",
        ctx.get_meta("connected")
    );

    println!("\n=== 示例完成 ===");
}
