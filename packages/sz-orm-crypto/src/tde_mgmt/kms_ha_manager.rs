//! KMS 高可用管理器
//!
//! 主 KMS 故障 → 切换备 KMS → DEK 缓存兜底。主备全故障时使用 DEK 缓存继续服务。

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::dek_buffer::DekBuffer;
use crate::kms_client::{KmsClient, KmsError};

use super::SecError;

/// KMS 节点配置
pub struct KmsNode {
    /// 节点标识（如 "primary" / "backup"）
    pub id: String,
    /// KMS 客户端
    pub client: Arc<dyn KmsClient>,
    /// 节点是否健康（false 表示已故障）
    pub healthy: bool,
}

impl std::fmt::Debug for KmsNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KmsNode")
            .field("id", &self.id)
            .field("healthy", &self.healthy)
            .finish_non_exhaustive()
    }
}

impl Clone for KmsNode {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            client: Arc::clone(&self.client),
            healthy: self.healthy,
        }
    }
}

impl KmsNode {
    /// 创建 KMS 节点
    pub fn new(id: &str, client: Arc<dyn KmsClient>) -> Self {
        Self {
            id: id.to_string(),
            client,
            healthy: true,
        }
    }

    /// 标记故障
    pub fn mark_unhealthy(&mut self) {
        self.healthy = false;
    }

    /// 标记恢复
    pub fn mark_healthy(&mut self) {
        self.healthy = true;
    }
}

/// KMS 高可用配置
#[derive(Debug, Clone)]
pub struct KmsHaConfig {
    /// 故障切换超时（默认 1s）
    pub failover_timeout: Duration,
    /// DEK 缓存兜底期望命中率（默认 99%）
    pub expected_cache_hit_rate: f64,
}

impl Default for KmsHaConfig {
    fn default() -> Self {
        Self {
            failover_timeout: Duration::from_secs(1),
            expected_cache_hit_rate: 0.99,
        }
    }
}

impl KmsHaConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

/// 故障切换记录
#[derive(Debug, Clone)]
pub struct FailoverRecord {
    /// 源 KMS 节点
    pub from_kms: String,
    /// 目标 KMS 节点
    pub to_kms: String,
    /// 切换时间
    pub failover_at: Instant,
    /// DEK 缓存命中率
    pub dek_cache_hit_rate: f64,
}

/// KMS 高可用管理器
///
/// 维护主备 KMS 节点列表 + DEK 缓存。主节点故障时切换到备节点；
/// 主备全故障时使用 DEK 缓存兜底，返回 `SecError::KmsAllUnavailable`。
pub struct KmsHaManager {
    nodes: RwLock<Vec<KmsNode>>,
    active_node_id: RwLock<String>,
    config: KmsHaConfig,
    /// DEK 缓存（column.version → DekBuffer）
    dek_cache: RwLock<std::collections::HashMap<(String, u32), DekBuffer>>,
    /// 缓存命中统计
    cache_hits: RwLock<u64>,
    cache_total: RwLock<u64>,
}

impl KmsHaManager {
    /// 创建 KMS 高可用管理器
    ///
    /// `nodes` 至少包含一个节点；第一个节点为初始活跃节点。
    pub fn new(nodes: Vec<KmsNode>, config: KmsHaConfig) -> Result<Self, SecError> {
        if nodes.is_empty() {
            return Err(SecError::InvalidArgument(
                "至少需要一个 KMS 节点".to_string(),
            ));
        }
        let active_id = nodes[0].id.clone();
        Ok(Self {
            nodes: RwLock::new(nodes),
            active_node_id: RwLock::new(active_id),
            config,
            dek_cache: RwLock::new(std::collections::HashMap::new()),
            cache_hits: RwLock::new(0),
            cache_total: RwLock::new(0),
        })
    }

