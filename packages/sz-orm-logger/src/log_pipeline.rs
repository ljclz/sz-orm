//! 日志管道：过滤器链、路由器、格式化器链与输出器链
//!
//! 本模块提供可组合的日志处理管道，支持：
//!
//! - **过滤器**（[`LogFilter`] trait）：决定日志是否继续传递
//! - **格式化器**（[`LogFormatter`] trait）：将日志记录格式化为字符串
//! - **输出器**（[`LogOutput`] trait）：将格式化后的日志写入目标
//! - **管道**（[`LogPipeline`]）：过滤器链 → 格式化器 → 输出器链
//! - **路由器**（[`LogRouter`]）：按规则将日志路由到不同管道
//! - **构建器**（[`LogPipelineBuilder`]）：流式构建管道
//!
//! ## 示例
//!
//! ```no_run
//! use sz_orm_logger::LogLevel;
//! use sz_orm_logger::log_pipeline::{
//!     LogPipelineBuilder, LogRecord, LevelThresholdFilter,
//!     JsonFormatter, MemoryOutput,
//! };
//!
//! let output = MemoryOutput::new();
//! let output_handle = output.handle();
//! let pipeline = LogPipelineBuilder::new()
//!     .filter(Box::new(LevelThresholdFilter::new(LogLevel::Info)))
//!     .formatter(Box::new(JsonFormatter))
//!     .output(Box::new(output))
//!     .build();
//!
//! let record = LogRecord::new(LogLevel::Info, "app", "hello");
//! pipeline.process(&record);
//! assert_eq!(output_handle.entries().len(), 1);
//! ```

use crate::LogLevel;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

// ============================================================================
// 日志记录
// ============================================================================

/// 日志记录，包含级别、目标、消息、时间戳与结构化字段
///
/// 相比 [`crate::LogEntry`]，增加了 `target`（模块名）与 `fields`（键值对），
/// 便于过滤、路由与结构化输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    /// 日志级别
    pub level: LogLevel,
    /// 日志目标（模块名/组件名）
    pub target: String,
    /// 日志消息
    pub message: String,
    /// 时间戳（UTC）
    pub timestamp: DateTime<Utc>,
    /// 结构化字段
    pub fields: HashMap<String, String>,
}

impl LogRecord {
    /// 创建新日志记录，自动填充当前时间戳
    pub fn new(level: LogLevel, target: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            target: target.into(),
            message: message.into(),
            timestamp: Utc::now(),
            fields: HashMap::new(),
        }
    }

    /// 添加结构化字段
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// 添加多个结构化字段
    pub fn with_fields(mut self, fields: HashMap<String, String>) -> Self {
        self.fields.extend(fields);
        self
    }

    /// 设置时间戳
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// 判断级别是否达到指定阈值
    pub fn level_at_least(&self, threshold: LogLevel) -> bool {
        self.level >= threshold
    }
}

impl fmt::Display for LogRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} {} - {}",
            self.level.as_str(),
            self.timestamp.to_rfc3339(),
            self.target,
            self.message
        )
    }
}

// ============================================================================
// 过滤器 trait
// ============================================================================

/// 日志过滤器接口
///
/// 返回 `true` 表示日志应继续传递，`false` 表示丢弃。
pub trait LogFilter: Send + Sync {
    /// 判断日志记录是否应保留
    fn should_keep(&self, record: &LogRecord) -> bool;

    /// 过滤器名称（用于调试）
    fn name(&self) -> &str {
        "filter"
    }
}

// ============================================================================
// 内置过滤器
// ============================================================================

/// 级别阈值过滤器：仅保留级别 >= threshold 的日志
#[derive(Debug, Clone)]
pub struct LevelThresholdFilter {
    /// 最低级别
    pub threshold: LogLevel,
}

impl LevelThresholdFilter {
    /// 创建过滤器，指定最低级别
    pub fn new(threshold: LogLevel) -> Self {
        Self { threshold }
    }
}

impl LogFilter for LevelThresholdFilter {
    fn should_keep(&self, record: &LogRecord) -> bool {
        record.level >= self.threshold
    }

    fn name(&self) -> &str {
        "level_threshold"
    }
}

/// 目标过滤器：按 target 名称过滤
///
/// `allow` 为 `true` 时仅允许列表中的 target，为 `false` 时排除列表中的 target。
#[derive(Debug, Clone)]
pub struct TargetFilter {
    /// 目标名称集合
    pub targets: HashSet<String>,
    /// true = 白名单（仅允许），false = 黑名单（排除）
    pub allow: bool,
}

impl TargetFilter {
    /// 创建白名单过滤器（仅允许列表中的 target）
    pub fn allowlist(targets: &[&str]) -> Self {
        Self {
            targets: targets.iter().map(|s| s.to_string()).collect(),
            allow: true,
        }
    }

    /// 创建黑名单过滤器（排除列表中的 target）
    pub fn blocklist(targets: &[&str]) -> Self {
        Self {
            targets: targets.iter().map(|s| s.to_string()).collect(),
            allow: false,
        }
    }
}

impl LogFilter for TargetFilter {
    fn should_keep(&self, record: &LogRecord) -> bool {
        let contains = self.targets.contains(&record.target);
        if self.allow {
            contains
        } else {
            !contains
        }
    }

    fn name(&self) -> &str {
        if self.allow {
            "target_allowlist"
        } else {
            "target_blocklist"
        }
    }
}

/// 子串过滤器：消息包含指定子串则保留
#[derive(Debug, Clone)]
pub struct ContainsFilter {
    /// 匹配子串
    pub pattern: String,
    /// true = 包含则保留，false = 包含则排除
    pub include: bool,
}

impl ContainsFilter {
    /// 创建包含过滤器（消息包含 pattern 则保留）
    pub fn include(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            include: true,
        }
    }

    /// 创建排除过滤器（消息包含 pattern 则丢弃）
    pub fn exclude(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            include: false,
        }
    }
}

