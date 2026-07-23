//! # SZ-ORM Limit — 限流器
//!
//! 提供令牌桶与滑动窗口限流，内置 OOM 防护（默认 max_keys=10000）。
//!
//! ## 主要类型
//!
//! - [`RateLimiter`] trait — 限流器接口
//! - `TokenBucketLimiter` — 令牌桶实现
//! - `SlidingWindowLimiter` — 滑动窗口实现
//!
//! v0.2.1 修复 Critical S-3：引入 `DEFAULT_MAX_KEYS` 防止无限 key 导致 OOM。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// 默认最大 key 数量（v0.2.1 新增，修复 Critical S-3 OOM DoS）
///
/// 当 entries.len() 超过此值时，会强制淘汰一个 entry。
/// 调用方可通过 `with_max_keys()` 调整。
pub const DEFAULT_MAX_KEYS: usize = 10_000;

pub trait RateLimiter: Send + Sync {
    fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError>;
    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError>;
    fn reset(&self, key: &str) -> Result<(), RateLimitError>;
}

#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u64,
    pub reset_at: i64,
}

impl RateLimitResult {
    pub fn allowed(remaining: u64, reset_at: i64) -> Self {
        Self {
            allowed: true,
            remaining,
            reset_at,
        }
    }

    pub fn rejected(remaining: u64, reset_at: i64) -> Self {
        Self {
            allowed: false,
            remaining,
            reset_at,
        }
    }
}

pub struct SlidingWindowRateLimiter {
    max_requests: u64,
    window_size: Duration,
    entries: Arc<RwLock<HashMap<String, SlidingWindowEntry>>>,
    /// 最大 key 数量（v0.2.1 新增，修复 Critical S-3 OOM DoS）
    max_keys: usize,
}

#[derive(Clone)]
struct SlidingWindowEntry {
    requests: Vec<Instant>,
}

impl SlidingWindowRateLimiter {
    pub fn new(max_requests: u64, window_size: Duration) -> Self {
        Self {
            max_requests,
            window_size,
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_keys: DEFAULT_MAX_KEYS,
        }
    }

    /// 配置最大 key 数量（v0.2.1 新增，修复 Critical S-3 OOM DoS）
    ///
    /// 当 entries.len() 超过 `max_keys` 时，会强制淘汰一个最旧的 entry。
    /// 默认值为 `DEFAULT_MAX_KEYS`（10000）。
    pub fn with_max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = max_keys;
        self
    }

    fn cleanup_old_requests(&self, entry: &mut SlidingWindowEntry) {
        let now = Instant::now();
        entry
            .requests
            .retain(|&time| now.duration_since(time) < self.window_size);
    }

    /// 强制淘汰超出 `max_keys` 的最旧 entry（v0.2.1 新增，修复 Critical S-3）
    ///
    /// 策略：遍历所有 entry，找到 `requests[0]`（窗口内最早请求时间）最小的那个并删除。
    /// 复杂度 O(n)，但仅在 `entries.len() > max_keys` 时触发。
    fn enforce_max_keys(&self, entries: &mut HashMap<String, SlidingWindowEntry>) {
        while entries.len() > self.max_keys {
            // 找到最旧的 entry（requests.first() 时间最早）
            let now = Instant::now();
            let oldest_key = entries
                .iter()
                .min_by_key(|(_, e)| e.requests.first().copied().unwrap_or(now))
                .map(|(k, _)| k.clone());
            match oldest_key {
                Some(k) => {
                    entries.remove(&k);
                }
                None => break,
            }
        }
    }
}

impl RateLimiter for SlidingWindowRateLimiter {
    fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        // v0.2.1 修复 Critical S-3：写入前强制淘汰超出 max_keys 的 entry
        if entries.len() >= self.max_keys && !entries.contains_key(key) {
            self.enforce_max_keys(&mut entries);
        }

        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| SlidingWindowEntry {
                requests: Vec::new(),
            });

        self.cleanup_old_requests(entry);

        if entry.requests.len() < self.max_requests as usize {
            entry.requests.push(Instant::now());
            let remaining = self.max_requests - entry.requests.len() as u64;
            let reset_at = now_timestamp() + self.window_size.as_millis() as i64;
            Ok(RateLimitResult::allowed(remaining, reset_at))
        } else {
            let oldest = entry
                .requests
                .first()
                .map(|t| {
                    let elapsed = t.elapsed().as_millis() as i64;
                    let window_ms = self.window_size.as_millis() as i64;
                    now_timestamp() + (window_ms - elapsed)
                })
                .unwrap_or(now_timestamp());

            let remaining = 0;
            Ok(RateLimitResult::rejected(remaining, oldest))
        }
    }

    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        self.acquire(key)
    }

    fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

pub struct TokenBucketRateLimiter {
    capacity: f64,
    refill_rate: f64,
    entries: Arc<RwLock<HashMap<String, TokenBucketEntry>>>,
    /// 最大 key 数量（v0.2.1 新增，修复 Critical S-3 OOM DoS）
    max_keys: usize,
}

