//! # 连接池管理
//!
//! 实现 WebSocket 连接池：限制最大连接数，LRU 淘汰最久未活跃的连接。
//! 适用于高并发场景下的连接数控制。
//!
//! ## 主要类型
//!
//! - [`PoolConfig`] — 连接池配置
//! - [`PooledConnection`] — 池中连接条目
//! - [`ConnectionPool`] — LRU 连接池

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 连接池配置
#[derive(Debug, Clone, Copy)]
pub struct PoolConfig {
    /// 最大连接数
    pub max_connections: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10_000,
        }
    }
}

impl PoolConfig {
    pub fn new(max_connections: usize) -> Self {
        Self { max_connections }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections == 0 {
            return Err("max_connections must be > 0".to_string());
        }
        Ok(())
    }
}

/// 池中连接条目
#[derive(Debug, Clone)]
pub struct PooledConnection {
    /// 连接 ID
    pub connection_id: String,
    /// 关联的用户 ID
    pub user_id: Option<i64>,
    /// 最后活跃时间戳（毫秒）
    pub last_active_at: i64,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 已发送消息数
    pub messages_sent: u64,
    /// 已接收消息数
    pub messages_received: u64,
}

impl PooledConnection {
    pub fn new(connection_id: impl Into<String>, now_ms: i64) -> Self {
        Self {
            connection_id: connection_id.into(),
            user_id: None,
            last_active_at: now_ms,
            created_at: now_ms,
            messages_sent: 0,
            messages_received: 0,
        }
    }

    pub fn with_user(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// 更新最后活跃时间
    pub fn touch(&mut self, now_ms: i64) {
        self.last_active_at = now_ms;
    }

    /// 记录发送消息
    pub fn record_sent(&mut self) {
        self.messages_sent += 1;
    }

    /// 记录接收消息
    pub fn record_received(&mut self) {
        self.messages_received += 1;
    }

    /// 空闲时长（毫秒）
    pub fn idle_ms(&self, now_ms: i64) -> i64 {
        now_ms - self.last_active_at
    }

    /// 存活时长（毫秒）
    pub fn uptime_ms(&self, now_ms: i64) -> i64 {
        now_ms - self.created_at
    }
}

/// LRU 连接池
#[derive(Debug)]
pub struct ConnectionPool {
    config: PoolConfig,
    /// 连接表：connection_id -> PooledConnection
    connections: Arc<RwLock<HashMap<String, PooledConnection>>>,
    /// LRU 顺序队列：最近活跃的在前，最久未活跃的在后
    lru_order: Arc<RwLock<VecDeque<String>>>,
}

/// 加入连接池的结果
#[derive(Debug, PartialEq, Eq)]
pub enum AdmitResult {
    /// 连接被接纳
    Admitted,
    /// 连接已存在（重复添加）
    AlreadyExists,
    /// 池已满，淘汰了最久未活跃的连接以腾出空间
    EvictedAndAdmitted { evicted_id: String },
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            lru_order: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// 尝试将连接加入池中。
    /// - 若已存在，返回 AlreadyExists
    /// - 若池已满，淘汰 LRU 末尾连接后接纳
    /// - 否则接纳
    pub async fn admit(
        &self,
        connection_id: impl Into<String>,
        now_ms: i64,
    ) -> AdmitResult {
        let id = connection_id.into();
        let mut connections = self.connections.write().await;
        if connections.contains_key(&id) {
            return AdmitResult::AlreadyExists;
        }

        let mut lru = self.lru_order.write().await;
        // 池已满：淘汰 LRU 末尾
        let evicted = if connections.len() >= self.config.max_connections {
            // 从末尾找到一个仍存在的连接（可能已被 remove 清除）
            let mut evicted_id = None;
            while let Some(candidate) = lru.pop_back() {
                if connections.contains_key(&candidate) {
                    connections.remove(&candidate);
                    evicted_id = Some(candidate);
                    break;
                }
            }
            evicted_id
        } else {
            None
        };

        // 插入新连接
        connections.insert(id.clone(), PooledConnection::new(&id, now_ms));
        lru.push_front(id.clone());

        match evicted {
            Some(evicted_id) => AdmitResult::EvictedAndAdmitted { evicted_id },
            None => AdmitResult::Admitted,
        }
    }

    /// 移除连接
    pub async fn remove(&self, connection_id: &str) -> Option<PooledConnection> {
        let mut connections = self.connections.write().await;
        let removed = connections.remove(connection_id);
        if removed.is_some() {
            let mut lru = self.lru_order.write().await;
            lru.retain(|id| id != connection_id);
        }
        removed
    }

    /// 更新连接活跃时间（移到 LRU 头部）
    pub async fn touch(&self, connection_id: &str, now_ms: i64) -> bool {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(connection_id) {
            conn.touch(now_ms);
            drop(connections);
            let mut lru = self.lru_order.write().await;
            lru.retain(|id| id != connection_id);
            lru.push_front(connection_id.to_string());
            return true;
        }
        false
    }