impl LogFilter for ContainsFilter {
    fn should_keep(&self, record: &LogRecord) -> bool {
        let contains = record.message.contains(self.pattern.as_str());
        if self.include {
            contains
        } else {
            !contains
        }
    }

    fn name(&self) -> &str {
        "contains"
    }
}

/// 复合过滤器：所有子过滤器均通过才保留（AND 逻辑）
pub struct AllFilter {
    filters: Vec<Box<dyn LogFilter>>,
}

impl AllFilter {
    /// 创建复合过滤器
    pub fn new(filters: Vec<Box<dyn LogFilter>>) -> Self {
        Self { filters }
    }
}

impl fmt::Debug for AllFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AllFilter")
            .field("filter_count", &self.filters.len())
            .finish()
    }
}

impl LogFilter for AllFilter {
    fn should_keep(&self, record: &LogRecord) -> bool {
        self.filters.iter().all(|f| f.should_keep(record))
    }

    fn name(&self) -> &str {
        "all"
    }
}

/// 复合过滤器：任一子过滤器通过则保留（OR 逻辑）
pub struct AnyFilter {
    filters: Vec<Box<dyn LogFilter>>,
}

impl AnyFilter {
    /// 创建复合过滤器
    pub fn new(filters: Vec<Box<dyn LogFilter>>) -> Self {
        Self { filters }
    }
}

impl fmt::Debug for AnyFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnyFilter")
            .field("filter_count", &self.filters.len())
            .finish()
    }
}

impl LogFilter for AnyFilter {
    fn should_keep(&self, record: &LogRecord) -> bool {
        self.filters.iter().any(|f| f.should_keep(record))
    }

    fn name(&self) -> &str {
        "any"
    }
}

// ============================================================================
// 格式化器 trait
// ============================================================================

/// 日志格式化器接口
pub trait LogFormatter: Send + Sync {
    /// 将日志记录格式化为字符串
    fn format(&self, record: &LogRecord) -> String;

    /// 格式化器名称
    fn name(&self) -> &str {
        "formatter"
    }
}

// ============================================================================
// 内置格式化器
// ============================================================================

/// JSON 格式化器：输出 JSON 行（JSONL）
#[derive(Debug, Default)]
pub struct JsonFormatter;

impl LogFormatter for JsonFormatter {
    fn format(&self, record: &LogRecord) -> String {
        // 手动构建 JSON 以避免 serde_json::to_string 的额外分配
        // 但为正确性起见使用 serde_json
        serde_json::to_string(record).unwrap_or_else(|_| "{}".to_string())
    }

    fn name(&self) -> &str {
        "json"
    }
}

/// 文本格式化器：输出可读的文本行
#[derive(Debug, Clone)]
pub struct TextFormatter {
    /// 格式模板，支持占位符 {level}, {target}, {message}, {timestamp}
    pub template: String,
}

impl TextFormatter {
    /// 创建默认文本格式化器
    pub fn new() -> Self {
        Self {
            template: "[{level}] {timestamp} {target} - {message}".to_string(),
        }
    }

    /// 创建自定义模板的格式化器
    pub fn with_template(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
        }
    }

    fn render(&self, record: &LogRecord) -> String {
        self.template
            .replace("{level}", record.level.as_str())
            .replace("{timestamp}", &record.timestamp.to_rfc3339())
            .replace("{target}", &record.target)
            .replace("{message}", &record.message)
    }
}

impl Default for TextFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl LogFormatter for TextFormatter {
    fn format(&self, record: &LogRecord) -> String {
        self.render(record)
    }

    fn name(&self) -> &str {
        "text"
    }
}

/// 结构化字段格式化器：输出 `key=value` 对
#[derive(Debug, Default)]
pub struct StructuredFormatter;

impl StructuredFormatter {
    /// 创建结构化格式化器
    pub fn new() -> Self {
        Self
    }
}

impl LogFormatter for StructuredFormatter {
    fn format(&self, record: &LogRecord) -> String {
        let mut parts = Vec::with_capacity(4 + record.fields.len());
        parts.push(format!("level={}", record.level.as_str()));
        parts.push(format!("ts={}", record.timestamp.to_rfc3339()));
        parts.push(format!("target={}", record.target));
        parts.push(format!("msg={}", record.message));
        // 字段按键排序以保证输出稳定
        let mut field_keys: Vec<&String> = record.fields.keys().collect();
        field_keys.sort();
        for key in field_keys {
            parts.push(format!("{}={}", key, record.fields[key]));
        }
        parts.join(" ")
    }

    fn name(&self) -> &str {
        "structured"
    }
}

// ============================================================================
// 输出器 trait
// ============================================================================

/// 日志输出器接口
pub trait LogOutput: Send + Sync {
    /// 写入格式化后的日志行
    fn write(&self, formatted: &str);

    /// 刷新输出缓冲（如有）
    fn flush(&self) {}

    /// 输出器名称
    fn name(&self) -> &str {
        "output"
    }
}

// ============================================================================
// 内置输出器
// ============================================================================

/// 内存输出器：将日志行存入内存缓冲，用于测试与采集
pub struct MemoryOutput {
    buffer: Mutex<Vec<String>>,
}

impl MemoryOutput {
    /// 创建空内存输出器
    pub fn new() -> Self {
        Self {
            buffer: Mutex::new(Vec::new()),
        }
    }

    /// 创建带初始容量的内存输出器
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Mutex::new(Vec::with_capacity(capacity)),
        }
    }

    /// 返回可共享的句柄，用于在外部读取输出内容
    pub fn handle(&self) -> MemoryOutputHandle {
        MemoryOutputHandle {
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for MemoryOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MemoryOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryOutput")
            .field("count", &self.buffer.lock().len())
            .finish()
    }
}

