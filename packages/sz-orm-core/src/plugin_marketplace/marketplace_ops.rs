//! PluginMarketplaceOps — 插件市场运营层（v8.1.0 组 6：插件市场运营）
//!
//! 上架 → 签名验证 → 审核 ≤ 60s → 计费配置 → 版本管理（不丢历史）→ 下载统计。
//! 复用 `PluginSigner::verify`（plugin.rs:423）和 `PluginBillingEngine`。

use crate::plugin::{PluginError, PluginSigner, SignatureStatus};
use crate::plugin_marketplace::billing_engine::{BillingMode, PluginBillingEngine};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;

/// 插件包（上架单元）
#[derive(Debug, Clone)]
pub struct PluginPackage {
    /// 插件 ID
    pub plugin_id: String,
    /// 名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 字节内容
    pub content: Vec<u8>,
    /// HMAC-SHA256 签名
    pub signature: Vec<u8>,
    /// 签名密钥
    pub public_key: Vec<u8>,
    /// 计费模式
    pub billing_mode: BillingMode,
    /// 作者
    pub author: String,
}

/// 审核工单
#[derive(Debug, Clone)]
pub struct ReviewTicket {
    /// 工单 ID
    pub ticket_id: String,
    /// 插件 ID
    pub plugin_id: String,
    /// 提交时间（UTC RFC3339）
    pub submitted_at: String,
    /// 审核状态
    pub status: ReviewStatus,
}

/// 审核状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewStatus {
    /// 待审核
    Pending,
    /// 已通过
    Approved,
    /// 已拒绝
    Rejected(String),
}

/// 已上架插件
#[derive(Debug, Clone)]
pub struct PluginListing {
    /// 插件 ID
    pub plugin_id: String,
    /// 名称
    pub name: String,
    /// 当前版本
    pub current_version: String,
    /// 历史版本列表（不丢历史）
    pub historical_versions: Vec<String>,
    /// 下载次数
    pub download_count: u64,
    /// 计费模式
    pub billing_mode: BillingMode,
    /// 上架时间
    pub listed_at: String,
}

/// 插件市场运营层
///
/// 生产入口：`PluginMarketplaceOps::submit_for_review` / `approve` / `record_download`。
pub struct PluginMarketplaceOps {
    signer: Arc<PluginSigner>,
    billing: Arc<PluginBillingEngine>,
    /// 审核工单
    tickets: RwLock<HashMap<String, ReviewTicket>>,
    /// 已上架插件
    listings: RwLock<HashMap<String, PluginListing>>,
    /// 工单计数器
    ticket_counter: RwLock<u64>,
}

impl PluginMarketplaceOps {
    /// 创建运营层
    pub fn new(signer: Arc<PluginSigner>, billing: Arc<PluginBillingEngine>) -> Self {
        Self {
            signer,
            billing,
            tickets: RwLock::new(HashMap::new()),
            listings: RwLock::new(HashMap::new()),
            ticket_counter: RwLock::new(0),
        }
    }

    /// 生成工单 ID
    fn next_ticket_id(&self) -> String {
        let mut counter = self.ticket_counter.write();
        *counter += 1;
        format!("ticket-{}", *counter)
    }

    /// 提交审核（签名验证 → 创建工单，≤ 60s）
    ///
    /// 生产入口：`PluginMarketplaceOps::submit_for_review`。
    pub fn submit_for_review(&self, plugin: PluginPackage) -> Result<ReviewTicket, PluginError> {
        let start = Instant::now();

        // 签名验证（复用 PluginSigner::verify plugin.rs:423）
        let status = self
            .signer
            .verify(&plugin.content, &plugin.signature, &plugin.public_key);
        match status {
            SignatureStatus::Signed => {}
            SignatureStatus::Unsigned => {
                return Err(PluginError::SignatureInvalid(format!(
                    "插件 {} 未签名",
                    plugin.plugin_id
                )));
            }
            SignatureStatus::Invalid => {
                return Err(PluginError::SignatureInvalid(format!(
                    "插件 {} 签名无效",
                    plugin.plugin_id
                )));
            }
        }

        let ticket_id = self.next_ticket_id();
        let ticket = ReviewTicket {
            ticket_id: ticket_id.clone(),
            plugin_id: plugin.plugin_id.clone(),
            submitted_at: Utc::now().to_rfc3339(),
            status: ReviewStatus::Pending,
        };

        // 暂存插件包待审核（通过工单 ID 关联，简化为存 plugin_id）
        self.tickets.write().insert(ticket_id, ticket.clone());

        // 审核 ≤ 60s 守卫
        let elapsed = start.elapsed();
        if elapsed > std::time::Duration::from_secs(60) {
            return Err(PluginError::ReviewRejected("审核提交超时 60s".to_string()));
        }

        Ok(ticket)
    }

