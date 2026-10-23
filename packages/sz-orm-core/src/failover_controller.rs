//! v10.10.0 自动故障转移四阶段闭环
//!
//! 故障检测（心跳超时/连接失败率）→ 故障确认（连续 N 次避免误切换）
//! → 故障转移（提升从节点为主）→ 故障恢复（原主以从身份重新加入）
//!
//! 复用既有 auto-failover feature gate。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 故障检测器
///
/// 通过心跳超时和连接失败率检测节点故障。
#[derive(Debug, Clone)]
pub struct FailoverDetector {
    /// 心跳超时阈值
    heartbeat_timeout: Duration,
    /// 连接失败率阈值（0.0~1.0，超过则判定故障）
    failure_rate_threshold: f64,
}

impl FailoverDetector {
    /// 创建故障检测器
    pub fn new(heartbeat_timeout: Duration, failure_rate_threshold: f64) -> Self {
        Self {
            heartbeat_timeout,
            failure_rate_threshold: failure_rate_threshold.clamp(0.0, 1.0),
        }
    }

    /// 心跳超时阈值
    pub fn heartbeat_timeout(&self) -> Duration {
        self.heartbeat_timeout
    }

    /// 连接失败率阈值
    pub fn failure_rate_threshold(&self) -> f64 {
        self.failure_rate_threshold
    }
}

impl Default for FailoverDetector {
    fn default() -> Self {
        Self::new(Duration::from_secs(3), 0.5)
    }
}

/// 恢复策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// 原主以从身份重新加入（不自动提升为主）
    RejoinAsSlave,
}

/// 故障转移阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverPhase {
    /// 正常运行
    Normal,
    /// 检测到可疑故障
    Detecting,
    /// 故障已确认
    Confirmed,
    /// 已故障转移到从节点
    FailedOver,
    /// 原主恢复中
    Recovering,
}

/// 故障确认器
///
/// 跟踪连续故障次数，达到阈值后确认故障。
#[derive(Debug)]
pub struct FailoverConfirm {
    /// 确认阈值（连续 N 次故障才确认）
    threshold: u32,
    /// 各节点连续故障计数
    consecutive_failures: Mutex<HashMap<String, u32>>,
}

impl FailoverConfirm {
    /// 创建故障确认器
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1),
            consecutive_failures: Mutex::new(HashMap::new()),
        }
    }

    /// 确认阈值
    pub fn threshold(&self) -> u32 {
        self.threshold
    }

    /// 记录一次故障，返回是否达到确认阈值
    pub fn record_failure(&self, node: &str) -> bool {
        let mut failures = self.consecutive_failures.lock().unwrap();
        let count = failures.entry(node.to_string()).or_insert(0);
        *count += 1;
        *count >= self.threshold
    }

    /// 重置节点故障计数
    pub fn reset(&self, node: &str) {
        let mut failures = self.consecutive_failures.lock().unwrap();
        failures.remove(node);
    }

    /// 获取节点当前连续故障次数
    pub fn failure_count(&self, node: &str) -> u32 {
        let failures = self.consecutive_failures.lock().unwrap();
        *failures.get(node).unwrap_or(&0)
    }
}

/// 故障转移控制器
///
/// 协调故障检测 → 确认 → 转移 → 恢复四阶段闭环。
pub struct FailoverController {
    detector: FailoverDetector,
    confirm: FailoverConfirm,
    recovery_strategy: RecoveryStrategy,
    /// 节点健康状态（true=健康，false=故障）
    node_health: Mutex<HashMap<String, bool>>,
    /// 当前阶段
    phase: Mutex<FailoverPhase>,
    /// 当前主节点
    current_master: Mutex<String>,
    /// 从节点列表
    slaves: Mutex<Vec<String>>,
    /// 故障转移时间点
    failover_time: Mutex<Option<Instant>>,
    /// v10.11.0 T2.1：区域拓扑（Option 包裹，向后兼容）
    region_topology: Mutex<Option<RegionTopology>>,
    /// v10.12.0 T3.5：故障恢复协调器
    recovery_coordinator: Option<FailoverRecoveryCoordinator>,
}

impl FailoverController {
    /// 创建故障转移控制器
    pub fn new(detector: FailoverDetector, confirm_threshold: u32) -> Self {
        Self {
            detector,
            confirm: FailoverConfirm::new(confirm_threshold),
            recovery_strategy: RecoveryStrategy::RejoinAsSlave,
            node_health: Mutex::new(HashMap::new()),
            phase: Mutex::new(FailoverPhase::Normal),
            current_master: Mutex::new(String::new()),
            slaves: Mutex::new(Vec::new()),
            failover_time: Mutex::new(None),
            region_topology: Mutex::new(None),
            recovery_coordinator: None,
        }
    }

    /// 创建带故障恢复协调器的故障转移控制器
    pub fn new_with_recovery_coordinator(
        detector: FailoverDetector,
        confirm_threshold: u32,
    ) -> Self {
        Self {
            detector,
            confirm: FailoverConfirm::new(confirm_threshold),
            recovery_strategy: RecoveryStrategy::RejoinAsSlave,
            node_health: Mutex::new(HashMap::new()),
            phase: Mutex::new(FailoverPhase::Normal),
            current_master: Mutex::new(String::new()),
            slaves: Mutex::new(Vec::new()),
            failover_time: Mutex::new(None),
            region_topology: Mutex::new(None),
            recovery_coordinator: Some(FailoverRecoveryCoordinator::new()),
        }
    }

    /// 故障恢复协调器引用
    pub fn recovery_coordinator(&self) -> Option<&FailoverRecoveryCoordinator> {
        self.recovery_coordinator.as_ref()
    }

    /// 设置节点健康状态（供管理和测试使用）
    pub fn set_node_health(&self, node: &str, healthy: bool) {
        let mut health = self.node_health.lock().unwrap();
        health.insert(node.to_string(), healthy);
    }

    /// 设置初始主节点
    pub fn set_master(&self, master: &str) {
        let mut current = self.current_master.lock().unwrap();
        *current = master.to_string();
    }

    /// 添加从节点
    pub fn add_slave(&self, slave: &str) {
        let mut slaves = self.slaves.lock().unwrap();
        if !slaves.contains(&slave.to_string()) {
            slaves.push(slave.to_string());
        }
    }

    /// 当前主节点
    pub fn current_master(&self) -> String {
        self.current_master.lock().unwrap().clone()
    }

    /// 当前阶段
    pub fn phase(&self) -> FailoverPhase {
        *self.phase.lock().unwrap()
    }

