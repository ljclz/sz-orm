//! v8.0.0 组 6：插件市场与沙箱（plugin-marketplace feature gate）
//! v8.1.0 组 6：插件市场运营（plugin-marketplace-ops feature gate）
//!
//! 提供 `PluginSandbox` 和 `PluginMarketplaceVerifier`。
//! v8.1.0 新增 `PluginMarketplaceOps` 和 `PluginBillingEngine`。
//! 复用既有 `PluginRegistry`（plugin.rs:133）和 `PluginSigner`（plugin.rs:423）。

pub mod plugin_marketplace_verifier;
pub mod plugin_sandbox;

pub use plugin_marketplace_verifier::{
    EcoError, LoadResult, MarketPlugin, MarketplaceConfig, PluginMarketplaceVerifier,
};
pub use plugin_sandbox::{PluginSandbox, SandboxConfig, SandboxEnv, SandboxViolation};

// v8.1.0 组 6：插件市场运营层（plugin-marketplace-ops feature gate）
#[cfg(feature = "plugin-marketplace-ops")]
pub mod billing_engine;
#[cfg(feature = "plugin-marketplace-ops")]
pub mod marketplace_ops;

#[cfg(feature = "plugin-marketplace-ops")]
pub use billing_engine::{
    BillingAuditEntry, BillingMode, BillingRecord, PluginBillingEngine, Usage,
};
#[cfg(feature = "plugin-marketplace-ops")]
pub use marketplace_ops::{
    PluginListing, PluginMarketplaceOps, PluginPackage, ReviewStatus, ReviewTicket,
};
