//! # 心跳机制（Ping/Pong）
//!
//! 实现 WebSocket 心跳保活：周期性发送 Ping，等待 Pong 应答；
//! 超时未应答则判定连接失活并触发断开。
//!
//! ## 主要类型
//!
//! - [`HeartbeatConfig`] — 心跳配置
//! - [`HeartbeatState`] — 单连接的心跳状态
//! - [`HeartbeatTracker`] — 多连接心跳跟踪器

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 心跳配置
#[derive(Debug, Clone, Copy)]
pub struct HeartbeatConfig {
    /// Ping 发送间隔（毫秒）
    pub interval_ms: u64,
    /// 等待 Pong 的超时时间（毫秒）
    pub timeout_ms: u64,
    /// 连续未收到 Pong 的最大次数，超过则判定连接失活
    pub max_missed: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_ms: 30_000,
            timeout_ms: 10_000,
            max_missed: 3,
        }
    }
}

impl HeartbeatConfig {
    /// 创建自定义配置
    pub fn new(interval_ms: u64, timeout_ms: u64, max_missed: u32) -> Self {
        Self {
            interval_ms,
            timeout_ms,
            max_missed,
        }
    }

    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), String> {
        if self.interval_ms == 0 {
            return Err("interval_ms must be > 0".to_string());
        }
        if self.timeout_ms == 0 {
            return Err("timeout_ms must be > 0".to_string());
        }
        if self.timeout_ms >= self.interval_ms {
            return Err("timeout_ms must be < interval_ms".to_string());
        }
        if self.max_missed == 0 {
            return Err("max_missed must be > 0".to_string());
        }
        Ok(())
    }
}

/// 单连接的心跳状态
#[derive(Debug, Clone)]
pub struct HeartbeatState {
    /// 连接 ID
    pub connection_id: String,
    /// 上次发送 Ping 的时间戳（毫秒），None 表示尚未发送
    pub last_ping_at: Option<i64>,
    /// 上次收到 Pong 的时间戳（毫秒）
    pub last_pong_at: Option<i64>,
    /// 连续未收到 Pong 的次数
    pub missed_count: u32,
    /// 总共发送的 Ping 次数
    pub total_pings: u64,
    /// 总共收到的 Pong 次数
    pub total_pongs: u64,
    /// 是否被判定为失活
    pub is_dead: bool,
}

impl HeartbeatState {
    pub fn new(connection_id: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            last_ping_at: None,
            last_pong_at: None,
            missed_count: 0,
            total_pings: 0,
            total_pongs: 0,
            is_dead: false,
        }
    }

    /// 记录发送了一次 Ping
    pub fn record_ping(&mut self, now_ms: i64) {
        self.last_ping_at = Some(now_ms);
        self.total_pings += 1;
    }

    /// 记录收到了一次 Pong。返回是否清除了未应答计数。
    pub fn record_pong(&mut self, now_ms: i64) -> bool {
        self.last_pong_at = Some(now_ms);
        self.total_pongs += 1;
        let cleared = self.missed_count > 0;
        self.missed_count = 0;
        cleared
    }

    /// 检查是否超时未收到 Pong。
    /// 返回 true 表示本次检查新增了一次未应答。
    pub fn check_timeout(&mut self, now_ms: i64, config: &HeartbeatConfig) -> bool {
        if self.is_dead {
            return false;
        }
        let Some(last_ping) = self.last_ping_at else {
            return false; // 尚未发送 Ping
        };
        // 若已收到 Pong 且 Pong 时间 >= Ping 时间，说明本次 Ping 已被应答，不超时
        if let Some(last_pong) = self.last_pong_at {
            if last_pong >= last_ping {
                return false;
            }
        }
        // 未超时
        if now_ms - last_ping < config.timeout_ms as i64 {
            return false;
        }
        // 超时：增加 missed_count
        self.missed_count += 1;
        if self.missed_count >= config.max_missed {
            self.is_dead = true;
        }
        true
    }

    /// 当前 RTT 估计（毫秒）。需要 last_ping 和 last_pong 都存在。
    pub fn rtt_ms(&self) -> Option<i64> {
        match (self.last_ping_at, self.last_pong_at) {
            (Some(ping), Some(pong)) if pong >= ping => Some(pong - ping),
            _ => None,
        }
    }

    /// 是否正在等待 Pong 应答
    pub fn awaiting_pong(&self) -> bool {
        match (self.last_ping_at, self.last_pong_at) {
            (Some(ping), Some(pong)) => ping > pong,
            (Some(_), None) => true,
            _ => false,
        }
    }
}

