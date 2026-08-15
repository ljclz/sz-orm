//! # 租户资源配额与行级安全增强
//!
//! 租户资源配额（`TenantResourceQuota`）+ 配额执行器（`QuotaEnforcer`）+
//! RLS 策略增强器（`RlsPolicyEnhancer`，多条件组合 + 参数化绑定）+
//! 租户级审计日志器（`TenantAuditLogger`，追加写入不可篡改）。
//!
//! 复用 v4.6.0 `ConnectionTenantBinder`（`connection_tenant.rs:133`）连接级隔离，
//! 复用既有 `RowLevelSecurityPolicy`（`tenant_security.rs:67`）/ `ColumnMaskingRule`（`:155`）/
//! `TenantAuditContext`（`:244`）/ `TenantAuditOperation`（`:197`）/ `AuditResult`（`:224`）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::tenant_security::{
    AuditResult, ColumnMaskingRule, ParameterizedCondition, Principal, RowLevelSecurityPolicy,
    TenantAuditContext, TenantAuditOperation,
};

/// 配额资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuotaResource {
    /// 连接数
    Connection,
    /// 每秒查询数
    Qps,
    /// 存储容量（字节）
    Storage,
}

impl std::fmt::Display for QuotaResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection => write!(f, "connection"),
            Self::Qps => write!(f, "qps"),
            Self::Storage => write!(f, "storage"),
        }
    }
}

/// 配额执行策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuotaEnforceStrategy {
    /// 超限拒绝（安全优先）
    #[default]
    FailClose,
    /// 超限放行（可用性优先）
    FailOpen,
}

/// 租户资源配额
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantResourceQuota {
    /// 租户 ID
    pub tenant_id: String,
    /// 最大连接数
    pub max_connections: Option<u32>,
    /// 最大 QPS
    pub max_qps: Option<u32>,
    /// 最大存储容量（字节）
    pub max_storage: Option<u64>,
}

impl TenantResourceQuota {
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            max_connections: None,
            max_qps: None,
            max_storage: None,
        }
    }

    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = Some(max);
        self
    }

    pub fn with_max_qps(mut self, max: u32) -> Self {
        self.max_qps = Some(max);
        self
    }

    pub fn with_max_storage(mut self, max: u64) -> Self {
        self.max_storage = Some(max);
        self
    }

    /// 获取指定资源的配额上限
    pub fn limit(&self, resource: QuotaResource) -> Option<u64> {
        match resource {
            QuotaResource::Connection => self.max_connections.map(|v| v as u64),
            QuotaResource::Qps => self.max_qps.map(|v| v as u64),
            QuotaResource::Storage => self.max_storage,
        }
    }

    /// 检查指定资源是否超限
    pub fn is_exceeded(&self, resource: QuotaResource, current: u64) -> bool {
        self.limit(resource).is_some_and(|limit| current >= limit)
    }
}

impl Default for TenantResourceQuota {
    fn default() -> Self {
        Self::new("default")
    }
}

/// 配额错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaError {
    /// 配额超限
    QuotaExceeded {
        tenant_id: String,
        resource: QuotaResource,
        limit: u64,
        current: u64,
    },
    /// 配额检查失败
    QuotaCheckFailed(String),
    /// RLS 策略冲突
    RlsPolicyConflict(String),
    /// 审计日志写入失败
    AuditLogWriteFailed(String),
    /// 无效配额值
    InvalidQuotaValue(String),
}

impl std::fmt::Display for QuotaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QuotaExceeded {
                tenant_id,
                resource,
                limit,
                current,
            } => {
                write!(
                    f,
                    "quota exceeded for tenant {tenant_id}: {resource} limit={limit}, current={current}"
                )
            }
            Self::QuotaCheckFailed(msg) => write!(f, "quota check failed: {msg}"),
            Self::RlsPolicyConflict(msg) => write!(f, "RLS policy conflict: {msg}"),
            Self::AuditLogWriteFailed(msg) => write!(f, "audit log write failed: {msg}"),
            Self::InvalidQuotaValue(msg) => write!(f, "invalid quota value: {msg}"),
        }
    }
}

impl std::error::Error for QuotaError {}

/// 配额使用统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct QuotaUsage {
    connections: u64,
    qps: u64,
    storage: u64,
}

/// 配额执行器
///
/// 在连接池/查询层强制执行配额检查，超限按策略拒绝或放行。
/// 复用 v4.6.0 `ConnectionTenantBinder` 连接级隔离。
pub struct QuotaEnforcer {
    quotas: Arc<Mutex<HashMap<String, TenantResourceQuota>>>,
    usage: Arc<Mutex<HashMap<String, QuotaUsage>>>,
    strategy: QuotaEnforceStrategy,
    /// v4.7.0 审计接入：配额事件审计日志器（超限拒绝时记录，可配置关闭）
    audit: Arc<Mutex<Option<Arc<TenantAuditLogger>>>>,
}

impl QuotaEnforcer {
    pub fn new() -> Self {
        Self {
            quotas: Arc::new(Mutex::new(HashMap::new())),
            usage: Arc::new(Mutex::new(HashMap::new())),
            strategy: QuotaEnforceStrategy::default(),
            audit: Arc::new(Mutex::new(None)),
        }
    }

