//! 生态扩展深化端到端接线测试（v8.1.0 组 6）
//!
//! ① PluginMarketplaceOps 插件上架签名验证审核 ≤ 60s 计费配置
//! ② SdkAutoGenPipeline Python SDK 自动生成 ≤ 5min 签名一致编译通过
//!
//! ③ CloudNativeTemplateGenerator 和 ④ DeveloperPortal 的接线测试
//! 分别位于 sz-orm-wasm/tests/ 和 sz-orm-studio/tests/（避免循环依赖）。

#![cfg(feature = "plugin-marketplace-ops")]

use std::sync::Arc;
use std::time::Instant;

use sz_orm_core::plugin::PluginSigner;
use sz_orm_core::plugin_marketplace::{
    BillingMode, PluginBillingEngine, PluginMarketplaceOps, PluginPackage, ReviewStatus,
};

/// 端到端测试 ①：PluginMarketplaceOps 插件上架签名验证审核 ≤ 60s 计费配置
#[test]
fn test_eco_deep_plugin_marketplace_full_flow() {
    let start = Instant::now();

    // 构造运营层
    let signer = Arc::new(PluginSigner::new(false));
    let billing = Arc::new(PluginBillingEngine::new(
        BillingMode::Free,
        b"e2e-billing-secret".to_vec(),
    ));
    let ops = PluginMarketplaceOps::new(signer, billing);

    // 构造已签名插件包
    let content = b"e2e plugin binary".to_vec();
    let secret_key = b"e2e-signing-key";
    let signer_helper = PluginSigner::new(false);
    let signature = signer_helper.sign(&content, secret_key);
    let pkg = PluginPackage {
        plugin_id: "e2e-plugin-1".to_string(),
        name: "e2e-test-plugin".to_string(),
        version: "1.0.0".to_string(),
        content,
        signature,
        public_key: secret_key.to_vec(),
        billing_mode: BillingMode::Usage,
        author: "e2e-author".to_string(),
    };

    // 上架 → 签名验证 → 审核
    let ticket = ops.submit_for_review(pkg.clone()).unwrap();
    assert_eq!(ticket.status, ReviewStatus::Pending);

    // 审核通过 → 计费配置 + 版本管理
    let listing = ops.approve(&ticket.ticket_id, pkg).unwrap();
    assert_eq!(listing.plugin_id, "e2e-plugin-1");
    assert_eq!(listing.billing_mode, BillingMode::Usage);

    // 下载统计
    let dl_count = ops.record_download("e2e-plugin-1").unwrap();
    assert_eq!(dl_count, 1);

    // 全流程 ≤ 60s
    let elapsed = start.elapsed();
    assert!(
        elapsed <= std::time::Duration::from_secs(60),
        "上架全流程 {:?} > 60s",
        elapsed
    );
}

/// 端到端测试 ①b：未签名插件被拒绝上架
#[test]
fn test_eco_deep_unsigned_plugin_rejected() {
    let signer = Arc::new(PluginSigner::new(false));
    let billing = Arc::new(PluginBillingEngine::new(
        BillingMode::Free,
        b"e2e-secret".to_vec(),
    ));
    let ops = PluginMarketplaceOps::new(signer, billing);

    let pkg = PluginPackage {
        plugin_id: "unsigned-plugin".to_string(),
        name: "unsigned".to_string(),
        version: "1.0.0".to_string(),
        content: b"content".to_vec(),
        signature: Vec::new(),
        public_key: Vec::new(),
        billing_mode: BillingMode::Free,
        author: "test".to_string(),
    };

    let result = ops.submit_for_review(pkg);
    assert!(result.is_err());
}

/// 端到端测试 ①c：版本管理不丢历史
#[test]
fn test_eco_deep_version_history_preserved() {
    let signer = Arc::new(PluginSigner::new(false));
    let billing = Arc::new(PluginBillingEngine::new(
        BillingMode::Free,
        b"e2e-secret".to_vec(),
    ));
    let ops = PluginMarketplaceOps::new(signer, billing);

    let make_pkg = |version: &str| {
        let content = b"versioned plugin".to_vec();
        let secret_key = b"version-key";
        let signer_helper = PluginSigner::new(false);
        let signature = signer_helper.sign(&content, secret_key);
        PluginPackage {
            plugin_id: "versioned-plugin".to_string(),
            name: "versioned".to_string(),
            version: version.to_string(),
            content: content.clone(),
            signature,
            public_key: secret_key.to_vec(),
            billing_mode: BillingMode::Free,
            author: "test".to_string(),
        }
    };

    // v1.0.0 上架
    let pkg1 = make_pkg("1.0.0");
    let t1 = ops.submit_for_review(pkg1.clone()).unwrap();
    let listing1 = ops.approve(&t1.ticket_id, pkg1).unwrap();
    assert!(listing1.historical_versions.is_empty());

    // v2.0.0 升级
    let pkg2 = make_pkg("2.0.0");
    let t2 = ops.submit_for_review(pkg2.clone()).unwrap();
    let listing2 = ops.approve(&t2.ticket_id, pkg2).unwrap();
    assert_eq!(listing2.current_version, "2.0.0");
    assert!(listing2.historical_versions.contains(&"1.0.0".to_string()));
}