    /// 审核通过（上架 + 计费配置 + 版本管理）
    ///
    /// 生产入口：`PluginMarketplaceOps::approve`。
    pub fn approve(
        &self,
        ticket_id: &str,
        plugin: PluginPackage,
    ) -> Result<PluginListing, PluginError> {
        let mut tickets = self.tickets.write();
        let ticket = tickets
            .get_mut(ticket_id)
            .ok_or_else(|| PluginError::ReviewRejected(format!("工单 {} 不存在", ticket_id)))?;
        if ticket.status != ReviewStatus::Pending {
            return Err(PluginError::ReviewRejected(format!(
                "工单 {} 状态非 Pending",
                ticket_id
            )));
        }
        ticket.status = ReviewStatus::Approved;
        drop(tickets);

        // 计费配置
        self.billing
            .set_plugin_mode(&plugin.plugin_id, plugin.billing_mode.clone());

        // 版本管理（不丢历史）
        let mut listings = self.listings.write();
        let now = Utc::now().to_rfc3339();
        let listing = listings
            .entry(plugin.plugin_id.clone())
            .and_modify(|existing| {
                // 保留历史版本
                if !existing
                    .historical_versions
                    .contains(&existing.current_version)
                {
                    existing
                        .historical_versions
                        .push(existing.current_version.clone());
                }
                existing.current_version = plugin.version.clone();
                existing.billing_mode = plugin.billing_mode.clone();
            })
            .or_insert_with(|| PluginListing {
                plugin_id: plugin.plugin_id.clone(),
                name: plugin.name.clone(),
                current_version: plugin.version.clone(),
                historical_versions: Vec::new(),
                download_count: 0,
                billing_mode: plugin.billing_mode.clone(),
                listed_at: now,
            });
        Ok(listing.clone())
    }

    /// 审核拒绝
    pub fn reject(&self, ticket_id: &str, reason: &str) -> Result<(), PluginError> {
        let mut tickets = self.tickets.write();
        let ticket = tickets
            .get_mut(ticket_id)
            .ok_or_else(|| PluginError::ReviewRejected(format!("工单 {} 不存在", ticket_id)))?;
        if ticket.status != ReviewStatus::Pending {
            return Err(PluginError::ReviewRejected(format!(
                "工单 {} 状态非 Pending",
                ticket_id
            )));
        }
        ticket.status = ReviewStatus::Rejected(reason.to_string());
        Ok(())
    }

    /// 记录下载（下载统计）
    ///
    /// 生产入口：`PluginMarketplaceOps::record_download`。
    pub fn record_download(&self, plugin_id: &str) -> Result<u64, PluginError> {
        let mut listings = self.listings.write();
        let listing = listings
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
        listing.download_count += 1;
        Ok(listing.download_count)
    }

    /// 获取上架信息
    pub fn get_listing(&self, plugin_id: &str) -> Option<PluginListing> {
        self.listings.read().get(plugin_id).cloned()
    }

    /// 获取审核工单
    pub fn get_ticket(&self, ticket_id: &str) -> Option<ReviewTicket> {
        self.tickets.read().get(ticket_id).cloned()
    }