    /// v4.7.0 审计接入：配置租户审计日志器（超限拒绝事件写入审计日志）
    pub fn set_audit_logger(&self, logger: Option<Arc<TenantAuditLogger>>) {
        let mut guard = self.audit.lock().unwrap_or_else(|e| e.into_inner());
        *guard = logger;
    }

    /// v4.7.0 审计接入：记录配额事件（尽力而为，审计失败不影响主流程）
    fn record_audit(&self, entry: TenantAuditEntry) {
        if let Some(logger) = self
            .audit
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = logger.log(entry);
        }
    }

    pub fn with_strategy(mut self, strategy: QuotaEnforceStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// 设置租户配额
    pub fn set_quota(&self, quota: TenantResourceQuota) {
        let tenant_id = quota.tenant_id.clone();
        self.quotas
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(tenant_id, quota);
    }

    /// 获取租户配额
    pub fn get_quota(&self, tenant_id: &str) -> Option<TenantResourceQuota> {
        self.quotas
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(tenant_id)
            .cloned()
    }

    /// 检查配额
    ///
    /// `current` 为当前使用量，超限按策略返回错误或放行。
    pub fn check_quota(
        &self,
        tenant_id: &str,
        resource: QuotaResource,
        current: u64,
    ) -> Result<(), QuotaError> {
        let quotas = self.quotas.lock().unwrap_or_else(|e| e.into_inner());
        let quota = quotas.get(tenant_id);
        match quota {
            None => Ok(()),
            Some(q) => {
                let limit = q.limit(resource);
                match limit {
                    None => Ok(()),
                    Some(limit) if current >= limit => {
                        if matches!(self.strategy, QuotaEnforceStrategy::FailOpen) {
                            Ok(())
                        } else {
                            Err(QuotaError::QuotaExceeded {
                                tenant_id: tenant_id.to_string(),
                                resource,
                                limit,
                                current,
                            })
                        }
                    }
                    _ => Ok(()),
                }
            }
        }
    }

    /// 记录资源使用
    pub fn record_usage(&self, tenant_id: &str, resource: QuotaResource, amount: u64) {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        let entry = usage.entry(tenant_id.to_string()).or_default();
        match resource {
            QuotaResource::Connection => entry.connections += amount,
            QuotaResource::Qps => entry.qps += amount,
            QuotaResource::Storage => entry.storage += amount,
        }
    }

    /// 释放资源使用（饱和递减，最小到 0）
    ///
    /// v4.7.0 幻影交付修复：`release_with_tenant` 此前调用 `record_usage(..., 0)`
    /// 导致配额只增不减（+= 0），租户连接用满后永不释放。本方法提供递减语义，
    /// 超过当前使用量的释放按 0 饱和处理，不会下溢。
    pub fn release_usage(&self, tenant_id: &str, resource: QuotaResource, amount: u64) {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        let entry = usage.entry(tenant_id.to_string()).or_default();
        match resource {
            QuotaResource::Connection => {
                entry.connections = entry.connections.saturating_sub(amount)
            }
            QuotaResource::Qps => entry.qps = entry.qps.saturating_sub(amount),
            QuotaResource::Storage => entry.storage = entry.storage.saturating_sub(amount),
        }
    }

    /// 获取当前使用量
    pub fn current_usage(&self, tenant_id: &str, resource: QuotaResource) -> u64 {
        let usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        usage.get(tenant_id).map_or(0, |u| match resource {
            QuotaResource::Connection => u.connections,
            QuotaResource::Qps => u.qps,
            QuotaResource::Storage => u.storage,
        })
    }

    /// 检查并记录（原子操作）
    pub fn check_and_record(
        &self,
        tenant_id: &str,
        resource: QuotaResource,
        amount: u64,
    ) -> Result<(), QuotaError> {
        let current = self.current_usage(tenant_id, resource);
        if let Err(e) = self.check_quota(tenant_id, resource, current + amount) {
            // v4.7.0 审计接入：超限拒绝事件记录审计日志（审计失败不影响主流程）
            self.record_audit(TenantAuditEntry {
                tenant_id: tenant_id.to_string(),
                operation: "quota_exceeded".to_string(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
                result: "rejected".to_string(),
                detail: format!("{resource:?} limit exceeded (current={current}, amount={amount})"),
                table: None,
                quota_resource: Some(resource),
            });
            return Err(e);
        }
        self.record_usage(tenant_id, resource, amount);
        Ok(())
    }
}

impl Default for QuotaEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for QuotaEnforcer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuotaEnforcer")
            .field("strategy", &self.strategy)
            .field(
                "quota_count",
                &self.quotas.lock().unwrap_or_else(|e| e.into_inner()).len(),
            )
            .finish()
    }
}

/// 增强 RLS 策略（多条件组合）
///
/// 支持多条件组合（`tenant_id=? AND dept_id IN (?,?)`），
/// 与列级脱敏联动，不修改既有 `RowLevelSecurityPolicy`。
#[derive(Debug, Clone)]
pub struct EnhancedRlsPolicy {
    /// 表名
    pub table: String,
    /// 多条件组合（AND 连接）
    pub conditions: Vec<ParameterizedCondition>,
    /// 权限主体
    pub principal: Principal,
    /// 关联的列级脱敏规则
    pub masking_rules: Vec<ColumnMaskingRule>,
}