/// 多连接心跳跟踪器
#[derive(Debug)]
pub struct HeartbeatTracker {
    config: HeartbeatConfig,
    states: Arc<RwLock<HashMap<String, HeartbeatState>>>,
}

impl HeartbeatTracker {
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &HeartbeatConfig {
        &self.config
    }

    /// 注册一个连接（若已存在则保留原状态）
    pub async fn register(&self, connection_id: impl Into<String>) {
        let id = connection_id.into();
        let mut states = self.states.write().await;
        states
            .entry(id.clone())
            .or_insert_with(|| HeartbeatState::new(id));
    }

    /// 注册一个连接（确保新建状态）
    pub async fn register_new(&self, connection_id: impl Into<String>) {
        let id = connection_id.into();
        let mut states = self.states.write().await;
        states.insert(id.clone(), HeartbeatState::new(id));
    }

    /// 注销连接
    pub async fn unregister(&self, connection_id: &str) -> Option<HeartbeatState> {
        let mut states = self.states.write().await;
        states.remove(connection_id)
    }

    /// 记录发送 Ping
    pub async fn record_ping(&self, connection_id: &str, now_ms: i64) -> bool {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(connection_id) {
            state.record_ping(now_ms);
            return true;
        }
        false
    }

    /// 记录收到 Pong
    pub async fn record_pong(&self, connection_id: &str, now_ms: i64) -> bool {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(connection_id) {
            state.record_pong(now_ms);
            return true;
        }
        false
    }

    /// 检查所有连接的超时状态，返回 (本次新增超时的连接列表, 本次被判定为失活的连接列表)
    pub async fn check_timeouts(&self, now_ms: i64) -> (Vec<String>, Vec<String>) {
        let mut states = self.states.write().await;
        let mut newly_missed = Vec::new();
        let mut newly_dead = Vec::new();
        for (id, state) in states.iter_mut() {
            let was_dead = state.is_dead;
            let missed = state.check_timeout(now_ms, &self.config);
            // 新增超时的连接（含本次同时被判定为失活的）
            if missed {
                newly_missed.push(id.clone());
            }
            if !was_dead && state.is_dead {
                newly_dead.push(id.clone());
            }
        }
        (newly_missed, newly_dead)
    }

    /// 获取指定连接的心跳状态
    pub async fn state(&self, connection_id: &str) -> Option<HeartbeatState> {
        let states = self.states.read().await;
        states.get(connection_id).cloned()
    }

    /// 当前跟踪的连接数
    pub async fn count(&self) -> usize {
        let states = self.states.read().await;
        states.len()
    }

    /// 获取所有失活连接的 ID
    pub async fn dead_connections(&self) -> Vec<String> {
        let states = self.states.read().await;
        let mut dead: Vec<String> = states
            .iter()
            .filter(|(_, s)| s.is_dead)
            .map(|(id, _)| id.clone())
            .collect();
        dead.sort();
        dead
    }

    /// 清理所有失活连接，返回被清理的数量
    pub async fn purge_dead(&self) -> usize {
        let mut states = self.states.write().await;
        let before = states.len();
        states.retain(|_, s| !s.is_dead);
        before - states.len()
    }