#[derive(Clone)]
struct TokenBucketEntry {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucketRateLimiter {
    pub fn new(capacity: u64, refill_per_second: f64) -> Self {
        Self {
            capacity: capacity as f64,
            refill_rate: refill_per_second,
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_keys: DEFAULT_MAX_KEYS,
        }
    }

    /// 配置最大 key 数量（v0.2.1 新增，修复 Critical S-3 OOM DoS）
    pub fn with_max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = max_keys;
        self
    }

    fn refill(&self, entry: &mut TokenBucketEntry) {
        let now = Instant::now();
        let elapsed = now.duration_since(entry.last_refill).as_secs_f64();
        // 修复：refill_rate <= 0.0 时不补充令牌（也不消耗）
        // 避免负数 refill_rate 导致 tokens 递减
        let tokens_to_add = if self.refill_rate > 0.0 {
            elapsed * self.refill_rate
        } else {
            0.0
        };

        entry.tokens = (entry.tokens + tokens_to_add).min(self.capacity);
        entry.last_refill = now;
    }

    /// 强制淘汰超出 `max_keys` 的最旧 entry（v0.2.1 新增，修复 Critical S-3）
    ///
    /// 策略：找到 `last_refill` 最早的 entry（即最久未访问的）并删除。
    /// 复杂度 O(n)，但仅在 `entries.len() > max_keys` 时触发。
    fn enforce_max_keys(&self, entries: &mut HashMap<String, TokenBucketEntry>) {
        while entries.len() > self.max_keys {
            let oldest_key = entries
                .iter()
                .min_by_key(|(_, e)| e.last_refill)
                .map(|(k, _)| k.clone());
            match oldest_key {
                Some(k) => {
                    entries.remove(&k);
                }
                None => break,
            }
        }
    }
}

impl RateLimiter for TokenBucketRateLimiter {
    fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        // v0.2.1 修复 Critical S-3：写入前强制淘汰超出 max_keys 的 entry
        if entries.len() >= self.max_keys && !entries.contains_key(key) {
            self.enforce_max_keys(&mut entries);
        }

        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| TokenBucketEntry {
                tokens: self.capacity,
                last_refill: Instant::now(),
            });

        self.refill(entry);

        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            let remaining = entry.tokens.floor() as u64;
            // 修复：refill_rate <= 0.0 时令牌不补充，reset_at 设为远期时间
            // 避免除零产生 inf，inf as i64 触发 panic
            let reset_at = if self.refill_rate > 0.0 {
                now_timestamp() + (1000.0 / self.refill_rate) as i64
            } else {
                // 令牌永不补充，reset_at 设为远期时间（约 292 年后的 i64::MAX）
                i64::MAX
            };
            Ok(RateLimitResult::allowed(remaining, reset_at))
        } else {
            // 修复：refill_rate <= 0.0 时令牌不补充，永远等待
            let reset_at = if self.refill_rate > 0.0 {
                let wait_time = ((1.0 - entry.tokens) / self.refill_rate * 1000.0) as i64;
                now_timestamp() + wait_time
            } else {
                // 令牌永不补充，永远等待
                i64::MAX
            };
            Ok(RateLimitResult::rejected(0, reset_at))
        }
    }

    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        self.acquire(key)
    }

    fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

#[derive(Debug)]
pub enum RateLimitError {
    KeyNotFound(String),
    Internal(String),
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::KeyNotFound(key) => write!(f, "Key not found: {}", key),
            RateLimitError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for RateLimitError {}

impl serde::Serialize for RateLimitError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn now_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ============================================================================
// 固定窗口算法（Fixed Window）
//
// 经典的固定窗口计数器限流：将时间划分为固定窗口（如每分钟），
// 每个窗口内维护一个计数器，请求到来时计数器+1，超过阈值则拒绝。
// 窗口结束时计数器重置。
//
// 优点：实现简单、内存占用低
// 缺点：存在边界突刺问题（窗口切换瞬间可能通过 2 倍阈值的请求）
// ============================================================================

/// 固定窗口限流器
///
/// 将时间划分为固定大小的窗口，每个 key 在每个窗口内有独立的计数器。
/// 当计数器超过 `max_requests` 时拒绝请求。
///
/// # 边界突刺
///
/// 固定窗口算法存在边界突刺问题：如果窗口结束前 1 秒通过了 max_requests
/// 个请求，新窗口开始后第 1 秒又通过了 max_requests 个请求，则 2 秒内
/// 通过了 2 * max_requests 个请求。如需更平滑的限流，请使用
/// `SlidingWindowRateLimiter` 或 `TokenBucketRateLimiter`。
pub struct FixedWindowRateLimiter {
    max_requests: u64,
    window_size: Duration,
    entries: Arc<RwLock<HashMap<String, FixedWindowEntry>>>,
    max_keys: usize,
}

#[derive(Clone)]
struct FixedWindowEntry {
    count: u64,
    window_start: Instant,
}

impl FixedWindowRateLimiter {
    /// 创建固定窗口限流器
    ///
    /// - `max_requests`：每个窗口内允许的最大请求数
    /// - `window_size`：窗口大小（如 60 秒）
    pub fn new(max_requests: u64, window_size: Duration) -> Self {
        Self {
            max_requests,
            window_size,
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_keys: DEFAULT_MAX_KEYS,
        }
    }