    /// 执行故障切换
    ///
    /// 标记当前活跃节点为 unhealthy，切换到下一个健康节点。
    /// 若无健康节点可用，返回 `SecError::KmsAllUnavailable`。
    pub fn failover(&self) -> Result<FailoverRecord, SecError> {
        let mut nodes = self.nodes.write();
        let from_kms = self.active_node_id.read().clone();

        // 标记当前节点为 unhealthy
        for node in nodes.iter_mut() {
            if node.id == from_kms {
                node.mark_unhealthy();
            }
        }

        // 寻找下一个健康节点
        let next_healthy = nodes.iter().find(|n| n.healthy).map(|n| n.id.clone());

        match next_healthy {
            Some(to_kms) => {
                *self.active_node_id.write() = to_kms.clone();
                let hit_rate = self.cache_hit_rate();
                Ok(FailoverRecord {
                    from_kms,
                    to_kms,
                    failover_at: Instant::now(),
                    dek_cache_hit_rate: hit_rate,
                })
            }
            None => Err(SecError::KmsAllUnavailable(format!(
                "主备 KMS 全部故障（原活跃节点: {}），使用 DEK 缓存兜底",
                from_kms
            ))),
        }
    }

    /// 获取 DEK（优先缓存，其次活跃 KMS 节点）
    ///
    /// 主备全故障时若缓存命中则继续服务，否则返回 `SecError::KmsAllUnavailable`。
    pub async fn get_dek(&self, column: &str, version: u32) -> Result<DekBuffer, SecError> {
        *self.cache_total.write() += 1;

        // 优先缓存
        if let Some(dek) = self.dek_cache.read().get(&(column.to_string(), version)) {
            *self.cache_hits.write() += 1;
            return Ok(DekBuffer::new(dek.as_bytes().to_vec()));
        }

        // 收集活跃节点 + 其他健康节点的客户端（避免跨 await 持锁）
        let active_id = self.active_node_id.read().clone();
        let (active_client, fallback_clients): (
            Option<Arc<dyn KmsClient>>,
            Vec<(String, Arc<dyn KmsClient>)>,
        ) = {
            let nodes = self.nodes.read();
            let active = nodes
                .iter()
                .find(|n| n.id == active_id && n.healthy)
                .map(|n| Arc::clone(&n.client));
            let fallbacks = nodes
                .iter()
                .filter(|n| n.healthy && n.id != active_id)
                .map(|n| (n.id.clone(), Arc::clone(&n.client)))
                .collect();
            (active, fallbacks)
        };

        // 尝试活跃节点
        if let Some(client) = active_client {
            match client.get_dek(column, version).await {
                Ok(dek) => {
                    self.dek_cache.write().insert(
                        (column.to_string(), version),
                        DekBuffer::new(dek.as_bytes().to_vec()),
                    );
                    return Ok(dek);
                }
                Err(KmsError::KmsUnavailable(_)) => {}
                Err(e) => return Err(SecError::KmsAllUnavailable(e.to_string())),
            }
        }

        // 活跃节点故障，尝试其他健康节点
        for (node_id, client) in &fallback_clients {
            match client.get_dek(column, version).await {
                Ok(dek) => {
                    *self.active_node_id.write() = node_id.clone();
                    self.dek_cache.write().insert(
                        (column.to_string(), version),
                        DekBuffer::new(dek.as_bytes().to_vec()),
                    );
                    return Ok(dek);
                }
                Err(KmsError::KmsUnavailable(_)) => continue,
                Err(e) => return Err(SecError::KmsAllUnavailable(e.to_string())),
            }
        }

        // 主备全故障，尝试缓存（兜底）
        if let Some(dek) = self.dek_cache.read().get(&(column.to_string(), version)) {
            *self.cache_hits.write() += 1;
            return Ok(DekBuffer::new(dek.as_bytes().to_vec()));
        }

        Err(SecError::KmsAllUnavailable(format!(
            "主备 KMS 全部故障且缓存未命中 column={} version={}",
            column, version
        )))
    }

    /// 预热缓存
    pub fn prewarm_cache(&self, column: &str, version: u32, dek: DekBuffer) {
        self.dek_cache
            .write()
            .insert((column.to_string(), version), dek);
    }

    /// 当前活跃节点 ID
    pub fn active_node_id(&self) -> String {
        self.active_node_id.read().clone()
    }

    /// 缓存命中率
    pub fn cache_hit_rate(&self) -> f64 {
        let total = *self.cache_total.read();
        if total == 0 {
            return 1.0;
        }
        let hits = *self.cache_hits.read();
        hits as f64 / total as f64
    }

    /// 缓存条目数
    pub fn cache_size(&self) -> usize {
        self.dek_cache.read().len()
    }

    /// 节点数
    pub fn node_count(&self) -> usize {
        self.nodes.read().len()
    }