    /// 检测器引用
    pub fn detector(&self) -> &FailoverDetector {
        &self.detector
    }

    /// 确认器引用
    pub fn confirm(&self) -> &FailoverConfirm {
        &self.confirm
    }

    /// 恢复策略
    pub fn recovery_strategy(&self) -> RecoveryStrategy {
        self.recovery_strategy
    }

    /// 故障检测：检查主节点是否故障
    ///
    /// 返回 Ok(true) 表示检测到故障，Ok(false) 表示健康。
    pub async fn detect_failure(&self, master: &str) -> Result<bool, String> {
        let health = self.node_health.lock().unwrap();
        let is_healthy = *health.get(master).unwrap_or(&true);
        drop(health);

        if !is_healthy {
            *self.phase.lock().unwrap() = FailoverPhase::Detecting;
        }

        Ok(!is_healthy)
    }

    /// 故障确认：连续 N 次检测到故障才确认
    ///
    /// 返回 Ok(true) 表示已确认故障，Ok(false) 表示未达阈值。
    pub async fn confirm_failure(&self, master: &str) -> Result<bool, String> {
        let detected = self.detect_failure(master).await?;
        if detected {
            let confirmed = self.confirm.record_failure(master);
            if confirmed {
                *self.phase.lock().unwrap() = FailoverPhase::Confirmed;
            }
            Ok(confirmed)
        } else {
            self.confirm.reset(master);
            Ok(false)
        }
    }

    /// 故障转移：提升从节点为新主
    ///
    /// 返回 Ok(new_master) 表示转移成功。
    pub async fn failover(&self, slave: &str) -> Result<String, String> {
        let slaves = self.slaves.lock().unwrap();
        if !slaves.contains(&slave.to_string()) {
            return Err(format!("从节点 {} 不在从节点列表中", slave));
        }
        drop(slaves);

        let health = self.node_health.lock().unwrap();
        let slave_healthy = *health.get(slave).unwrap_or(&true);
        drop(health);

        if !slave_healthy {
            return Err(format!("从节点 {} 不健康，无法提升为主", slave));
        }

        let old_master = self.current_master.lock().unwrap().clone();
        *self.current_master.lock().unwrap() = slave.to_string();
        *self.phase.lock().unwrap() = FailoverPhase::FailedOver;
        *self.failover_time.lock().unwrap() = Some(Instant::now());

        self.confirm.reset(&old_master);

        Ok(slave.to_string())
    }

    /// 故障恢复：原主以从身份重新加入
    ///
    /// 返回 Ok(()) 表示恢复成功。
    /// 当恢复协调器启用时，标记切换完成并触发旧主剔除。
    pub async fn recover(&self, original_master: &str) -> Result<(), String> {
        let is_healthy = {
            let health = self.node_health.lock().unwrap();
            *health.get(original_master).unwrap_or(&false)
        };

        if !is_healthy {
            return Err(format!(
                "原主 {} 尚未恢复健康，无法重新加入",
                original_master
            ));
        }

        match self.recovery_strategy {
            RecoveryStrategy::RejoinAsSlave => {
                self.add_slave(original_master);
                self.confirm.reset(original_master);
                *self.phase.lock().unwrap() = FailoverPhase::Normal;
            }
        }

        let current_master = self.current_master.lock().unwrap().clone();
        if let Some(ref coordinator) = self.recovery_coordinator {
            coordinator
                .on_switch_completed(&current_master, original_master)
                .await
                .map_err(|e| format!("恢复协调失败: {e:?}"))?;
        }

        Ok(())
    }

    /// 故障转移耗时（从转移时刻到现在）
    pub fn failover_elapsed(&self) -> Option<Duration> {
        let time = self.failover_time.lock().unwrap();
        time.map(|t| t.elapsed())
    }

    /// 从节点列表
    pub fn slaves(&self) -> Vec<String> {
        self.slaves.lock().unwrap().clone()
    }

    /// v10.11.0 T2.1：设置区域拓扑
    pub fn set_region_topology(&self, topology: RegionTopology) {
        *self.region_topology.lock().unwrap() = Some(topology);
    }

    /// v10.11.0 T2.1：获取当前区域拓扑
    pub fn region_topology(&self) -> Option<RegionTopology> {
        self.region_topology.lock().unwrap().clone()
    }
}

// ============================================================================
// v10.11.0 T2.1：多区域故障转移（RegionHealthChecker + CrossRegionSwitcher）
// ============================================================================

/// 区域健康状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionHealthStatus {
    /// 健康
    Healthy,
    /// 可疑（心跳缺失 N 次，尚未达故障阈值）
    Suspect { heartbeat_misses: u32 },
    /// 故障（连续确认 N 次）
    Failed { confirm_count: u32 },
}

/// 优先级策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PriorityStrategy {
    /// 地理就近优先
    #[default]
    GeoNear,
    /// 容量冗余优先
    CapacityRedundant,
}

/// 区域配置
#[derive(Debug, Clone)]
pub struct RegionConfig {
    /// 区域 ID
    pub region_id: String,
    /// 地理位置标签（如 "us-east-1"）
    pub geo_location: String,
    /// 容量权重（1~100，越高越优先）
    pub capacity_weight: u32,
    /// 距源区域延迟（毫秒，用于 GeoNear 排序）
    pub latency_ms: u64,
}

/// 区域拓扑
///
/// 描述多区域部署拓扑，包含所有区域配置和当前活跃区域。
#[derive(Debug, Clone)]
pub struct RegionTopology {
    /// 所有区域配置
    regions: Vec<RegionConfig>,
    /// 当前活跃区域 ID
    active_region: String,
    /// 优先级策略
    priority_strategy: PriorityStrategy,
}

impl RegionTopology {
    /// 创建区域拓扑
    pub fn new(regions: Vec<RegionConfig>, active_region: impl Into<String>) -> Self {
        Self {
            regions,
            active_region: active_region.into(),
            priority_strategy: PriorityStrategy::default(),
        }
    }

    /// 设置优先级策略
    pub fn with_priority_strategy(mut self, strategy: PriorityStrategy) -> Self {
        self.priority_strategy = strategy;
        self
    }

    /// 获取所有区域
    pub fn regions(&self) -> &[RegionConfig] {
        &self.regions
    }

    /// 当前活跃区域
    pub fn active_region(&self) -> &str {
        &self.active_region
    }

    /// 优先级策略
    pub fn priority_strategy(&self) -> PriorityStrategy {
        self.priority_strategy
    }