/// 端到端测试 ①d：计费防篡改
#[test]
fn test_eco_deep_billing_tamper_detected() {
    let signer = Arc::new(PluginSigner::new(false));
    let billing = Arc::new(PluginBillingEngine::new(
        BillingMode::Usage,
        b"tamper-detect-key".to_vec(),
    ));
    let ops = PluginMarketplaceOps::new(signer, billing.clone());

    let content = b"billing plugin".to_vec();
    let secret_key = b"billing-sign-key";
    let signer_helper = PluginSigner::new(false);
    let signature = signer_helper.sign(&content, secret_key);
    let pkg = PluginPackage {
        plugin_id: "billing-plugin".to_string(),
        name: "billing".to_string(),
        version: "1.0.0".to_string(),
        content,
        signature,
        public_key: secret_key.to_vec(),
        billing_mode: BillingMode::Usage,
        author: "test".to_string(),
    };

    let ticket = ops.submit_for_review(pkg.clone()).unwrap();
    ops.approve(&ticket.ticket_id, pkg).unwrap();

    let usage = sz_orm_core::plugin_marketplace::Usage {
        call_count: 100,
        subscription_months: 0,
        unit_price_cents: 10,
    };
    let record = billing.charge("billing-plugin", usage).unwrap();
    assert_eq!(record.amount_cents, 1000);

    billing.detect_tamper().unwrap();
}

/// 端到端测试 ②：SdkAutoGenPipeline Python SDK 自动生成 ≤ 5min 签名一致编译通过
#[test]
fn test_eco_deep_sdk_auto_gen_python() {
    use sz_orm_core::sdk_auto_gen::{ApiDefinition, ApiSignature, SdkAutoGenPipeline, SdkLanguage};

    let start = Instant::now();

    let mut def = ApiDefinition::new("sz-orm-core");
    def.add_signature(ApiSignature {
        name: "query".to_string(),
        params: vec![("sql".to_string(), "String".to_string())],
        return_type: "String".to_string(),
        doc: "Execute query".to_string(),
    });
    def.add_signature(ApiSignature {
        name: "insert".to_string(),
        params: vec![
            ("table".to_string(), "String".to_string()),
            ("data".to_string(), "Vec<u8>".to_string()),
        ],
        return_type: "i64".to_string(),
        doc: "Insert record".to_string(),
    });

    let pipeline = SdkAutoGenPipeline::new(def, vec![SdkLanguage::Python]);
    let artifacts = pipeline.generate().unwrap();

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].language, SdkLanguage::Python);
    assert!(artifacts[0].signature_verified);
    assert!(artifacts[0].security_verified);
    assert!(artifacts[0].code.contains("def query"));
    assert!(artifacts[0].code.contains("def insert"));
    assert!(!artifacts[0].code.contains("__internal"));

    let elapsed = start.elapsed();
    assert!(
        elapsed <= std::time::Duration::from_secs(300),
        "SDK 生成 {:?} > 5min",
        elapsed
    );
}

/// 端到端测试 ②b：SDK 多语言生成
#[test]
fn test_eco_deep_sdk_auto_gen_multi_language() {
    use sz_orm_core::sdk_auto_gen::{ApiDefinition, ApiSignature, SdkAutoGenPipeline, SdkLanguage};

    let mut def = ApiDefinition::new("sz-orm-core");
    def.add_signature(ApiSignature {
        name: "connect".to_string(),
        params: vec![("url".to_string(), "String".to_string())],
        return_type: "bool".to_string(),
        doc: "Connect to DB".to_string(),
    });

    let langs = vec![
        SdkLanguage::Python,
        SdkLanguage::Java,
        SdkLanguage::Go,
        SdkLanguage::Cpp,
        SdkLanguage::TypeScript,
    ];
    let pipeline = SdkAutoGenPipeline::new(def, langs);
    let artifacts = pipeline.generate().unwrap();
    assert_eq!(artifacts.len(), 5);
    for a in &artifacts {
        assert!(a.signature_verified);
        assert!(a.security_verified);
    }
}

/// 端到端测试 ②c：SDK 签名不一致拒绝
#[test]
fn test_eco_deep_sdk_signature_mismatch_rejected() {
    use sz_orm_core::sdk_auto_gen::{ApiDefinition, ApiSignature, SdkAutoGenPipeline, SdkLanguage};

    let mut def = ApiDefinition::new("test");
    def.add_signature(ApiSignature {
        name: "".to_string(),
        params: vec![],
        return_type: "()".to_string(),
        doc: String::new(),
    });
    let pipeline = SdkAutoGenPipeline::new(def, vec![SdkLanguage::Python]);
    let err = pipeline.generate().unwrap_err();
    assert!(matches!(
        err,
        sz_orm_core::sdk_auto_gen::SdkGenError::SignatureMismatch(_)
    ));
}