    /// 配置最大 key 数量（OOM 防护）
    pub fn with_max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = max_keys;
        self
    }

    /// 强制淘汰超出 `max_keys` 的最旧 entry
    fn enforce_max_keys(&self, entries: &mut HashMap<String, FixedWindowEntry>) {
        while entries.len() > self.max_keys {
            let oldest_key = entries
                .iter()
                .min_by_key(|(_, e)| e.window_start)
                .map(|(k, _)| k.clone());
            match oldest_key {
                Some(k) => {
                    entries.remove(&k);
                }
                None => break,
            }
        }
    }
}

impl RateLimiter for FixedWindowRateLimiter {
    fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        if entries.len() >= self.max_keys && !entries.contains_key(key) {
            self.enforce_max_keys(&mut entries);
        }

        let now = Instant::now();
        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| FixedWindowEntry {
                count: 0,
                window_start: now,
            });

        // 检查窗口是否过期，过期则重置
        if now.duration_since(entry.window_start) >= self.window_size {
            entry.count = 0;
            entry.window_start = now;
        }

        if entry.count < self.max_requests {
            entry.count += 1;
            let remaining = self.max_requests - entry.count;
            let reset_at =
                now_timestamp() + self.window_size.as_millis() as i64;
            Ok(RateLimitResult::allowed(remaining, reset_at))
        } else {
            // 计算窗口重置时间
            let elapsed = now.duration_since(entry.window_start);
            let remaining_window = self.window_size.checked_sub(elapsed).unwrap_or(Duration::ZERO);
            let reset_at = now_timestamp() + remaining_window.as_millis() as i64;
            Ok(RateLimitResult::rejected(0, reset_at))
        }
    }

    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        self.acquire(key)
    }

    fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

// ============================================================================
// 分布式限流（Distributed Rate Limiting）
//
// 通过抽象后端存储实现分布式限流，支持多实例共享限流状态。
// 提供内存后端（InMemoryBackend）模拟 Redis 行为，无需外部依赖。
//
// 接口设计参照 Redis 的 INCR + EXPIRE 原子操作模式：
// 1. INCR key -> 获取当前计数
// 2. 如果计数 == 1，设置 EXPIRE
// 3. 根据计数判断是否允许
// ============================================================================

/// 分布式后端 trait
///
/// 抽象分布式存储后端（如 Redis），提供原子计数器操作。
/// 内存实现 `InMemoryBackend` 可用于单机测试和开发。
pub trait DistributedBackend: Send + Sync {
    /// 原子递增并返回递增后的值
    ///
    /// 如果 key 不存在，创建并返回 1。
    /// 如果 key 存在且未过期，递增并返回新值。
    /// 如果 key 存在但已过期，重置为 1 并返回。
    ///
    /// - `key`：限流键
    /// - `window_secs`：窗口大小（秒），仅在 key 新建或过期时设置 TTL
    /// - `window_start`：当前窗口起始时间（Unix 秒）
    /// - `max_requests`：窗口内最大请求数
    ///
    /// 返回 `(count, reset_at_secs)`：
    /// - `count`：递增后的计数
    /// - `reset_at_secs`：窗口重置时间（Unix 秒）
    fn incr_and_get(
        &self,
        key: &str,
        window_secs: u64,
        window_start: i64,
        max_requests: u64,
    ) -> Result<(u64, i64), RateLimitError>;

    /// 获取当前计数（不递增）
    fn get(&self, key: &str) -> Result<Option<(u64, i64)>, RateLimitError>;

    /// 重置 key（删除）
    fn reset_key(&self, key: &str) -> Result<(), RateLimitError>;
}

/// 内存后端（模拟 Redis）
///
/// 使用 `RwLock<HashMap>` 存储计数器，模拟 Redis 的 INCR + EXPIRE 行为。
/// 适用于单机场景和测试，不适用于真正的分布式环境。
pub struct InMemoryBackend {
    entries: RwLock<HashMap<String, (u64, i64)>>, // key -> (count, window_start_secs)
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DistributedBackend for InMemoryBackend {
    fn incr_and_get(
        &self,
        key: &str,
        window_secs: u64,
        window_start: i64,
        _max_requests: u64,
    ) -> Result<(u64, i64), RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| (0, window_start));

        // 检查窗口是否过期
        if window_start - entry.1 >= window_secs as i64 {
            // 窗口过期，重置
            *entry = (0, window_start);
        }

        entry.0 += 1;
        let reset_at = entry.1 + window_secs as i64;
        Ok((entry.0, reset_at))
    }

    fn get(&self, key: &str) -> Result<Option<(u64, i64)>, RateLimitError> {
        let entries = self
            .entries
            .read()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        Ok(entries.get(key).copied())
    }

    fn reset_key(&self, key: &str) -> Result<(), RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