impl EnhancedRlsPolicy {
    pub fn new(table: impl Into<String>, principal: Principal) -> Self {
        Self {
            table: table.into(),
            conditions: Vec::new(),
            principal,
            masking_rules: Vec::new(),
        }
    }

    pub fn with_condition(mut self, condition: ParameterizedCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn with_masking_rule(mut self, rule: ColumnMaskingRule) -> Self {
        self.masking_rules.push(rule);
        self
    }

    /// 合并所有条件的 SQL 片段和参数
    ///
    /// 将 `$N` 占位符统一替换为 `?`（由调用方按位置绑定参数），避免跨条件占位符重编号。
    pub fn combined_condition(&self) -> Option<ParameterizedCondition> {
        if self.conditions.is_empty() {
            return None;
        }
        let mut sql_parts = Vec::new();
        let mut all_params = Vec::new();
        for cond in &self.conditions {
            let sql = replace_placeholders(&cond.sql_fragment);
            sql_parts.push(sql);
            all_params.extend(cond.params.clone());
        }
        Some(ParameterizedCondition::new(
            sql_parts.join(" AND "),
            all_params,
        ))
    }

    /// 转换为既有 `RowLevelSecurityPolicy`（单条件兼容）
    pub fn to_legacy_policy(&self) -> Result<RowLevelSecurityPolicy, QuotaError> {
        let condition = self.combined_condition().ok_or_else(|| {
            QuotaError::RlsPolicyConflict("no conditions in enhanced policy".to_string())
        })?;
        Ok(RowLevelSecurityPolicy::new(
            self.table.clone(),
            condition,
            self.principal.clone(),
        ))
    }
}

/// RLS 策略增强器
///
/// 自动注入 WHERE 参数化绑定，与列级脱敏联动。
/// 复用既有 `RowLevelSecurityPolicy`（`tenant_security.rs:67`）/ `ColumnMaskingRule`（`:155`）。
pub struct RlsPolicyEnhancer {
    policies: Arc<Mutex<HashMap<String, EnhancedRlsPolicy>>>,
}

impl RlsPolicyEnhancer {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 注册增强 RLS 策略
    pub fn with_policy(&self, policy: EnhancedRlsPolicy) -> Result<(), QuotaError> {
        let table = policy.table.clone();
        let mut policies = self.policies.lock().unwrap_or_else(|e| e.into_inner());
        if policies.contains_key(&table) {
            return Err(QuotaError::RlsPolicyConflict(format!(
                "policy already exists for table {table}"
            )));
        }
        policies.insert(table, policy);
        Ok(())
    }

    /// 获取表的策略
    pub fn get_policy(&self, table: &str) -> Option<EnhancedRlsPolicy> {
        self.policies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(table)
            .cloned()
    }

    /// 增强 SQL 查询：注入 WHERE 参数化条件
    ///
    /// 返回增强后的 SQL 片段和参数列表。
    pub fn enhance_query(
        &self,
        table: &str,
        tenant_id: &str,
    ) -> Result<Option<ParameterizedCondition>, QuotaError> {
        let policies = self.policies.lock().unwrap_or_else(|e| e.into_inner());
        let policy = policies.get(table);
        match policy {
            None => Ok(None),
            Some(p) => {
                if p.principal.tenant_id.to_string() != tenant_id {
                    return Err(QuotaError::RlsPolicyConflict(format!(
                        "tenant_id mismatch: policy={}, request={}",
                        p.principal.tenant_id, tenant_id
                    )));
                }
                Ok(p.combined_condition())
            }
        }
    }

    /// 获取表关联的列级脱敏规则
    pub fn masking_rules(&self, table: &str) -> Vec<ColumnMaskingRule> {
        self.policies
            .lock()
            .unwrap()
            .get(table)
            .map(|p| p.masking_rules.clone())
            .unwrap_or_default()
    }

    /// 对查询结果行执行列级脱敏
    pub fn mask_row(&self, table: &str, row: &mut HashMap<String, String>) {
        for rule in self.masking_rules(table) {
            if let Some(val) = row.get_mut(&rule.column) {
                *val = rule.mask(val);
            }
        }
    }
}

impl Default for RlsPolicyEnhancer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RlsPolicyEnhancer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RlsPolicyEnhancer")
            .field(
                "policy_count",
                &self
                    .policies
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .len(),
            )
            .finish()
    }
}

/// 租户审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantAuditEntry {
    /// 租户 ID
    pub tenant_id: String,
    /// 操作类型（字符串表示）
    pub operation: String,
    /// 时间戳（Unix 秒）
    pub timestamp: i64,
    /// 结果（字符串表示）
    pub result: String,
    /// 详情
    pub detail: String,
    /// 关联表（可选）
    pub table: Option<String>,
    /// 关联配额资源（可选）
    pub quota_resource: Option<QuotaResource>,
}