    /// 获取所有连接的 RTT（毫秒），仅返回有有效 RTT 的
    pub async fn rtts(&self) -> Vec<(String, i64)> {
        let states = self.states.read().await;
        let mut result: Vec<(String, i64)> = states
            .iter()
            .filter_map(|(id, s)| s.rtt_ms().map(|rtt| (id.clone(), rtt)))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_config_default() {
        let cfg = HeartbeatConfig::default();
        assert_eq!(cfg.interval_ms, 30_000);
        assert_eq!(cfg.timeout_ms, 10_000);
        assert_eq!(cfg.max_missed, 3);
    }

    #[test]
    fn test_heartbeat_config_validate_ok() {
        let cfg = HeartbeatConfig::new(30_000, 10_000, 3);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_heartbeat_config_validate_zero_interval() {
        let cfg = HeartbeatConfig::new(0, 10_000, 3);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_heartbeat_config_validate_zero_timeout() {
        let cfg = HeartbeatConfig::new(30_000, 0, 3);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_heartbeat_config_validate_timeout_ge_interval() {
        let cfg = HeartbeatConfig::new(10_000, 10_000, 3);
        assert!(cfg.validate().is_err());
        let cfg2 = HeartbeatConfig::new(10_000, 20_000, 3);
        assert!(cfg2.validate().is_err());
    }

    #[test]
    fn test_heartbeat_config_validate_zero_max_missed() {
        let cfg = HeartbeatConfig::new(30_000, 10_000, 0);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_heartbeat_state_new_defaults() {
        let state = HeartbeatState::new("c1");
        assert_eq!(state.connection_id, "c1");
        assert!(state.last_ping_at.is_none());
        assert!(state.last_pong_at.is_none());
        assert_eq!(state.missed_count, 0);
        assert_eq!(state.total_pings, 0);
        assert_eq!(state.total_pongs, 0);
        assert!(!state.is_dead);
    }

    #[test]
    fn test_record_ping_updates_state() {
        let mut state = HeartbeatState::new("c1");
        state.record_ping(1000);
        assert_eq!(state.last_ping_at, Some(1000));
        assert_eq!(state.total_pings, 1);
        assert!(state.awaiting_pong());
    }

    #[test]
    fn test_record_pong_clears_missed_count() {
        let mut state = HeartbeatState::new("c1");
        state.record_ping(1000);
        state.missed_count = 2;
        let cleared = state.record_pong(2000);
        assert!(cleared);
        assert_eq!(state.missed_count, 0);
        assert_eq!(state.total_pongs, 1);
        assert!(!state.awaiting_pong());
    }

    #[test]
    fn test_record_pong_no_missed_returns_false() {
        let mut state = HeartbeatState::new("c1");
        state.record_ping(1000);
        let cleared = state.record_pong(2000);
        assert!(!cleared); // missed_count 本来就是 0
    }

    #[test]
    fn test_rtt_ms_calculated_correctly() {
        let mut state = HeartbeatState::new("c1");
        state.record_ping(1000);
        state.record_pong(1500);
        assert_eq!(state.rtt_ms(), Some(500));
    }

    #[test]
    fn test_rtt_ms_none_without_pong() {
        let mut state = HeartbeatState::new("c1");
        state.record_ping(1000);
        assert_eq!(state.rtt_ms(), None);
    }

    #[test]
    fn test_rtt_ms_none_without_ping() {
        let state = HeartbeatState::new("c1");
        assert_eq!(state.rtt_ms(), None);
    }

    #[test]
    fn test_awaiting_pong_states() {
        let mut state = HeartbeatState::new("c1");
        assert!(!state.awaiting_pong());
        state.record_ping(1000);
        assert!(state.awaiting_pong());
        state.record_pong(2000);
        assert!(!state.awaiting_pong());
    }

    #[test]
    fn test_check_timeout_no_ping_returns_false() {
        let mut state = HeartbeatState::new("c1");
        let cfg = HeartbeatConfig::default();
        assert!(!state.check_timeout(100_000, &cfg));
    }

    #[test]
    fn test_check_timeout_within_window_returns_false() {
        let mut state = HeartbeatState::new("c1");
        let cfg = HeartbeatConfig::new(30_000, 10_000, 3);
        state.record_ping(1000);
        // 5 秒后检查（未超时）
        assert!(!state.check_timeout(6_000, &cfg));
        assert_eq!(state.missed_count, 0);
    }

    #[test]
    fn test_check_timeout_expired_increments_missed() {
        let mut state = HeartbeatState::new("c1");
        let cfg = HeartbeatConfig::new(30_000, 10_000, 3);
        state.record_ping(1000);
        // 15 秒后检查（已超时）
        assert!(state.check_timeout(16_000, &cfg));
        assert_eq!(state.missed_count, 1);
        assert!(!state.is_dead);
    }

    #[test]
    fn test_check_timeout_marks_dead_after_max_missed() {
        let mut state = HeartbeatState::new("c1");
        let cfg = HeartbeatConfig::new(30_000, 10_000, 2);
        state.record_ping(1000);
        state.check_timeout(16_000, &cfg); // missed=1
        assert!(!state.is_dead);
        // 再次检查（模拟下一个周期）
        state.record_ping(40_000);
        state.check_timeout(56_000, &cfg); // missed=2
        assert!(state.is_dead);
    }

    #[test]
    fn test_check_timeout_dead_state_returns_false() {
        let mut state = HeartbeatState::new("c1");
        let cfg = HeartbeatConfig::new(30_000, 10_000, 1);
        state.record_ping(1000);
        state.check_timeout(16_000, &cfg);
        assert!(state.is_dead);
        // 已死，再次检查应返回 false
        assert!(!state.check_timeout(100_000, &cfg));
    }

    #[tokio::test]
    async fn test_tracker_register_new() {
        let tracker = HeartbeatTracker::new(HeartbeatConfig::default());
        tracker.register_new("c1").await;
        assert_eq!(tracker.count().await, 1);
        let state = tracker.state("c1").await.unwrap();
        assert_eq!(state.connection_id, "c1");
    }

    #[tokio::test]
    async fn test_tracker_unregister() {
        let tracker = HeartbeatTracker::new(HeartbeatConfig::default());
        tracker.register_new("c1").await;
        let removed = tracker.unregister("c1").await;
        assert!(removed.is_some());
        assert_eq!(tracker.count().await, 0);
    }

    #[tokio::test]
    async fn test_tracker_record_ping_and_pong() {
        let tracker = HeartbeatTracker::new(HeartbeatConfig::default());
        tracker.register_new("c1").await;
        assert!(tracker.record_ping("c1", 1000).await);
        assert!(tracker.record_pong("c1", 1500).await);
        let state = tracker.state("c1").await.unwrap();
        assert_eq!(state.total_pings, 1);
        assert_eq!(state.total_pongs, 1);
        assert_eq!(state.rtt_ms(), Some(500));
    }

    #[tokio::test]
    async fn test_tracker_record_ping_unknown_returns_false() {
        let tracker = HeartbeatTracker::new(HeartbeatConfig::default());
        assert!(!tracker.record_ping("ghost", 1000).await);
    }

    #[tokio::test]
    async fn test_tracker_record_pong_unknown_returns_false() {
        let tracker = HeartbeatTracker::new(HeartbeatConfig::default());
        assert!(!tracker.record_pong("ghost", 1000).await);
    }

    #[tokio::test]
    async fn test_tracker_check_timeouts_detects_missed() {
        let cfg = HeartbeatConfig::new(30_000, 10_000, 3);
        let tracker = HeartbeatTracker::new(cfg);
        tracker.register_new("c1").await;
        tracker.register_new("c2").await;
        tracker.record_ping("c1", 1000).await;
        tracker.record_ping("c2", 1000).await;
        // c2 立即回复 Pong
        tracker.record_pong("c2", 1500).await;
        // 15 秒后检查：c1 超时，c2 正常
        let (missed, dead) = tracker.check_timeouts(16_000).await;
        assert_eq!(missed.len(), 1);
        assert_eq!(missed[0], "c1");
        assert!(dead.is_empty());
    }

    #[tokio::test]
    async fn test_tracker_check_timeouts_detects_dead() {
        let cfg = HeartbeatConfig::new(30_000, 10_000, 1);
        let tracker = HeartbeatTracker::new(cfg);
        tracker.register_new("c1").await;
        tracker.record_ping("c1", 1000).await;
        let (missed, dead) = tracker.check_timeouts(16_000).await;
        assert_eq!(missed.len(), 1);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0], "c1");
    }

    #[tokio::test]
    async fn test_tracker_dead_connections() {
        let cfg = HeartbeatConfig::new(30_000, 10_000, 1);
        let tracker = HeartbeatTracker::new(cfg);
        tracker.register_new("c1").await;
        tracker.register_new("c2").await;
        tracker.record_ping("c1", 1000).await;
        tracker.check_timeouts(16_000).await; // c1 失活
        let dead = tracker.dead_connections().await;
        assert_eq!(dead, vec!["c1"]);
    }

    #[tokio::test]
    async fn test_tracker_purge_dead() {
        let cfg = HeartbeatConfig::new(30_000, 10_000, 1);
        let tracker = HeartbeatTracker::new(cfg);
        tracker.register_new("c1").await;
        tracker.register_new("c2").await;
        tracker.record_ping("c1", 1000).await;
        tracker.check_timeouts(16_000).await;
        let purged = tracker.purge_dead().await;
        assert_eq!(purged, 1);
        assert_eq!(tracker.count().await, 1);
    }

    #[tokio::test]
    async fn test_tracker_rtts() {
        let tracker = HeartbeatTracker::new(HeartbeatConfig::default());
        tracker.register_new("c1").await;
        tracker.register_new("c2").await;
        tracker.record_ping("c1", 1000).await;
        tracker.record_pong("c1", 1500).await;
        tracker.record_ping("c2", 2000).await;
        tracker.record_pong("c2", 2800).await;
        let rtts = tracker.rtts().await;
        assert_eq!(rtts.len(), 2);
        assert_eq!(rtts[0], ("c1".to_string(), 500));
        assert_eq!(rtts[1], ("c2".to_string(), 800));
    }

    #[tokio::test]
    async fn test_tracker_rtts_excludes_no_rtt() {
        let tracker = HeartbeatTracker::new(HeartbeatConfig::default());
        tracker.register_new("c1").await;
        tracker.register_new("c2").await;
        tracker.record_ping("c1", 1000).await;
        tracker.record_pong("c1", 1500).await;
        // c2 仅发送 Ping 未收到 Pong
        tracker.record_ping("c2", 2000).await;
        let rtts = tracker.rtts().await;
        assert_eq!(rtts.len(), 1);
        assert_eq!(rtts[0].0, "c1");
    }

    #[tokio::test]
    async fn test_tracker_multiple_pings_accumulate_stats() {
        let tracker = HeartbeatTracker::new(HeartbeatConfig::default());
        tracker.register_new("c1").await;
        tracker.record_ping("c1", 1000).await;
        tracker.record_pong("c1", 1500).await;
        tracker.record_ping("c1", 2000).await;
        tracker.record_pong("c1", 2200).await;
        let state = tracker.state("c1").await.unwrap();
        assert_eq!(state.total_pings, 2);
        assert_eq!(state.total_pongs, 2);
        assert_eq!(state.rtt_ms(), Some(200)); // 最近一次 RTT
    }
}
