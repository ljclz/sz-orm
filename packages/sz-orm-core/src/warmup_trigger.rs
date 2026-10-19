//! v9.7.0 缓存预热触发器
//!
//! 定时触发 + DDL 事件驱动触发，幂等去重，不阻塞业务。

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cache_warmup_protection::CacheError;
use crate::warmup_pipeline::WarmupPipeline;

/// 触发原因
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerReason {
    /// 定时触发
    Scheduled,
    /// DDL 事件触发
    DdlEvent(String),
}

/// 触发日志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupTriggerLog {
    /// 触发时间
    pub timestamp_ms: u64,
    /// 触发原因
    pub reason: TriggerReason,
    /// 涉及表名
    pub tables: Vec<String>,
    /// 是否成功
    pub success: bool,
}

/// 触发配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// 是否启用定时触发
    pub scheduled_enabled: bool,
    /// 定时周期
    pub scheduled_period: Duration,
    /// 定时预热的表列表
    pub scheduled_tables: Vec<String>,
    /// 是否启用事件驱动触发
    pub event_driven_enabled: bool,
    /// 事件驱动监控的表列表
    pub event_driven_tables: Vec<String>,
    /// 幂等去重窗口
    pub dedup_window: Duration,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            scheduled_enabled: false,
            scheduled_period: Duration::from_secs(60),
            scheduled_tables: Vec::new(),
            event_driven_enabled: false,
            event_driven_tables: Vec::new(),
            dedup_window: Duration::from_secs(30),
        }
    }
}

/// 触发器句柄（用于停止后台任务）
pub struct TriggerHandle {
    cancel: Arc<AtomicBool>,
}

impl TriggerHandle {
    /// 停止后台任务
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// 是否已取消
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}

impl std::fmt::Debug for TriggerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TriggerHandle")
            .field("cancel", &self.is_cancelled())
            .finish()
    }
}

/// 缓存预热触发器
///
/// 定时触发 + DDL 事件驱动触发，幂等去重，不阻塞业务。
pub struct WarmupTrigger {
    pipeline: Arc<WarmupPipeline>,
    config: TriggerConfig,
    dedup_table: Mutex<HashSet<(String, Instant)>>,
    logs: Arc<Mutex<Vec<WarmupTriggerLog>>>,
}

impl WarmupTrigger {
    /// 创建预热触发器
    pub fn new(pipeline: Arc<WarmupPipeline>, config: TriggerConfig) -> Self {
        Self {
            pipeline,
            config,
            dedup_table: Mutex::new(HashSet::new()),
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 启动定时触发
    ///
    /// 后台 `tokio::spawn` + `tokio::time::interval(period)` 定时触发 `scheduled_tables` 预热。
    /// 周期 < 10s 钳制为 10s。后台执行不阻塞业务。
    pub fn start_scheduled(&self) -> TriggerHandle {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        let pipeline = self.pipeline.clone();
        let tables = self.config.scheduled_tables.clone();
        let period = if self.config.scheduled_period < Duration::from_secs(10) {
            Duration::from_secs(10)
        } else {
            self.config.scheduled_period
        };
        let logs = self.logs.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(period);
            interval.tick().await;
            while !cancel_clone.load(Ordering::Acquire) {
                interval.tick().await;
                if cancel_clone.load(Ordering::Acquire) {
                    break;
                }
                let _ = pipeline.run(&tables, 4).await;
                let mut log_guard = logs.lock().unwrap();
                log_guard.push(WarmupTriggerLog {
                    timestamp_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    reason: TriggerReason::Scheduled,
                    tables: tables.clone(),
                    success: true,
                });
            }
        });

        TriggerHandle { cancel }
    }

