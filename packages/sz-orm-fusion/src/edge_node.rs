//! 边缘节点路由器
//!
//! 地理距离计算 + 就近路由 + 边缘缓存一致性检查。

/// 地理位置坐标
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoLocation {
    /// 纬度
    pub lat: f64,
    /// 经度
    pub lon: f64,
}

impl GeoLocation {
    /// 创建坐标
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// Haversine 距离（公里）
    pub fn distance_km(&self, other: &GeoLocation) -> f64 {
        const R: f64 = 6371.0;
        let dlat = (other.lat - self.lat).to_radians();
        let dlon = (other.lon - self.lon).to_radians();
        let lat1 = self.lat.to_radians();
        let lat2 = other.lat.to_radians();
        let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().asin();
        R * c
    }
}

/// 边缘节点
#[derive(Debug, Clone)]
pub struct EdgeNode {
    /// 节点 ID
    pub node_id: String,
    /// 位置名称
    pub location: String,
    /// 纬度
    pub lat: f64,
    /// 经度
    pub lon: f64,
}

impl EdgeNode {
    /// 创建边缘节点
    pub fn new(
        node_id: impl Into<String>,
        location: impl Into<String>,
        lat: f64,
        lon: f64,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            location: location.into(),
            lat,
            lon,
        }
    }

    /// 获取坐标
    pub fn geo(&self) -> GeoLocation {
        GeoLocation::new(self.lat, self.lon)
    }
}

/// 边缘路由策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeRoutingPolicy {
    /// 就近路由
    #[default]
    Nearest,
    /// 最低延迟
    LowestLatency,
}

/// 边缘缓存一致性状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeCacheStatus {
    /// 一致
    Consistent,
    /// 不一致，需回源
    Stale,
}

/// 边缘节点路由器
pub struct EdgeNodeRouter {
    nodes: Vec<EdgeNode>,
    policy: EdgeRoutingPolicy,
    cache_threshold_km: f64,
}

impl EdgeNodeRouter {
    /// 创建路由器
    pub fn new(nodes: Vec<EdgeNode>) -> Self {
        Self {
            nodes,
            policy: EdgeRoutingPolicy::default(),
            cache_threshold_km: 100.0,
        }
    }

    /// 设置路由策略
    pub fn with_policy(mut self, policy: EdgeRoutingPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// 设置缓存一致性阈值
    pub fn with_cache_threshold(mut self, threshold_km: f64) -> Self {
        self.cache_threshold_km = threshold_km;
        self
    }

    /// 查找最近边缘节点
    pub fn find_nearest(&self, location: &GeoLocation) -> Option<&EdgeNode> {
        self.nodes.iter().min_by(|a, b| {
            let da = location.distance_km(&a.geo());
            let db = location.distance_km(&b.geo());
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// 检查边缘缓存一致性
    ///
    /// 距离超阈值时标记 Stale，需回源读取。
    pub fn check_cache_consistency(
        &self,
        node: &EdgeNode,
        request_location: &GeoLocation,
    ) -> EdgeCacheStatus {
        let distance = request_location.distance_km(&node.geo());
        if distance > self.cache_threshold_km {
            EdgeCacheStatus::Stale
        } else {
            EdgeCacheStatus::Consistent
        }
    }

    /// 节点数量
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_nearest_node() {
        let router = EdgeNodeRouter::new(vec![
            EdgeNode::new("nyc", "New York", 40.7128, -74.0060),
            EdgeNode::new("sfo", "San Francisco", 37.7749, -122.4194),
            EdgeNode::new("london", "London", 51.5074, -0.1278),
        ]);
        let boston = GeoLocation::new(42.3601, -71.0589);
        let nearest = router.find_nearest(&boston).unwrap();
        assert_eq!(nearest.node_id, "nyc");
    }

    #[test]
    fn find_nearest_empty() {
        let router = EdgeNodeRouter::new(vec![]);
        let loc = GeoLocation::new(0.0, 0.0);
        assert!(router.find_nearest(&loc).is_none());
    }

    #[test]
    fn cache_consistent_within_threshold() {
        let node = EdgeNode::new("nyc", "New York", 40.7128, -74.0060);
        let router = EdgeNodeRouter::new(vec![node.clone()]);
        let nearby = GeoLocation::new(40.75, -74.0);
        assert_eq!(
            router.check_cache_consistency(&node, &nearby),
            EdgeCacheStatus::Consistent
        );
    }

    #[test]
    fn cache_stale_beyond_threshold() {
        let node = EdgeNode::new("nyc", "New York", 40.7128, -74.0060);
        let router = EdgeNodeRouter::new(vec![node.clone()]).with_cache_threshold(10.0);
        let far = GeoLocation::new(51.5074, -0.1278);
        assert_eq!(
            router.check_cache_consistency(&node, &far),
            EdgeCacheStatus::Stale
        );
    }

    #[test]
    fn haversine_distance() {
        let nyc = GeoLocation::new(40.7128, -74.0060);
        let london = GeoLocation::new(51.5074, -0.1278);
        let dist = nyc.distance_km(&london);
        assert!(dist > 5500.0 && dist < 5600.0);
    }
}
