//! # 配置热更新机制（v6.9.0 REQ-CN-002）
//!
//! 支持 SIGHUP 信号触发配置重新加载，以及 ConfigMap 文件变更监听。
//! 可热更新项（日志级别、连接池上限）即时生效，
//! 不可热更新项（监听端口）忽略并输出告警。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime};

/// 配置项是否可热更新
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReloadPolicy {
    /// 可热更新（如日志级别、连接池上限）
    HotReloadable,
    /// 不可热更新，需重启（如监听端口）
    RequiresRestart,
}

/// 热更新配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotReloadConfigItem {
    /// 配置键
    pub key: String,
    /// 当前值
    pub value: String,
    /// 热更新策略
    pub policy: ReloadPolicy,
}

impl HotReloadConfigItem {
    /// 创建可热更新项
    pub fn hot(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            policy: ReloadPolicy::HotReloadable,
        }
    }

    /// 创建需重启项
    pub fn restart_required(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            policy: ReloadPolicy::RequiresRestart,
        }
    }
}

/// 热更新事件日志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadEvent {
    /// 事件时间戳
    pub timestamp: String,
    /// 触发源
    pub source: ReloadSource,
    /// 应用的配置项数量
    pub applied_count: usize,
    /// 跳过的配置项数量（需重启）
    pub skipped_count: usize,
    /// 跳过项的告警消息
    pub warnings: Vec<String>,
}

/// 热更新触发源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReloadSource {
    /// SIGHUP 信号
    SignalSighup,
    /// ConfigMap 文件变更
    FileWatch,
    /// 手动触发
    Manual,
}

/// ConfigMap 文件监听配置
#[derive(Debug, Clone)]
pub struct FileWatchConfig {
    /// 监听的文件路径
    pub path: PathBuf,
    /// 轮询间隔
    pub poll_interval: Duration,
}

impl FileWatchConfig {
    /// 创建文件监听配置
    pub fn new(path: impl Into<PathBuf>, poll_interval: Duration) -> Self {
        Self {
            path: path.into(),
            poll_interval,
        }
    }
}

/// 热更新管理器
///
/// 管理配置项的热更新策略，监听 SIGHUP 信号和文件变更，
/// 可热更新项即时生效，不可热更新项输出告警。
pub struct HotReloadManager {
    /// 配置项注册表
    items: RwLock<HashMap<String, HotReloadConfigItem>>,
    /// 事件历史
    events: Mutex<Vec<ReloadEvent>>,
    /// 文件监听配置
    file_watch: Option<FileWatchConfig>,
    /// 上次文件修改时间
    last_modified: Mutex<Option<SystemTime>>,
    /// 是否已初始化
    initialized: std::sync::atomic::AtomicBool,
}