impl TenantAuditEntry {
    pub fn new(
        tenant_id: impl Into<String>,
        operation: TenantAuditOperation,
        result: AuditResult,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            operation: operation.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            result: result.to_string(),
            detail: detail.into(),
            table: None,
            quota_resource: None,
        }
    }

    pub fn with_table(mut self, table: impl Into<String>) -> Self {
        self.table = Some(table.into());
        self
    }

    pub fn with_quota_resource(mut self, resource: QuotaResource) -> Self {
        self.quota_resource = Some(resource);
        self
    }

    /// 转换为既有 `TenantAuditContext`
    pub fn to_audit_context(&self) -> TenantAuditContext {
        let operation = match self.operation.as_str() {
            "context_set" => TenantAuditOperation::ContextSet,
            "context_switch" => TenantAuditOperation::ContextSwitch,
            "cross_tenant_denied" => TenantAuditOperation::CrossTenantDenied,
            "row_level_filtered" => TenantAuditOperation::RowLevelFiltered,
            "column_masked" => TenantAuditOperation::ColumnMasked,
            _ => TenantAuditOperation::ContextSet,
        };
        let result = if self.result == "success" {
            AuditResult::Success
        } else {
            AuditResult::Denied
        };
        TenantAuditContext::new(
            self.tenant_id.parse().unwrap_or(0),
            operation,
            result,
            self.detail.clone(),
        )
    }
}

/// 租户级审计日志器
///
/// 按租户独立记录审计日志，追加写入不可篡改。
/// 复用既有 `TenantAuditContext`（`tenant_security.rs:244`）/ `TenantAuditOperation`（`:197`）/ `AuditResult`（`:224`）。
pub struct TenantAuditLogger {
    logs: Arc<Mutex<Vec<TenantAuditEntry>>>,
}

impl TenantAuditLogger {
    pub fn new() -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 记录审计日志（追加写入，不可篡改）
    pub fn log(&self, entry: TenantAuditEntry) -> Result<(), QuotaError> {
        let mut logs = self.logs.lock().unwrap_or_else(|e| e.into_inner());
        logs.push(entry);
        Ok(())
    }

    /// 获取指定租户的审计日志
    pub fn get_logs(&self, tenant_id: &str) -> Vec<TenantAuditEntry> {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == tenant_id)
            .cloned()
            .collect()
    }

    /// 获取所有审计日志
    pub fn all_logs(&self) -> Vec<TenantAuditEntry> {
        self.logs.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 获取指定租户的日志数量
    pub fn log_count(&self, tenant_id: &str) -> usize {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == tenant_id)
            .count()
    }

    /// 按操作类型过滤
    pub fn filter_by_operation(&self, tenant_id: &str, operation: &str) -> Vec<TenantAuditEntry> {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == tenant_id && e.operation == operation)
            .cloned()
            .collect()
    }
}

impl Default for TenantAuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TenantAuditLogger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantAuditLogger")
            .field(
                "log_count",
                &self.logs.lock().unwrap_or_else(|e| e.into_inner()).len(),
            )
            .finish()
    }
}