/// 分布式限流器
///
/// 使用 `DistributedBackend` 实现跨实例共享的固定窗口限流。
/// 适用于多实例部署场景。
pub struct DistributedRateLimiter {
    backend: Arc<dyn DistributedBackend>,
    max_requests: u64,
    window_secs: u64,
}

impl DistributedRateLimiter {
    /// 创建分布式限流器
    ///
    /// - `backend`：分布式后端（如 `InMemoryBackend`）
    /// - `max_requests`：每个窗口内允许的最大请求数
    /// - `window_secs`：窗口大小（秒）
    pub fn new(
        backend: Arc<dyn DistributedBackend>,
        max_requests: u64,
        window_secs: u64,
    ) -> Self {
        Self {
            backend,
            max_requests,
            window_secs,
        }
    }

    /// 使用内存后端创建分布式限流器（便捷方法）
    pub fn in_memory(max_requests: u64, window_secs: u64) -> Self {
        Self::new(Arc::new(InMemoryBackend::new()), max_requests, window_secs)
    }
}

impl RateLimiter for DistributedRateLimiter {
    fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let window_start = now_secs();
        let (count, reset_at) = self.backend.incr_and_get(
            key,
            self.window_secs,
            window_start,
            self.max_requests,
        )?;

        if count <= self.max_requests {
            let remaining = self.max_requests - count;
            Ok(RateLimitResult::allowed(remaining, reset_at * 1000))
        } else {
            Ok(RateLimitResult::rejected(0, reset_at * 1000))
        }
    }

    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        self.acquire(key)
    }

    fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        self.backend.reset_key(key)
    }
}

// ============================================================================
// 限流响应策略（Rate Limit Response Strategy）
//
// 提供标准化的限流响应头和响应体生成，符合 IETF draft-ietf-httpapi-ratelimit-headers
// 和常见 API 网关（如 Kong、AWS API Gateway）的实践。
// ============================================================================

/// 限流响应头
///
/// 生成标准的限流响应头，适用于 HTTP API 响应。
///
/// # 标准头
///
/// - `X-RateLimit-Limit`：窗口内最大请求数
/// - `X-RateLimit-Remaining`：剩余请求数
/// - `X-RateLimit-Reset`：窗口重置时间（Unix 秒）
/// - `Retry-After`：拒绝时建议的重试等待时间（秒），仅在拒绝时包含
#[derive(Debug, Clone)]
pub struct RateLimitHeaders {
    /// 窗口内最大请求数
    pub limit: u64,
    /// 剩余请求数
    pub remaining: u64,
    /// 窗口重置时间（Unix 秒）
    pub reset: i64,
    /// 重试等待时间（秒），仅在被拒绝时设置
    pub retry_after: Option<u64>,
}

impl RateLimitHeaders {
    /// 从限流结果创建响应头
    ///
    /// - `result`：限流结果
    /// - `limit`：窗口内最大请求数
    pub fn from_result(result: &RateLimitResult, limit: u64) -> Self {
        let reset_secs = result.reset_at / 1000;
        let now_secs_val = now_secs();
        let retry_after = if !result.allowed {
            let diff = reset_secs - now_secs_val;
            if diff > 0 {
                Some(diff as u64)
            } else {
                Some(1)
            }
        } else {
            None
        };

        Self {
            limit,
            remaining: result.remaining,
            reset: reset_secs,
            retry_after,
        }
    }

    /// 转换为 HTTP 头键值对
    pub fn to_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![
            ("X-RateLimit-Limit".to_string(), self.limit.to_string()),
            (
                "X-RateLimit-Remaining".to_string(),
                self.remaining.to_string(),
            ),
            ("X-RateLimit-Reset".to_string(), self.reset.to_string()),
        ];
        if let Some(retry) = self.retry_after {
            headers.push(("Retry-After".to_string(), retry.to_string()));
        }
        headers
    }

    /// 转换为 JSON 对象
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::json!({
            "X-RateLimit-Limit": self.limit,
            "X-RateLimit-Remaining": self.remaining,
            "X-RateLimit-Reset": self.reset,
        });
        if let Some(retry) = self.retry_after {
            map["Retry-After"] = serde_json::json!(retry);
        }
        map
    }
}

/// 限流响应策略
///
/// 定义被限流时的响应行为。
#[derive(Debug, Clone)]
pub enum RateLimitResponseStrategy {
    /// 返回 429 Too Many Requests
    TooManyRequests,
    /// 返回 503 Service Unavailable
    ServiceUnavailable,
    /// 自定义状态码
    Custom(u16),
}

impl RateLimitResponseStrategy {
    /// 获取对应的 HTTP 状态码
    pub fn status_code(&self) -> u16 {
        match self {
            RateLimitResponseStrategy::TooManyRequests => 429,
            RateLimitResponseStrategy::ServiceUnavailable => 503,
            RateLimitResponseStrategy::Custom(code) => *code,
        }
    }
}

/// 限流响应
///
/// 封装被限流时的完整响应信息，包括状态码、头和响应体。
#[derive(Debug, Clone)]
pub struct RateLimitResponse {
    /// HTTP 状态码
    pub status_code: u16,
    /// 响应头
    pub headers: RateLimitHeaders,
    /// JSON 响应体
    pub body: serde_json::Value,
}