impl Default for HotReloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HotReloadManager {
    /// 创建热更新管理器
    pub fn new() -> Self {
        Self {
            items: RwLock::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
            file_watch: None,
            last_modified: Mutex::new(None),
            initialized: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 创建带文件监听的热更新管理器
    pub fn with_file_watch(file_watch: FileWatchConfig) -> Self {
        let mut manager = Self::new();
        manager.file_watch = Some(file_watch);
        manager
    }

    /// 注册配置项
    pub fn register(&self, item: HotReloadConfigItem) {
        if let Ok(mut items) = self.items.write() {
            items.insert(item.key.clone(), item);
        }
    }

    /// 批量注册配置项
    pub fn register_batch(&self, items: Vec<HotReloadConfigItem>) {
        if let Ok(mut map) = self.items.write() {
            for item in items {
                map.insert(item.key.clone(), item);
            }
        }
    }

    /// 获取配置项
    pub fn get(&self, key: &str) -> Option<HotReloadConfigItem> {
        self.items.read().ok()?.get(key).cloned()
    }

    /// 应用配置变更
    ///
    /// 可热更新项即时生效，不可热更新项跳过并记录告警。
    /// 返回 ReloadEvent 记录。
    pub fn apply_changes(
        &self,
        changes: &HashMap<String, String>,
        source: ReloadSource,
    ) -> ReloadEvent {
        let mut applied = 0usize;
        let mut skipped = 0usize;
        let mut warnings = Vec::new();

        if let Ok(mut items) = self.items.write() {
            for (key, new_value) in changes {
                if let Some(item) = items.get_mut(key) {
                    match item.policy {
                        ReloadPolicy::HotReloadable => {
                            item.value = new_value.clone();
                            applied += 1;
                        }
                        ReloadPolicy::RequiresRestart => {
                            skipped += 1;
                            warnings.push(format!(
                                "{} change requires restart, ignoring new value '{}'",
                                key, new_value
                            ));
                        }
                    }
                }
            }
        }

        let event = ReloadEvent {
            timestamp: format!("{:?}", SystemTime::now()),
            source,
            applied_count: applied,
            skipped_count: skipped,
            warnings: warnings.clone(),
        };

        if let Ok(mut events) = self.events.lock() {
            events.push(event.clone());
        }

        event
    }

    /// 模拟 SIGHUP 信号触发热更新
    pub fn reload_via_signal(&self, changes: &HashMap<String, String>) -> ReloadEvent {
        self.apply_changes(changes, ReloadSource::SignalSighup)
    }

    /// 手动触发热更新
    pub fn reload_manual(&self, changes: &HashMap<String, String>) -> ReloadEvent {
        self.apply_changes(changes, ReloadSource::Manual)
    }

    /// 检查文件是否已变更（轮询模式）
    ///
    /// 返回 `Some(true)` 表示文件已变更，`Some(false)` 表示未变更，
    /// `None` 表示未配置文件监听或文件不存在。
    pub fn check_file_changed(&self) -> Option<bool> {
        let watch = self.file_watch.as_ref()?;
        let metadata = std::fs::metadata(&watch.path).ok()?;
        let mtime = metadata.modified().ok()?;

        let mut last = self.last_modified.lock().ok()?;
        match *last {
            Some(prev) if prev == mtime => Some(false),
            _ => {
                *last = Some(mtime);
                Some(true)
            }
        }
    }

    /// 从文件加载配置并触发热更新
    ///
    /// 读取 JSON 格式配置文件，解析为 `HashMap<String, String>` 后应用。
    pub fn reload_from_file(&self) -> Result<ReloadEvent, String> {
        let watch = self
            .file_watch
            .as_ref()
            .ok_or_else(|| "no file watch configured".to_string())?;

        let content = std::fs::read_to_string(&watch.path)
            .map_err(|e| format!("failed to read config file: {}", e))?;

        let changes: HashMap<String, String> = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse config JSON: {}", e))?;

        Ok(self.apply_changes(&changes, ReloadSource::FileWatch))
    }

    /// 获取事件历史
    pub fn events(&self) -> Vec<ReloadEvent> {
        self.events
            .lock()
            .ok()
            .map(|e| e.clone())
            .unwrap_or_default()
    }

    /// 标记初始化完成
    pub fn mark_initialized(&self) {
        self.initialized
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 是否已初始化完成
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// 验证配置值合法性
///
/// 返回 `Ok(())` 表示合法，`Err(message)` 表示非法。
pub fn validate_config_value(key: &str, value: &str) -> Result<(), String> {
    match key {
        "pool.max_connections" => {
            let v: i64 = value
                .parse()
                .map_err(|_| format!("{} must be a number, got '{}'", key, value))?;
            if v <= 0 {
                return Err(format!("{} must be positive, got {}", key, v));
            }
            Ok(())
        }
        "log.level" => {
            let valid = ["trace", "debug", "info", "warn", "error"];
            if !valid.contains(&value.to_lowercase().as_str()) {
                return Err(format!(
                    "{} must be one of {:?}, got '{}'",
                    key, valid, value
                ));
            }
            Ok(())
        }
        "server.port" => {
            let v: u16 = value
                .parse()
                .map_err(|_| format!("{} must be a valid port number, got '{}'", key, value))?;
            if v == 0 {
                return Err(format!("{} must not be 0", key));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 带验证的配置应用
///
/// 先验证所有变更，任一非法值则全部不应用（原子性）。
pub fn apply_with_validation(
    manager: &HotReloadManager,
    changes: &HashMap<String, String>,
    source: ReloadSource,
) -> Result<ReloadEvent, Vec<String>> {
    let mut errors = Vec::new();
    for (key, value) in changes {
        if let Err(e) = validate_config_value(key, value) {
            errors.push(format!("config validation failed for '{}': {}", key, e));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(manager.apply_changes(changes, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hot_reload_config_item_hot() {
        let item = HotReloadConfigItem::hot("log.level", "info");
        assert_eq!(item.policy, ReloadPolicy::HotReloadable);
        assert_eq!(item.key, "log.level");
    }

    #[test]
    fn test_hot_reload_config_item_restart() {
        let item = HotReloadConfigItem::restart_required("server.port", "8080");
        assert_eq!(item.policy, ReloadPolicy::RequiresRestart);
    }

    #[test]
    fn test_register_and_get() {
        let manager = HotReloadManager::new();
        manager.register(HotReloadConfigItem::hot("log.level", "info"));
        let item = manager.get("log.level").unwrap();
        assert_eq!(item.value, "info");
    }

    #[test]
    fn test_apply_changes_hot_reloadable() {
        let manager = HotReloadManager::new();
        manager.register(HotReloadConfigItem::hot("log.level", "info"));

        let mut changes = HashMap::new();
        changes.insert("log.level".to_string(), "debug".to_string());

        let event = manager.reload_via_signal(&changes);
        assert_eq!(event.applied_count, 1);
        assert_eq!(event.skipped_count, 0);
        assert!(event.warnings.is_empty());

        let item = manager.get("log.level").unwrap();
        assert_eq!(item.value, "debug");
    }

    #[test]
    fn test_apply_changes_requires_restart() {
        let manager = HotReloadManager::new();
        manager.register(HotReloadConfigItem::restart_required("server.port", "8080"));

        let mut changes = HashMap::new();
        changes.insert("server.port".to_string(), "9090".to_string());

        let event = manager.reload_via_signal(&changes);
        assert_eq!(event.applied_count, 0);
        assert_eq!(event.skipped_count, 1);
        assert!(event.warnings[0].contains("requires restart"));

        let item = manager.get("server.port").unwrap();
        assert_eq!(item.value, "8080");
    }

    #[test]
    fn test_apply_changes_mixed() {
        let manager = HotReloadManager::new();
        manager.register(HotReloadConfigItem::hot("log.level", "info"));
        manager.register(HotReloadConfigItem::hot("pool.max_connections", "10"));
        manager.register(HotReloadConfigItem::restart_required("server.port", "8080"));

        let mut changes = HashMap::new();
        changes.insert("log.level".to_string(), "warn".to_string());
        changes.insert("pool.max_connections".to_string(), "20".to_string());
        changes.insert("server.port".to_string(), "9090".to_string());

        let event = manager.reload_manual(&changes);
        assert_eq!(event.applied_count, 2);
        assert_eq!(event.skipped_count, 1);
    }

    #[test]
    fn test_event_history() {
        let manager = HotReloadManager::new();
        manager.register(HotReloadConfigItem::hot("log.level", "info"));

        let mut changes = HashMap::new();
        changes.insert("log.level".to_string(), "debug".to_string());

        manager.reload_via_signal(&changes);
        manager.reload_manual(&changes);

        let events = manager.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].source, ReloadSource::SignalSighup);
        assert_eq!(events[1].source, ReloadSource::Manual);
    }

    #[test]
    fn test_validate_config_value_valid() {
        assert!(validate_config_value("pool.max_connections", "10").is_ok());
        assert!(validate_config_value("log.level", "info").is_ok());
        assert!(validate_config_value("server.port", "8080").is_ok());
    }

    #[test]
    fn test_validate_config_value_invalid_pool() {
        let result = validate_config_value("pool.max_connections", "-5");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("positive"));
    }

    #[test]
    fn test_validate_config_value_invalid_log_level() {
        let result = validate_config_value("log.level", "verbose");
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_with_validation_rejects_invalid() {
        let manager = HotReloadManager::new();
        manager.register(HotReloadConfigItem::hot("pool.max_connections", "10"));

        let mut changes = HashMap::new();
        changes.insert("pool.max_connections".to_string(), "-5".to_string());

        let result = apply_with_validation(&manager, &changes, ReloadSource::Manual);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors[0].contains("config validation failed"));

        let item = manager.get("pool.max_connections").unwrap();
        assert_eq!(item.value, "10");
    }

    #[test]
    fn test_apply_with_validation_accepts_valid() {
        let manager = HotReloadManager::new();
        manager.register(HotReloadConfigItem::hot("log.level", "info"));

        let mut changes = HashMap::new();
        changes.insert("log.level".to_string(), "warn".to_string());

        let result = apply_with_validation(&manager, &changes, ReloadSource::Manual);
        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.applied_count, 1);
    }

    #[test]
    fn test_mark_initialized() {
        let manager = HotReloadManager::new();
        assert!(!manager.is_initialized());
        manager.mark_initialized();
        assert!(manager.is_initialized());
    }

    #[test]
    fn test_register_batch() {
        let manager = HotReloadManager::new();
        manager.register_batch(vec![
            HotReloadConfigItem::hot("log.level", "info"),
            HotReloadConfigItem::hot("pool.max_connections", "10"),
            HotReloadConfigItem::restart_required("server.port", "8080"),
        ]);
        assert!(manager.get("log.level").is_some());
        assert!(manager.get("pool.max_connections").is_some());
        assert!(manager.get("server.port").is_some());
    }

    #[test]
    fn test_reload_source_serialization() {
        let source = ReloadSource::SignalSighup;
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("SignalSighup"));
    }
}
