//! 高级日志功能：日志轮转、多输出、级别过滤、结构化字段
//!
//! 本模块在 [`StructuredLogger`] 基础上补充生产级日志所需的核心能力：
//!
//! - **日志轮转**（[`LogRotator`]）：按大小或时间自动轮转日志缓冲，
//!   保留最近 N 份历史日志，防止单一日志无限增长。
//! - **多输出**（[`MultiOutputLogger`] / [`LogSink`]）：将日志扇出到多个
//!   后端（内存、控制台、回调），便于同时写入文件、终端与远程采集器。
//! - **级别过滤**（[`LevelFilter`]）：在全局级别之上支持按 target（模块名）
//!   细粒度过滤，例如全局 Info 但 `database` 模块开 Debug。
//! - **结构化字段**（[`StructuredFields`] / [`StructuredLogEntry`]）：
//!   在消息之外附加键值对字段，便于日志聚合系统（ELK/Loki）索引与查询。

use crate::{LogEntry, LogLevel, Logger};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ============================================================================
// 日志轮转
// ============================================================================

/// 日志轮转策略
#[derive(Debug, Clone)]
pub enum RotationPolicy {
    /// 按字节大小轮转：当前日志达到 `max_bytes` 时轮转
    Size(u64),
    /// 按时间轮转：当前日志存活超过 `max_age` 时轮转
    Time(Duration),
    /// 大小或时间任一条件满足即轮转
    SizeOrTime(u64, Duration),
}

impl RotationPolicy {
    /// 判断当前日志是否应该轮转
    ///
    /// # 参数
    /// - `current_size`：当前日志字节数
    /// - `elapsed`：当前日志已存活时长
    fn should_rotate(&self, current_size: u64, elapsed: Duration) -> bool {
        match self {
            RotationPolicy::Size(max_bytes) => current_size >= *max_bytes,
            RotationPolicy::Time(max_age) => elapsed >= *max_age,
            RotationPolicy::SizeOrTime(max_bytes, max_age) => {
                current_size >= *max_bytes || elapsed >= *max_age
            }
        }
    }
}

/// 日志轮转器（内存模拟）。
///
/// 维护一个当前日志缓冲与一组已轮转的历史日志。当当前缓冲满足轮转策略时，
/// 将其移入历史列表并开启新的空缓冲。历史列表长度不超过 `max_files`，
/// 超出时丢弃最旧的。
///
/// 实际生产环境中，轮转器会将缓冲刷盘到文件（如 `app.log` -> `app.log.1`），
/// 此处用内存 `Vec<u8>` 模拟以便测试。
pub struct LogRotator {
    /// 轮转策略
    policy: RotationPolicy,
    /// 保留的最大历史文件数
    max_files: usize,
    /// 当前日志缓冲
    current: Mutex<Vec<u8>>,
    /// 已轮转的历史日志（按时间倒序，索引 0 为最近一次轮转）
    rotated: Mutex<Vec<Vec<u8>>>,
    /// 当前缓冲的开始时间
    started_at: Mutex<Instant>,
    /// 总轮转次数
    rotation_count: Mutex<u64>,
}

impl LogRotator {
    /// 创建轮转器
    ///
    /// # 参数
    /// - `policy`：轮转策略
    /// - `max_files`：保留的最大历史文件数（0 表示不保留）
    pub fn new(policy: RotationPolicy, max_files: usize) -> Self {
        Self {
            policy,
            max_files,
            current: Mutex::new(Vec::new()),
            rotated: Mutex::new(Vec::new()),
            started_at: Mutex::new(Instant::now()),
            rotation_count: Mutex::new(0),
        }
    }

    /// 写入一条日志（字节形式）。写入后自动检查是否需要轮转。
    pub fn write(&self, data: &[u8]) {
        let should_rotate = {
            let mut current = self.current.lock().unwrap();
            current.extend_from_slice(data);
            let started_at = self.started_at.lock().unwrap();
            self.policy
                .should_rotate(current.len() as u64, started_at.elapsed())
        };

        if should_rotate {
            self.rotate();
        }
    }

    /// 手动触发轮转：将当前缓冲移入历史列表，开启新的空缓冲。
    pub fn rotate(&self) {
        let mut current = self.current.lock().unwrap();
        let mut rotated = self.rotated.lock().unwrap();
        let mut started_at = self.started_at.lock().unwrap();
        let mut count = self.rotation_count.lock().unwrap();

        // 将当前缓冲移入历史列表头部
        let old_buffer = std::mem::take(&mut *current);
        if !old_buffer.is_empty() {
            rotated.insert(0, old_buffer);
        }

        // 超出最大保留数时丢弃最旧的
        while rotated.len() > self.max_files {
            rotated.pop();
        }

        // 重置当前缓冲的开始时间
        *started_at = Instant::now();
        *count += 1;
    }

    /// 获取当前缓冲的字节大小
    pub fn current_size(&self) -> usize {
        self.current.lock().unwrap().len()
    }

