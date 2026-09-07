//! v6.6.0 多租户连接池隔离
//!
//! 按租户隔离连接池，支持动态扩缩容和配额限制。
//!
//! # 特性
//! - 每个租户独立连接池（互不干扰）
//! - 动态扩缩容（按负载调整连接数）
//! - 配额限制（最大连接数 + 查询速率限制）

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 租户 ID
pub type TenantId = i64;

/// 租户配额
#[derive(Debug, Clone)]
pub struct TenantQuota {
    /// 最大连接数
    pub max_connections: usize,
    /// 每秒最大查询数（None = 无限制）
    pub max_queries_per_second: Option<u32>,
}

impl Default for TenantQuota {
    fn default() -> Self {
        Self {
            max_connections: 10,
            max_queries_per_second: None,
        }
    }
}

/// 租户连接池统计
#[derive(Debug, Clone, Default)]
pub struct TenantPoolStats {
    /// 当前活跃连接数
    pub active_connections: usize,
    /// 累计查询数
    pub total_queries: u64,
    /// 被限流的查询数
    pub throttled_queries: u64,
    /// 最后查询时间
    pub last_query_at: Option<Instant>,
}

/// 租户连接池条目
struct TenantEntry {
    /// 配额配置
    quota: TenantQuota,
    /// 统计信息
    stats: TenantPoolStats,
    /// 查询时间窗口（用于速率限制）
    query_window: Vec<Instant>,
}

/// 多租户连接池管理器
///
/// 按租户隔离连接池，支持配额限制和动态扩缩容。
pub struct MultiTenantPoolManager {
    /// 租户 → 连接池条目
    entries: Mutex<HashMap<TenantId, TenantEntry>>,
    /// 默认配额
    default_quota: TenantQuota,
}