impl RateLimitResponse {
    /// 创建被限流的响应
    ///
    /// - `result`：限流结果（必须是 rejected）
    /// - `limit`：窗口内最大请求数
    /// - `strategy`：响应策略
    pub fn rejected(
        result: &RateLimitResult,
        limit: u64,
        strategy: RateLimitResponseStrategy,
    ) -> Self {
        let headers = RateLimitHeaders::from_result(result, limit);
        let status_code = strategy.status_code();
        let body = serde_json::json!({
            "error": "rate_limit_exceeded",
            "message": "Rate limit exceeded. Please retry later.",
            "retry_after": headers.retry_after.unwrap_or(1),
        });

        Self {
            status_code,
            headers,
            body,
        }
    }

    /// 创建允许通过的响应（仅包含头，无响应体）
    pub fn allowed(result: &RateLimitResult, limit: u64) -> Self {
        let headers = RateLimitHeaders::from_result(result, limit);
        Self {
            status_code: 200,
            headers,
            body: serde_json::Value::Null,
        }
    }
}

// ============================================================================
// 限流策略组合器（Rate Limit Policy）
//
// 允许将多个限流器组合，实现多维度限流（如同时限制 IP 和用户）。
// ============================================================================

/// 多维度限流策略
///
/// 对同一个请求应用多个限流器，只要任一限流器拒绝则拒绝。
/// 适用于同时限制 IP 级别和用户级别的场景。
pub struct MultiRateLimiter {
    limiters: Vec<Arc<dyn RateLimiter>>,
}

impl MultiRateLimiter {
    /// 创建多维度限流器
    pub fn new(limiters: Vec<Arc<dyn RateLimiter>>) -> Self {
        Self { limiters }
    }

    /// 添加限流器
    pub fn with_limiter(mut self, limiter: Arc<dyn RateLimiter>) -> Self {
        self.limiters.push(limiter);
        self
    }

