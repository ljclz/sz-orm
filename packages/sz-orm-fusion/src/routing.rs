//! 查询路由策略（Query Routing Strategy）
//!
//! 根据查询特征和数据源状态，将查询路由到合适的数据源。
//! 支持读写分离、负载均衡、亲和性路由等策略。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use crate::health_check::{HealthChecker, HealthStatus};

/// 查询类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QueryType {
    Read,
    Write,
    Batch,
    Analytical,
}

impl QueryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueryType::Read => "read",
            QueryType::Write => "write",
            QueryType::Batch => "batch",
            QueryType::Analytical => "analytical",
        }
    }

    pub fn is_read(&self) -> bool {
        matches!(self, QueryType::Read | QueryType::Analytical)
    }
}

/// 数据源角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceRole {
    Primary,
    Replica,
    Analytics,
    Cache,
}

impl SourceRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceRole::Primary => "primary",
            SourceRole::Replica => "replica",
            SourceRole::Analytics => "analytics",
            SourceRole::Cache => "cache",
        }
    }
}

/// 数据源描述
#[derive(Debug, Clone, serde::Serialize)]
pub struct DataSource {
    pub name: String,
    pub role: SourceRole,
    pub weight: u32,
    pub tags: Vec<String>,
}

impl DataSource {
    pub fn new(name: &str, role: SourceRole) -> Self {
        Self {
            name: name.to_string(),
            role,
            weight: 1,
            tags: Vec::new(),
        }
    }