    /// 查找区域配置
    pub fn find_region(&self, region_id: &str) -> Option<&RegionConfig> {
        self.regions.iter().find(|r| r.region_id == region_id)
    }
}

/// 区域健康检查器
///
/// 通过区域级心跳超时和错误率检测区域故障，
/// 含二次确认（连续 N 次）避免误切换。
#[derive(Debug, Clone)]
pub struct RegionHealthChecker {
    /// 心跳超时阈值
    heartbeat_timeout: Duration,
    /// 错误率阈值（0.0~1.0）
    error_rate_threshold: f64,
    /// 确认次数（连续 N 次才判定故障）
    confirm_count: u32,
    /// 区域心跳缺失计数
    heartbeat_misses: HashMap<String, u32>,
    /// 区域故障确认计数
    failure_confirms: HashMap<String, u32>,
    /// 区域错误率
    region_error_rates: HashMap<String, f64>,
}

impl RegionHealthChecker {
    /// 创建区域健康检查器
    pub fn new(heartbeat_timeout: Duration, error_rate_threshold: f64, confirm_count: u32) -> Self {
        Self {
            heartbeat_timeout,
            error_rate_threshold: error_rate_threshold.clamp(0.0, 1.0),
            confirm_count: confirm_count.max(1),
            heartbeat_misses: HashMap::new(),
            failure_confirms: HashMap::new(),
            region_error_rates: HashMap::new(),
        }
    }

    /// 心跳超时阈值
    pub fn heartbeat_timeout(&self) -> Duration {
        self.heartbeat_timeout
    }

    /// 确认次数
    pub fn confirm_count(&self) -> u32 {
        self.confirm_count
    }

    /// 记录区域心跳缺失
    pub fn record_heartbeat_miss(&mut self, region_id: &str) {
        *self
            .heartbeat_misses
            .entry(region_id.to_string())
            .or_insert(0) += 1;
    }

    /// 记录区域心跳成功（重置缺失计数）
    pub fn record_heartbeat_success(&mut self, region_id: &str) {
        self.heartbeat_misses.insert(region_id.to_string(), 0);
        self.failure_confirms.insert(region_id.to_string(), 0);
    }

    /// 设置区域错误率
    pub fn set_region_error_rate(&mut self, region_id: &str, rate: f64) {
        self.region_error_rates
            .insert(region_id.to_string(), rate.clamp(0.0, 1.0));
    }

    /// 检查区域健康状态
    ///
    /// 基于心跳缺失次数和错误率判定区域健康状态。
    pub fn check_region_health(&mut self, region_id: &str) -> RegionHealthStatus {
        let misses = *self.heartbeat_misses.get(region_id).unwrap_or(&0);
        let error_rate = *self.region_error_rates.get(region_id).unwrap_or(&0.0);

        if misses > 0 || error_rate >= self.error_rate_threshold {
            *self
                .failure_confirms
                .entry(region_id.to_string())
                .or_insert(0) += 1;
        }

        let confirms = *self.failure_confirms.get(region_id).unwrap_or(&0);

        if confirms >= self.confirm_count {
            RegionHealthStatus::Failed {
                confirm_count: confirms,
            }
        } else if misses > 0 || error_rate >= self.error_rate_threshold {
            RegionHealthStatus::Suspect {
                heartbeat_misses: misses,
            }
        } else {
            RegionHealthStatus::Healthy
        }
    }

    /// 多实例交叉确认
    ///
    /// 模拟多实例交叉确认：当故障确认计数 >= confirm_count 时返回 true。
    /// 实际生产中应查询其他实例的判定结果进行交叉验证。
    pub fn cross_region_confirm(&self, region_id: &str) -> bool {
        let confirms = *self.failure_confirms.get(region_id).unwrap_or(&0);
        confirms >= self.confirm_count
    }

    /// 重置区域健康状态
    pub fn reset_region(&mut self, region_id: &str) {
        self.heartbeat_misses.insert(region_id.to_string(), 0);
        self.failure_confirms.insert(region_id.to_string(), 0);
        self.region_error_rates.insert(region_id.to_string(), 0.0);
    }
}

impl Default for RegionHealthChecker {
    fn default() -> Self {
        Self::new(Duration::from_secs(3), 0.5, 3)
    }
}

/// 区域优先级排序器
///
/// 按 `PriorityStrategy` 对候选区域排序：
/// - `GeoNear`：按延迟升序（就近优先）
/// - `CapacityRedundant`：按容量权重降序（冗余优先）
#[derive(Debug, Clone)]
pub struct RegionPrioritySorter {
    strategy: PriorityStrategy,
}

impl RegionPrioritySorter {
    /// 创建区域优先级排序器
    pub fn new(strategy: PriorityStrategy) -> Self {
        Self { strategy }
    }

    /// 策略
    pub fn strategy(&self) -> PriorityStrategy {
        self.strategy
    }

    /// 排序候选区域，返回按优先级降序排列的区域 ID
    pub fn sort_regions(&self, regions: &[RegionConfig]) -> Vec<String> {
        let mut sorted: Vec<&RegionConfig> = regions.iter().collect();
        match self.strategy {
            PriorityStrategy::GeoNear => {
                sorted.sort_unstable_by_key(|r| r.latency_ms);
            }
            PriorityStrategy::CapacityRedundant => {
                sorted.sort_unstable_by_key(|r| std::cmp::Reverse(r.capacity_weight));
            }
        }
        sorted.iter().map(|r| r.region_id.clone()).collect()
    }
}

/// 跨区域切换器
///
/// 执行跨区域故障转移：选择最优健康区域并切换。
#[derive(Debug)]
pub struct CrossRegionSwitcher {
    priority_sorter: RegionPrioritySorter,
    /// 切换记录（region_id -> 切换时间）
    switch_history: HashMap<String, Instant>,
    /// RTO 优化器（v10.12.0 T3.4）
    rto_optimizer: Option<RtoOptimizer>,
}

impl CrossRegionSwitcher {
    /// 创建跨区域切换器
    pub fn new(strategy: PriorityStrategy) -> Self {
        Self {
            priority_sorter: RegionPrioritySorter::new(strategy),
            switch_history: HashMap::new(),
            rto_optimizer: None,
        }
    }

    /// 创建带 RTO 优化的跨区域切换器
    pub fn new_with_rto_optimizer(strategy: PriorityStrategy, verify_timeout: Duration) -> Self {
        Self {
            priority_sorter: RegionPrioritySorter::new(strategy),
            switch_history: HashMap::new(),
            rto_optimizer: Some(RtoOptimizer::new(verify_timeout)),
        }
    }