    /// 检查所有限流器，返回最严格的结果
    ///
    /// 如果任一限流器拒绝，则返回拒绝结果（取剩余最少的）。
    /// 如果全部允许，则返回剩余最少的结果。
    pub fn check_all(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut best_result: Option<RateLimitResult> = None;
        for limiter in &self.limiters {
            let result = limiter.acquire(key)?;
            match &best_result {
                None => best_result = Some(result),
                Some(current) => {
                    // 取更严格的结果
                    if !result.allowed {
                        // 拒绝优先
                        if !current.allowed {
                            // 两个都拒绝，取剩余更少的
                            if result.remaining <= current.remaining {
                                best_result = Some(result);
                            }
                        } else {
                            // 当前允许但新的拒绝 -> 用拒绝结果
                            best_result = Some(result);
                        }
                    } else if current.allowed && result.remaining < current.remaining {
                        // 两个都允许，取剩余更少的
                        best_result = Some(result);
                    }
                }
            }
        }
        best_result.ok_or_else(|| RateLimitError::Internal("No limiters configured".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_result_allowed() {
        let result = RateLimitResult::allowed(5, 1000);
        assert!(result.allowed);
        assert_eq!(result.remaining, 5);
        assert_eq!(result.reset_at, 1000);
    }

    #[test]
    fn test_rate_limit_result_rejected() {
        let result = RateLimitResult::rejected(0, 2000);
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
    }

    #[test]
    fn test_sliding_window_limiter_new() {
        let limiter = SlidingWindowRateLimiter::new(10, Duration::from_secs(60));
        let result = limiter.acquire("test-key");
        assert!(result.is_ok());
        assert!(result.unwrap().allowed);
    }

    #[test]
    fn test_sliding_window_limiter_full() {
        let limiter = SlidingWindowRateLimiter::new(2, Duration::from_secs(60));

        let r1 = limiter.acquire("key1").unwrap();
        assert!(r1.allowed);

        let r2 = limiter.acquire("key1").unwrap();
        assert!(r2.allowed);

        let r3 = limiter.acquire("key1").unwrap();
        assert!(!r3.allowed);
    }

    #[test]
    fn test_sliding_window_different_keys() {
        let limiter = SlidingWindowRateLimiter::new(1, Duration::from_secs(60));

        let r1 = limiter.acquire("key-a").unwrap();
        assert!(r1.allowed);

        let r2 = limiter.acquire("key-b").unwrap();
        assert!(r2.allowed);
    }

    #[test]
    fn test_sliding_window_reset() {
        let limiter = SlidingWindowRateLimiter::new(1, Duration::from_secs(60));

        limiter.acquire("reset-key").unwrap();
        limiter.acquire("reset-key").unwrap();

        limiter.reset("reset-key").unwrap();

        let result = limiter.acquire("reset-key").unwrap();
        assert!(result.allowed);
    }

    #[test]
    fn test_token_bucket_limiter_new() {
        let limiter = TokenBucketRateLimiter::new(10, 1.0);
        let result = limiter.acquire("test-key");
        assert!(result.is_ok());
        assert!(result.unwrap().allowed);
    }

    #[test]
    fn test_token_bucket_limiter_depletes() {
        let limiter = TokenBucketRateLimiter::new(2, 1.0);

        let r1 = limiter.acquire("key1").unwrap();
        assert!(r1.allowed);
        assert_eq!(r1.remaining, 1);

        let r2 = limiter.acquire("key1").unwrap();
        assert!(r2.allowed);
        assert_eq!(r2.remaining, 0);

        let r3 = limiter.acquire("key1").unwrap();
        assert!(!r3.allowed);
    }

    #[test]
    fn test_token_bucket_different_keys() {
        let limiter = TokenBucketRateLimiter::new(1, 1.0);

        let r1 = limiter.acquire("key-a").unwrap();
        assert!(r1.allowed);

        let r2 = limiter.acquire("key-b").unwrap();
        assert!(r2.allowed);
    }

    #[test]
    fn test_token_bucket_reset() {
        let limiter = TokenBucketRateLimiter::new(1, 1.0);

        limiter.acquire("reset-key").unwrap();
        limiter.acquire("reset-key").unwrap();

        limiter.reset("reset-key").unwrap();

        let result = limiter.acquire("reset-key").unwrap();
        assert!(result.allowed);
    }

    #[test]
    fn test_limiter_try_acquire() {
        let limiter = SlidingWindowRateLimiter::new(1, Duration::from_secs(60));

        let r1 = limiter.try_acquire("key").unwrap();
        assert!(r1.allowed);

        let r2 = limiter.try_acquire("key").unwrap();
        assert!(!r2.allowed);
    }

    // ===== TDD RED：refill_rate=0 panic 修复测试 =====

    #[test]
    fn test_token_bucket_zero_refill_rate_does_not_panic() {
        // refill_rate=0.0 表示令牌永不补充
        // capacity=1，第一次 acquire 应允许，第二次应拒绝且不 panic
        let limiter = TokenBucketRateLimiter::new(1, 0.0);

        let r1 = limiter.acquire("zero-refill").unwrap();
        assert!(r1.allowed, "first acquire should be allowed");

        // 第二次 acquire：令牌耗尽且不补充，应拒绝而非 panic
        let r2 = limiter.acquire("zero-refill").unwrap();
        assert!(!r2.allowed, "second acquire should be rejected");
        // reset_at 应为一个合理的远期时间（令牌不补充，永远等待）
        assert!(
            r2.reset_at > 0,
            "reset_at should be a valid timestamp, got: {}",
            r2.reset_at
        );
    }

    #[test]
    fn test_token_bucket_negative_refill_rate_does_not_panic() {
        // refill_rate=-1.0 是错误配置，应被当作 0.0 处理而非 panic
        let limiter = TokenBucketRateLimiter::new(1, -1.0);

        let r1 = limiter.acquire("neg-refill").unwrap();
        assert!(r1.allowed, "first acquire should be allowed");

        let r2 = limiter.acquire("neg-refill").unwrap();
        assert!(!r2.allowed, "second acquire should be rejected");
        assert!(
            r2.reset_at > 0,
            "reset_at should be a valid timestamp, got: {}",
            r2.reset_at
        );
    }

    // ===== 固定窗口算法测试 =====

    #[test]
    fn test_fixed_window_limiter_allows_within_limit() {
        let limiter = FixedWindowRateLimiter::new(5, Duration::from_secs(60));
        for i in 0..5 {
            let r = limiter.acquire("key").unwrap();
            assert!(r.allowed, "request {} should be allowed", i);
        }
    }

    #[test]
    fn test_fixed_window_limiter_rejects_over_limit() {
        let limiter = FixedWindowRateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.acquire("key").unwrap().allowed);
        assert!(limiter.acquire("key").unwrap().allowed);
        let r3 = limiter.acquire("key").unwrap();
        assert!(!r3.allowed);
        assert_eq!(r3.remaining, 0);
    }

    #[test]
    fn test_fixed_window_limiter_different_keys() {
        let limiter = FixedWindowRateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.acquire("key-a").unwrap().allowed);
        assert!(limiter.acquire("key-b").unwrap().allowed);
    }

    #[test]
    fn test_fixed_window_limiter_reset() {
        let limiter = FixedWindowRateLimiter::new(1, Duration::from_secs(60));
        limiter.acquire("key").unwrap();
        assert!(!limiter.acquire("key").unwrap().allowed);
        limiter.reset("key").unwrap();
        assert!(limiter.acquire("key").unwrap().allowed);
    }

    #[test]
    fn test_fixed_window_limiter_remaining_decreases() {
        let limiter = FixedWindowRateLimiter::new(3, Duration::from_secs(60));
        let r1 = limiter.acquire("key").unwrap();
        assert_eq!(r1.remaining, 2);
        let r2 = limiter.acquire("key").unwrap();
        assert_eq!(r2.remaining, 1);
        let r3 = limiter.acquire("key").unwrap();
        assert_eq!(r3.remaining, 0);
    }

    #[test]
    fn test_fixed_window_limiter_reset_at_positive() {
        let limiter = FixedWindowRateLimiter::new(1, Duration::from_secs(60));
        let r = limiter.acquire("key").unwrap();
        assert!(r.reset_at > 0);
    }

    #[test]
    fn test_fixed_window_limiter_try_acquire() {
        let limiter = FixedWindowRateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.try_acquire("key").unwrap().allowed);
        assert!(!limiter.try_acquire("key").unwrap().allowed);
    }