    pub fn with_weight(mut self, w: u32) -> Self {
        self.weight = w;
        self
    }

    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// 路由决策
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoutingDecision {
    pub source: String,
    pub role: SourceRole,
    pub reason: String,
}

impl RoutingDecision {
    pub fn new(source: &str, role: SourceRole, reason: &str) -> Self {
        Self {
            source: source.to_string(),
            role,
            reason: reason.to_string(),
        }
    }
}

/// 路由策略 trait
pub trait RoutingStrategy: Send + Sync {
    fn route(
        &self,
        query_type: QueryType,
        sources: &[DataSource],
        health: &HealthChecker,
    ) -> Option<RoutingDecision>;
}

/// 读写分离策略
pub struct ReadWriteSplitStrategy;

impl RoutingStrategy for ReadWriteSplitStrategy {
    fn route(
        &self,
        query_type: QueryType,
        sources: &[DataSource],
        health: &HealthChecker,
    ) -> Option<RoutingDecision> {
        if query_type == QueryType::Write {
            let primary = sources.iter().find(|s| s.role == SourceRole::Primary)?;
            if health.status(&primary.name).is_available() {
                return Some(RoutingDecision::new(
                    &primary.name,
                    SourceRole::Primary,
                    "write to primary",
                ));
            }
            return None;
        }
        let replicas: Vec<&DataSource> = sources
            .iter()
            .filter(|s| s.role == SourceRole::Replica)
            .collect();
        for replica in &replicas {
            if health.status(&replica.name).is_available() {
                return Some(RoutingDecision::new(
                    &replica.name,
                    SourceRole::Replica,
                    "read from replica",
                ));
            }
        }
        let primary = sources.iter().find(|s| s.role == SourceRole::Primary)?;
        if health.status(&primary.name).is_available() {
            return Some(RoutingDecision::new(
                &primary.name,
                SourceRole::Primary,
                "fallback read to primary",
            ));
        }
        None
    }
}

/// 加权轮询策略
pub struct WeightedRoundRobinStrategy {
    counters: RwLock<HashMap<String, AtomicU64>>,
}

impl WeightedRoundRobinStrategy {
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for WeightedRoundRobinStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl RoutingStrategy for WeightedRoundRobinStrategy {
    fn route(
        &self,
        _query_type: QueryType,
        sources: &[DataSource],
        health: &HealthChecker,
    ) -> Option<RoutingDecision> {
        let available: Vec<&DataSource> = sources
            .iter()
            .filter(|s| health.status(&s.name).is_available())
            .collect();
        if available.is_empty() {
            return None;
        }
        let mut counters = self.counters.write().ok()?;
        let mut best: Option<&DataSource> = None;
        let mut best_score = u64::MAX;
        for source in &available {
            let counter = counters
                .entry(source.name.clone())
                .or_insert_with(|| AtomicU64::new(0));
            let count = counter.load(Ordering::Relaxed);
            let score = if source.weight > 0 {
                count / source.weight as u64
            } else {
                count
            };
            if score < best_score {
                best_score = score;
                best = Some(source);
            }
        }
        if let Some(source) = best {
            if let Some(counter) = counters.get(&source.name) {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            return Some(RoutingDecision::new(
                &source.name,
                source.role,
                "weighted round robin",
            ));
        }
        None
    }
}

/// 亲和性路由策略
///
/// 同一 key 优先路由到同一数据源（利用本地缓存）。
pub struct AffinityRoutingStrategy {
    affinity_map: RwLock<HashMap<String, String>>,
}

impl AffinityRoutingStrategy {
    pub fn new() -> Self {
        Self {
            affinity_map: RwLock::new(HashMap::new()),
        }
    }

    pub fn route_with_key(
        &self,
        key: &str,
        _query_type: QueryType,
        sources: &[DataSource],
        health: &HealthChecker,
    ) -> Option<RoutingDecision> {
        let affinity = self.affinity_map.read().ok()?;
        if let Some(preferred) = affinity.get(key) {
            if health.status(preferred).is_available() {
                if let Some(source) = sources.iter().find(|s| &s.name == preferred) {
                    return Some(RoutingDecision::new(
                        &source.name,
                        source.role,
                        "affinity hit",
                    ));
                }
            }
        }
        drop(affinity);
        let available: Vec<&DataSource> = sources
            .iter()
            .filter(|s| health.status(&s.name).is_available())
            .collect();
        if available.is_empty() {
            return None;
        }
        let selected = available[0];
        if let Ok(mut map) = self.affinity_map.write() {
            map.insert(key.to_string(), selected.name.clone());
        }
        Some(RoutingDecision::new(
            &selected.name,
            selected.role,
            "affinity miss, assigned",
        ))
    }

    pub fn clear_affinity(&self, key: &str) {
        if let Ok(mut map) = self.affinity_map.write() {
            map.remove(key);
        }
    }

    pub fn affinity_count(&self) -> usize {
        self.affinity_map.read().map(|m| m.len()).unwrap_or(0)
    }
}

impl Default for AffinityRoutingStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// 查询路由器
pub struct QueryRouter {
    sources: Vec<DataSource>,
    health: Arc<HealthChecker>,
    read_write: ReadWriteSplitStrategy,
    round_robin: WeightedRoundRobinStrategy,
    affinity: AffinityRoutingStrategy,
    total_routed: AtomicU64,
}

impl QueryRouter {
    pub fn new(sources: Vec<DataSource>, health: Arc<HealthChecker>) -> Self {
        Self {
            sources,
            health,
            read_write: ReadWriteSplitStrategy,
            round_robin: WeightedRoundRobinStrategy::new(),
            affinity: AffinityRoutingStrategy::new(),
            total_routed: AtomicU64::new(0),
        }
    }

    pub fn route(&self, query_type: QueryType) -> Option<RoutingDecision> {
        self.total_routed.fetch_add(1, Ordering::Relaxed);
        self.read_write
            .route(query_type, &self.sources, &self.health)
    }

    pub fn route_with_key(&self, key: &str, query_type: QueryType) -> Option<RoutingDecision> {
        self.total_routed.fetch_add(1, Ordering::Relaxed);
        self.affinity
            .route_with_key(key, query_type, &self.sources, &self.health)
    }

    pub fn route_round_robin(&self, query_type: QueryType) -> Option<RoutingDecision> {
        self.total_routed.fetch_add(1, Ordering::Relaxed);
        self.round_robin
            .route(query_type, &self.sources, &self.health)
    }

    pub fn sources(&self) -> &[DataSource] {
        &self.sources
    }

    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub fn total_routed(&self) -> u64 {
        self.total_routed.load(Ordering::Relaxed)
    }

    pub fn add_source(&mut self, source: DataSource) {
        self.sources.push(source);
    }

    pub fn find_source(&self, name: &str) -> Option<&DataSource> {
        self.sources.iter().find(|s| s.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health_check::HealthCheckResult;

    fn make_sources() -> Vec<DataSource> {
        vec![
            DataSource::new("primary", SourceRole::Primary),
            DataSource::new("replica1", SourceRole::Replica).with_weight(2),
            DataSource::new("replica2", SourceRole::Replica).with_weight(1),
        ]
    }

    fn make_health() -> Arc<HealthChecker> {
        let h = Arc::new(HealthChecker::default());
        h.record(&HealthCheckResult::healthy("primary", 10));
        h.record(&HealthCheckResult::healthy("replica1", 10));
        h.record(&HealthCheckResult::healthy("replica2", 10));
        h
    }

    #[test]
    fn test_query_type_as_str() {
        assert_eq!(QueryType::Read.as_str(), "read");
        assert_eq!(QueryType::Write.as_str(), "write");
    }

    #[test]
    fn test_query_type_is_read() {
        assert!(QueryType::Read.is_read());
        assert!(QueryType::Analytical.is_read());
        assert!(!QueryType::Write.is_read());
    }

    #[test]
    fn test_source_role_as_str() {
        assert_eq!(SourceRole::Primary.as_str(), "primary");
        assert_eq!(SourceRole::Replica.as_str(), "replica");
    }

    #[test]
    fn test_data_source_with_weight() {
        let s = DataSource::new("db", SourceRole::Primary).with_weight(5);
        assert_eq!(s.weight, 5);
    }

    #[test]
    fn test_data_source_with_tag() {
        let s = DataSource::new("db", SourceRole::Primary)
            .with_tag("fast")
            .with_tag("ssd");
        assert!(s.has_tag("fast"));
        assert!(s.has_tag("ssd"));
        assert!(!s.has_tag("slow"));
    }

    #[test]
    fn test_read_write_split_write() {
        let sources = make_sources();
        let health = make_health();
        let strategy = ReadWriteSplitStrategy;
        let decision = strategy.route(QueryType::Write, &sources, &health).unwrap();
        assert_eq!(decision.source, "primary");
        assert_eq!(decision.role, SourceRole::Primary);
    }

    #[test]
    fn test_read_write_split_read() {
        let sources = make_sources();
        let health = make_health();
        let strategy = ReadWriteSplitStrategy;
        let decision = strategy.route(QueryType::Read, &sources, &health).unwrap();
        assert_eq!(decision.role, SourceRole::Replica);
    }

    #[test]
    fn test_read_write_split_no_primary() {
        let sources = vec![DataSource::new("replica1", SourceRole::Replica)];
        let health = HealthChecker::default();
        let strategy = ReadWriteSplitStrategy;
        let decision = strategy.route(QueryType::Write, &sources, &health);
        assert!(decision.is_none());
    }

    #[test]
    fn test_weighted_round_robin() {
        let sources = make_sources();
        let health = make_health();
        let strategy = WeightedRoundRobinStrategy::new();
        let d1 = strategy.route(QueryType::Read, &sources, &health).unwrap();
        let d2 = strategy.route(QueryType::Read, &sources, &health).unwrap();
        assert_ne!(d1.source, d2.source);
    }

    #[test]
    fn test_affinity_routing() {
        let sources = make_sources();
        let health = make_health();
        let strategy = AffinityRoutingStrategy::new();
        let d1 = strategy
            .route_with_key("user:1", QueryType::Read, &sources, &health)
            .unwrap();
        let d2 = strategy
            .route_with_key("user:1", QueryType::Read, &sources, &health)
            .unwrap();
        assert_eq!(d1.source, d2.source);
        assert_eq!(d2.reason, "affinity hit");
    }

    #[test]
    fn test_affinity_clear() {
        let sources = make_sources();
        let health = make_health();
        let strategy = AffinityRoutingStrategy::new();
        strategy.route_with_key("k", QueryType::Read, &sources, &health);
        assert_eq!(strategy.affinity_count(), 1);
        strategy.clear_affinity("k");
        assert_eq!(strategy.affinity_count(), 0);
    }

    #[test]
    fn test_query_router_route_write() {
        let sources = make_sources();
        let health = make_health();
        let router = QueryRouter::new(sources, health);
        let d = router.route(QueryType::Write).unwrap();
        assert_eq!(d.source, "primary");
    }

    #[test]
    fn test_query_router_route_read() {
        let sources = make_sources();
        let health = make_health();
        let router = QueryRouter::new(sources, health);
        let d = router.route(QueryType::Read).unwrap();
        assert_eq!(d.role, SourceRole::Replica);
    }

    #[test]
    fn test_query_router_route_with_key() {
        let sources = make_sources();
        let health = make_health();
        let router = QueryRouter::new(sources, health);
        let d1 = router.route_with_key("k", QueryType::Read).unwrap();
        let d2 = router.route_with_key("k", QueryType::Read).unwrap();
        assert_eq!(d1.source, d2.source);
    }

    #[test]
    fn test_query_router_source_count() {
        let sources = make_sources();
        let health = make_health();
        let router = QueryRouter::new(sources, health);
        assert_eq!(router.source_count(), 3);
    }

    #[test]
    fn test_query_router_total_routed() {
        let sources = make_sources();
        let health = make_health();
        let router = QueryRouter::new(sources, health);
        router.route(QueryType::Read);
        router.route(QueryType::Write);
        assert_eq!(router.total_routed(), 2);
    }

    #[test]
    fn test_query_router_find_source() {
        let sources = make_sources();
        let health = make_health();
        let router = QueryRouter::new(sources, health);
        assert!(router.find_source("primary").is_some());
        assert!(router.find_source("nonexistent").is_none());
    }

    #[test]
    fn test_routing_decision_new() {
        let d = RoutingDecision::new("db", SourceRole::Primary, "test");
        assert_eq!(d.source, "db");
        assert_eq!(d.reason, "test");
    }

    #[test]
    fn test_read_write_split_replica_down() {
        let sources = make_sources();
        let health = HealthChecker::default();
        health.record(&HealthCheckResult::healthy("primary", 10));
        health.record(&HealthCheckResult::unhealthy("replica1", "down"));
        health.record(&HealthCheckResult::unhealthy("replica2", "down"));
        let strategy = ReadWriteSplitStrategy;
        let decision = strategy.route(QueryType::Read, &sources, &health).unwrap();
        assert_eq!(decision.source, "primary");
        assert_eq!(decision.reason, "fallback read to primary");
    }
}