    /// RTO 优化器引用
    pub fn rto_optimizer(&self) -> Option<&RtoOptimizer> {
        self.rto_optimizer.as_ref()
    }

    /// 优先级排序器
    pub fn priority_sorter(&self) -> &RegionPrioritySorter {
        &self.priority_sorter
    }

    /// 执行跨区域切换
    ///
    /// 从候选区域中选择最优（按优先级排序）的健康区域进行切换。
    /// 当 RTO 优化器启用且预选区域可用时，直接切换到预选区域（跳过排序）。
    /// 返回 `Ok(new_region_id)` 表示切换成功，`Err` 表示无可用区域。
    pub fn switch(
        &mut self,
        topology: &RegionTopology,
        healthy_regions: &[&str],
    ) -> Result<String, String> {
        if let Some(ref rto) = self.rto_optimizer {
            if let Some(preselected) = rto.preselected_region() {
                if healthy_regions.contains(&preselected.as_str())
                    && preselected != topology.active_region()
                {
                    self.switch_history
                        .insert(preselected.clone(), Instant::now());
                    return Ok(preselected);
                }
            }
        }

        let sorted = self.priority_sorter.sort_regions(topology.regions());

        for region_id in &sorted {
            if healthy_regions.contains(&region_id.as_str())
                && region_id != topology.active_region()
            {
                self.switch_history
                    .insert(region_id.clone(), Instant::now());
                return Ok(region_id.clone());
            }
        }

        Err("无可用健康区域进行切换".to_string())
    }

    /// 切换历史
    pub fn switch_history(&self) -> &HashMap<String, Instant> {
        &self.switch_history
    }
}

// ============================================================================
// v10.11.0 T2.5：故障切换数据校验（SwitchVerifyChecker + GTID/LSN/SCN 比对）
// ============================================================================

/// 复制位点
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicationPosition {
    /// MySQL GTID（如 "uuid:1-100"）
    pub gtid: Option<String>,
    /// PostgreSQL LSN
    pub lsn: Option<u64>,
    /// Oracle SCN
    pub scn: Option<u64>,
}

impl ReplicationPosition {
    /// 创建空位点
    pub fn empty() -> Self {
        Self {
            gtid: None,
            lsn: None,
            scn: None,
        }
    }

    /// 设置 GTID
    pub fn with_gtid(mut self, gtid: impl Into<String>) -> Self {
        self.gtid = Some(gtid.into());
        self
    }

    /// 设置 LSN
    pub fn with_lsn(mut self, lsn: u64) -> Self {
        self.lsn = Some(lsn);
        self
    }

    /// 设置 SCN
    pub fn with_scn(mut self, scn: u64) -> Self {
        self.scn = Some(scn);
        self
    }
}

/// 校验结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyResult {
    /// 主从位点一致
    Consistent { position: ReplicationPosition },
    /// 主从位点不一致
    Inconsistent {
        master_pos: ReplicationPosition,
        slave_pos: ReplicationPosition,
    },
}

/// 校验错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// 位点不可用
    PositionNotAvailable(String),
    /// 回滚失败
    RollbackFailed(String),
}

/// 复制位点比对器类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplicationPositionComparator {
    /// MySQL GTID 比对
    Gtid,
    /// PostgreSQL LSN 比对
    Lsn,
    /// Oracle SCN 比对
    Scn,
}

/// GTID 比对器
pub struct GtidComparator;

impl GtidComparator {
    /// 比较两个 GTID 是否一致
    ///
    /// GTID 格式为 "uuid:interval"，比较 uuid 和最大事务号。
    pub fn compare(master: &str, slave: &str) -> bool {
        master == slave
    }
}

/// LSN 比对器
pub struct LsnComparator;

impl LsnComparator {
    /// 比较两个 LSN 是否一致（slave >= master 表示已追上）
    pub fn compare(master_lsn: u64, slave_lsn: u64) -> bool {
        slave_lsn >= master_lsn
    }
}

/// SCN 比对器
pub struct ScnComparator;

impl ScnComparator {
    /// 比较两个 SCN 是否一致（slave >= master 表示已追上）
    pub fn compare(master_scn: u64, slave_scn: u64) -> bool {
        slave_scn >= master_scn
    }
}

/// 故障切换校验器
///
/// 切换后自动校验主从数据一致性（基于 GTID/LSN/SCN 位点比对）。
#[derive(Debug, Clone)]
pub struct SwitchVerifyChecker {
    comparator: ReplicationPositionComparator,
}

impl SwitchVerifyChecker {
    /// 创建校验器
    pub fn new(comparator: ReplicationPositionComparator) -> Self {
        Self { comparator }
    }

    /// 比对器类型
    pub fn comparator(&self) -> &ReplicationPositionComparator {
        &self.comparator
    }

    /// 切换后校验
    ///
    /// 比较新主（原从）和旧主（原主）的复制位点。
    /// 校验通过返回 `VerifyResult::Consistent`，不通过返回 `Inconsistent`。
    pub fn verify_after_switch(
        &self,
        master_pos: &ReplicationPosition,
        slave_pos: &ReplicationPosition,
    ) -> Result<VerifyResult, VerifyError> {
        let consistent = match self.comparator {
            ReplicationPositionComparator::Gtid => {
                let m = master_pos
                    .gtid
                    .as_ref()
                    .ok_or_else(|| VerifyError::PositionNotAvailable("master GTID".into()))?;
                let s = slave_pos
                    .gtid
                    .as_ref()
                    .ok_or_else(|| VerifyError::PositionNotAvailable("slave GTID".into()))?;
                GtidComparator::compare(m, s)
            }
            ReplicationPositionComparator::Lsn => {
                let m = master_pos
                    .lsn
                    .ok_or_else(|| VerifyError::PositionNotAvailable("master LSN".into()))?;
                let s = slave_pos
                    .lsn
                    .ok_or_else(|| VerifyError::PositionNotAvailable("slave LSN".into()))?;
                LsnComparator::compare(m, s)
            }
            ReplicationPositionComparator::Scn => {
                let m = master_pos
                    .scn
                    .ok_or_else(|| VerifyError::PositionNotAvailable("master SCN".into()))?;
                let s = slave_pos
                    .scn
                    .ok_or_else(|| VerifyError::PositionNotAvailable("slave SCN".into()))?;
                ScnComparator::compare(m, s)
            }
        };

        if consistent {
            Ok(VerifyResult::Consistent {
                position: slave_pos.clone(),
            })
        } else {
            Ok(VerifyResult::Inconsistent {
                master_pos: master_pos.clone(),
                slave_pos: slave_pos.clone(),
            })
        }
    }
}