impl LogOutput for MemoryOutput {
    fn write(&self, formatted: &str) {
        self.buffer.lock().push(formatted.to_string());
    }

    fn name(&self) -> &str {
        "memory"
    }
}

/// 内存输出器句柄，可在管道构建后读取已写入的日志行
///
/// 注意：句柄与输出器独立，仅用于测试中预创建句柄并传入管道。
/// 生产中建议直接使用 `MemoryOutput` 并通过 `Arc` 共享。
pub struct MemoryOutputHandle {
    buffer: Arc<Mutex<Vec<String>>>,
}

impl MemoryOutputHandle {
    /// 创建句柄与对应的输出器
    pub fn new() -> (Self, MemoryOutputShared) {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let handle = Self {
            buffer: buffer.clone(),
        };
        let output = MemoryOutputShared { buffer };
        (handle, output)
    }

    /// 返回已写入的日志行快照
    pub fn entries(&self) -> Vec<String> {
        self.buffer.lock().clone()
    }

    /// 返回已写入的日志行数
    pub fn count(&self) -> usize {
        self.buffer.lock().len()
    }

    /// 清空缓冲
    pub fn clear(&self) {
        self.buffer.lock().clear();
    }
}

impl Default for MemoryOutputHandle {
    fn default() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// 共享内存输出器，与 [`MemoryOutputHandle`] 配对使用
pub struct MemoryOutputShared {
    buffer: Arc<Mutex<Vec<String>>>,
}

impl fmt::Debug for MemoryOutputShared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryOutputShared")
            .field("count", &self.buffer.lock().len())
            .finish()
    }
}

impl LogOutput for MemoryOutputShared {
    fn write(&self, formatted: &str) {
        self.buffer.lock().push(formatted.to_string());
    }

    fn name(&self) -> &str {
        "memory_shared"
    }
}

/// 回调输出器：将日志行传入闭包处理
pub struct CallbackOutput {
    callback: Box<dyn Fn(&str) + Send + Sync>,
}

impl CallbackOutput {
    /// 创建回调输出器
    pub fn new(callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self {
            callback: Box::new(callback),
        }
    }
}

impl fmt::Debug for CallbackOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallbackOutput").finish()
    }
}

impl LogOutput for CallbackOutput {
    fn write(&self, formatted: &str) {
        (self.callback)(formatted);
    }

    fn name(&self) -> &str {
        "callback"
    }
}

/// 计数输出器：仅统计写入次数，不存储内容
pub struct CountingOutput {
    count: std::sync::atomic::AtomicU64,
}

impl CountingOutput {
    /// 创建计数输出器
    pub fn new() -> Self {
        Self {
            count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 返回已写入次数
    pub fn count(&self) -> u64 {
        self.count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for CountingOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for CountingOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CountingOutput")
            .field("count", &self.count())
            .finish()
    }
}

impl LogOutput for CountingOutput {
    fn write(&self, _formatted: &str) {
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn name(&self) -> &str {
        "counting"
    }
}

// ============================================================================
// 日志管道
// ============================================================================

/// 日志管道：过滤器链 → 格式化器 → 输出器链
///
/// 处理流程：
/// 1. 依次执行所有过滤器，任一过滤失败则丢弃
/// 2. 格式化器将记录转为字符串
/// 3. 依次写入所有输出器
pub struct LogPipeline {
    filters: Vec<Box<dyn LogFilter>>,
    formatter: Box<dyn LogFormatter>,
    outputs: Vec<Box<dyn LogOutput>>,
    /// 已处理记录数
    processed_count: std::sync::atomic::AtomicU64,
    /// 已丢弃记录数
    dropped_count: std::sync::atomic::AtomicU64,
}

impl LogPipeline {
    /// 创建管道
    pub fn new(
        filters: Vec<Box<dyn LogFilter>>,
        formatter: Box<dyn LogFormatter>,
        outputs: Vec<Box<dyn LogOutput>>,
    ) -> Self {
        Self {
            filters,
            formatter,
            outputs,
            processed_count: std::sync::atomic::AtomicU64::new(0),
            dropped_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 处理一条日志记录
    pub fn process(&self, record: &LogRecord) {
        // 过滤器链
        for filter in &self.filters {
            if !filter.should_keep(record) {
                self.dropped_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }
        }
        // 格式化
        let formatted = self.formatter.format(record);
        // 输出器链
        for output in &self.outputs {
            output.write(&formatted);
        }
        self.processed_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 批量处理日志记录
    pub fn process_batch(&self, records: &[LogRecord]) {
        for record in records {
            self.process(record);
        }
    }

    /// 返回已处理记录数
    pub fn processed_count(&self) -> u64 {
        self.processed_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 返回已丢弃记录数
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 返回过滤器数量
    pub fn filter_count(&self) -> usize {
        self.filters.len()
    }

    /// 返回输出器数量
    pub fn output_count(&self) -> usize {
        self.outputs.len()
    }

    /// 刷新所有输出器
    pub fn flush(&self) {
        for output in &self.outputs {
            output.flush();
        }
    }
}

impl fmt::Debug for LogPipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogPipeline")
            .field("filters", &self.filters.len())
            .field("outputs", &self.outputs.len())
            .field("processed", &self.processed_count())
            .field("dropped", &self.dropped_count())
            .finish()
    }
}

// ============================================================================
// 管道构建器
// ============================================================================

/// 日志管道构建器，支持流式 API
pub struct LogPipelineBuilder {
    filters: Vec<Box<dyn LogFilter>>,
    formatter: Option<Box<dyn LogFormatter>>,
    outputs: Vec<Box<dyn LogOutput>>,
}

impl LogPipelineBuilder {
    /// 创建空构建器
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            formatter: None,
            outputs: Vec::new(),
        }
    }

    /// 添加过滤器
    pub fn filter(mut self, filter: Box<dyn LogFilter>) -> Self {
        self.filters.push(filter);
        self
    }

    /// 添加多个过滤器
    pub fn filters(mut self, filters: Vec<Box<dyn LogFilter>>) -> Self {
        self.filters.extend(filters);
        self
    }

    /// 设置格式化器（默认 JSON）
    pub fn formatter(mut self, formatter: Box<dyn LogFormatter>) -> Self {
        self.formatter = Some(formatter);
        self
    }

    /// 添加输出器
    pub fn output(mut self, output: Box<dyn LogOutput>) -> Self {
        self.outputs.push(output);
        self
    }

    /// 添加多个输出器
    pub fn outputs(mut self, outputs: Vec<Box<dyn LogOutput>>) -> Self {
        self.outputs.extend(outputs);
        self
    }

    /// 构建管道
    pub fn build(self) -> LogPipeline {
        let formatter = self.formatter.unwrap_or_else(|| Box::new(JsonFormatter));
        LogPipeline::new(self.filters, formatter, self.outputs)
    }
}

impl Default for LogPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for LogPipelineBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogPipelineBuilder")
            .field("filters", &self.filters.len())
            .field("has_formatter", &self.formatter.is_some())
            .field("outputs", &self.outputs.len())
            .finish()
    }
}

// ============================================================================
// 路由规则与路由器
// ============================================================================

/// 路由规则：匹配的日志发送到指定管道
pub struct RoutingRule {
    /// 匹配过滤器
    pub filter: Box<dyn LogFilter>,
    /// 目标管道
    pub pipeline: LogPipeline,
    /// 规则名称
    pub name: String,
}

impl fmt::Debug for RoutingRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoutingRule")
            .field("name", &self.name)
            .field("filter", &self.filter.name())
            .finish()
    }
}