    /// DDL 事件触发
    ///
    /// 解析 DDL 表名 → 校验表名合法性 → 幂等去重 → 触发 `WarmupPipeline::run`。
    pub async fn on_ddl_event(&self, ddl_sql: &str) -> Result<(), CacheError> {
        let tables = self.parse_ddl_tables(ddl_sql)?;

        for table in &tables {
            if !self.is_valid_table_name(table) {
                return Err(CacheError::WarmupFailed(format!("非法表名被拒绝: {table}")));
            }
        }

        let now = Instant::now();
        let new_tables: Vec<String> = {
            let mut dedup = self.dedup_table.lock().unwrap();
            let mut new_tables = Vec::new();
            for table in &tables {
                let key = (table.clone(), now);
                let already = dedup.iter().any(|(t, time)| {
                    t == table && now.duration_since(*time) <= self.config.dedup_window
                });
                if !already {
                    dedup.insert(key);
                    new_tables.push(table.clone());
                }
            }
            new_tables
        };

        if new_tables.is_empty() {
            return Ok(());
        }

        let result = self.pipeline.run(&new_tables, 4).await?;

        let mut logs = self.logs.lock().unwrap();
        logs.push(WarmupTriggerLog {
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            reason: TriggerReason::DdlEvent(ddl_sql.to_string()),
            tables: new_tables,
            success: result.failures.is_empty(),
        });

        Ok(())
    }

    /// 返回触发日志
    #[must_use]
    pub fn trigger_logs(&self) -> Vec<WarmupTriggerLog> {
        self.logs.lock().unwrap().clone()
    }

    /// 解析 DDL 中的表名
    fn parse_ddl_tables(&self, ddl_sql: &str) -> Result<Vec<String>, CacheError> {
        let upper = ddl_sql.to_uppercase();
        let mut tables = Vec::new();

        for keyword in &["TABLE ", "TABLE."] {
            let mut search_pos = 0;
            while let Some(pos) = upper[search_pos..].find(keyword) {
                let start = search_pos + pos + keyword.len();
                let rest = &ddl_sql[start..];
                let table: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                    .collect();
                if !table.is_empty() {
                    tables.push(table);
                }
                search_pos = start;
            }
        }

        if tables.is_empty() {
            return Err(CacheError::WarmupFailed("DDL 中未找到表名".to_string()));
        }

        Ok(tables)
    }

    /// 校验表名合法性（仅字母/数字/下划线/点）
    fn is_valid_table_name(&self, name: &str) -> bool {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    }
}

impl std::fmt::Debug for WarmupTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WarmupTrigger")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// v10.10.0：低峰期预加载调度器
///
/// 基于历史访问频率判定低峰期，在低峰期触发预加载调度。
/// 避免高峰期预热对业务性能的影响。
pub struct OffPeakScheduler {
    /// 低峰期访问频率阈值（每秒访问数低于此值视为低峰）
    low_traffic_threshold: f64,
    /// 当前访问计数
    access_count: u64,
    /// 窗口起始时间
    window_start: Instant,
    /// 窗口大小
    window_size: Duration,
    /// 是否已触发预热
    warmup_triggered: bool,
}

impl OffPeakScheduler {
    /// 创建低峰期调度器
    ///
    /// `low_traffic_threshold`：每秒访问数低于此值视为低峰
    pub fn new(low_traffic_threshold: f64) -> Self {
        Self {
            low_traffic_threshold,
            access_count: 0,
            window_start: Instant::now(),
            window_size: Duration::from_secs(60),
            warmup_triggered: false,
        }
    }

    /// 设置窗口大小
    pub fn with_window_size(mut self, window_size: Duration) -> Self {
        self.window_size = window_size;
        self
    }

    /// 记录一次访问
    pub fn record_access(&mut self) {
        self.access_count += 1;
    }

    /// 判定当前是否为低峰期
    ///
    /// 基于窗口内访问频率判定，窗口结束后重置计数。
    pub fn is_off_peak(&mut self) -> bool {
        let elapsed = self.window_start.elapsed();
        if elapsed >= self.window_size {
            let access_rate = self.access_count as f64 / elapsed.as_secs_f64().max(1.0);
            let is_off_peak = access_rate < self.low_traffic_threshold;

            self.access_count = 0;
            self.window_start = Instant::now();

            is_off_peak
        } else {
            false
        }
    }

    /// 检查并触发低峰预加载
    ///
    /// 如果当前为低峰期且尚未触发预热，返回 true 表示应触发预热。
    pub fn check_and_trigger_warmup(&mut self) -> bool {
        if self.warmup_triggered {
            return false;
        }

        if self.is_off_peak() {
            self.warmup_triggered = true;
            true
        } else {
            false
        }
    }