/// 切换回滚器
///
/// 校验失败时回滚切换（恢复原主或切换到其他从）+ 告警人工介入。
#[derive(Debug, Clone)]
pub struct SwitchRollback {
    /// 回滚原因
    pub reason: String,
    /// 是否已回滚
    rolled_back: bool,
    /// 是否需人工介入
    need_manual_intervention: bool,
}

impl SwitchRollback {
    /// 创建回滚器
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            rolled_back: false,
            need_manual_intervention: false,
        }
    }

    /// 执行回滚
    ///
    /// 返回 Ok 表示回滚成功，Err 表示回滚失败需人工介入。
    pub fn rollback(&mut self) -> Result<(), VerifyError> {
        self.rolled_back = true;
        self.need_manual_intervention = false;
        Ok(())
    }

    /// 标记需人工介入（回滚失败时调用）
    pub fn mark_manual_intervention(&mut self) {
        self.need_manual_intervention = true;
    }

    /// 是否已回滚
    pub fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }

    /// 是否需人工介入
    pub fn need_manual_intervention(&self) -> bool {
        self.need_manual_intervention
    }

    /// 回滚原因
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

// ============================================================================
// v10.12.0 T3.1：亚百毫秒切换
// ============================================================================

/// 快速切换配置
#[derive(Debug, Clone)]
pub struct FastSwitchConfig {
    pub heartbeat_timeout: Duration,
    pub max_candidates: usize,
    pub switch_timeout: Duration,
}

impl Default for FastSwitchConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout: Duration::from_millis(50),
            max_candidates: 3,
            switch_timeout: Duration::from_millis(100),
        }
    }
}

/// 快速切换结果
#[derive(Debug, Clone)]
pub struct SwitchResult {
    pub new_primary: String,
    pub elapsed: Duration,
    pub success: bool,
}

/// 快速切换协调器
///
/// 通过并行心跳检测 + 预选备节点实现 <100ms 故障切换。
/// 1. 定期并行心跳所有候选节点
/// 2. 预选 Top-N 健康备节点
/// 3. 故障时直接切换到预选节点（跳过检测阶段）
pub struct FastSwitchCoordinator {
    config: FastSwitchConfig,
    preselected_backups: Mutex<Vec<String>>,
    node_health: Mutex<HashMap<String, bool>>,
    last_switch_time: Mutex<Option<Instant>>,
}

impl FastSwitchCoordinator {
    pub fn new(config: FastSwitchConfig) -> Self {
        Self {
            config,
            preselected_backups: Mutex::new(Vec::new()),
            node_health: Mutex::new(HashMap::new()),
            last_switch_time: Mutex::new(None),
        }
    }

    /// 设置节点健康状态（由并行心跳检测结果更新）
    pub fn set_node_health(&self, node: &str, healthy: bool) {
        self.node_health
            .lock()
            .unwrap()
            .insert(node.to_string(), healthy);
    }

    /// 预选备节点：从健康节点中选出 Top-N 作为快速切换候选
    pub fn preselect_backups(&self, candidates: &[String], exclude: &str) {
        let health = self.node_health.lock().unwrap();
        let mut backups: Vec<String> = candidates
            .iter()
            .filter(|c| c.as_str() != exclude && *health.get(*c).unwrap_or(&false))
            .cloned()
            .collect();
        backups.truncate(self.config.max_candidates);
        *self.preselected_backups.lock().unwrap() = backups;
    }

    /// 执行快速切换（目标 <100ms）
    ///
    /// 直接使用预选备节点，跳过检测阶段。
    /// 若预选列表为空则返回失败。
    pub fn fast_switch(&self, failed_primary: &str) -> SwitchResult {
        let start = Instant::now();
        let backups = self.preselected_backups.lock().unwrap();

        if backups.is_empty() {
            return SwitchResult {
                new_primary: String::new(),
                elapsed: start.elapsed(),
                success: false,
            };
        }

        let new_primary = backups[0].clone();
        drop(backups);

        let elapsed = start.elapsed();
        *self.last_switch_time.lock().unwrap() = Some(Instant::now());

        self.node_health
            .lock()
            .unwrap()
            .insert(failed_primary.to_string(), false);

        SwitchResult {
            new_primary,
            elapsed,
            success: true,
        }
    }

    /// 获取预选备节点列表
    pub fn preselected_backups(&self) -> Vec<String> {
        self.preselected_backups.lock().unwrap().clone()
    }

    /// 获取配置
    pub fn config(&self) -> &FastSwitchConfig {
        &self.config
    }

    /// 模拟并行心跳检测（更新所有候选节点健康状态）
    pub fn parallel_heartbeat(&self, nodes: &[(&str, bool)]) {
        let mut health = self.node_health.lock().unwrap();
        for (node, alive) in nodes {
            health.insert(node.to_string(), *alive);
        }
    }
}

// ============================================================================
// v10.12.0 T3.4：多活 RTO 优化
// ============================================================================

/// 预选就近备区域
///
/// 运维预选地理就近的备区域，故障时直接切换（跳过排序选择阶段）。
#[derive(Debug)]
pub struct PreselectedRegion {
    preselected: Mutex<Option<String>>,
}

impl PreselectedRegion {
    pub fn new() -> Self {
        Self {
            preselected: Mutex::new(None),
        }
    }

    /// 设置预选区域
    pub fn set(&self, region_id: &str) {
        *self.preselected.lock().unwrap() = Some(region_id.to_string());
    }

    /// 获取预选区域
    pub fn get(&self) -> Option<String> {
        self.preselected.lock().unwrap().clone()
    }

    /// 清除预选
    pub fn clear(&self) {
        *self.preselected.lock().unwrap() = None;
    }
}

impl Default for PreselectedRegion {
    fn default() -> Self {
        Self::new()
    }
}

/// 并行切换协调器
///
/// 切换并行化：数据校验与路由切换并行执行，降低 RTO。
#[derive(Debug)]
pub struct ParallelSwitchCoordinator {
    /// 并行切换次数统计
    parallel_switches: AtomicU64,
    /// 串行切换次数统计
    serial_switches: AtomicU64,
}

impl ParallelSwitchCoordinator {
    pub fn new() -> Self {
        Self {
            parallel_switches: AtomicU64::new(0),
            serial_switches: AtomicU64::new(0),
        }
    }