    /// 获取已轮转的历史日志数量
    pub fn rotated_count(&self) -> usize {
        self.rotated.lock().unwrap().len()
    }

    /// 获取总轮转次数（含因 max_files 限制被丢弃的）
    pub fn total_rotations(&self) -> u64 {
        *self.rotation_count.lock().unwrap()
    }

    /// 获取当前缓冲内容的快照（拷贝）
    pub fn current_content(&self) -> Vec<u8> {
        self.current.lock().unwrap().clone()
    }

    /// 获取第 `index` 个历史日志的快照（0 = 最近一次轮转）
    pub fn rotated_content(&self, index: usize) -> Option<Vec<u8>> {
        let rotated = self.rotated.lock().unwrap();
        rotated.get(index).cloned()
    }

    /// 获取当前缓冲已存活的时长
    pub fn current_age(&self) -> Duration {
        self.started_at.lock().unwrap().elapsed()
    }

    /// 获取轮转策略引用
    pub fn policy(&self) -> &RotationPolicy {
        &self.policy
    }

    /// 获取最大保留文件数
    pub fn max_files(&self) -> usize {
        self.max_files
    }

    /// 清空所有日志（当前缓冲 + 历史）
    pub fn clear(&self) {
        self.current.lock().unwrap().clear();
        self.rotated.lock().unwrap().clear();
        *self.started_at.lock().unwrap() = Instant::now();
    }
}

// ============================================================================
// 多输出（LogSink / MultiOutputLogger）
// ============================================================================

/// 日志输出目标 trait：将 [`LogEntry`] 写入某个后端。
///
/// 实现方需保证 `write` 不 panic（lock poisoned 时降级处理），
/// 以免一个 sink 故障影响其他 sink 的日志写入。
pub trait LogSink: Send + Sync {
    /// 写入一条日志条目
    fn write(&self, entry: &LogEntry);
    /// sink 名称（用于调试与统计）
    fn name(&self) -> &str;
}

/// 内存日志 sink：将日志条目存入内存 Vec，便于测试验证。
pub struct MemorySink {
    name: String,
    entries: Mutex<Vec<LogEntry>>,
}

