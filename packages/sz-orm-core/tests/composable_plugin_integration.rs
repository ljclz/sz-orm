//! v7.0.0 可组合性插件系统端到端集成测试

#![cfg(feature = "composable-plugin")]

use std::sync::Arc;

use sz_orm_core::hooks::{ExtensionPoint, ExtensionPointRegistry, HookContext};
use sz_orm_core::plugin::{
    MiddlewareExtension, PanicSafeRegistry, PluginError, PluginMetadata, PluginSigner,
    SignatureStatus, SzOrmPlugin,
};

struct SignedMiddlewarePlugin {
    metadata: PluginMetadata,
}

impl SzOrmPlugin for SignedMiddlewarePlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }
    fn middleware_extension(&self) -> Option<&dyn MiddlewareExtension> {
        struct AuditMiddleware;
        impl MiddlewareExtension for AuditMiddleware {
            fn name(&self) -> &str {
                "audit_mw"
            }
            fn before_query(&self, sql: &str) -> Result<String, PluginError> {
                Ok(format!("/* audit */ {}", sql))
            }
            fn after_query(&self, _sql: &str, result: &str) -> Result<String, PluginError> {
                Ok(result.to_string())
            }
        }
        Some(&AuditMiddleware)
    }
}

#[test]
fn full_plugin_workflow() {
    let registry = PanicSafeRegistry::new();
    let plugin = Arc::new(SignedMiddlewarePlugin {
        metadata: PluginMetadata::new("audit-plugin", "1.0.0", "审计中间件插件"),
    });

    registry.register_safe(plugin.clone()).unwrap();

    let signer = PluginSigner::new(false);
    let plugin_bytes = b"audit-plugin-v1";
    let key = b"signing-key";
    let sig = signer.sign(plugin_bytes, key);
    let status = signer.verify(plugin_bytes, &sig, key);
    assert_eq!(status, SignatureStatus::Signed);
    assert!(signer.is_allowed(status));

    registry
        .invoke_safe("audit-plugin", |p| {
            if let Some(mw) = p.middleware_extension() {
                let sql = mw.before_query("SELECT 1")?;
                assert!(sql.contains("/* audit */"));
            }
            Ok(())
        })
        .unwrap();

    let ext_reg = ExtensionPointRegistry::new();
    let mut ctx = HookContext::new();
    ctx.set_meta("plugin", "audit-plugin");
    ext_reg
        .trigger(ExtensionPoint::BeforeInsert, &mut ctx)
        .unwrap();
}

#[test]
fn panic_plugin_does_not_crash_host() {
    let registry = PanicSafeRegistry::new();

    struct PanicPlugin;
    impl SzOrmPlugin for PanicPlugin {
        fn metadata(&self) -> &PluginMetadata {
            use std::sync::OnceLock;
            static META: OnceLock<PluginMetadata> = OnceLock::new();
            META.get_or_init(|| PluginMetadata::new("panic-plugin", "1.0.0", "会 panic 的插件"))
        }
        fn middleware_extension(&self) -> Option<&dyn MiddlewareExtension> {
            struct PanicMw;
            impl MiddlewareExtension for PanicMw {
                fn name(&self) -> &str {
                    "panic_mw"
                }
                fn before_query(&self, _sql: &str) -> Result<String, PluginError> {
                    panic!("插件内部 panic")
                }
                fn after_query(&self, _sql: &str, _result: &str) -> Result<String, PluginError> {
                    Ok(String::new())
                }
            }
            Some(&PanicMw)
        }
    }

    registry.register_safe(Arc::new(PanicPlugin)).unwrap();

    let result = registry.invoke_safe("panic-plugin", |p| {
        if let Some(mw) = p.middleware_extension() {
            mw.before_query("SELECT 1")?;
        }
        Ok(())
    });

    assert!(result.is_err());
    assert_eq!(
        registry.state("panic-plugin"),
        Some(sz_orm_core::PluginState::AutoDisabled)
    );

    assert!(registry.invoke_safe("nonexistent", |_| Ok(())).is_err());
}

#[test]
fn unsigned_plugin_rejected_in_production() {
    let signer = PluginSigner::new(false);
    let status = signer.verify(b"plugin", &[], b"key");
    assert_eq!(status, SignatureStatus::Unsigned);
    assert!(!signer.is_allowed(status));
}

#[test]
fn invalid_signature_rejected() {
    let signer = PluginSigner::new(false);
    let bad_sig = vec![0u8; 32];
    let status = signer.verify(b"plugin", &bad_sig, b"key");
    assert_eq!(status, SignatureStatus::Invalid);
    assert!(!signer.is_allowed(status));
}