impl MultiTenantPoolManager {
    /// 创建多租户连接池管理器
    pub fn new(default_quota: TenantQuota) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(HashMap::new()),
            default_quota,
        })
    }

    /// 创建默认配置的多租户连接池管理器
    pub fn with_default() -> Arc<Self> {
        Self::new(TenantQuota::default())
    }

    /// 注册租户
    pub fn register_tenant(&self, tenant_id: TenantId, quota: TenantQuota) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(
            tenant_id,
            TenantEntry {
                quota,
                stats: TenantPoolStats::default(),
                query_window: Vec::new(),
            },
        );
    }

    /// 注销租户（释放资源）
    pub fn unregister_tenant(&self, tenant_id: TenantId) -> bool {
        let mut entries = self.entries.lock().unwrap();
        entries.remove(&tenant_id).is_some()
    }

    /// 检查是否允许查询（配额 + 速率限制）
    ///
    /// 返回 `Ok(())` 表示允许，`Err(reason)` 表示拒绝。
    pub fn check_query(&self, tenant_id: TenantId) -> Result<(), String> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.entry(tenant_id).or_insert_with(|| TenantEntry {
            quota: self.default_quota.clone(),
            stats: TenantPoolStats::default(),
            query_window: Vec::new(),
        });

        let now = Instant::now();
        entry.stats.total_queries += 1;
        entry.stats.last_query_at = Some(now);

        if let Some(max_qps) = entry.quota.max_queries_per_second {
            entry
                .query_window
                .retain(|t| now.duration_since(*t) < Duration::from_secs(1));
            if entry.query_window.len() >= max_qps as usize {
                entry.stats.throttled_queries += 1;
                return Err(format!("rate limit exceeded for tenant {}", tenant_id));
            }
            entry.query_window.push(now);
        }

        Ok(())
    }

    /// 获取连接（检查配额）
    pub fn acquire_connection(&self, tenant_id: TenantId) -> Result<(), String> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.entry(tenant_id).or_insert_with(|| TenantEntry {
            quota: self.default_quota.clone(),
            stats: TenantPoolStats::default(),
            query_window: Vec::new(),
        });

        if entry.stats.active_connections >= entry.quota.max_connections {
            return Err(format!(
                "max connections exceeded for tenant {} ({}/{})",
                tenant_id, entry.stats.active_connections, entry.quota.max_connections
            ));
        }

        entry.stats.active_connections += 1;
        Ok(())
    }

    /// 释放连接
    pub fn release_connection(&self, tenant_id: TenantId) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(&tenant_id) {
            if entry.stats.active_connections > 0 {
                entry.stats.active_connections -= 1;
            }
        }
    }

    /// 动态调整配额
    pub fn resize_quota(&self, tenant_id: TenantId, new_quota: TenantQuota) -> bool {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(&tenant_id) {
            entry.quota = new_quota;
            true
        } else {
            false
        }
    }

    /// 获取租户统计
    pub fn stats(&self, tenant_id: TenantId) -> Option<TenantPoolStats> {
        let entries = self.entries.lock().unwrap();
        entries.get(&tenant_id).map(|e| e.stats.clone())
    }

    /// 获取租户配额
    pub fn quota(&self, tenant_id: TenantId) -> Option<TenantQuota> {
        let entries = self.entries.lock().unwrap();
        entries.get(&tenant_id).map(|e| e.quota.clone())
    }

    /// 活跃租户数
    pub fn tenant_count(&self) -> usize {
        let entries = self.entries.lock().unwrap();
        entries.len()
    }

    /// 动态扩容：增加所有租户的最大连接数
    pub fn scale_up_all(&self, delta: usize) {
        let mut entries = self.entries.lock().unwrap();
        for entry in entries.values_mut() {
            entry.quota.max_connections += delta;
        }
    }

    /// 动态缩容：减少所有租户的最大连接数（不低于当前活跃数）
    pub fn scale_down_all(&self, delta: usize) {
        let mut entries = self.entries.lock().unwrap();
        for entry in entries.values_mut() {
            let min = entry.stats.active_connections;
            entry.quota.max_connections =
                entry.quota.max_connections.saturating_sub(delta).max(min);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_unregister_tenant() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(1, TenantQuota::default());
        assert_eq!(mgr.tenant_count(), 1);
        assert!(mgr.unregister_tenant(1));
        assert_eq!(mgr.tenant_count(), 0);
    }

    #[test]
    fn test_unregister_nonexistent() {
        let mgr = MultiTenantPoolManager::with_default();
        assert!(!mgr.unregister_tenant(999));
    }

    #[test]
    fn test_acquire_within_quota() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 3,
                max_queries_per_second: None,
            },
        );
        assert!(mgr.acquire_connection(1).is_ok());
        assert!(mgr.acquire_connection(1).is_ok());
        assert!(mgr.acquire_connection(1).is_ok());
        let stats = mgr.stats(1).unwrap();
        assert_eq!(stats.active_connections, 3);
    }

    #[test]
    fn test_acquire_exceeds_quota() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 2,
                max_queries_per_second: None,
            },
        );
        assert!(mgr.acquire_connection(1).is_ok());
        assert!(mgr.acquire_connection(1).is_ok());
        assert!(mgr.acquire_connection(1).is_err());
    }

    #[test]
    fn test_release_connection() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 1,
                max_queries_per_second: None,
            },
        );
        assert!(mgr.acquire_connection(1).is_ok());
        mgr.release_connection(1);
        assert!(mgr.acquire_connection(1).is_ok());
    }

    #[test]
    fn test_rate_limit() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 10,
                max_queries_per_second: Some(3),
            },
        );
        assert!(mgr.check_query(1).is_ok());
        assert!(mgr.check_query(1).is_ok());
        assert!(mgr.check_query(1).is_ok());
        assert!(mgr.check_query(1).is_err());
        let stats = mgr.stats(1).unwrap();
        assert!(stats.throttled_queries >= 1);
    }

    #[test]
    fn test_dynamic_resize_quota() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 2,
                max_queries_per_second: None,
            },
        );
        assert!(mgr.acquire_connection(1).is_ok());
        assert!(mgr.acquire_connection(1).is_ok());
        assert!(mgr.acquire_connection(1).is_err());
        assert!(mgr.resize_quota(
            1,
            TenantQuota {
                max_connections: 5,
                max_queries_per_second: None
            }
        ));
        assert!(mgr.acquire_connection(1).is_ok());
    }

    #[test]
    fn test_scale_up_scale_down_all() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 5,
                max_queries_per_second: None,
            },
        );
        mgr.register_tenant(
            2,
            TenantQuota {
                max_connections: 5,
                max_queries_per_second: None,
            },
        );
        mgr.scale_up_all(3);
        assert_eq!(mgr.quota(1).unwrap().max_connections, 8);
        assert_eq!(mgr.quota(2).unwrap().max_connections, 8);
        mgr.scale_down_all(2);
        assert_eq!(mgr.quota(1).unwrap().max_connections, 6);
    }

    #[test]
    fn test_scale_down_not_below_active() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 10,
                max_queries_per_second: None,
            },
        );
        mgr.acquire_connection(1).unwrap();
        mgr.acquire_connection(1).unwrap();
        mgr.scale_down_all(10);
        assert!(mgr.quota(1).unwrap().max_connections >= 2);
    }

    #[test]
    fn test_auto_register_on_query() {
        let mgr = MultiTenantPoolManager::with_default();
        assert!(mgr.check_query(1).is_ok());
        assert_eq!(mgr.tenant_count(), 1);
    }

    #[test]
    fn test_tenant_isolation() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 1,
                max_queries_per_second: None,
            },
        );
        mgr.register_tenant(
            2,
            TenantQuota {
                max_connections: 1,
                max_queries_per_second: None,
            },
        );
        assert!(mgr.acquire_connection(1).is_ok());
        assert!(mgr.acquire_connection(2).is_ok());
        assert!(mgr.acquire_connection(1).is_err());
        assert!(mgr.acquire_connection(2).is_err());
    }

    #[test]
    fn test_stats_tracking() {
        let mgr = MultiTenantPoolManager::with_default();
        mgr.register_tenant(
            1,
            TenantQuota {
                max_connections: 10,
                max_queries_per_second: None,
            },
        );
        mgr.check_query(1).unwrap();
        mgr.check_query(1).unwrap();
        mgr.acquire_connection(1).unwrap();
        let stats = mgr.stats(1).unwrap();
        assert_eq!(stats.total_queries, 2);
        assert_eq!(stats.active_connections, 1);
        assert!(stats.last_query_at.is_some());
    }
}