/// 日志路由器：按规则将日志路由到不同管道
///
/// 按规则顺序匹配，第一个匹配的规则处理日志。
/// 若无规则匹配，则发送到默认管道（如有）。
pub struct LogRouter {
    rules: Vec<RoutingRule>,
    default: Option<LogPipeline>,
    /// 已路由记录数
    routed_count: std::sync::atomic::AtomicU64,
    /// 未匹配记录数
    unmatched_count: std::sync::atomic::AtomicU64,
}

impl LogRouter {
    /// 创建空路由器
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            default: None,
            routed_count: std::sync::atomic::AtomicU64::new(0),
            unmatched_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 添加路由规则
    pub fn route(mut self, rule: RoutingRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// 设置默认管道（无规则匹配时使用）
    pub fn default_pipeline(mut self, pipeline: LogPipeline) -> Self {
        self.default = Some(pipeline);
        self
    }

    /// 路由一条日志记录
    pub fn route_record(&self, record: &LogRecord) {
        for rule in &self.rules {
            if rule.filter.should_keep(record) {
                rule.pipeline.process(record);
                self.routed_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }
        }
        // 无规则匹配
        if let Some(default) = &self.default {
            default.process(record);
            self.routed_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.unmatched_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// 批量路由日志记录
    pub fn route_batch(&self, records: &[LogRecord]) {
        for record in records {
            self.route_record(record);
        }
    }

    /// 返回已路由记录数
    pub fn routed_count(&self) -> u64 {
        self.routed_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 返回未匹配记录数
    pub fn unmatched_count(&self) -> u64 {
        self.unmatched_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 返回规则数量
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 是否有默认管道
    pub fn has_default(&self) -> bool {
        self.default.is_some()
    }
}

impl Default for LogRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for LogRouter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogRouter")
            .field("rules", &self.rules.len())
            .field("has_default", &self.default.is_some())
            .field("routed", &self.routed_count())
            .field("unmatched", &self.unmatched_count())
            .finish()
    }
}

// ============================================================================
// 日志速率限制器
// ============================================================================

/// 日志速率限制器：在时间窗口内最多允许 N 条日志通过
pub struct RateLimitFilter {
    /// 时间窗口（毫秒）
    window_ms: u64,
    /// 窗口内最大条数
    max_count: u32,
    /// 当前窗口起始时间
    window_start: parking_lot::Mutex<Option<std::time::Instant>>,
    /// 当前窗口内计数
    count: parking_lot::Mutex<u32>,
}

impl RateLimitFilter {
    /// 创建速率限制器
    pub fn new(window_ms: u64, max_count: u32) -> Self {
        Self {
            window_ms,
            max_count,
            window_start: parking_lot::Mutex::new(None),
            count: parking_lot::Mutex::new(0),
        }
    }

    /// 每秒最多 N 条
    pub fn per_second(max_per_sec: u32) -> Self {
        Self::new(1000, max_per_sec)
    }
}

impl LogFilter for RateLimitFilter {
    fn should_keep(&self, _record: &LogRecord) -> bool {
        let now = std::time::Instant::now();
        let mut start = self.window_start.lock();
        let mut count = self.count.lock();
        match *start {
            None => {
                *start = Some(now);
                *count = 1;
                true
            }
            Some(s) => {
                let elapsed = now.duration_since(s);
                if elapsed.as_millis() as u64 >= self.window_ms {
                    *start = Some(now);
                    *count = 1;
                    true
                } else {
                    *count += 1;
                    *count <= self.max_count
                }
            }
        }
    }
}

// ============================================================================
// 日志采样器
// ============================================================================

/// 日志采样器：按比例随机采样日志
pub struct SamplingFilter {
    /// 采样率（0.0-1.0，1.0 = 全部通过）
    rate: f64,
    /// 伪随机状态
    state: parking_lot::Mutex<u64>,
}

impl SamplingFilter {
    /// 创建采样器
    ///
    /// `rate` 会被 clamp 到 [0.0, 1.0]
    pub fn new(rate: f64) -> Self {
        Self {
            rate: rate.clamp(0.0, 1.0),
            state: parking_lot::Mutex::new(0x12345678),
        }
    }

    /// 10% 采样
    pub fn ten_percent() -> Self {
        Self::new(0.1)
    }

    /// 1% 采样
    pub fn one_percent() -> Self {
        Self::new(0.01)
    }

    /// 获取采样率
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// 简单 LCG 伪随机
    fn next_random(&self) -> f64 {
        let mut state = self.state.lock();
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (*state >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl LogFilter for SamplingFilter {
    fn should_keep(&self, _record: &LogRecord) -> bool {
        self.next_random() < self.rate
    }
}

// ============================================================================
// 日志聚合器
// ============================================================================

/// 日志聚合器：相同消息的日志在时间窗口内只输出第一条和计数
pub struct LogAggregator {
    /// 时间窗口（毫秒）
    window_ms: u64,
    /// 聚合表：message -> (first_seen, count)
    entries: parking_lot::Mutex<HashMap<String, (std::time::Instant, u32)>>,
}

impl LogAggregator {
    /// 创建聚合器
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            entries: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// 处理一条日志，返回是否应输出
    ///
    /// 首次出现返回 true，窗口内重复出现返回 false，
    /// 窗口过期后首次出现返回 true 并携带上一窗口的计数。
    pub fn should_output(&self, message: &str) -> bool {
        let now = std::time::Instant::now();
        let mut entries = self.entries.lock();
        match entries.get_mut(message) {
            Some((first_seen, count)) => {
                let elapsed = now.duration_since(*first_seen);
                if elapsed.as_millis() as u64 >= self.window_ms {
                    *first_seen = now;
                    *count = 1;
                    true
                } else {
                    *count += 1;
                    false
                }
            }
            None => {
                entries.insert(message.to_string(), (now, 1));
                true
            }
        }
    }

    /// 获取消息的当前窗口计数
    pub fn count(&self, message: &str) -> u32 {
        self.entries
            .lock()
            .get(message)
            .map(|(_, c)| *c)
            .unwrap_or(0)
    }

    /// 清除所有聚合条目
    pub fn clear(&self) {
        self.entries.lock().clear();
    }
}

// ============================================================================
// 日志缓冲器
// ============================================================================

/// 日志缓冲器：积累日志到阈值后批量输出
pub struct LogBuffer {
    /// 缓冲区容量
    capacity: usize,
    /// 缓冲的日志记录
    buffer: parking_lot::Mutex<Vec<LogRecord>>,
}

impl LogBuffer {
    /// 创建缓冲器
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            buffer: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// 添加日志记录，返回是否达到 flush 阈值
    pub fn push(&self, record: LogRecord) -> bool {
        let mut buf = self.buffer.lock();
        buf.push(record);
        buf.len() >= self.capacity
    }

    /// 取出所有缓冲的日志（flush）
    pub fn flush(&self) -> Vec<LogRecord> {
        let mut buf = self.buffer.lock();
        std::mem::take(&mut *buf)
    }

    /// 当前缓冲区大小
    pub fn len(&self) -> usize {
        self.buffer.lock().len()
    }

    /// 缓冲区是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 缓冲区容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- LogRecord 测试 ----

    #[test]
    fn test_log_record_new() {
        let record = LogRecord::new(LogLevel::Info, "app", "hello");
        assert_eq!(record.level, LogLevel::Info);
        assert_eq!(record.target, "app");
        assert_eq!(record.message, "hello");
        assert!(record.fields.is_empty());
    }

    #[test]
    fn test_log_record_with_field() {
        let record = LogRecord::new(LogLevel::Warn, "db", "slow query")
            .with_field("duration", "150ms")
            .with_field("sql", "SELECT * FROM users");
        assert_eq!(record.fields.get("duration"), Some(&"150ms".to_string()));
        assert_eq!(
            record.fields.get("sql"),
            Some(&"SELECT * FROM users".to_string())
        );
    }

    #[test]
    fn test_log_record_level_at_least() {
        let record = LogRecord::new(LogLevel::Warn, "app", "msg");
        assert!(record.level_at_least(LogLevel::Warn));
        assert!(record.level_at_least(LogLevel::Info));
        assert!(!record.level_at_least(LogLevel::Error));
    }

    #[test]
    fn test_log_record_display() {
        let record = LogRecord::new(LogLevel::Error, "app", "crash");
        let s = format!("{}", record);
        assert!(s.contains("ERROR"));
        assert!(s.contains("app"));
        assert!(s.contains("crash"));
    }

    // ---- LevelThresholdFilter 测试 ----

    #[test]
    fn test_level_threshold_filter_passes() {
        let filter = LevelThresholdFilter::new(LogLevel::Info);
        let record = LogRecord::new(LogLevel::Info, "app", "msg");
        assert!(filter.should_keep(&record));
    }

    #[test]
    fn test_level_threshold_filter_blocks() {
        let filter = LevelThresholdFilter::new(LogLevel::Warn);
        let record = LogRecord::new(LogLevel::Debug, "app", "msg");
        assert!(!filter.should_keep(&record));
    }

    #[test]
    fn test_level_threshold_filter_boundary() {
        let filter = LevelThresholdFilter::new(LogLevel::Warn);
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Warn, "a", "m")));
        assert!(!filter.should_keep(&LogRecord::new(LogLevel::Info, "a", "m")));
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Error, "a", "m")));
    }