    /// 记录发送消息
    pub async fn record_sent(&self, connection_id: &str) -> bool {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(connection_id) {
            conn.record_sent();
            return true;
        }
        false
    }

    /// 记录接收消息
    pub async fn record_received(&self, connection_id: &str) -> bool {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(connection_id) {
            conn.record_received();
            return true;
        }
        false
    }

    /// 获取连接
    pub async fn get(&self, connection_id: &str) -> Option<PooledConnection> {
        let connections = self.connections.read().await;
        connections.get(connection_id).cloned()
    }

    /// 当前连接数
    pub async fn count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    /// 是否已满
    pub async fn is_full(&self) -> bool {
        self.count().await >= self.config.max_connections
    }

    /// 按用户 ID 查询连接
    pub async fn find_by_user(&self, user_id: i64) -> Vec<PooledConnection> {
        let connections = self.connections.read().await;
        let mut result: Vec<PooledConnection> = connections
            .values()
            .filter(|c| c.user_id == Some(user_id))
            .cloned()
            .collect();
        result.sort_by(|a, b| a.connection_id.cmp(&b.connection_id));
        result
    }

    /// 清理空闲超过指定时长的连接，返回被清理的数量
    pub async fn evict_idle(&self, idle_threshold_ms: i64, now_ms: i64) -> usize {
        let mut connections = self.connections.write().await;
        let mut lru = self.lru_order.write().await;
        let before = connections.len();
        let to_remove: Vec<String> = connections
            .iter()
            .filter(|(_, c)| c.idle_ms(now_ms) >= idle_threshold_ms)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &to_remove {
            connections.remove(id);
        }
        lru.retain(|id| !to_remove.contains(id));
        before - connections.len()
    }

    /// 清空连接池
    pub async fn clear(&self) {
        let mut connections = self.connections.write().await;
        let mut lru = self.lru_order.write().await;
        connections.clear();
        lru.clear();
    }

    /// 获取 LRU 顺序的连接 ID 列表（最近活跃在前）
    pub async fn lru_order_list(&self) -> Vec<String> {
        let lru = self.lru_order.read().await;
        lru.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let cfg = PoolConfig::default();
        assert_eq!(cfg.max_connections, 10_000);
    }

    #[test]
    fn test_pool_config_validate_ok() {
        let cfg = PoolConfig::new(100);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_pool_config_validate_zero() {
        let cfg = PoolConfig::new(0);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_pooled_connection_new() {
        let conn = PooledConnection::new("c1", 1000);
        assert_eq!(conn.connection_id, "c1");
        assert!(conn.user_id.is_none());
        assert_eq!(conn.last_active_at, 1000);
        assert_eq!(conn.created_at, 1000);
        assert_eq!(conn.messages_sent, 0);
        assert_eq!(conn.messages_received, 0);
    }

    #[test]
    fn test_pooled_connection_with_user() {
        let conn = PooledConnection::new("c1", 1000).with_user(42);
        assert_eq!(conn.user_id, Some(42));
    }

    #[test]
    fn test_pooled_connection_touch_updates_last_active() {
        let mut conn = PooledConnection::new("c1", 1000);
        conn.touch(2000);
        assert_eq!(conn.last_active_at, 2000);
    }

    #[test]
    fn test_pooled_connection_record_sent_and_received() {
        let mut conn = PooledConnection::new("c1", 1000);
        conn.record_sent();
        conn.record_sent();
        conn.record_received();
        assert_eq!(conn.messages_sent, 2);
        assert_eq!(conn.messages_received, 1);
    }

    #[test]
    fn test_pooled_connection_idle_ms() {
        let conn = PooledConnection::new("c1", 1000);
        assert_eq!(conn.idle_ms(1500), 500);
    }

    #[test]
    fn test_pooled_connection_uptime_ms() {
        let conn = PooledConnection::new("c1", 1000);
        assert_eq!(conn.uptime_ms(3000), 2000);
    }

    #[tokio::test]
    async fn test_pool_admit_new_connection() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        let result = pool.admit("c1", 1000).await;
        assert_eq!(result, AdmitResult::Admitted);
        assert_eq!(pool.count().await, 1);
    }

    #[tokio::test]
    async fn test_pool_admit_duplicate_returns_already_exists() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        let result = pool.admit("c1", 2000).await;
        assert_eq!(result, AdmitResult::AlreadyExists);
        assert_eq!(pool.count().await, 1);
    }

    #[tokio::test]
    async fn test_pool_admit_evicts_lru_when_full() {
        let pool = ConnectionPool::new(PoolConfig::new(2));
        pool.admit("c1", 1000).await;
        pool.admit("c2", 2000).await;
        // c1 更早活跃，应被淘汰
        let result = pool.admit("c3", 3000).await;
        match result {
            AdmitResult::EvictedAndAdmitted { evicted_id } => {
                assert_eq!(evicted_id, "c1");
            }
            _ => panic!("expected EvictedAndAdmitted, got {:?}", result),
        }
        assert_eq!(pool.count().await, 2);
        assert!(pool.get("c1").await.is_none());
        assert!(pool.get("c2").await.is_some());
        assert!(pool.get("c3").await.is_some());
    }

