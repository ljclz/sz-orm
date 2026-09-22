//! PluginBillingEngine — 插件计费引擎（v8.1.0 组 6：插件市场运营）
//!
//! 计费模式配置 → 用量统计 → 签名防篡改 → 审计记录。
//! 复用 `sz_orm_crypto::hmac_sha256` 实现计费记录签名防篡改。

use crate::plugin::PluginError;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_crypto::hmac_sha256;

use parking_lot::RwLock;

/// 计费模式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillingMode {
    /// 免费
    Free,
    /// 一次性付费
    OneTime,
    /// 订阅制
    Subscription,
    /// 按用量计费
    Usage,
}

/// 用量数据
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// 调用次数
    pub call_count: u64,
    /// 订阅月数（Subscription 模式生效）
    pub subscription_months: u64,
    /// 单次价格（分，Usage/OneTime 模式生效）
    pub unit_price_cents: u64,
}

/// 计费记录
#[derive(Debug, Clone)]
pub struct BillingRecord {
    /// 插件 ID
    pub plugin_id: String,
    /// 计费金额（分）
    pub amount_cents: u64,
    /// 计费模式
    pub mode: BillingMode,
    /// HMAC-SHA256 签名（32 字节，防篡改）
    pub signature: Vec<u8>,
    /// 时间戳（UTC RFC3339）
    pub timestamp: String,
    /// 用量快照
    pub usage: Usage,
}

/// 计费审计条目
#[derive(Debug, Clone)]
pub struct BillingAuditEntry {
    /// 记录
    pub record: BillingRecord,
    /// 验证结果
    pub verified: bool,
}

/// 插件计费引擎
///
/// 计费模式配置 → 用量统计 → 签名防篡改 → 审计记录。
/// 生产入口：`PluginBillingEngine::charge`。
pub struct PluginBillingEngine {
    mode: BillingMode,
    /// 插件 → 计费模式覆盖（可选，按插件定制）
    plugin_modes: RwLock<HashMap<String, BillingMode>>,
    /// 签名密钥
    secret_key: Arc<Vec<u8>>,
    /// 审计日志
    audit_log: RwLock<Vec<BillingAuditEntry>>,
}

impl PluginBillingEngine {
    /// 创建计费引擎
    pub fn new(mode: BillingMode, secret_key: Vec<u8>) -> Self {
        Self {
            mode,
            plugin_modes: RwLock::new(HashMap::new()),
            secret_key: Arc::new(secret_key),
            audit_log: RwLock::new(Vec::new()),
        }
    }

    /// 为指定插件设置定制计费模式
    pub fn set_plugin_mode(&self, plugin_id: &str, mode: BillingMode) {
        self.plugin_modes
            .write()
            .insert(plugin_id.to_string(), mode);
    }

    /// 获取插件计费模式（优先插件定制，回退全局）
    pub fn mode_for(&self, plugin_id: &str) -> BillingMode {
        self.plugin_modes
            .read()
            .get(plugin_id)
            .cloned()
            .unwrap_or_else(|| self.mode.clone())
    }

    /// 计算计费金额（分）
    fn compute_amount(&self, plugin_id: &str, usage: &Usage) -> u64 {
        match self.mode_for(plugin_id) {
            BillingMode::Free => 0,
            BillingMode::OneTime => usage.unit_price_cents,
            BillingMode::Subscription => usage.subscription_months * usage.unit_price_cents,
            BillingMode::Usage => usage.call_count * usage.unit_price_cents,
        }
    }

    /// 对计费记录签名（HMAC-SHA256）
    fn sign_record(&self, plugin_id: &str, amount_cents: u64, timestamp: &str) -> Vec<u8> {
        let mut message = Vec::with_capacity(plugin_id.len() + 24);
        message.extend_from_slice(plugin_id.as_bytes());
        message.extend_from_slice(&amount_cents.to_le_bytes());
        message.extend_from_slice(timestamp.as_bytes());
        hmac_sha256(&self.secret_key, &message).to_vec()
    }

    /// 验证计费记录签名（防篡改）
    pub fn verify_record(&self, record: &BillingRecord) -> bool {
        let expected = self.sign_record(&record.plugin_id, record.amount_cents, &record.timestamp);
        expected == record.signature
    }

    /// 计费（≤ 10ms）
    ///
    /// 生产入口：`PluginBillingEngine::charge`。
    /// 流程：计费模式 → 用量统计 → 签名防篡改 → 审计记录。
    pub fn charge(&self, plugin_id: &str, usage: Usage) -> Result<BillingRecord, PluginError> {
        if plugin_id.is_empty() {
            return Err(PluginError::BillingFailed("插件 ID 为空".to_string()));
        }
        let amount_cents = self.compute_amount(plugin_id, &usage);
        let timestamp = Utc::now().to_rfc3339();
        let signature = self.sign_record(plugin_id, amount_cents, &timestamp);
        let mode = self.mode_for(plugin_id);
        let record = BillingRecord {
            plugin_id: plugin_id.to_string(),
            amount_cents,
            mode,
            signature,
            timestamp,
            usage,
        };
        // 审计记录
        let verified = self.verify_record(&record);
        if !verified {
            return Err(PluginError::BillingTampered(
                "签名自校验失败（不可达）".to_string(),
            ));
        }
        self.audit_log.write().push(BillingAuditEntry {
            record: record.clone(),
            verified,
        });
        Ok(record)
    }