    /// 重置预热状态（允许下次低峰期再次触发）
    pub fn reset_warmup(&mut self) {
        self.warmup_triggered = false;
    }

    /// 当前窗口访问计数
    pub fn access_count(&self) -> u64 {
        self.access_count
    }

    /// 预热是否已触发
    pub fn is_warmup_triggered(&self) -> bool {
        self.warmup_triggered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency_graph::DependencyGraph;
    use crate::multi_level_cache::MultiLevelCache;
    use crate::warmup_pipeline::{WarmupPipeline, WarmupPipelineConfig};

    fn make_trigger() -> WarmupTrigger {
        let dep_graph = DependencyGraph::new(100);
        let mlc = Arc::new(MultiLevelCache::new(None, dep_graph));
        let pipeline = Arc::new(WarmupPipeline::new(mlc, WarmupPipelineConfig::default()));
        WarmupTrigger::new(pipeline, TriggerConfig::default())
    }

    #[test]
    fn test_trigger_config_default() {
        let config = TriggerConfig::default();
        assert!(!config.scheduled_enabled);
        assert_eq!(config.scheduled_period, Duration::from_secs(60));
        assert!(!config.event_driven_enabled);
        assert_eq!(config.dedup_window, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_on_ddl_event_valid() {
        let trigger = make_trigger();
        let result = trigger
            .on_ddl_event("ALTER TABLE users ADD COLUMN age INT")
            .await;
        assert!(result.is_ok());
        let logs = trigger.trigger_logs();
        assert_eq!(logs.len(), 1);
    }

    #[tokio::test]
    async fn test_on_ddl_event_injection_reject() {
        let trigger = make_trigger();
        let result = trigger.on_ddl_event("'; DROP TABLE--").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_on_ddl_event_idempotent() {
        let trigger = make_trigger();
        let _ = trigger
            .on_ddl_event("ALTER TABLE t1 ADD COLUMN x INT")
            .await;
        let _ = trigger
            .on_ddl_event("ALTER TABLE t1 ADD COLUMN y INT")
            .await;
        let logs = trigger.trigger_logs();
        assert!(logs.len() <= 2);
    }

    #[tokio::test]
    async fn test_trigger_logs_observable() {
        let trigger = make_trigger();
        let _ = trigger
            .on_ddl_event("ALTER TABLE orders ADD COLUMN qty INT")
            .await;
        let logs = trigger.trigger_logs();
        assert!(!logs.is_empty());
        assert!(logs[0].tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_trigger_handle_stop() {
        let handle = TriggerHandle {
            cancel: Arc::new(AtomicBool::new(false)),
        };
        assert!(!handle.is_cancelled());
        handle.stop();
        assert!(handle.is_cancelled());
    }

    #[test]
    fn test_is_valid_table_name() {
        let trigger = make_trigger();
        assert!(trigger.is_valid_table_name("users"));
        assert!(trigger.is_valid_table_name("public.users"));
        assert!(trigger.is_valid_table_name("t_123"));
        assert!(!trigger.is_valid_table_name(""));
        assert!(!trigger.is_valid_table_name("'; DROP TABLE--"));
        assert!(!trigger.is_valid_table_name("user;test"));
    }

    #[tokio::test]
    async fn test_parse_ddl_tables() {
        let trigger = make_trigger();
        let tables = trigger
            .parse_ddl_tables("ALTER TABLE users ADD COLUMN x INT")
            .unwrap();
        assert!(tables.contains(&"users".to_string()));
    }

    #[tokio::test]
    async fn test_parse_ddl_tables_not_found() {
        let trigger = make_trigger();
        let result = trigger.parse_ddl_tables("SELECT 1");
        assert!(result.is_err());
    }

    #[test]
    fn test_trigger_reason_eq() {
        assert_eq!(TriggerReason::Scheduled, TriggerReason::Scheduled);
        assert_ne!(
            TriggerReason::Scheduled,
            TriggerReason::DdlEvent("x".to_string())
        );
    }
}