    #[tokio::test]
    async fn test_pool_admit_touch_updates_lru_order() {
        let pool = ConnectionPool::new(PoolConfig::new(2));
        pool.admit("c1", 1000).await;
        pool.admit("c2", 2000).await;
        // touch c1 使其成为最近活跃
        pool.touch("c1", 5000).await;
        // 现在 c2 是最久未活跃的
        let result = pool.admit("c3", 6000).await;
        match result {
            AdmitResult::EvictedAndAdmitted { evicted_id } => {
                assert_eq!(evicted_id, "c2");
            }
            _ => panic!("expected c2 to be evicted"),
        }
    }

    #[tokio::test]
    async fn test_pool_remove() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        let removed = pool.remove("c1").await;
        assert!(removed.is_some());
        assert_eq!(pool.count().await, 0);
    }

    #[tokio::test]
    async fn test_pool_remove_missing_returns_none() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        assert!(pool.remove("ghost").await.is_none());
    }

    #[tokio::test]
    async fn test_pool_touch_updates_last_active() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        pool.touch("c1", 5000).await;
        let conn = pool.get("c1").await.unwrap();
        assert_eq!(conn.last_active_at, 5000);
    }

    #[tokio::test]
    async fn test_pool_touch_unknown_returns_false() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        assert!(!pool.touch("ghost", 1000).await);
    }

    #[tokio::test]
    async fn test_pool_record_sent_and_received() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        assert!(pool.record_sent("c1").await);
        assert!(pool.record_received("c1").await);
        let conn = pool.get("c1").await.unwrap();
        assert_eq!(conn.messages_sent, 1);
        assert_eq!(conn.messages_received, 1);
    }

    #[tokio::test]
    async fn test_pool_record_sent_unknown_returns_false() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        assert!(!pool.record_sent("ghost").await);
    }

    #[tokio::test]
    async fn test_pool_find_by_user() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        pool.admit("c2", 1000).await;
        // 设置用户 ID
        {
            let mut conns = pool.connections.write().await;
            conns.get_mut("c1").unwrap().user_id = Some(100);
            conns.get_mut("c2").unwrap().user_id = Some(200);
        }
        let found = pool.find_by_user(100).await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].connection_id, "c1");
    }

    #[tokio::test]
    async fn test_pool_find_by_user_none() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        let found = pool.find_by_user(999).await;
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn test_pool_evict_idle() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        pool.admit("c2", 2000).await;
        pool.admit("c3", 5000).await;
        // 清理空闲超过 3000ms 的连接（now=6000）
        // c1 idle=5000, c2 idle=4000, c3 idle=1000
        let evicted = pool.evict_idle(3000, 6000).await;
        assert_eq!(evicted, 2); // c1 和 c2
        assert_eq!(pool.count().await, 1);
        assert!(pool.get("c3").await.is_some());
    }

    #[tokio::test]
    async fn test_pool_evict_idle_none() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        // 空闲阈值很大，不应清理
        let evicted = pool.evict_idle(100_000, 2000).await;
        assert_eq!(evicted, 0);
    }

    #[tokio::test]
    async fn test_pool_clear() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        pool.admit("c2", 2000).await;
        pool.clear().await;
        assert_eq!(pool.count().await, 0);
    }

    #[tokio::test]
    async fn test_pool_is_full() {
        let pool = ConnectionPool::new(PoolConfig::new(2));
        assert!(!pool.is_full().await);
        pool.admit("c1", 1000).await;
        assert!(!pool.is_full().await);
        pool.admit("c2", 2000).await;
        assert!(pool.is_full().await);
    }

    #[tokio::test]
    async fn test_pool_lru_order_list() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        pool.admit("c2", 2000).await;
        pool.admit("c3", 3000).await;
        // 初始顺序：c3, c2, c1（最近在前）
        let order = pool.lru_order_list().await;
        assert_eq!(order, vec!["c3", "c2", "c1"]);
        // touch c1
        pool.touch("c1", 4000).await;
        let order2 = pool.lru_order_list().await;
        assert_eq!(order2, vec!["c1", "c3", "c2"]);
    }

    #[tokio::test]
    async fn test_pool_remove_updates_lru_order() {
        let pool = ConnectionPool::new(PoolConfig::new(10));
        pool.admit("c1", 1000).await;
        pool.admit("c2", 2000).await;
        pool.admit("c3", 3000).await;
        pool.remove("c2").await;
        let order = pool.lru_order_list().await;
        assert_eq!(order, vec!["c3", "c1"]);
    }

    #[tokio::test]
    async fn test_pool_admit_after_evict_maintains_count() {
        let pool = ConnectionPool::new(PoolConfig::new(1));
        pool.admit("c1", 1000).await;
        pool.admit("c2", 2000).await; // 淘汰 c1
        pool.admit("c3", 3000).await; // 淘汰 c2
        assert_eq!(pool.count().await, 1);
        assert!(pool.get("c3").await.is_some());
    }
}