    /// 市场上架成功率（健康指标）
    pub fn list_success_rate(&self) -> f64 {
        let tickets = self.tickets.read();
        if tickets.is_empty() {
            return 1.0;
        }
        let approved = tickets
            .values()
            .filter(|t| t.status == ReviewStatus::Approved)
            .count();
        approved as f64 / tickets.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ops() -> PluginMarketplaceOps {
        let signer = Arc::new(PluginSigner::new(false));
        let billing = Arc::new(PluginBillingEngine::new(
            BillingMode::Free,
            b"marketplace-secret".to_vec(),
        ));
        PluginMarketplaceOps::new(signer, billing)
    }

    fn make_signed_package(id: &str, version: &str) -> PluginPackage {
        let content = b"plugin binary content".to_vec();
        let secret_key = b"signing-secret-key";
        let signer = PluginSigner::new(false);
        let signature = signer.sign(&content, secret_key);
        PluginPackage {
            plugin_id: id.to_string(),
            name: format!("plugin-{}", id),
            version: version.to_string(),
            content,
            signature,
            public_key: secret_key.to_vec(),
            billing_mode: BillingMode::Free,
            author: "test-author".to_string(),
        }
    }

    #[test]
    fn test_submit_signed_plugin_creates_ticket() {
        let ops = make_ops();
        let pkg = make_signed_package("p1", "1.0.0");
        let ticket = ops.submit_for_review(pkg).unwrap();
        assert_eq!(ticket.status, ReviewStatus::Pending);
        assert_eq!(ticket.plugin_id, "p1");
    }

    #[test]
    fn test_submit_unsigned_plugin_rejected() {
        let ops = make_ops();
        let mut pkg = make_signed_package("p2", "1.0.0");
        pkg.signature = Vec::new();
        let err = ops.submit_for_review(pkg).unwrap_err();
        assert!(matches!(err, PluginError::SignatureInvalid(_)));
    }

    #[test]
    fn test_submit_invalid_signature_rejected() {
        let ops = make_ops();
        let mut pkg = make_signed_package("p3", "1.0.0");
        pkg.signature = vec![0u8; 32];
        let err = ops.submit_for_review(pkg).unwrap_err();
        assert!(matches!(err, PluginError::SignatureInvalid(_)));
    }

    #[test]
    fn test_approve_creates_listing() {
        let ops = make_ops();
        let pkg = make_signed_package("p4", "1.0.0");
        let ticket = ops.submit_for_review(pkg.clone()).unwrap();
        let listing = ops.approve(&ticket.ticket_id, pkg).unwrap();
        assert_eq!(listing.plugin_id, "p4");
        assert_eq!(listing.current_version, "1.0.0");
        assert_eq!(listing.download_count, 0);
    }

    #[test]
    fn test_approve_nonexistent_ticket_rejected() {
        let ops = make_ops();
        let pkg = make_signed_package("p5", "1.0.0");
        let err = ops.approve("nonexistent", pkg).unwrap_err();
        assert!(matches!(err, PluginError::ReviewRejected(_)));
    }

    #[test]
    fn test_reject_ticket() {
        let ops = make_ops();
        let pkg = make_signed_package("p6", "1.0.0");
        let ticket = ops.submit_for_review(pkg).unwrap();
        ops.reject(&ticket.ticket_id, "违规内容").unwrap();
        let t = ops.get_ticket(&ticket.ticket_id).unwrap();
        assert!(matches!(t.status, ReviewStatus::Rejected(_)));
    }

    #[test]
    fn test_record_download_increments() {
        let ops = make_ops();
        let pkg = make_signed_package("p7", "1.0.0");
        let ticket = ops.submit_for_review(pkg.clone()).unwrap();
        ops.approve(&ticket.ticket_id, pkg).unwrap();
        let c1 = ops.record_download("p7").unwrap();
        assert_eq!(c1, 1);
        let c2 = ops.record_download("p7").unwrap();
        assert_eq!(c2, 2);
    }

    #[test]
    fn test_record_download_nonexistent_rejected() {
        let ops = make_ops();
        let err = ops.record_download("nonexistent").unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
    }

    #[test]
    fn test_version_history_preserved() {
        let ops = make_ops();
        // v1.0.0 上架
        let pkg1 = make_signed_package("p8", "1.0.0");
        let ticket1 = ops.submit_for_review(pkg1.clone()).unwrap();
        let listing1 = ops.approve(&ticket1.ticket_id, pkg1).unwrap();
        assert!(listing1.historical_versions.is_empty());
        // v1.1.0 升级
        let pkg2 = make_signed_package("p8", "1.1.0");
        let ticket2 = ops.submit_for_review(pkg2.clone()).unwrap();
        let listing2 = ops.approve(&ticket2.ticket_id, pkg2).unwrap();
        assert_eq!(listing2.current_version, "1.1.0");
        assert!(listing2.historical_versions.contains(&"1.0.0".to_string()));
    }

    #[test]
    fn test_review_within_60s() {
        let ops = make_ops();
        let pkg = make_signed_package("p9", "1.0.0");
        let start = Instant::now();
        let ticket = ops.submit_for_review(pkg.clone()).unwrap();
        ops.approve(&ticket.ticket_id, pkg).unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed <= std::time::Duration::from_secs(60),
            "审核耗时 {:?} > 60s",
            elapsed
        );
    }

    #[test]
    fn test_list_success_rate() {
        let ops = make_ops();
        // 无工单 → 1.0
        assert_eq!(ops.list_success_rate(), 1.0);
        // 1 通过 + 1 拒绝 = 0.5
        let pkg1 = make_signed_package("p10", "1.0.0");
        let t1 = ops.submit_for_review(pkg1.clone()).unwrap();
        ops.approve(&t1.ticket_id, pkg1).unwrap();
        let pkg2 = make_signed_package("p11", "1.0.0");
        let t2 = ops.submit_for_review(pkg2).unwrap();
        ops.reject(&t2.ticket_id, "测试拒绝").unwrap();
        assert_eq!(ops.list_success_rate(), 0.5);
    }

    #[test]
    fn test_billing_mode_configured_on_approve() {
        let ops = make_ops();
        let mut pkg = make_signed_package("p12", "1.0.0");
        pkg.billing_mode = BillingMode::Usage;
        let ticket = ops.submit_for_review(pkg.clone()).unwrap();
        let listing = ops.approve(&ticket.ticket_id, pkg).unwrap();
        assert_eq!(listing.billing_mode, BillingMode::Usage);
    }
}