    // ===== 分布式限流测试 =====

    #[test]
    fn test_in_memory_backend_new() {
        let backend = InMemoryBackend::new();
        let result = backend.get("key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_in_memory_backend_incr_and_get() {
        let backend = InMemoryBackend::new();
        let now = now_secs();
        let (count1, reset1) = backend.incr_and_get("key", 60, now, 10).unwrap();
        assert_eq!(count1, 1);
        assert_eq!(reset1, now + 60);

        let (count2, _) = backend.incr_and_get("key", 60, now, 10).unwrap();
        assert_eq!(count2, 2);
    }

    #[test]
    fn test_in_memory_backend_get() {
        let backend = InMemoryBackend::new();
        let now = now_secs();
        backend.incr_and_get("key", 60, now, 10).unwrap();
        let result = backend.get("key").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 1);
    }

    #[test]
    fn test_in_memory_backend_reset_key() {
        let backend = InMemoryBackend::new();
        let now = now_secs();
        backend.incr_and_get("key", 60, now, 10).unwrap();
        assert!(backend.get("key").unwrap().is_some());
        backend.reset_key("key").unwrap();
        assert!(backend.get("key").unwrap().is_none());
    }

    #[test]
    fn test_in_memory_backend_window_expiry() {
        let backend = InMemoryBackend::new();
        let now = now_secs();
        // 第一次请求，窗口开始
        backend.incr_and_get("key", 60, now, 10).unwrap();
        backend.incr_and_get("key", 60, now, 10).unwrap();
        // 窗口过期后（now + 61），应重置
        let (count, _) = backend.incr_and_get("key", 60, now + 61, 10).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_distributed_rate_limiter_allows() {
        let limiter = DistributedRateLimiter::in_memory(5, 60);
        for i in 0..5 {
            let r = limiter.acquire("key").unwrap();
            assert!(r.allowed, "request {} should be allowed", i);
        }
    }

    #[test]
    fn test_distributed_rate_limiter_rejects() {
        let limiter = DistributedRateLimiter::in_memory(2, 60);
        assert!(limiter.acquire("key").unwrap().allowed);
        assert!(limiter.acquire("key").unwrap().allowed);
        assert!(!limiter.acquire("key").unwrap().allowed);
    }

    #[test]
    fn test_distributed_rate_limiter_shared_backend() {
        // 两个限流器共享同一个后端
        let backend = Arc::new(InMemoryBackend::new());
        let limiter1 = DistributedRateLimiter::new(backend.clone(), 2, 60);
        let limiter2 = DistributedRateLimiter::new(backend.clone(), 2, 60);

        // limiter1 消耗 1 个
        assert!(limiter1.acquire("key").unwrap().allowed);
        // limiter2 消耗 1 个（共享计数）
        assert!(limiter2.acquire("key").unwrap().allowed);
        // 第 3 个应被拒绝（共享计数已达 2）
        assert!(!limiter1.acquire("key").unwrap().allowed);
    }

    #[test]
    fn test_distributed_rate_limiter_reset() {
        let limiter = DistributedRateLimiter::in_memory(1, 60);
        limiter.acquire("key").unwrap();
        assert!(!limiter.acquire("key").unwrap().allowed);
        limiter.reset("key").unwrap();
        assert!(limiter.acquire("key").unwrap().allowed);
    }

    #[test]
    fn test_distributed_rate_limiter_different_keys() {
        let limiter = DistributedRateLimiter::in_memory(1, 60);
        assert!(limiter.acquire("key-a").unwrap().allowed);
        assert!(limiter.acquire("key-b").unwrap().allowed);
    }

    #[test]
    fn test_distributed_rate_limiter_remaining() {
        let limiter = DistributedRateLimiter::in_memory(3, 60);
        let r1 = limiter.acquire("key").unwrap();
        assert_eq!(r1.remaining, 2);
        let r2 = limiter.acquire("key").unwrap();
        assert_eq!(r2.remaining, 1);
    }

    // ===== 限流响应策略测试 =====

    #[test]
    fn test_rate_limit_headers_allowed() {
        let result = RateLimitResult::allowed(5, now_secs() * 1000 + 60000);
        let headers = RateLimitHeaders::from_result(&result, 10);
        assert_eq!(headers.limit, 10);
        assert_eq!(headers.remaining, 5);
        assert!(headers.retry_after.is_none());
    }

    #[test]
    fn test_rate_limit_headers_rejected() {
        let result = RateLimitResult::rejected(0, now_secs() * 1000 + 60000);
        let headers = RateLimitHeaders::from_result(&result, 10);
        assert_eq!(headers.limit, 10);
        assert_eq!(headers.remaining, 0);
        assert!(headers.retry_after.is_some());
        assert!(headers.retry_after.unwrap() > 0);
    }

    #[test]
    fn test_rate_limit_headers_to_headers_allowed() {
        let result = RateLimitResult::allowed(5, now_secs() * 1000 + 60000);
        let headers = RateLimitHeaders::from_result(&result, 10);
        let hdrs = headers.to_headers();
        assert_eq!(hdrs.len(), 3); // 不包含 Retry-After
        assert!(hdrs.iter().any(|(k, v)| k == "X-RateLimit-Limit" && v == "10"));
        assert!(hdrs.iter().any(|(k, v)| k == "X-RateLimit-Remaining" && v == "5"));
    }

    #[test]
    fn test_rate_limit_headers_to_headers_rejected() {
        let result = RateLimitResult::rejected(0, now_secs() * 1000 + 60000);
        let headers = RateLimitHeaders::from_result(&result, 10);
        let hdrs = headers.to_headers();
        assert_eq!(hdrs.len(), 4); // 包含 Retry-After
        assert!(hdrs.iter().any(|(k, _)| k == "Retry-After"));
    }

    #[test]
    fn test_rate_limit_headers_to_json() {
        let result = RateLimitResult::allowed(5, now_secs() * 1000 + 60000);
        let headers = RateLimitHeaders::from_result(&result, 10);
        let json = headers.to_json();
        assert_eq!(json["X-RateLimit-Limit"], 10);
        assert_eq!(json["X-RateLimit-Remaining"], 5);
        assert!(json.get("Retry-After").is_none());
    }

    #[test]
    fn test_rate_limit_headers_to_json_rejected() {
        let result = RateLimitResult::rejected(0, now_secs() * 1000 + 60000);
        let headers = RateLimitHeaders::from_result(&result, 10);
        let json = headers.to_json();
        assert!(json.get("Retry-After").is_some());
    }

    #[test]
    fn test_rate_limit_response_strategy_status_codes() {
        assert_eq!(RateLimitResponseStrategy::TooManyRequests.status_code(), 429);
        assert_eq!(
            RateLimitResponseStrategy::ServiceUnavailable.status_code(),
            503
        );
        assert_eq!(RateLimitResponseStrategy::Custom(502).status_code(), 502);
    }

    #[test]
    fn test_rate_limit_response_rejected() {
        let result = RateLimitResult::rejected(0, now_secs() * 1000 + 60000);
        let response = RateLimitResponse::rejected(
            &result,
            10,
            RateLimitResponseStrategy::TooManyRequests,
        );
        assert_eq!(response.status_code, 429);
        assert_eq!(response.body["error"], "rate_limit_exceeded");
        assert!(response.body["retry_after"].as_u64().unwrap() > 0);
        assert!(response.headers.retry_after.is_some());
    }

    #[test]
    fn test_rate_limit_response_allowed() {
        let result = RateLimitResult::allowed(5, now_secs() * 1000 + 60000);
        let response = RateLimitResponse::allowed(&result, 10);
        assert_eq!(response.status_code, 200);
        assert!(response.body.is_null());
        assert!(response.headers.retry_after.is_none());
    }

    #[test]
    fn test_rate_limit_response_custom_strategy() {
        let result = RateLimitResult::rejected(0, now_secs() * 1000 + 60000);
        let response =
            RateLimitResponse::rejected(&result, 10, RateLimitResponseStrategy::Custom(503));
        assert_eq!(response.status_code, 503);
    }

    // ===== 多维度限流策略测试 =====

    #[test]
    fn test_multi_rate_limiter_all_allowed() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let l2 = Arc::new(TokenBucketRateLimiter::new(10, 1.0));
        let multi = MultiRateLimiter::new(vec![l1, l2]);

        let result = multi.check_all("key").unwrap();
        assert!(result.allowed);
    }