    // ---- TargetFilter 测试 ----

    #[test]
    fn test_target_filter_allowlist() {
        let filter = TargetFilter::allowlist(&["app", "db"]);
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Info, "app", "m")));
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Info, "db", "m")));
        assert!(!filter.should_keep(&LogRecord::new(LogLevel::Info, "cache", "m")));
    }

    #[test]
    fn test_target_filter_blocklist() {
        let filter = TargetFilter::blocklist(&["debug", "trace"]);
        assert!(!filter.should_keep(&LogRecord::new(LogLevel::Info, "debug", "m")));
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Info, "app", "m")));
    }

    // ---- ContainsFilter 测试 ----

    #[test]
    fn test_contains_filter_include() {
        let filter = ContainsFilter::include("error");
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Info, "a", "an error occurred")));
        assert!(!filter.should_keep(&LogRecord::new(LogLevel::Info, "a", "all good")));
    }

    #[test]
    fn test_contains_filter_exclude() {
        let filter = ContainsFilter::exclude("password");
        assert!(!filter.should_keep(&LogRecord::new(LogLevel::Info, "a", "user password leaked")));
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Info, "a", "user logged in")));
    }

    // ---- AllFilter / AnyFilter 测试 ----

    #[test]
    fn test_all_filter() {
        let filter = AllFilter::new(vec![
            Box::new(LevelThresholdFilter::new(LogLevel::Info)),
            Box::new(TargetFilter::allowlist(&["app"])),
        ]);
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Info, "app", "m")));
        assert!(!filter.should_keep(&LogRecord::new(LogLevel::Debug, "app", "m")));
        assert!(!filter.should_keep(&LogRecord::new(LogLevel::Info, "db", "m")));
    }

    #[test]
    fn test_any_filter() {
        let filter = AnyFilter::new(vec![
            Box::new(TargetFilter::allowlist(&["app"])),
            Box::new(LevelThresholdFilter::new(LogLevel::Error)),
        ]);
        // 匹配 target
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Info, "app", "m")));
        // 匹配 level
        assert!(filter.should_keep(&LogRecord::new(LogLevel::Error, "db", "m")));
        // 都不匹配
        assert!(!filter.should_keep(&LogRecord::new(LogLevel::Info, "db", "m")));
    }

    // ---- JsonFormatter 测试 ----

    #[test]
    fn test_json_formatter() {
        let formatter = JsonFormatter;
        let record = LogRecord::new(LogLevel::Info, "app", "hello");
        let json = formatter.format(&record);
        assert!(json.contains("\"level\":\"Info\""));
        assert!(json.contains("\"target\":\"app\""));
        assert!(json.contains("\"message\":\"hello\""));
    }

    #[test]
    fn test_json_formatter_with_fields() {
        let formatter = JsonFormatter;
        let record = LogRecord::new(LogLevel::Warn, "db", "slow").with_field("duration", "100ms");
        let json = formatter.format(&record);
        assert!(json.contains("duration"));
        assert!(json.contains("100ms"));
    }

    // ---- TextFormatter 测试 ----

    #[test]
    fn test_text_formatter_default() {
        let formatter = TextFormatter::new();
        let record = LogRecord::new(LogLevel::Error, "app", "crash");
        let text = formatter.format(&record);
        assert!(text.contains("ERROR"));
        assert!(text.contains("app"));
        assert!(text.contains("crash"));
    }

    #[test]
    fn test_text_formatter_custom_template() {
        let formatter = TextFormatter::with_template("{level} - {message}");
        let record = LogRecord::new(LogLevel::Info, "app", "hello");
        let text = formatter.format(&record);
        assert_eq!(text, "INFO - hello");
    }

    // ---- StructuredFormatter 测试 ----

    #[test]
    fn test_structured_formatter() {
        let formatter = StructuredFormatter::new();
        let record = LogRecord::new(LogLevel::Info, "app", "hello").with_field("key", "value");
        let text = formatter.format(&record);
        assert!(text.contains("level=INFO"));
        assert!(text.contains("target=app"));
        assert!(text.contains("msg=hello"));
        assert!(text.contains("key=value"));
    }

    // ---- MemoryOutput 测试 ----

    #[test]
    fn test_memory_output_write() {
        let output = MemoryOutput::new();
        output.write("line 1");
        output.write("line 2");
        let entries = output.buffer.lock().clone();
        assert_eq!(entries, vec!["line 1", "line 2"]);
    }

    // ---- MemoryOutputHandle 测试 ----

    #[test]
    fn test_memory_output_handle() {
        let (handle, output) = MemoryOutputHandle::new();
        output.write("test line");
        assert_eq!(handle.count(), 1);
        assert_eq!(handle.entries(), vec!["test line"]);
    }

    #[test]
    fn test_memory_output_handle_clear() {
        let (handle, output) = MemoryOutputHandle::new();
        output.write("a");
        output.write("b");
        assert_eq!(handle.count(), 2);
        handle.clear();
        assert_eq!(handle.count(), 0);
    }

    // ---- CountingOutput 测试 ----

    #[test]
    fn test_counting_output() {
        let output = CountingOutput::new();
        output.write("a");
        output.write("b");
        output.write("c");
        assert_eq!(output.count(), 3);
    }

    // ---- CallbackOutput 测试 ----

    #[test]
    fn test_callback_output() {
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counter_clone = counter.clone();
        let output = CallbackOutput::new(move |_s| {
            counter_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        output.write("a");
        output.write("b");
        assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    // ---- LogPipeline 测试 ----

    #[test]
    fn test_log_pipeline_basic() {
        let (handle, output) = MemoryOutputHandle::new();
        let pipeline = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(output)],
        );
        let record = LogRecord::new(LogLevel::Info, "app", "hello");
        pipeline.process(&record);
        assert_eq!(handle.count(), 1);
        assert_eq!(pipeline.processed_count(), 1);
        assert_eq!(pipeline.dropped_count(), 0);
    }

    #[test]
    fn test_log_pipeline_filter_drops() {
        let (handle, output) = MemoryOutputHandle::new();
        let pipeline = LogPipeline::new(
            vec![Box::new(LevelThresholdFilter::new(LogLevel::Warn))],
            Box::new(TextFormatter::new()),
            vec![Box::new(output)],
        );
        let record = LogRecord::new(LogLevel::Debug, "app", "debug msg");
        pipeline.process(&record);
        assert_eq!(handle.count(), 0);
        assert_eq!(pipeline.processed_count(), 0);
        assert_eq!(pipeline.dropped_count(), 1);
    }

    #[test]
    fn test_log_pipeline_multiple_filters() {
        let (handle, output) = MemoryOutputHandle::new();
        let pipeline = LogPipeline::new(
            vec![
                Box::new(LevelThresholdFilter::new(LogLevel::Info)),
                Box::new(TargetFilter::allowlist(&["app"])),
            ],
            Box::new(JsonFormatter),
            vec![Box::new(output)],
        );
        pipeline.process(&LogRecord::new(LogLevel::Info, "app", "ok"));
        pipeline.process(&LogRecord::new(LogLevel::Info, "db", "filtered"));
        pipeline.process(&LogRecord::new(LogLevel::Debug, "app", "filtered"));
        assert_eq!(handle.count(), 1);
        assert_eq!(pipeline.processed_count(), 1);
        assert_eq!(pipeline.dropped_count(), 2);
    }

    #[test]
    fn test_log_pipeline_multiple_outputs() {
        let counter = CountingOutput::new();
        let counter_ref = Arc::new(CountingOutput::new());
        // 使用 CallbackOutput 作为第二个输出
        let count2 = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let count2_clone = count2.clone();
        let callback = CallbackOutput::new(move |_| {
            count2_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        let pipeline = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(counter), Box::new(callback)],
        );
        pipeline.process(&LogRecord::new(LogLevel::Info, "a", "m"));
        pipeline.process(&LogRecord::new(LogLevel::Info, "a", "m"));
        assert_eq!(count2.load(std::sync::atomic::Ordering::Relaxed), 2);
        let _ = counter_ref;
    }

    #[test]
    fn test_log_pipeline_batch() {
        let (handle, output) = MemoryOutputHandle::new();
        let pipeline = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(output)],
        );
        let records = vec![
            LogRecord::new(LogLevel::Info, "a", "1"),
            LogRecord::new(LogLevel::Warn, "a", "2"),
            LogRecord::new(LogLevel::Error, "a", "3"),
        ];
        pipeline.process_batch(&records);
        assert_eq!(handle.count(), 3);
        assert_eq!(pipeline.processed_count(), 3);
    }

    // ---- LogPipelineBuilder 测试 ----

    #[test]
    fn test_pipeline_builder_basic() {
        let (handle, output) = MemoryOutputHandle::new();
        let pipeline = LogPipelineBuilder::new()
            .filter(Box::new(LevelThresholdFilter::new(LogLevel::Info)))
            .formatter(Box::new(TextFormatter::new()))
            .output(Box::new(output))
            .build();
        pipeline.process(&LogRecord::new(LogLevel::Info, "app", "hello"));
        pipeline.process(&LogRecord::new(LogLevel::Debug, "app", "dropped"));
        assert_eq!(handle.count(), 1);
        assert_eq!(pipeline.processed_count(), 1);
        assert_eq!(pipeline.dropped_count(), 1);
    }

    #[test]
    fn test_pipeline_builder_default_formatter() {
        let (handle, output) = MemoryOutputHandle::new();
        let pipeline = LogPipelineBuilder::new().output(Box::new(output)).build();
        pipeline.process(&LogRecord::new(LogLevel::Info, "app", "hello"));
        assert_eq!(handle.count(), 1);
        // 默认 JSON 格式器
        assert!(handle.entries()[0].contains("\"level\""));
    }

    #[test]
    fn test_pipeline_builder_multiple_outputs() {
        let (handle1, output1) = MemoryOutputHandle::new();
        let (handle2, output2) = MemoryOutputHandle::new();
        let pipeline = LogPipelineBuilder::new()
            .formatter(Box::new(TextFormatter::new()))
            .outputs(vec![Box::new(output1), Box::new(output2)])
            .build();
        pipeline.process(&LogRecord::new(LogLevel::Info, "a", "m"));
        assert_eq!(handle1.count(), 1);
        assert_eq!(handle2.count(), 1);
    }

    // ---- LogRouter 测试 ----

    #[test]
    fn test_log_router_basic() {
        let (handle1, output1) = MemoryOutputHandle::new();
        let (handle2, output2) = MemoryOutputHandle::new();

        let pipeline1 = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(output1)],
        );
        let pipeline2 = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(output2)],
        );

        let router = LogRouter::new()
            .route(RoutingRule {
                filter: Box::new(TargetFilter::allowlist(&["app"])),
                pipeline: pipeline1,
                name: "app_rule".to_string(),
            })
            .route(RoutingRule {
                filter: Box::new(TargetFilter::allowlist(&["db"])),
                pipeline: pipeline2,
                name: "db_rule".to_string(),
            });

        router.route_record(&LogRecord::new(LogLevel::Info, "app", "app msg"));
        router.route_record(&LogRecord::new(LogLevel::Info, "db", "db msg"));
        router.route_record(&LogRecord::new(LogLevel::Info, "cache", "unmatched"));

        assert_eq!(handle1.count(), 1);
        assert_eq!(handle2.count(), 1);
        assert_eq!(router.routed_count(), 2);
        assert_eq!(router.unmatched_count(), 1);
    }

    #[test]
    fn test_log_router_default_pipeline() {
        let (handle, output) = MemoryOutputHandle::new();
        let default_pipeline = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(output)],
        );

        let router = LogRouter::new().default_pipeline(default_pipeline);
        router.route_record(&LogRecord::new(LogLevel::Info, "any", "msg"));
        assert_eq!(handle.count(), 1);
        assert_eq!(router.routed_count(), 1);
        assert_eq!(router.unmatched_count(), 0);
    }

    #[test]
    fn test_log_router_no_match_no_default() {
        let router = LogRouter::new();
        router.route_record(&LogRecord::new(LogLevel::Info, "any", "msg"));
        assert_eq!(router.routed_count(), 0);
        assert_eq!(router.unmatched_count(), 1);
    }

    #[test]
    fn test_log_router_batch() {
        let (handle, output) = MemoryOutputHandle::new();
        let pipeline = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(output)],
        );
        let router = LogRouter::new().default_pipeline(pipeline);
        let records = vec![
            LogRecord::new(LogLevel::Info, "a", "1"),
            LogRecord::new(LogLevel::Warn, "b", "2"),
        ];
        router.route_batch(&records);
        assert_eq!(handle.count(), 2);
        assert_eq!(router.routed_count(), 2);
    }

    #[test]
    fn test_log_router_first_match_wins() {
        let (handle1, output1) = MemoryOutputHandle::new();
        let (handle2, output2) = MemoryOutputHandle::new();

        let pipeline1 = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(output1)],
        );
        let pipeline2 = LogPipeline::new(
            vec![],
            Box::new(TextFormatter::new()),
            vec![Box::new(output2)],
        );

        // 两个规则都能匹配 Error 级别，第一个规则应胜出
        let router = LogRouter::new()
            .route(RoutingRule {
                filter: Box::new(LevelThresholdFilter::new(LogLevel::Warn)),
                pipeline: pipeline1,
                name: "warn_plus".to_string(),
            })
            .route(RoutingRule {
                filter: Box::new(LevelThresholdFilter::new(LogLevel::Error)),
                pipeline: pipeline2,
                name: "error_only".to_string(),
            });

        router.route_record(&LogRecord::new(LogLevel::Error, "a", "m"));
        assert_eq!(handle1.count(), 1);
        assert_eq!(handle2.count(), 0);
    }

    // ---- RateLimitFilter 测试 ----

    #[test]
    fn test_rate_limit_allows_within_limit() {
        let filter = RateLimitFilter::per_second(5);
        let record = LogRecord::new(LogLevel::Info, "app", "msg");
        for _ in 0..5 {
            assert!(filter.should_keep(&record));
        }
    }

    #[test]
    fn test_rate_limit_blocks_over_limit() {
        let filter = RateLimitFilter::per_second(3);
        let record = LogRecord::new(LogLevel::Info, "app", "msg");
        for _ in 0..3 {
            assert!(filter.should_keep(&record));
        }
        assert!(!filter.should_keep(&record));
    }

    #[test]
    fn test_rate_limit_per_second_constructor() {
        let f = RateLimitFilter::per_second(10);
        assert_eq!(f.max_count, 10);
        assert_eq!(f.window_ms, 1000);
    }

    // ---- SamplingFilter 测试 ----

    #[test]
    fn test_sampling_full_rate() {
        let filter = SamplingFilter::new(1.0);
        let record = LogRecord::new(LogLevel::Info, "app", "msg");
        for _ in 0..100 {
            assert!(filter.should_keep(&record));
        }
    }

    #[test]
    fn test_sampling_zero_rate() {
        let filter = SamplingFilter::new(0.0);
        let record = LogRecord::new(LogLevel::Info, "app", "msg");
        for _ in 0..100 {
            assert!(!filter.should_keep(&record));
        }
    }

    #[test]
    fn test_sampling_rate_clamped() {
        let filter = SamplingFilter::new(2.0);
        assert_eq!(filter.rate(), 1.0);
        let filter2 = SamplingFilter::new(-1.0);
        assert_eq!(filter2.rate(), 0.0);
    }

    #[test]
    fn test_sampling_ten_percent() {
        let filter = SamplingFilter::ten_percent();
        assert!((filter.rate() - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_sampling_one_percent() {
        let filter = SamplingFilter::one_percent();
        assert!((filter.rate() - 0.01).abs() < 1e-10);
    }

    // ---- LogAggregator 测试 ----

    #[test]
    fn test_aggregator_first_output() {
        let agg = LogAggregator::new(1000);
        assert!(agg.should_output("error: db connection failed"));
    }

    #[test]
    fn test_aggregator_suppresses_duplicates() {
        let agg = LogAggregator::new(1000);
        assert!(agg.should_output("error: timeout"));
        assert!(!agg.should_output("error: timeout"));
        assert!(!agg.should_output("error: timeout"));
        assert_eq!(agg.count("error: timeout"), 3);
    }

    #[test]
    fn test_aggregator_different_messages() {
        let agg = LogAggregator::new(1000);
        assert!(agg.should_output("error A"));
        assert!(agg.should_output("error B"));
        assert!(!agg.should_output("error A"));
        assert!(!agg.should_output("error B"));
    }

    #[test]
    fn test_aggregator_clear() {
        let agg = LogAggregator::new(1000);
        agg.should_output("msg");
        assert_eq!(agg.count("msg"), 1);
        agg.clear();
        assert_eq!(agg.count("msg"), 0);
    }

    #[test]
    fn test_aggregator_count_unknown() {
        let agg = LogAggregator::new(1000);
        assert_eq!(agg.count("unknown"), 0);
    }

    // ---- LogBuffer 测试 ----

    #[test]
    fn test_log_buffer_push_and_flush() {
        let buf = LogBuffer::new(3);
        assert!(buf.is_empty());
        buf.push(LogRecord::new(LogLevel::Info, "a", "1"));
        buf.push(LogRecord::new(LogLevel::Info, "a", "2"));
        assert_eq!(buf.len(), 2);
        let flushed = buf.flush();
        assert_eq!(flushed.len(), 2);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_log_buffer_threshold() {
        let buf = LogBuffer::new(2);
        assert!(!buf.push(LogRecord::new(LogLevel::Info, "a", "1")));
        assert!(buf.push(LogRecord::new(LogLevel::Info, "a", "2")));
    }

    #[test]
    fn test_log_buffer_capacity() {
        let buf = LogBuffer::new(5);
        assert_eq!(buf.capacity(), 5);
    }

    #[test]
    fn test_log_buffer_capacity_clamped() {
        let buf = LogBuffer::new(0);
        assert_eq!(buf.capacity(), 1);
    }
}