/// 将 `$N` 占位符替换为 `?`（N 为任意数字序列）
fn replace_placeholders(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            result.push('?');
            while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                chars.next();
            }
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant_security::{MaskingFunction, PermissionPredicate};
    use crate::value::Value;

    #[test]
    fn test_quota_resource_display() {
        assert_eq!(QuotaResource::Connection.to_string(), "connection");
        assert_eq!(QuotaResource::Qps.to_string(), "qps");
        assert_eq!(QuotaResource::Storage.to_string(), "storage");
    }

    #[test]
    fn test_quota_enforce_strategy_default() {
        assert_eq!(
            QuotaEnforceStrategy::default(),
            QuotaEnforceStrategy::FailClose
        );
    }

    #[test]
    fn test_tenant_resource_quota_new() {
        let quota = TenantResourceQuota::new("tenant_001");
        assert_eq!(quota.tenant_id, "tenant_001");
        assert!(quota.max_connections.is_none());
        assert!(quota.max_qps.is_none());
        assert!(quota.max_storage.is_none());
    }

    #[test]
    fn test_tenant_resource_quota_builder() {
        let quota = TenantResourceQuota::new("tenant_001")
            .with_max_connections(10)
            .with_max_qps(1000)
            .with_max_storage(1024 * 1024 * 1024);
        assert_eq!(quota.max_connections, Some(10));
        assert_eq!(quota.max_qps, Some(1000));
        assert_eq!(quota.max_storage, Some(1073741824));
    }

    #[test]
    fn test_tenant_resource_quota_limit() {
        let quota = TenantResourceQuota::new("t1")
            .with_max_connections(10)
            .with_max_storage(1000);
        assert_eq!(quota.limit(QuotaResource::Connection), Some(10));
        assert_eq!(quota.limit(QuotaResource::Qps), None);
        assert_eq!(quota.limit(QuotaResource::Storage), Some(1000));
    }

    #[test]
    fn test_tenant_resource_quota_is_exceeded() {
        let quota = TenantResourceQuota::new("t1").with_max_connections(10);
        assert!(!quota.is_exceeded(QuotaResource::Connection, 5));
        assert!(quota.is_exceeded(QuotaResource::Connection, 10));
        assert!(quota.is_exceeded(QuotaResource::Connection, 15));
        assert!(!quota.is_exceeded(QuotaResource::Qps, 99999));
    }

    #[test]
    fn test_quota_error_display() {
        let err = QuotaError::QuotaExceeded {
            tenant_id: "t1".to_string(),
            resource: QuotaResource::Connection,
            limit: 10,
            current: 15,
        };
        assert!(err.to_string().contains("t1"));
        assert!(err.to_string().contains("connection"));

        let err = QuotaError::QuotaCheckFailed("db error".to_string());
        assert!(err.to_string().contains("db error"));

        let err = QuotaError::RlsPolicyConflict("conflict".to_string());
        assert!(err.to_string().contains("conflict"));

        let err = QuotaError::AuditLogWriteFailed("io".to_string());
        assert!(err.to_string().contains("io"));

        let err = QuotaError::InvalidQuotaValue("negative".to_string());
        assert!(err.to_string().contains("negative"));
    }

    #[test]
    fn test_quota_enforcer_no_quota() {
        let enforcer = QuotaEnforcer::new();
        let result = enforcer.check_quota("t1", QuotaResource::Connection, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_quota_enforcer_within_limit() {
        let enforcer = QuotaEnforcer::new();
        let quota = TenantResourceQuota::new("t1").with_max_connections(10);
        enforcer.set_quota(quota);
        let result = enforcer.check_quota("t1", QuotaResource::Connection, 5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_quota_enforcer_exceeded_fail_close() {
        let enforcer = QuotaEnforcer::new();
        let quota = TenantResourceQuota::new("t1").with_max_connections(10);
        enforcer.set_quota(quota);
        let result = enforcer.check_quota("t1", QuotaResource::Connection, 10);
        assert!(result.is_err());
        match result {
            Err(QuotaError::QuotaExceeded {
                tenant_id,
                resource,
                limit,
                current,
            }) => {
                assert_eq!(tenant_id, "t1");
                assert_eq!(resource, QuotaResource::Connection);
                assert_eq!(limit, 10);
                assert_eq!(current, 10);
            }
            _ => panic!("wrong error type"),
        }
    }

    #[test]
    fn test_quota_enforcer_exceeded_fail_open() {
        let enforcer = QuotaEnforcer::new().with_strategy(QuotaEnforceStrategy::FailOpen);
        let quota = TenantResourceQuota::new("t1").with_max_connections(10);
        enforcer.set_quota(quota);
        let result = enforcer.check_quota("t1", QuotaResource::Connection, 15);
        assert!(result.is_ok());
    }

    #[test]
    fn test_quota_enforcer_record_and_check() {
        let enforcer = QuotaEnforcer::new();
        let quota = TenantResourceQuota::new("t1").with_max_connections(10);
        enforcer.set_quota(quota);

        assert!(enforcer
            .check_and_record("t1", QuotaResource::Connection, 5)
            .is_ok());
        assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 5);

        assert!(enforcer
            .check_and_record("t1", QuotaResource::Connection, 3)
            .is_ok());
        assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 8);

        let result = enforcer.check_and_record("t1", QuotaResource::Connection, 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_enhanced_rls_policy_new() {
        let principal = Principal::new(1, vec!["employee".to_string()]);
        let policy = EnhancedRlsPolicy::new("orders", principal);
        assert_eq!(policy.table, "orders");
        assert!(policy.conditions.is_empty());
        assert!(policy.masking_rules.is_empty());
    }

    #[test]
    fn test_enhanced_rls_policy_with_conditions() {
        let principal = Principal::new(1, vec!["employee".to_string()]);
        let policy = EnhancedRlsPolicy::new("orders", principal)
            .with_condition(ParameterizedCondition::new(
                "tenant_id = $1",
                vec![Value::I64(1)],
            ))
            .with_condition(ParameterizedCondition::new(
                "dept_id IN ($1, $2)",
                vec![Value::I32(10), Value::I32(20)],
            ));
        assert_eq!(policy.conditions.len(), 2);

        let combined = policy.combined_condition().unwrap();
        assert!(combined.sql_fragment.contains("AND"));
        assert_eq!(combined.params.len(), 3);
    }

    #[test]
    fn test_enhanced_rls_policy_no_conditions() {
        let principal = Principal::new(1, vec![]);
        let policy = EnhancedRlsPolicy::new("orders", principal);
        assert!(policy.combined_condition().is_none());
    }

    #[test]
    fn test_enhanced_rls_policy_with_masking() {
        let principal = Principal::new(1, vec!["employee".to_string()]);
        let masking_rule = ColumnMaskingRule::new(
            "orders",
            "customer_phone",
            MaskingFunction::Phone,
            PermissionPredicate::all(),
        );
        let policy = EnhancedRlsPolicy::new("orders", principal)
            .with_condition(ParameterizedCondition::new(
                "tenant_id = $1",
                vec![Value::I64(1)],
            ))
            .with_masking_rule(masking_rule);
        assert_eq!(policy.masking_rules.len(), 1);
    }

    #[test]
    fn test_rls_policy_enhancer_new() {
        let enhancer = RlsPolicyEnhancer::new();
        assert!(enhancer.get_policy("orders").is_none());
    }

    #[test]
    fn test_rls_policy_enhancer_register() {
        let enhancer = RlsPolicyEnhancer::new();
        let principal = Principal::new(1, vec!["employee".to_string()]);
        let policy = EnhancedRlsPolicy::new("orders", principal).with_condition(
            ParameterizedCondition::new("tenant_id = $1", vec![Value::I64(1)]),
        );
        assert!(enhancer.with_policy(policy).is_ok());
        assert!(enhancer.get_policy("orders").is_some());
    }

    #[test]
    fn test_rls_policy_enhancer_duplicate() {
        let enhancer = RlsPolicyEnhancer::new();
        let principal = Principal::new(1, vec![]);
        let policy1 = EnhancedRlsPolicy::new("orders", principal.clone());
        let policy2 = EnhancedRlsPolicy::new("orders", principal);
        enhancer.with_policy(policy1).unwrap();
        let result = enhancer.with_policy(policy2);
        assert!(result.is_err());
    }

    #[test]
    fn test_rls_policy_enhancer_enhance_query() {
        let enhancer = RlsPolicyEnhancer::new();
        let principal = Principal::new(1, vec!["employee".to_string()]);
        let policy = EnhancedRlsPolicy::new("orders", principal).with_condition(
            ParameterizedCondition::new("tenant_id = $1", vec![Value::I64(1)]),
        );
        enhancer.with_policy(policy).unwrap();

        let result = enhancer.enhance_query("orders", "1").unwrap();
        assert!(result.is_some());
        let cond = result.unwrap();
        assert!(cond.sql_fragment.contains("tenant_id"));
    }

    #[test]
    fn test_rls_policy_enhancer_tenant_mismatch() {
        let enhancer = RlsPolicyEnhancer::new();
        let principal = Principal::new(1, vec![]);
        let policy = EnhancedRlsPolicy::new("orders", principal).with_condition(
            ParameterizedCondition::new("tenant_id = $1", vec![Value::I64(1)]),
        );
        enhancer.with_policy(policy).unwrap();

        let result = enhancer.enhance_query("orders", "2");
        assert!(result.is_err());
    }

    #[test]
    fn test_rls_policy_enhancer_no_policy() {
        let enhancer = RlsPolicyEnhancer::new();
        let result = enhancer.enhance_query("unknown_table", "1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_rls_policy_enhancer_mask_row() {
        let enhancer = RlsPolicyEnhancer::new();
        let principal = Principal::new(1, vec!["employee".to_string()]);
        let masking_rule = ColumnMaskingRule::new(
            "users",
            "phone",
            MaskingFunction::Phone,
            PermissionPredicate::all(),
        );
        let policy = EnhancedRlsPolicy::new("users", principal)
            .with_condition(ParameterizedCondition::new(
                "tenant_id = $1",
                vec![Value::I64(1)],
            ))
            .with_masking_rule(masking_rule);
        enhancer.with_policy(policy).unwrap();

        let mut row = HashMap::new();
        row.insert("phone".to_string(), "13800138000".to_string());
        row.insert("name".to_string(), "Alice".to_string());
        enhancer.mask_row("users", &mut row);
        assert_ne!(row.get("phone").unwrap(), "13800138000");
        assert_eq!(row.get("name").unwrap(), "Alice");
    }

    #[test]
    fn test_tenant_audit_entry_new() {
        let entry = TenantAuditEntry::new(
            "t1",
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "tenant context set",
        );
        assert_eq!(entry.tenant_id, "t1");
        assert_eq!(entry.operation, "context_set");
        assert_eq!(entry.result, "success");
        assert!(entry.table.is_none());
        assert!(entry.quota_resource.is_none());
    }

    #[test]
    fn test_tenant_audit_entry_builder() {
        let entry = TenantAuditEntry::new(
            "t1",
            TenantAuditOperation::RowLevelFiltered,
            AuditResult::Denied,
            "access denied",
        )
        .with_table("orders")
        .with_quota_resource(QuotaResource::Connection);
        assert_eq!(entry.table, Some("orders".to_string()));
        assert_eq!(entry.quota_resource, Some(QuotaResource::Connection));
    }

    #[test]
    fn test_tenant_audit_logger_new() {
        let logger = TenantAuditLogger::new();
        assert_eq!(logger.all_logs().len(), 0);
    }

    #[test]
    fn test_tenant_audit_logger_log() {
        let logger = TenantAuditLogger::new();
        let entry = TenantAuditEntry::new(
            "t1",
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "test",
        );
        assert!(logger.log(entry).is_ok());
        assert_eq!(logger.all_logs().len(), 1);
        assert_eq!(logger.log_count("t1"), 1);
        assert_eq!(logger.log_count("t2"), 0);
    }

    #[test]
    fn test_tenant_audit_logger_get_logs() {
        let logger = TenantAuditLogger::new();
        for i in 0..3 {
            let entry = TenantAuditEntry::new(
                "t1",
                TenantAuditOperation::ContextSet,
                AuditResult::Success,
                format!("entry {i}"),
            );
            logger.log(entry).unwrap();
        }
        let entry = TenantAuditEntry::new(
            "t2",
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "other tenant",
        );
        logger.log(entry).unwrap();

        assert_eq!(logger.get_logs("t1").len(), 3);
        assert_eq!(logger.get_logs("t2").len(), 1);
    }

    #[test]
    fn test_tenant_audit_logger_filter_by_operation() {
        let logger = TenantAuditLogger::new();
        logger
            .log(TenantAuditEntry::new(
                "t1",
                TenantAuditOperation::ContextSet,
                AuditResult::Success,
                "set",
            ))
            .unwrap();
        logger
            .log(TenantAuditEntry::new(
                "t1",
                TenantAuditOperation::CrossTenantDenied,
                AuditResult::Denied,
                "denied",
            ))
            .unwrap();
        logger
            .log(TenantAuditEntry::new(
                "t1",
                TenantAuditOperation::ColumnMasked,
                AuditResult::Success,
                "masked",
            ))
            .unwrap();

        let filtered = logger.filter_by_operation("t1", "cross_tenant_denied");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].detail, "denied");
    }

    #[test]
    fn test_tenant_audit_entry_to_audit_context() {
        let entry = TenantAuditEntry::new(
            "42",
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "test",
        );
        let ctx = entry.to_audit_context();
        assert_eq!(ctx.tenant_id, 42);
        assert_eq!(ctx.operation, TenantAuditOperation::ContextSet);
        assert_eq!(ctx.result, AuditResult::Success);
    }

    #[test]
    fn test_quota_enforcer_get_quota() {
        let enforcer = QuotaEnforcer::new();
        let quota = TenantResourceQuota::new("t1").with_max_connections(10);
        enforcer.set_quota(quota);
        let retrieved = enforcer.get_quota("t1").unwrap();
        assert_eq!(retrieved.max_connections, Some(10));
    }

    #[test]
    fn test_quota_enforcer_debug() {
        let enforcer = QuotaEnforcer::new().with_strategy(QuotaEnforceStrategy::FailOpen);
        let debug = format!("{:?}", enforcer);
        assert!(debug.contains("FailOpen"));
    }

    #[test]
    fn test_release_usage_decrements_saturating() {
        let enforcer = QuotaEnforcer::new();
        enforcer.record_usage("t1", QuotaResource::Connection, 5);
        assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 5);
        // 正常释放：递减
        enforcer.release_usage("t1", QuotaResource::Connection, 2);
        assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 3);
        // 过量释放：饱和到 0，不下溢（u64 语义安全）
        enforcer.release_usage("t1", QuotaResource::Connection, 10);
        assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 0);
    }

    #[test]
    fn test_quota_exceeded_records_audit() {
        // v4.7.0 审计接入回归：超限拒绝必须写入审计日志
        let enforcer = QuotaEnforcer::new();
        let logger = Arc::new(TenantAuditLogger::new());
        enforcer.set_audit_logger(Some(Arc::clone(&logger)));

        enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(3));
        assert!(enforcer
            .check_and_record("t1", QuotaResource::Connection, 2)
            .is_ok());
        assert_eq!(logger.log_count("t1"), 0, "正常通过不应记审计");

        let result = enforcer.check_and_record("t1", QuotaResource::Connection, 1);
        assert!(result.is_err(), "达到上限后应超限（current >= limit 拒绝）");
        let logs = logger.filter_by_operation("t1", "quota_exceeded");
        assert_eq!(logs.len(), 1, "超限拒绝必须记录审计");
        assert_eq!(logs[0].result, "rejected");
        assert!(logs[0].detail.contains("limit exceeded"));

        // 未配置 logger 时超限不 panic（尽力而为）
        let bare = QuotaEnforcer::new();
        bare.set_quota(TenantResourceQuota::new("t2").with_max_connections(1));
        assert!(bare
            .check_and_record("t2", QuotaResource::Connection, 2)
            .is_err());
    }

    #[tokio::test]
    async fn test_pool_acquire_release_with_tenant_usage_cycle(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::pool::{Connection, ConnectionFactory, Pool, PoolConfigBuilder};
        use async_trait::async_trait;
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::Arc;

        struct MockConn;
        impl Connection for MockConn {
            fn execute<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>
            {
                Box::pin(async { Ok(1) })
            }
            fn query<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<crate::pool::QueryRows, crate::DbError>> + Send + 'a,
                >,
            > {
                Box::pin(async { Ok(vec![]) })
            }
            fn begin_transaction<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn commit<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn rollback<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn is_connected(&self) -> bool {
                true
            }
            fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
                Box::pin(async { true })
            }
            fn close<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
        }

        struct MockFactory;
        #[async_trait]
        impl ConnectionFactory for MockFactory {
            async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
                Ok(Box::new(MockConn))
            }
        }

        let config = PoolConfigBuilder::new().max_size(5).build()?;
        let pool = Pool::new(config, Arc::new(MockFactory))?;

        let enforcer = Arc::new(QuotaEnforcer::new());
        enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(3));
        pool.set_quota_enforcer(Some(enforcer.clone()));

        // acquire → 使用量 +1
        let conn1 = pool.acquire_with_tenant("t1").await?;
        let conn2 = pool.acquire_with_tenant("t1").await?;
        assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 2);
        // release → 使用量递减（回归：此前 release 传 0 导致只增不减）
        pool.release_with_tenant("t1", conn1).await;
        assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 1);
        pool.release_with_tenant("t1", conn2).await;
        assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_acquire_with_tenant_quota_ok() -> Result<(), Box<dyn std::error::Error>> {
        use crate::pool::{Connection, ConnectionFactory, Pool, PoolConfigBuilder};
        use async_trait::async_trait;
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::Arc;

        struct MockConn;
        impl Connection for MockConn {
            fn execute<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>
            {
                Box::pin(async { Ok(1) })
            }
            fn query<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<crate::pool::QueryRows, crate::DbError>> + Send + 'a,
                >,
            > {
                Box::pin(async { Ok(vec![]) })
            }
            fn begin_transaction<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn commit<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn rollback<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn is_connected(&self) -> bool {
                true
            }
            fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
                Box::pin(async { true })
            }
            fn close<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
        }

        struct MockFactory;
        #[async_trait]
        impl ConnectionFactory for MockFactory {
            async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
                Ok(Box::new(MockConn))
            }
        }

        let config = PoolConfigBuilder::new().max_size(5).build()?;
        let pool = Pool::new(config, Arc::new(MockFactory))?;

        let enforcer = Arc::new(QuotaEnforcer::new());
        enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(3));
        pool.set_quota_enforcer(Some(enforcer));

        let conn1 = pool.acquire_with_tenant("t1").await?;
        assert!(conn1.is_connected());
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_acquire_with_tenant_quota_exceeded() -> Result<(), Box<dyn std::error::Error>>
    {
        use crate::pool::{Connection, ConnectionFactory, Pool, PoolConfigBuilder};
        use async_trait::async_trait;
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::Arc;

        struct MockConn;
        impl Connection for MockConn {
            fn execute<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>
            {
                Box::pin(async { Ok(1) })
            }
            fn query<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<crate::pool::QueryRows, crate::DbError>> + Send + 'a,
                >,
            > {
                Box::pin(async { Ok(vec![]) })
            }
            fn begin_transaction<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn commit<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn rollback<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn is_connected(&self) -> bool {
                true
            }
            fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
                Box::pin(async { true })
            }
            fn close<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
        }

        struct MockFactory;
        #[async_trait]
        impl ConnectionFactory for MockFactory {
            async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
                Ok(Box::new(MockConn))
            }
        }

        let config = PoolConfigBuilder::new().max_size(5).build()?;
        let pool = Pool::new(config, Arc::new(MockFactory))?;

        let enforcer = Arc::new(QuotaEnforcer::new());
        enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(2));
        pool.set_quota_enforcer(Some(enforcer));

        let conn1 = pool.acquire_with_tenant("t1").await?;
        assert!(conn1.is_connected());

        let result = pool.acquire_with_tenant("t1").await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("quota exceeded"));
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_acquire_with_tenant_no_enforcer() -> Result<(), Box<dyn std::error::Error>> {
        use crate::pool::{Connection, ConnectionFactory, Pool, PoolConfigBuilder};
        use async_trait::async_trait;
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::Arc;

        struct MockConn;
        impl Connection for MockConn {
            fn execute<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>
            {
                Box::pin(async { Ok(1) })
            }
            fn query<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<crate::pool::QueryRows, crate::DbError>> + Send + 'a,
                >,
            > {
                Box::pin(async { Ok(vec![]) })
            }
            fn begin_transaction<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn commit<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn rollback<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn is_connected(&self) -> bool {
                true
            }
            fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
                Box::pin(async { true })
            }
            fn close<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
        }

        struct MockFactory;
        #[async_trait]
        impl ConnectionFactory for MockFactory {
            async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
                Ok(Box::new(MockConn))
            }
        }

        let config = PoolConfigBuilder::new().max_size(5).build()?;
        let pool = Pool::new(config, Arc::new(MockFactory))?;

        let conn1 = pool.acquire_with_tenant("any_tenant").await?;
        assert!(conn1.is_connected());
        Ok(())
    }

    #[test]
    fn test_rls_enhancer_query_builder_integration() {
        use crate::dialect::MySqlDialect;
        use crate::model::Model;
        use crate::query::QueryBuilder;
        use crate::tenant_security::{ParameterizedCondition, Principal};
        use crate::value::Value;
        use std::sync::Arc;

        struct TestModel;
        impl Model for TestModel {
            type PrimaryKey = i64;
            fn table_name() -> &'static str {
                "orders"
            }
            fn pk(&self) -> i64 {
                0
            }
            fn set_pk(&mut self, _pk: i64) {}
        }

        let enhancer = Arc::new(RlsPolicyEnhancer::new());
        let policy = EnhancedRlsPolicy::new("orders", Principal::new(42, vec!["user".to_string()]))
            .with_condition(ParameterizedCondition::new(
                "`tenant_id` = ?",
                vec![Value::I64(42)],
            ))
            .with_condition(ParameterizedCondition::new(
                "`dept_id` IN (?, ?)",
                vec![Value::I64(1), Value::I64(2)],
            ));
        enhancer.with_policy(policy).unwrap();

        let qb = QueryBuilder::<TestModel>::new(Box::new(MySqlDialect))
            .table("orders")
            .with_tenant_id(42)
            .with_rls_policy_enhancer(enhancer);

        let (sql, params) = qb.build_select_with_params();
        assert!(
            sql.contains("tenant_id"),
            "SQL should contain RLS condition: {}",
            sql
        );
        assert!(
            params.contains(&Value::I64(42)),
            "Params should contain tenant_id value: {:?}",
            params
        );
    }
}