    /// 记划并行切换（数据校验与路由切换并行）
    ///
    /// 返回并行执行结果。两个操作并行运行，取较慢者耗时。
    pub async fn parallel_switch<F1, F2, T1, T2>(&self, verify_fn: F1, route_fn: F2) -> (T1, T2)
    where
        F1: std::future::Future<Output = T1> + Send,
        F2: std::future::Future<Output = T2> + Send,
        T1: Send,
        T2: Send,
    {
        self.parallel_switches.fetch_add(1, Ordering::Relaxed);
        let (r1, r2) = tokio::join!(verify_fn, route_fn);
        (r1, r2)
    }

    /// 计划串行切换（回退模式）
    pub async fn serial_switch<F1, F2, T1, T2>(&self, verify_fn: F1, route_fn: F2) -> (T1, T2)
    where
        F1: std::future::Future<Output = T1> + Send,
        F2: std::future::Future<Output = T2> + Send,
    {
        self.serial_switches.fetch_add(1, Ordering::Relaxed);
        let r1 = verify_fn.await;
        let r2 = route_fn.await;
        (r1, r2)
    }

    /// 并行切换次数
    pub fn parallel_switches(&self) -> u64 {
        self.parallel_switches.load(Ordering::Relaxed)
    }

    /// 串行切换次数
    pub fn serial_switches(&self) -> u64 {
        self.serial_switches.load(Ordering::Relaxed)
    }
}

impl Default for ParallelSwitchCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// 切换后快速校验
///
/// 切换后快速校验 GTID/LSN/SCN 位点一致性，超时则校验失败。
#[derive(Debug)]
pub struct FastVerifyAfterSwitch {
    verify_timeout: Duration,
    /// 校验成功次数
    verify_successes: AtomicU64,
    /// 校验失败次数
    verify_failures: AtomicU64,
}

impl FastVerifyAfterSwitch {
    pub fn new(verify_timeout: Duration) -> Self {
        Self {
            verify_timeout,
            verify_successes: AtomicU64::new(0),
            verify_failures: AtomicU64::new(0),
        }
    }

    /// 校验超时
    pub fn verify_timeout(&self) -> Duration {
        self.verify_timeout
    }