    /// 配置引用
    pub fn config(&self) -> &KmsHaConfig {
        &self.config
    }

    /// 健康节点数
    pub fn healthy_node_count(&self) -> usize {
        self.nodes.read().iter().filter(|n| n.healthy).count()
    }

    /// 标记节点恢复健康
    pub fn mark_node_healthy(&self, node_id: &str) {
        let mut nodes = self.nodes.write();
        for node in nodes.iter_mut() {
            if node.id == node_id {
                node.mark_healthy();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kms_client::LocalKmsClient;

    fn make_ha_manager() -> KmsHaManager {
        let primary = KmsNode::new(
            "primary",
            Arc::new(LocalKmsClient::with_dek("col", 1, vec![0x42u8; 32])),
        );
        let backup = KmsNode::new(
            "backup",
            Arc::new(LocalKmsClient::with_dek("col", 1, vec![0x42u8; 32])),
        );
        KmsHaManager::new(vec![primary, backup], KmsHaConfig::new()).unwrap()
    }

    #[tokio::test]
    async fn primary_failure_switches_to_backup() {
        let mgr = make_ha_manager();
        let record = mgr.failover().unwrap();
        assert_eq!(record.from_kms, "primary");
        assert_eq!(record.to_kms, "backup");
        assert_eq!(mgr.active_node_id(), "backup");
    }

    #[tokio::test]
    async fn dek_cache_fallback_when_all_unavailable() {
        let mgr = make_ha_manager();
        // 预热缓存
        mgr.prewarm_cache("col", 1, DekBuffer::new(vec![0x42u8; 32]));

        // 主备全部故障
        mgr.failover().unwrap(); // primary → backup
        mgr.failover().unwrap_err(); // backup 也故障

        // 缓存兜底
        let dek = mgr.get_dek("col", 1).await.unwrap();
        assert_eq!(dek.as_bytes(), &vec![0x42u8; 32][..]);
    }

    #[tokio::test]
    async fn all_kms_unavailable_no_cache_returns_error() {
        let mgr = make_ha_manager();
        mgr.failover().unwrap();
        let result = mgr.failover();
        assert!(matches!(result, Err(SecError::KmsAllUnavailable(_))));
    }

    #[tokio::test]
    async fn get_dek_from_active_node() {
        let mgr = make_ha_manager();
        let dek = mgr.get_dek("col", 1).await.unwrap();
        assert_eq!(dek.len(), 32);
        assert_eq!(mgr.cache_size(), 1);
    }

    #[tokio::test]
    async fn get_dek_cache_hit_on_second_request() {
        let mgr = make_ha_manager();
        mgr.get_dek("col", 1).await.unwrap();
        mgr.get_dek("col", 1).await.unwrap();
        // 第二次应命中缓存
        assert!(mgr.cache_hit_rate() >= 0.5);
    }

    #[test]
    fn reject_empty_nodes() {
        let result = KmsHaManager::new(vec![], KmsHaConfig::new());
        assert!(matches!(result, Err(SecError::InvalidArgument(_))));
    }

    #[test]
    fn node_count_and_healthy_count() {
        let mgr = make_ha_manager();
        assert_eq!(mgr.node_count(), 2);
        assert_eq!(mgr.healthy_node_count(), 2);
        mgr.failover().unwrap();
        assert_eq!(mgr.healthy_node_count(), 1);
    }

    #[tokio::test]
    async fn mark_node_healthy_restores() {
        let mgr = make_ha_manager();
        mgr.failover().unwrap();
        assert_eq!(mgr.healthy_node_count(), 1);
        mgr.mark_node_healthy("primary");
        assert_eq!(mgr.healthy_node_count(), 2);
    }

    #[test]
    fn cache_hit_rate_empty_is_one() {
        let mgr = make_ha_manager();
        assert_eq!(mgr.cache_hit_rate(), 1.0);
    }

    #[tokio::test]
    async fn failover_record_contains_cache_hit_rate() {
        let mgr = make_ha_manager();
        mgr.prewarm_cache("col", 1, DekBuffer::new(vec![0x42u8; 32]));
        let record = mgr.failover().unwrap();
        assert!(record.dek_cache_hit_rate >= 0.0);
        assert!(record.dek_cache_hit_rate <= 1.0);
    }
}