    /// 审计日志条数
    pub fn audit_count(&self) -> usize {
        self.audit_log.read().len()
    }

    /// 获取审计日志快照
    pub fn audit_snapshot(&self) -> Vec<BillingAuditEntry> {
        self.audit_log.read().clone()
    }

    /// 检测篡改：审计日志中所有记录签名验证
    pub fn detect_tamper(&self) -> Result<(), PluginError> {
        for entry in self.audit_log.read().iter() {
            if !self.verify_record(&entry.record) {
                return Err(PluginError::BillingTampered(format!(
                    "插件 {} 计费记录被篡改",
                    entry.record.plugin_id
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine(mode: BillingMode) -> PluginBillingEngine {
        PluginBillingEngine::new(mode, b"billing-secret-key".to_vec())
    }

    #[test]
    fn test_free_mode_zero_charge() {
        let engine = make_engine(BillingMode::Free);
        let usage = Usage {
            call_count: 100,
            subscription_months: 1,
            unit_price_cents: 1000,
        };
        let record = engine.charge("free-plugin", usage).unwrap();
        assert_eq!(record.amount_cents, 0);
        assert_eq!(record.mode, BillingMode::Free);
    }

    #[test]
    fn test_one_time_mode() {
        let engine = make_engine(BillingMode::OneTime);
        let usage = Usage {
            call_count: 0,
            subscription_months: 0,
            unit_price_cents: 5000,
        };
        let record = engine.charge("one-time-plugin", usage).unwrap();
        assert_eq!(record.amount_cents, 5000);
        assert_eq!(record.mode, BillingMode::OneTime);
    }

    #[test]
    fn test_subscription_mode() {
        let engine = make_engine(BillingMode::Subscription);
        let usage = Usage {
            call_count: 0,
            subscription_months: 3,
            unit_price_cents: 1000,
        };
        let record = engine.charge("sub-plugin", usage).unwrap();
        assert_eq!(record.amount_cents, 3000);
        assert_eq!(record.mode, BillingMode::Subscription);
    }

    #[test]
    fn test_usage_mode() {
        let engine = make_engine(BillingMode::Usage);
        let usage = Usage {
            call_count: 50,
            subscription_months: 0,
            unit_price_cents: 10,
        };
        let record = engine.charge("usage-plugin", usage).unwrap();
        assert_eq!(record.amount_cents, 500);
        assert_eq!(record.mode, BillingMode::Usage);
    }

    #[test]
    fn test_signature_tamper_detected() {
        let engine = make_engine(BillingMode::Usage);
        let usage = Usage {
            call_count: 10,
            subscription_months: 0,
            unit_price_cents: 100,
        };
        let mut record = engine.charge("tamper-plugin", usage).unwrap();
        // 篡改金额
        record.amount_cents = 999999;
        assert!(!engine.verify_record(&record));
    }

    #[test]
    fn test_audit_recorded() {
        let engine = make_engine(BillingMode::OneTime);
        let usage = Usage {
            call_count: 0,
            subscription_months: 0,
            unit_price_cents: 1000,
        };
        engine.charge("audited-plugin", usage).unwrap();
        assert_eq!(engine.audit_count(), 1);
    }

    #[test]
    fn test_detect_tamper_clean() {
        let engine = make_engine(BillingMode::Usage);
        let usage = Usage {
            call_count: 5,
            subscription_months: 0,
            unit_price_cents: 20,
        };
        engine.charge("clean-plugin", usage.clone()).unwrap();
        engine.charge("clean-plugin-2", usage).unwrap();
        engine.detect_tamper().unwrap();
    }

    #[test]
    fn test_plugin_specific_mode_override() {
        let engine = make_engine(BillingMode::Free);
        engine.set_plugin_mode("premium-plugin", BillingMode::Usage);
        let usage = Usage {
            call_count: 10,
            subscription_months: 0,
            unit_price_cents: 50,
        };
        let record = engine.charge("premium-plugin", usage.clone()).unwrap();
        assert_eq!(record.amount_cents, 500);
        assert_eq!(record.mode, BillingMode::Usage);
        // 其他插件仍为 Free
        let record2 = engine.charge("other-plugin", usage).unwrap();
        assert_eq!(record2.amount_cents, 0);
    }

    #[test]
    fn test_empty_plugin_id_rejected() {
        let engine = make_engine(BillingMode::Free);
        let err = engine.charge("", Usage::default()).unwrap_err();
        assert!(matches!(err, PluginError::BillingFailed(_)));
    }

    #[test]
    fn test_charge_latency_within_10ms() {
        let engine = make_engine(BillingMode::Usage);
        let usage = Usage {
            call_count: 100,
            subscription_months: 0,
            unit_price_cents: 10,
        };
        let start = std::time::Instant::now();
        engine.charge("latency-plugin", usage).unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed <= std::time::Duration::from_millis(10),
            "计费耗时 {:?} > 10ms",
            elapsed
        );
    }
}