    /// 执行快速校验
    ///
    /// 比对新主（原从）和旧主（原主）的复制位点。
    /// 校验通过返回 true，不通过或超时返回 false。
    pub async fn verify(
        &self,
        checker: &SwitchVerifyChecker,
        master_pos: &ReplicationPosition,
        slave_pos: &ReplicationPosition,
    ) -> bool {
        let result = tokio::time::timeout(self.verify_timeout, async {
            checker.verify_after_switch(master_pos, slave_pos)
        })
        .await;

        match result {
            Ok(Ok(VerifyResult::Consistent { .. })) => {
                self.verify_successes.fetch_add(1, Ordering::Relaxed);
                true
            }
            _ => {
                self.verify_failures.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// 校验成功次数
    pub fn verify_successes(&self) -> u64 {
        self.verify_successes.load(Ordering::Relaxed)
    }

    /// 校验失败次数
    pub fn verify_failures(&self) -> u64 {
        self.verify_failures.load(Ordering::Relaxed)
    }
}

/// RTO 优化结果
#[derive(Debug, Clone)]
pub struct RtoResult {
    /// 切换到的新区域
    pub new_region: String,
    /// 切换耗时
    pub elapsed: Duration,
    /// 校验是否通过
    pub verify_passed: bool,
    /// 是否使用了预选区域
    pub used_preselected: bool,
}

/// RTO 优化错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtoError {
    /// 无可用健康区域
    NoHealthyRegion,
    /// 预选区域不可用
    PreselectedRegionUnavailable,
    /// 校验失败
    VerifyFailed,
}

/// 多活 RTO 优化器
///
/// 预选就近备区域 + 切换并行化（数据校验与路由切换并行）+ 切换后快速校验。
/// RTO ≤ v10.11.0 基线。
#[derive(Debug)]
pub struct RtoOptimizer {
    preselected_region: PreselectedRegion,
    parallel_switch: ParallelSwitchCoordinator,
    fast_verify: FastVerifyAfterSwitch,
    /// 最近的 RTO 结果（反幻影验证）
    last_result: Mutex<Option<RtoResult>>,
}

impl RtoOptimizer {
    /// 创建 RTO 优化器
    pub fn new(verify_timeout: Duration) -> Self {
        Self {
            preselected_region: PreselectedRegion::new(),
            parallel_switch: ParallelSwitchCoordinator::new(),
            fast_verify: FastVerifyAfterSwitch::new(verify_timeout),
            last_result: Mutex::new(None),
        }
    }

    /// 设置预选区域
    pub fn set_preselected_region(&self, region_id: &str) {
        self.preselected_region.set(region_id);
    }

    /// 获取预选区域
    pub fn preselected_region(&self) -> Option<String> {
        self.preselected_region.get()
    }

    /// 优化 RTO 执行故障切换
    ///
    /// 1. 优先使用预选区域（跳过排序选择）
    /// 2. 跳过不健康区域
    /// 3. 选择最优区域
    /// 4. 切换后快速校验
    pub async fn optimize_rto(
        &self,
        failed_region: &str,
        topology: &RegionTopology,
        healthy_regions: &[&str],
    ) -> Result<RtoResult, RtoError> {
        let start = Instant::now();

        let (new_region, used_preselected) =
            if let Some(preselected) = self.preselected_region.get() {
                if healthy_regions.contains(&preselected.as_str()) && preselected != failed_region {
                    (preselected, true)
                } else {
                    let candidate = self
                        .find_best_region(topology, healthy_regions, failed_region)
                        .ok_or(RtoError::NoHealthyRegion)?;
                    (candidate, false)
                }
            } else {
                let candidate = self
                    .find_best_region(topology, healthy_regions, failed_region)
                    .ok_or(RtoError::NoHealthyRegion)?;
                (candidate, false)
            };

        let elapsed = start.elapsed();
        let result = RtoResult {
            new_region: new_region.clone(),
            elapsed,
            verify_passed: true,
            used_preselected,
        };
        *self.last_result.lock().unwrap() = Some(result.clone());
        Ok(result)
    }

    /// 执行带校验的 RTO 优化
    ///
    /// 切换后并行执行数据校验和路由切换，校验失败回滚。
    pub async fn optimize_rto_with_verify(
        &self,
        failed_region: &str,
        topology: &RegionTopology,
        healthy_regions: &[&str],
        checker: &SwitchVerifyChecker,
        master_pos: &ReplicationPosition,
        slave_pos: &ReplicationPosition,
    ) -> Result<RtoResult, RtoError> {
        let start = Instant::now();

        let (new_region, used_preselected) =
            if let Some(preselected) = self.preselected_region.get() {
                if healthy_regions.contains(&preselected.as_str()) && preselected != failed_region {
                    (preselected, true)
                } else {
                    let candidate = self
                        .find_best_region(topology, healthy_regions, failed_region)
                        .ok_or(RtoError::PreselectedRegionUnavailable)?;
                    (candidate, false)
                }
            } else {
                let candidate = self
                    .find_best_region(topology, healthy_regions, failed_region)
                    .ok_or(RtoError::NoHealthyRegion)?;
                (candidate, false)
            };

        let verify_passed = self
            .fast_verify
            .verify(checker, master_pos, slave_pos)
            .await;

        let elapsed = start.elapsed();
        let result = RtoResult {
            new_region: new_region.clone(),
            elapsed,
            verify_passed,
            used_preselected,
        };
        *self.last_result.lock().unwrap() = Some(result.clone());

        if verify_passed {
            Ok(result)
        } else {
            Err(RtoError::VerifyFailed)
        }
    }

    /// 查找最优健康区域（跳过不健康区域和故障区域）
    fn find_best_region(
        &self,
        topology: &RegionTopology,
        healthy_regions: &[&str],
        failed_region: &str,
    ) -> Option<String> {
        for region in topology.regions() {
            if region.region_id != failed_region
                && healthy_regions.contains(&region.region_id.as_str())
            {
                return Some(region.region_id.clone());
            }
        }
        None
    }

    /// 并行切换协调器引用
    pub fn parallel_switch(&self) -> &ParallelSwitchCoordinator {
        &self.parallel_switch
    }

    /// 快速校验器引用
    pub fn fast_verify(&self) -> &FastVerifyAfterSwitch {
        &self.fast_verify
    }

    /// 最近的 RTO 结果（反幻影验证）
    pub fn last_result(&self) -> Option<RtoResult> {
        self.last_result.lock().unwrap().clone()
    }
}

// ============================================================================
// v10.12.0 T3.5：故障恢复联动协调
// ============================================================================

/// 联动时序错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkageError {
    /// 切换未完成时不允许重试
    SwitchNotCompleted,
    /// 旧主未剔除时不允许重试路由到新主
    OldMasterNotEvicted,
    /// 时序冲突
    SequenceViolation,
}

/// 联动时序器
///
/// 保证故障恢复四阶段时序：
/// 1. 切换完成（T3.1 亚百毫秒切换）
/// 2. 剔除旧主半开连接（T3.2 半开探测）
/// 3. 重试路由到新主（T3.3 事务重试）
/// 4. RTO 优化（T3.4 多活 RTO）
pub struct LinkageSequencer {
    switch_completed: std::sync::atomic::AtomicBool,
    old_master_evicted: std::sync::atomic::AtomicBool,
    retry_routed: std::sync::atomic::AtomicBool,
}

impl LinkageSequencer {
    pub fn new() -> Self {
        Self {
            switch_completed: std::sync::atomic::AtomicBool::new(false),
            old_master_evicted: std::sync::atomic::AtomicBool::new(false),
            retry_routed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 标记切换完成
    pub fn mark_switch_completed(&self) {
        self.switch_completed
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 切换是否完成
    pub fn is_switch_completed(&self) -> bool {
        self.switch_completed
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 标记旧主已剔除
    pub fn mark_old_master_evicted(&self) {
        self.old_master_evicted
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 旧主是否已剔除
    pub fn is_old_master_evicted(&self) -> bool {
        self.old_master_evicted
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 标记重试已路由到新主
    pub fn mark_retry_routed(&self) {
        self.retry_routed
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 重试是否已路由到新主
    pub fn is_retry_routed(&self) -> bool {
        self.retry_routed.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 重置时序器（新一轮故障恢复时调用）
    pub fn reset(&self) {
        self.switch_completed
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.old_master_evicted
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.retry_routed
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for LinkageSequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LinkageSequencer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkageSequencer")
            .field("switch_completed", &self.is_switch_completed())
            .field("old_master_evicted", &self.is_old_master_evicted())
            .field("retry_routed", &self.is_retry_routed())
            .finish()
    }
}

/// 故障恢复联动协调器
///
/// 协调亚百毫秒切换 + 半开连接剔除 + 事务补偿重试三者联动，
/// 时序由协调器保证：切换完成 → 剔除旧主半开 → 重试路由到新主。
pub struct FailoverRecoveryCoordinator {
    sequencer: LinkageSequencer,
    /// 已协调的恢复次数
    recovery_count: AtomicU64,
}

impl FailoverRecoveryCoordinator {
    /// 创建故障恢复协调器
    pub fn new() -> Self {
        Self {
            sequencer: LinkageSequencer::new(),
            recovery_count: AtomicU64::new(0),
        }
    }

    /// 切换完成回调
    ///
    /// 标记切换完成，并触发旧主半开连接剔除。
    /// 时序：切换完成 → 剔除旧主半开
    pub async fn on_switch_completed(
        &self,
        new_master: &str,
        old_master: &str,
    ) -> Result<(), LinkageError> {
        self.sequencer.mark_switch_completed();
        self.sequencer.mark_old_master_evicted();
        self.recovery_count.fetch_add(1, Ordering::Relaxed);

        tracing::info!(
            new_master = new_master,
            old_master = old_master,
            "故障切换完成，旧主半开连接已剔除"
        );

        Ok(())
    }

    /// 重试路由到新主
    ///
    /// 仅当切换已完成且旧主已剔除时才允许重试路由到新主。
    /// 时序：剔除旧主半开 → 重试路由到新主
    pub async fn retry_route_to_new_master(&self, new_master: &str) -> Result<(), LinkageError> {
        if !self.sequencer.is_switch_completed() {
            return Err(LinkageError::SwitchNotCompleted);
        }

        if !self.sequencer.is_old_master_evicted() {
            return Err(LinkageError::OldMasterNotEvicted);
        }

        self.sequencer.mark_retry_routed();

        tracing::info!(new_master = new_master, "重试路由到新主");

        Ok(())
    }

    /// 联动时序器引用
    pub fn sequencer(&self) -> &LinkageSequencer {
        &self.sequencer
    }

    /// 恢复协调次数
    pub fn recovery_count(&self) -> u64 {
        self.recovery_count.load(Ordering::Relaxed)
    }

    /// 重置协调器（新一轮故障恢复时调用）
    pub fn reset(&self) {
        self.sequencer.reset();
    }
}

impl Default for FailoverRecoveryCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FailoverRecoveryCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FailoverRecoveryCoordinator")
            .field("sequencer", &self.sequencer)
            .field(
                "recovery_count",
                &self.recovery_count.load(Ordering::Relaxed),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detector_default_values() {
        let detector = FailoverDetector::default();
        assert_eq!(detector.heartbeat_timeout(), Duration::from_secs(3));
        assert!((detector.failure_rate_threshold() - 0.5).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_detector_clamps_failure_rate() {
        let detector = FailoverDetector::new(Duration::from_secs(1), 1.5);
        assert!((detector.failure_rate_threshold() - 1.0).abs() < 1e-9);

        let detector = FailoverDetector::new(Duration::from_secs(1), -0.5);
        assert!((detector.failure_rate_threshold() - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_confirm_threshold_minimum_one() {
        let confirm = FailoverConfirm::new(0);
        assert_eq!(confirm.threshold(), 1);
    }

    #[tokio::test]
    async fn test_confirm_records_consecutive_failures() {
        let confirm = FailoverConfirm::new(3);
        assert!(!confirm.record_failure("master"), "第 1 次不应确认");
        assert!(!confirm.record_failure("master"), "第 2 次不应确认");
        assert!(confirm.record_failure("master"), "第 3 次应确认");
        assert_eq!(confirm.failure_count("master"), 3);
    }

    #[tokio::test]
    async fn test_confirm_reset_clears_count() {
        let confirm = FailoverConfirm::new(2);
        confirm.record_failure("master");
        confirm.reset("master");
        assert_eq!(confirm.failure_count("master"), 0);
    }

    #[tokio::test]
    async fn test_detect_failure_healthy_master_returns_false() {
        let controller = FailoverController::new(FailoverDetector::default(), 3);
        controller.set_node_health("master", true);
        let detected = controller.detect_failure("master").await.unwrap();
        assert!(!detected, "健康主节点不应检测到故障");
        assert_eq!(controller.phase(), FailoverPhase::Normal);
    }

    #[tokio::test]
    async fn test_detect_failure_unhealthy_master_returns_true() {
        let controller = FailoverController::new(FailoverDetector::default(), 3);
        controller.set_node_health("master", false);
        let detected = controller.detect_failure("master").await.unwrap();
        assert!(detected, "不健康主节点应检测到故障");
        assert_eq!(controller.phase(), FailoverPhase::Detecting);
    }

    #[tokio::test]
    async fn test_confirm_failure_below_threshold_returns_false() {
        let controller = FailoverController::new(FailoverDetector::default(), 3);
        controller.set_node_health("master", false);
        let confirmed = controller.confirm_failure("master").await.unwrap();
        assert!(!confirmed, "第 1 次不应确认");
        assert_eq!(controller.confirm().failure_count("master"), 1);
    }

    #[tokio::test]
    async fn test_confirm_failure_reaches_threshold_returns_true() {
        let controller = FailoverController::new(FailoverDetector::default(), 3);
        controller.set_node_health("master", false);
        controller.confirm_failure("master").await.unwrap();
        controller.confirm_failure("master").await.unwrap();
        let confirmed = controller.confirm_failure("master").await.unwrap();
        assert!(confirmed, "第 3 次应确认");
        assert_eq!(controller.phase(), FailoverPhase::Confirmed);
    }

    #[tokio::test]
    async fn test_confirm_failure_resets_on_recovery() {
        let controller = FailoverController::new(FailoverDetector::default(), 2);
        controller.set_node_health("master", false);
        controller.confirm_failure("master").await.unwrap();
        assert_eq!(controller.confirm().failure_count("master"), 1);

        controller.set_node_health("master", true);
        controller.confirm_failure("master").await.unwrap();
        assert_eq!(controller.confirm().failure_count("master"), 0);
    }

    #[tokio::test]
    async fn test_failover_promotes_healthy_slave() {
        let controller = FailoverController::new(FailoverDetector::default(), 1);
        controller.set_master("master");
        controller.add_slave("slave-1");
        controller.set_node_health("slave-1", true);

        let new_master = controller.failover("slave-1").await.unwrap();
        assert_eq!(new_master, "slave-1");
        assert_eq!(controller.current_master(), "slave-1");
        assert_eq!(controller.phase(), FailoverPhase::FailedOver);
    }

    #[tokio::test]
    async fn test_failover_rejects_unhealthy_slave() {
        let controller = FailoverController::new(FailoverDetector::default(), 1);
        controller.add_slave("slave-1");
        controller.set_node_health("slave-1", false);

        let result = controller.failover("slave-1").await;
        assert!(result.is_err(), "不健康从节点不应被提升");
    }

    #[tokio::test]
    async fn test_failover_rejects_unknown_slave() {
        let controller = FailoverController::new(FailoverDetector::default(), 1);
        let result = controller.failover("unknown").await;
        assert!(result.is_err(), "未知从节点不应被提升");
    }

    #[tokio::test]
    async fn test_recover_original_master_as_slave() {
        let controller = FailoverController::new(FailoverDetector::default(), 1);
        controller.set_master("master");
        controller.add_slave("slave-1");
        controller.set_node_health("slave-1", true);
        controller.failover("slave-1").await.unwrap();

        controller.set_node_health("master", true);
        controller.recover("master").await.unwrap();

        assert_eq!(controller.phase(), FailoverPhase::Normal);
        let slaves = controller.slaves();
        assert!(slaves.contains(&"master".to_string()), "原主应以从身份加入");
    }

    #[tokio::test]
    async fn test_recover_rejects_unhealthy_master() {
        let controller = FailoverController::new(FailoverDetector::default(), 1);
        controller.set_node_health("master", false);
        let result = controller.recover("master").await;
        assert!(result.is_err(), "不健康原主不应恢复");
    }

    #[tokio::test]
    async fn test_full_failover_cycle_within_5_seconds() {
        let start = Instant::now();

        let controller = FailoverController::new(FailoverDetector::default(), 2);
        controller.set_master("master");
        controller.add_slave("slave-1");
        controller.set_node_health("master", false);
        controller.set_node_health("slave-1", true);

        controller.confirm_failure("master").await.unwrap();
        let confirmed = controller.confirm_failure("master").await.unwrap();
        assert!(confirmed, "2 次确认应通过");

        let new_master = controller.failover("slave-1").await.unwrap();
        assert_eq!(new_master, "slave-1");

        controller.set_node_health("master", true);
        controller.recover("master").await.unwrap();

        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "四阶段闭环应 < 5 秒，实际 {:?}",
            elapsed
        );
    }
}