impl MemorySink {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Mutex::new(Vec::new()),
        }
    }

    /// 获取已存储的日志条目快照
    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.lock().unwrap().clone()
    }

    /// 获取已存储的日志条目数量
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap().is_empty()
    }

    /// 清空存储的日志
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl LogSink for MemorySink {
    fn write(&self, entry: &LogEntry) {
        // lock poisoned 时跳过写入而非 panic
        if let Ok(mut entries) = self.entries.lock() {
            entries.push(entry.clone());
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 控制台日志 sink：将日志输出到 stdout。
///
/// 不持有状态，输出格式为 `[LEVEL] timestamp - message`。
pub struct ConsoleSink {
    name: String,
}

impl ConsoleSink {
    pub fn new() -> Self {
        Self {
            name: "console".to_string(),
        }
    }

    pub fn with_name(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Default for ConsoleSink {
    fn default() -> Self {
        Self::new()
    }
}

impl LogSink for ConsoleSink {
    fn write(&self, entry: &LogEntry) {
        println!(
            "[{}] {} - {}",
            entry.level.as_str(),
            entry.timestamp,
            entry.message
        );
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 回调日志 sink：将日志通过闭包传递给调用方。
///
/// 适用于将日志桥接到外部日志框架（如 `log` crate、`tracing`）。
/// 闭包必须实现 `Send + Sync`。
pub struct CallbackSink<F>
where
    F: Fn(&LogEntry) + Send + Sync,
{
    name: String,
    callback: F,
}

impl<F> CallbackSink<F>
where
    F: Fn(&LogEntry) + Send + Sync,
{
    pub fn new(name: impl Into<String>, callback: F) -> Self {
        Self {
            name: name.into(),
            callback,
        }
    }
}

impl<F> LogSink for CallbackSink<F>
where
    F: Fn(&LogEntry) + Send + Sync,
{
    fn write(&self, entry: &LogEntry) {
        (self.callback)(entry);
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 多输出日志器：将日志扇出到多个 [`LogSink`]。
///
/// 每条日志会依次写入所有已注册的 sink。单个 sink 写入失败（panic）
/// 不会影响其他 sink，因为 [`LogSink::write`] 要求实现方自行降级处理。
pub struct MultiOutputLogger {
    level: LogLevel,
    sinks: Vec<Arc<dyn LogSink>>,
}

impl MultiOutputLogger {
    /// 创建多输出日志器
    pub fn new(level: LogLevel) -> Self {
        Self {
            level,
            sinks: Vec::new(),
        }
    }

    /// 添加一个输出 sink
    pub fn add_sink(&mut self, sink: Arc<dyn LogSink>) -> &mut Self {
        self.sinks.push(sink);
        self
    }

    /// 获取已注册的 sink 名称列表
    pub fn sink_names(&self) -> Vec<String> {
        self.sinks.iter().map(|s| s.name().to_string()).collect()
    }

    /// 获取已注册的 sink 数量
    pub fn sink_count(&self) -> usize {
        self.sinks.len()
    }

    /// 获取当前日志级别
    pub fn level(&self) -> LogLevel {
        self.level
    }
}

impl Logger for MultiOutputLogger {
    fn log(&self, level: LogLevel, msg: &str) {
        if level < self.level {
            return;
        }
        let entry = LogEntry {
            level,
            message: msg.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        for sink in &self.sinks {
            sink.write(&entry);
        }
    }
}

// ============================================================================
// 级别过滤（LevelFilter）
// ============================================================================

/// 级别过滤器：支持全局级别 + 按 target 细粒度级别。
///
/// `target` 是日志的模块/组件名（如 `"database"`、`"http"`、`"cache"`）。
/// 当 target 未在 `target_levels` 中注册时，使用 `default_level`。
///
/// # 示例
///
/// ```
/// use sz_orm_logger::{LogLevel, advanced::LevelFilter};
///
/// let filter = LevelFilter::new(LogLevel::Info)
///     .with_target_level("database", LogLevel::Debug)
///     .with_target_level("http", LogLevel::Warn);
///
/// // 全局 Info：Debug 被过滤
/// assert!(!filter.should_log("app", LogLevel::Debug));
/// assert!(filter.should_log("app", LogLevel::Info));
///
/// // database 模块开 Debug
/// assert!(filter.should_log("database", LogLevel::Debug));
///
/// // http 模块只看 Warn 及以上
/// assert!(!filter.should_log("http", LogLevel::Info));
/// assert!(filter.should_log("http", LogLevel::Warn));
/// ```
#[derive(Debug, Clone)]
pub struct LevelFilter {
    /// 默认级别（未注册 target 使用此级别）
    default_level: LogLevel,
    /// 按 target 名设置的级别
    target_levels: HashMap<String, LogLevel>,
}

impl LevelFilter {
    /// 创建级别过滤器，默认级别为 `default_level`
    pub fn new(default_level: LogLevel) -> Self {
        Self {
            default_level,
            target_levels: HashMap::new(),
        }
    }

    /// 为指定 target 设置日志级别
    pub fn with_target_level(mut self, target: impl Into<String>, level: LogLevel) -> Self {
        self.target_levels.insert(target.into(), level);
        self
    }

    /// 移除指定 target 的级别覆盖，回退到 `default_level`
    pub fn remove_target(&mut self, target: &str) -> Option<LogLevel> {
        self.target_levels.remove(target)
    }

    /// 获取指定 target 的日志级别（优先 target_levels，其次 default_level）
    pub fn level_for(&self, target: &str) -> LogLevel {
        self.target_levels
            .get(target)
            .copied()
            .unwrap_or(self.default_level)
    }

    /// 获取默认级别
    pub fn default_level(&self) -> LogLevel {
        self.default_level
    }

    /// 设置默认级别
    pub fn set_default_level(&mut self, level: LogLevel) {
        self.default_level = level;
    }

    /// 判断指定 target 的日志是否应该被记录
    pub fn should_log(&self, target: &str, level: LogLevel) -> bool {
        level >= self.level_for(target)
    }

    /// 获取已注册的 target 数量
    pub fn target_count(&self) -> usize {
        self.target_levels.len()
    }

    /// 获取所有已注册的 target 名称
    pub fn targets(&self) -> Vec<String> {
        self.target_levels.keys().cloned().collect()
    }
}

impl Default for LevelFilter {
    fn default() -> Self {
        Self::new(LogLevel::Info)
    }
}

// ============================================================================
// 结构化字段（StructuredFields / StructuredLogEntry）
// ============================================================================

/// 结构化日志字段：键值对集合。
///
/// 用于在日志消息之外附加可被日志聚合系统索引的结构化数据，
/// 如 `user_id=12345`、`request_id=abc`、`latency_ms=42`。
pub type StructuredFields = HashMap<String, String>;

/// 带结构化字段与 target 的日志条目。
///
/// 相比 [`LogEntry`]，增加了：
/// - `target`：日志来源模块名（用于 [`LevelFilter`] 过滤）
/// - `fields`：结构化键值对字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredLogEntry {
    pub level: LogLevel,
    pub message: String,
    pub timestamp: String,
    pub target: Option<String>,
    pub fields: StructuredFields,
}

impl StructuredLogEntry {
    /// 创建结构化日志条目
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            target: None,
            fields: HashMap::new(),
        }
    }

    /// 设置 target（模块名）
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// 添加一个结构化字段
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// 从普通 [`LogEntry`] 转换（target 和 fields 为空）
    pub fn from_log_entry(entry: &LogEntry) -> Self {
        Self {
            level: entry.level,
            message: entry.message.clone(),
            timestamp: entry.timestamp.clone(),
            target: None,
            fields: HashMap::new(),
        }
    }

    /// 转换为普通 [`LogEntry`]（丢弃 target 和 fields）
    pub fn to_log_entry(&self) -> LogEntry {
        LogEntry {
            level: self.level,
            message: self.message.clone(),
            timestamp: self.timestamp.clone(),
        }
    }

    /// 序列化为 JSON 字符串（便于写入文件或发送到远程）
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// 从 JSON 字符串反序列化
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// 将结构化字段格式化为 `key=value` 列表（用于日志文本输出）
    pub fn format_fields(&self) -> String {
        let mut pairs: Vec<String> = self
            .fields
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        pairs.sort(); // 按字母序排列，保证输出确定性
        pairs.join(" ")
    }
}

/// 结构化日志器：支持结构化字段、target 过滤与多 sink 输出。
///
/// 结合 [`LevelFilter`] 与 [`LogSink`]，提供生产级日志能力：
/// 1. 按 target 过滤日志级别
/// 2. 记录结构化字段
/// 3. 扇出到多个输出后端
pub struct StructuredLogWriter {
    filter: LevelFilter,
    sinks: Vec<Arc<dyn StructuredSink>>,
}

/// 结构化日志 sink：接收 [`StructuredLogEntry`]。
///
/// 与 [`LogSink`] 的区别在于接收结构化条目，可以索引 fields。
pub trait StructuredSink: Send + Sync {
    fn write(&self, entry: &StructuredLogEntry);
    fn name(&self) -> &str;
}

/// 内存结构化 sink：存储 [`StructuredLogEntry`]。
pub struct MemoryStructuredSink {
    name: String,
    entries: Mutex<Vec<StructuredLogEntry>>,
}

impl MemoryStructuredSink {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Mutex::new(Vec::new()),
        }
    }

    pub fn entries(&self) -> Vec<StructuredLogEntry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap().is_empty()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl StructuredSink for MemoryStructuredSink {
    fn write(&self, entry: &StructuredLogEntry) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.push(entry.clone());
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl StructuredLogWriter {
    /// 创建结构化日志写入器
    pub fn new(filter: LevelFilter) -> Self {
        Self {
            filter,
            sinks: Vec::new(),
        }
    }

    /// 添加结构化 sink
    pub fn add_sink(&mut self, sink: Arc<dyn StructuredSink>) -> &mut Self {
        self.sinks.push(sink);
        self
    }

    /// 获取 sink 数量
    pub fn sink_count(&self) -> usize {
        self.sinks.len()
    }

    /// 获取 sink 名称列表
    pub fn sink_names(&self) -> Vec<String> {
        self.sinks.iter().map(|s| s.name().to_string()).collect()
    }

    /// 获取级别过滤器引用
    pub fn filter(&self) -> &LevelFilter {
        &self.filter
    }

    /// 获取级别过滤器可变引用
    pub fn filter_mut(&mut self) -> &mut LevelFilter {
        &mut self.filter
    }

    /// 记录一条结构化日志。
    ///
    /// 根据 target 通过 [`LevelFilter`] 过滤后扇出到所有 sink。
    /// target 为 `None` 时使用默认级别。
    pub fn log(&self, entry: &StructuredLogEntry) {
        let target = entry.target.as_deref().unwrap_or("");
        if !self.filter.should_log(target, entry.level) {
            return;
        }
        for sink in &self.sinks {
            sink.write(entry);
        }
    }

    /// 便捷方法：记录一条带 target 和字段的日志
    pub fn log_with_fields(
        &self,
        target: impl Into<String>,
        level: LogLevel,
        message: impl Into<String>,
        fields: StructuredFields,
    ) {
        let entry = StructuredLogEntry::new(level, message)
            .with_target(target)
            .with_fields(fields);
        self.log(&entry);
    }
}

// 为 StructuredLogEntry 批量设置 fields 的扩展方法
impl StructuredLogEntry {
    /// 批量设置结构化字段
    pub fn with_fields(mut self, fields: StructuredFields) -> Self {
        self.fields.extend(fields);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // ===================== RotationPolicy 测试 =====================

    #[test]
    fn test_rotation_policy_size_met() {
        let policy = RotationPolicy::Size(100);
        assert!(policy.should_rotate(100, Duration::from_secs(0)));
        assert!(policy.should_rotate(101, Duration::from_secs(0)));
        assert!(!policy.should_rotate(99, Duration::from_secs(0)));
    }

    #[test]
    fn test_rotation_policy_time_met() {
        let policy = RotationPolicy::Time(Duration::from_secs(60));
        assert!(policy.should_rotate(0, Duration::from_secs(60)));
        assert!(policy.should_rotate(0, Duration::from_secs(61)));
        assert!(!policy.should_rotate(0, Duration::from_secs(59)));
    }

    #[test]
    fn test_rotation_policy_size_or_time_either() {
        let policy = RotationPolicy::SizeOrTime(100, Duration::from_secs(60));
        // 大小满足
        assert!(policy.should_rotate(100, Duration::from_secs(0)));
        // 时间满足
        assert!(policy.should_rotate(0, Duration::from_secs(60)));
        // 都不满足
        assert!(!policy.should_rotate(99, Duration::from_secs(59)));
        // 都满足
        assert!(policy.should_rotate(200, Duration::from_secs(120)));
    }

    // ===================== LogRotator 测试 =====================

    #[test]
    fn test_log_rotator_new_empty() {
        let rotator = LogRotator::new(RotationPolicy::Size(1024), 3);
        assert_eq!(rotator.current_size(), 0);
        assert_eq!(rotator.rotated_count(), 0);
        assert_eq!(rotator.total_rotations(), 0);
        assert_eq!(rotator.max_files(), 3);
    }

    #[test]
    fn test_log_rotator_write_accumulates() {
        let rotator = LogRotator::new(RotationPolicy::Size(1024), 3);
        rotator.write(b"hello");
        rotator.write(b" world");
        assert_eq!(rotator.current_size(), 11);
        assert_eq!(rotator.current_content(), b"hello world");
    }

    #[test]
    fn test_log_rotator_size_triggers_rotation() {
        let rotator = LogRotator::new(RotationPolicy::Size(10), 3);
        rotator.write(b"12345"); // 5 bytes
        assert_eq!(rotator.rotated_count(), 0);
        rotator.write(b"67890"); // 10 bytes -> triggers rotation
        assert_eq!(rotator.rotated_count(), 1);
        assert_eq!(rotator.total_rotations(), 1);
        assert_eq!(rotator.current_size(), 0);
    }

    #[test]
    fn test_log_rotator_rotation_preserves_content() {
        let rotator = LogRotator::new(RotationPolicy::Size(10), 3);
        rotator.write(b"hello world"); // 11 bytes -> triggers rotation
        assert_eq!(rotator.rotated_count(), 1);
        let rotated = rotator.rotated_content(0).expect("rotated[0] must exist");
        assert_eq!(rotated, b"hello world");
    }

    #[test]
    fn test_log_rotator_max_files_drops_oldest() {
        let rotator = LogRotator::new(RotationPolicy::Size(5), 2);
        rotator.write(b"AAAAAA"); // rotate 1
        rotator.write(b"BBBBBB"); // rotate 2
        rotator.write(b"CCCCCC"); // rotate 3 -> drops oldest (AAAAAA)
        assert_eq!(rotator.rotated_count(), 2);
        assert_eq!(rotator.total_rotations(), 3);
        // rotated[0] = most recent = CCCCCC, rotated[1] = BBBBBB
        assert_eq!(rotator.rotated_content(0), Some(b"CCCCCC".to_vec()));
        assert_eq!(rotator.rotated_content(1), Some(b"BBBBBB".to_vec()));
        assert_eq!(rotator.rotated_content(2), None);
    }

    #[test]
    fn test_log_rotator_manual_rotate() {
        let rotator = LogRotator::new(RotationPolicy::Size(1024), 3);
        rotator.write(b"some data");
        rotator.rotate();
        assert_eq!(rotator.rotated_count(), 1);
        assert_eq!(rotator.current_size(), 0);
    }

    #[test]
    fn test_log_rotator_manual_rotate_empty_no_op() {
        let rotator = LogRotator::new(RotationPolicy::Size(1024), 3);
        rotator.rotate(); // empty buffer -> no rotation
        assert_eq!(rotator.rotated_count(), 0);
        assert_eq!(rotator.total_rotations(), 1); // count still increments
    }

    #[test]
    fn test_log_rotator_clear() {
        let rotator = LogRotator::new(RotationPolicy::Size(5), 3);
        rotator.write(b"hello world"); // triggers rotation
        rotator.write(b"more");
        assert!(!rotator.current_content().is_empty());
        assert_eq!(rotator.rotated_count(), 1);

        rotator.clear();
        assert_eq!(rotator.current_size(), 0);
        assert_eq!(rotator.rotated_count(), 0);
    }

    #[test]
    fn test_log_rotator_policy_accessor() {
        let rotator = LogRotator::new(RotationPolicy::Size(256), 5);
        match rotator.policy() {
            RotationPolicy::Size(n) => assert_eq!(*n, 256),
            _ => panic!("expected Size policy"),
        }
    }

    #[test]
    fn test_log_rotator_time_based_rotation() {
        let rotator = LogRotator::new(RotationPolicy::Time(Duration::from_millis(50)), 3);
        rotator.write(b"data");
        assert_eq!(rotator.rotated_count(), 0);
        thread::sleep(Duration::from_millis(60));
        rotator.write(b"more"); // triggers time-based rotation
        assert_eq!(rotator.rotated_count(), 1);
    }

    #[test]
    fn test_log_rotator_current_age_increases() {
        let rotator = LogRotator::new(RotationPolicy::Size(1024), 3);
        let age1 = rotator.current_age();
        thread::sleep(Duration::from_millis(10));
        let age2 = rotator.current_age();
        assert!(age2 > age1);
    }

    // ===================== MemorySink 测试 =====================

    #[test]
    fn test_memory_sink_new_empty() {
        let sink = MemorySink::new("test");
        assert_eq!(sink.name(), "test");
        assert!(sink.is_empty());
        assert_eq!(sink.len(), 0);
    }

    #[test]
    fn test_memory_sink_write_stores_entry() {
        let sink = MemorySink::new("mem");
        let entry = LogEntry {
            level: LogLevel::Info,
            message: "hello".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        sink.write(&entry);
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.entries()[0].message, "hello");
    }

    #[test]
    fn test_memory_sink_clear() {
        let sink = MemorySink::new("mem");
        let entry = LogEntry {
            level: LogLevel::Info,
            message: "x".to_string(),
            timestamp: "t".to_string(),
        };
        sink.write(&entry);
        sink.write(&entry);
        assert_eq!(sink.len(), 2);
        sink.clear();
        assert!(sink.is_empty());
    }

    // ===================== ConsoleSink 测试 =====================

    #[test]
    fn test_console_sink_name_default() {
        let sink = ConsoleSink::new();
        assert_eq!(sink.name(), "console");
    }

    #[test]
    fn test_console_sink_custom_name() {
        let sink = ConsoleSink::with_name("stdout");
        assert_eq!(sink.name(), "stdout");
    }

    #[test]
    fn test_console_sink_write_does_not_panic() {
        let sink = ConsoleSink::new();
        let entry = LogEntry {
            level: LogLevel::Info,
            message: "test".to_string(),
            timestamp: "t".to_string(),
        };
        sink.write(&entry); // should not panic
    }

    // ===================== CallbackSink 测试 =====================

    #[test]
    fn test_callback_sink_invokes_closure() {
        let counter = Arc::new(Mutex::new(0u32));
        let c = counter.clone();
        let sink = CallbackSink::new("cb", move |_entry| {
            *c.lock().unwrap() += 1;
        });
        let entry = LogEntry {
            level: LogLevel::Info,
            message: "x".to_string(),
            timestamp: "t".to_string(),
        };
        sink.write(&entry);
        sink.write(&entry);
        assert_eq!(*counter.lock().unwrap(), 2);
    }

    // ===================== MultiOutputLogger 测试 =====================

    #[test]
    fn test_multi_output_logger_fans_out_to_all_sinks() {
        let mut logger = MultiOutputLogger::new(LogLevel::Debug);
        let sink1 = Arc::new(MemorySink::new("s1"));
        let sink2 = Arc::new(MemorySink::new("s2"));

        logger.add_sink(sink1.clone());
        logger.add_sink(sink2.clone());

        logger.log(LogLevel::Info, "hello");

        assert_eq!(sink1.len(), 1);
        assert_eq!(sink2.len(), 1);
        assert_eq!(sink1.entries()[0].message, "hello");
        assert_eq!(sink2.entries()[0].message, "hello");
    }

    #[test]
    fn test_multi_output_logger_respects_level_filter() {
        let mut logger = MultiOutputLogger::new(LogLevel::Warn);
        let sink = Arc::new(MemorySink::new("s"));
        logger.add_sink(sink.clone());

        logger.log(LogLevel::Debug, "debug"); // filtered
        logger.log(LogLevel::Info, "info"); // filtered
        logger.log(LogLevel::Warn, "warn"); // passes
        logger.log(LogLevel::Error, "error"); // passes

        assert_eq!(sink.len(), 2);
    }

    #[test]
    fn test_multi_output_logger_sink_names() {
        let mut logger = MultiOutputLogger::new(LogLevel::Info);
        logger.add_sink(Arc::new(MemorySink::new("alpha")));
        logger.add_sink(Arc::new(MemorySink::new("beta")));
        let names = logger.sink_names();
        assert_eq!(names, vec!["alpha", "beta"]);
        assert_eq!(logger.sink_count(), 2);
    }

    #[test]
    fn test_multi_output_logger_empty_sinks_no_error() {
        let logger = MultiOutputLogger::new(LogLevel::Info);
        logger.log(LogLevel::Info, "msg"); // should not panic
        assert_eq!(logger.sink_count(), 0);
    }

    #[test]
    fn test_multi_output_logger_level_accessor() {
        let logger = MultiOutputLogger::new(LogLevel::Error);
        assert_eq!(logger.level(), LogLevel::Error);
    }

    // ===================== LevelFilter 测试 =====================

    #[test]
    fn test_level_filter_default_level() {
        let filter = LevelFilter::new(LogLevel::Info);
        assert_eq!(filter.default_level(), LogLevel::Info);
        assert_eq!(filter.level_for("anything"), LogLevel::Info);
        assert!(filter.should_log("anything", LogLevel::Info));
        assert!(!filter.should_log("anything", LogLevel::Debug));
    }

    #[test]
    fn test_level_filter_target_override() {
        let filter = LevelFilter::new(LogLevel::Info)
            .with_target_level("database", LogLevel::Debug)
            .with_target_level("http", LogLevel::Warn);

        // database 模块开 Debug
        assert!(filter.should_log("database", LogLevel::Debug));
        assert_eq!(filter.level_for("database"), LogLevel::Debug);

        // http 模块只看 Warn+
        assert!(!filter.should_log("http", LogLevel::Info));
        assert!(filter.should_log("http", LogLevel::Warn));
        assert_eq!(filter.level_for("http"), LogLevel::Warn);

        // 未注册的 target 用默认 Info
        assert_eq!(filter.level_for("cache"), LogLevel::Info);
        assert!(filter.should_log("cache", LogLevel::Info));
        assert!(!filter.should_log("cache", LogLevel::Debug));
    }

    #[test]
    fn test_level_filter_remove_target() {
        let mut filter = LevelFilter::new(LogLevel::Info)
            .with_target_level("db", LogLevel::Debug);
        assert_eq!(filter.level_for("db"), LogLevel::Debug);

        let removed = filter.remove_target("db");
        assert_eq!(removed, Some(LogLevel::Debug));
        assert_eq!(filter.level_for("db"), LogLevel::Info); // falls back
    }

    #[test]
    fn test_level_filter_remove_missing_target_returns_none() {
        let mut filter = LevelFilter::new(LogLevel::Info);
        assert_eq!(filter.remove_target("never"), None);
    }

    #[test]
    fn test_level_filter_set_default_level() {
        let mut filter = LevelFilter::new(LogLevel::Info);
        filter.set_default_level(LogLevel::Debug);
        assert_eq!(filter.default_level(), LogLevel::Debug);
        assert!(filter.should_log("any", LogLevel::Debug));
    }

    #[test]
    fn test_level_filter_target_count_and_names() {
        let filter = LevelFilter::new(LogLevel::Info)
            .with_target_level("a", LogLevel::Debug)
            .with_target_level("b", LogLevel::Warn);
        assert_eq!(filter.target_count(), 2);
        let mut targets = filter.targets();
        targets.sort();
        assert_eq!(targets, vec!["a", "b"]);
    }

    #[test]
    fn test_level_filter_default_impl() {
        let filter = LevelFilter::default();
        assert_eq!(filter.default_level(), LogLevel::Info);
    }

    // ===================== StructuredLogEntry 测试 =====================

    #[test]
    fn test_structured_log_entry_new() {
        let entry = StructuredLogEntry::new(LogLevel::Info, "hello");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "hello");
        assert!(entry.target.is_none());
        assert!(entry.fields.is_empty());
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn test_structured_log_entry_with_target() {
        let entry = StructuredLogEntry::new(LogLevel::Info, "msg").with_target("database");
        assert_eq!(entry.target, Some("database".to_string()));
    }

    #[test]
    fn test_structured_log_entry_with_field() {
        let entry = StructuredLogEntry::new(LogLevel::Info, "msg")
            .with_field("user_id", "12345")
            .with_field("action", "login");
        assert_eq!(entry.fields.get("user_id"), Some(&"12345".to_string()));
        assert_eq!(entry.fields.get("action"), Some(&"login".to_string()));
        assert_eq!(entry.fields.len(), 2);
    }

    #[test]
    fn test_structured_log_entry_with_fields_batch() {
        let mut fields = StructuredFields::new();
        fields.insert("k1".to_string(), "v1".to_string());
        fields.insert("k2".to_string(), "v2".to_string());
        let entry = StructuredLogEntry::new(LogLevel::Info, "msg").with_fields(fields);
        assert_eq!(entry.fields.len(), 2);
    }

    #[test]
    fn test_structured_log_entry_from_log_entry() {
        let original = LogEntry {
            level: LogLevel::Warn,
            message: "warning msg".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let structured = StructuredLogEntry::from_log_entry(&original);
        assert_eq!(structured.level, LogLevel::Warn);
        assert_eq!(structured.message, "warning msg");
        assert_eq!(structured.timestamp, "2024-01-01T00:00:00Z");
        assert!(structured.target.is_none());
        assert!(structured.fields.is_empty());
    }

    #[test]
    fn test_structured_log_entry_to_log_entry() {
        let structured = StructuredLogEntry::new(LogLevel::Error, "err")
            .with_target("db")
            .with_field("code", "500");
        let plain = structured.to_log_entry();
        assert_eq!(plain.level, LogLevel::Error);
        assert_eq!(plain.message, "err");
    }

    #[test]
    fn test_structured_log_entry_json_roundtrip() {
        let entry = StructuredLogEntry::new(LogLevel::Info, "test")
            .with_target("app")
            .with_field("key", "value");
        let json = entry.to_json().expect("serialize");
        let back = StructuredLogEntry::from_json(&json).expect("deserialize");
        assert_eq!(back.level, entry.level);
        assert_eq!(back.message, entry.message);
        assert_eq!(back.target, entry.target);
        assert_eq!(back.fields, entry.fields);
    }

    #[test]
    fn test_structured_log_entry_format_fields_sorted() {
        let entry = StructuredLogEntry::new(LogLevel::Info, "msg")
            .with_field("zebra", "1")
            .with_field("alpha", "2")
            .with_field("middle", "3");
        let formatted = entry.format_fields();
        // 按字母序排列
        assert_eq!(formatted, "alpha=2 middle=3 zebra=1");
    }

    #[test]
    fn test_structured_log_entry_format_fields_empty() {
        let entry = StructuredLogEntry::new(LogLevel::Info, "msg");
        assert_eq!(entry.format_fields(), "");
    }

    // ===================== MemoryStructuredSink 测试 =====================

    #[test]
    fn test_memory_structured_sink_new_empty() {
        let sink = MemoryStructuredSink::new("s");
        assert_eq!(sink.name(), "s");
        assert!(sink.is_empty());
    }

    #[test]
    fn test_memory_structured_sink_stores_entry() {
        let sink = MemoryStructuredSink::new("s");
        let entry = StructuredLogEntry::new(LogLevel::Info, "hello")
            .with_field("k", "v");
        sink.write(&entry);
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.entries()[0].message, "hello");
        assert_eq!(sink.entries()[0].fields.get("k"), Some(&"v".to_string()));
    }

    #[test]
    fn test_memory_structured_sink_clear() {
        let sink = MemoryStructuredSink::new("s");
        sink.write(&StructuredLogEntry::new(LogLevel::Info, "x"));
        sink.clear();
        assert!(sink.is_empty());
    }

    // ===================== StructuredLogWriter 测试 =====================

    #[test]
    fn test_structured_log_writer_filters_by_target() {
        let filter = LevelFilter::new(LogLevel::Info)
            .with_target_level("verbose", LogLevel::Debug);
        let mut writer = StructuredLogWriter::new(filter);
        let sink = Arc::new(MemoryStructuredSink::new("mem"));
        writer.add_sink(sink.clone());

        // verbose 模块的 Debug 日志应该被记录
        let debug_entry = StructuredLogEntry::new(LogLevel::Debug, "dbg")
            .with_target("verbose");
        writer.log(&debug_entry);
        assert_eq!(sink.len(), 1);

        // 默认 target 的 Debug 日志应该被过滤
        let filtered = StructuredLogEntry::new(LogLevel::Debug, "filtered");
        writer.log(&filtered);
        assert_eq!(sink.len(), 1); // still 1
    }

    #[test]
    fn test_structured_log_writer_fans_out_to_multiple_sinks() {
        let writer = StructuredLogWriter::new(LevelFilter::new(LogLevel::Debug));
        let sink1 = Arc::new(MemoryStructuredSink::new("s1"));
        let sink2 = Arc::new(MemoryStructuredSink::new("s2"));

        // 先添加 sink 再使用（需要可变引用）
        let mut writer = writer;
        writer.add_sink(sink1.clone());
        writer.add_sink(sink2.clone());

        let entry = StructuredLogEntry::new(LogLevel::Info, "hello");
        writer.log(&entry);

        assert_eq!(sink1.len(), 1);
        assert_eq!(sink2.len(), 1);
    }

    #[test]
    fn test_structured_log_writer_log_with_fields() {
        let mut writer = StructuredLogWriter::new(LevelFilter::new(LogLevel::Info));
        let sink = Arc::new(MemoryStructuredSink::new("mem"));
        writer.add_sink(sink.clone());

        let mut fields = StructuredFields::new();
        fields.insert("user_id".to_string(), "42".to_string());
        writer.log_with_fields("api", LogLevel::Info, "request", fields);

        assert_eq!(sink.len(), 1);
        let entry = &sink.entries()[0];
        assert_eq!(entry.target, Some("api".to_string()));
        assert_eq!(entry.fields.get("user_id"), Some(&"42".to_string()));
    }

    #[test]
    fn test_structured_log_writer_sink_names() {
        let mut writer = StructuredLogWriter::new(LevelFilter::new(LogLevel::Info));
        writer.add_sink(Arc::new(MemoryStructuredSink::new("alpha")));
        writer.add_sink(Arc::new(MemoryStructuredSink::new("beta")));
        let names = writer.sink_names();
        assert_eq!(names, vec!["alpha", "beta"]);
        assert_eq!(writer.sink_count(), 2);
    }

    #[test]
    fn test_structured_log_writer_filter_mut() {
        let mut writer = StructuredLogWriter::new(LevelFilter::new(LogLevel::Info));
        writer.filter_mut().set_default_level(LogLevel::Error);
        assert_eq!(writer.filter().default_level(), LogLevel::Error);
    }

    #[test]
    fn test_structured_log_writer_empty_sinks_no_error() {
        let writer = StructuredLogWriter::new(LevelFilter::new(LogLevel::Debug));
        let entry = StructuredLogEntry::new(LogLevel::Info, "msg");
        writer.log(&entry); // should not panic
        assert_eq!(writer.sink_count(), 0);
    }

    #[test]
    fn test_multi_output_logger_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MultiOutputLogger>();
        assert_send_sync::<MemorySink>();
        assert_send_sync::<ConsoleSink>();
    }
}