    #[test]
    fn test_multi_rate_limiter_one_rejects() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let l2 = Arc::new(TokenBucketRateLimiter::new(1, 1.0));
        let multi = MultiRateLimiter::new(vec![l1, l2]);

        // 第一次通过
        assert!(multi.check_all("key").unwrap().allowed);
        // 第二次：l2 拒绝
        let result = multi.check_all("key").unwrap();
        assert!(!result.allowed);
    }

    #[test]
    fn test_multi_rate_limiter_takes_strictest() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(5, Duration::from_secs(60)));
        let l2 = Arc::new(SlidingWindowRateLimiter::new(2, Duration::from_secs(60)));
        let multi = MultiRateLimiter::new(vec![l1, l2]);

        // 两次请求后 l2 达到上限
        multi.check_all("key").unwrap();
        multi.check_all("key").unwrap();
        // 第三次：l2 拒绝
        assert!(!multi.check_all("key").unwrap().allowed);
    }

    #[test]
    fn test_multi_rate_limiter_with_limiter() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let multi = MultiRateLimiter::new(vec![]).with_limiter(l1);
        assert!(multi.check_all("key").unwrap().allowed);
    }

    #[test]
    fn test_multi_rate_limiter_empty_errors() {
        let multi = MultiRateLimiter::new(vec![]);
        let result = multi.check_all("key");
        assert!(result.is_err());
    }
}
