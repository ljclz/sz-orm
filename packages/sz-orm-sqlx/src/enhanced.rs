//! 增强功能模块：事务隔离级别、连接池配置增强、预备语句缓存
//!
//! 本模块为 sz-orm-sqlx 提供三项深度增强能力：
//!
//! 1. **事务隔离级别**：支持四种标准隔离级别的设置与查询
//! 2. **连接池配置增强**：提供更丰富的连接池配置选项与构建器模式
//! 3. **预备语句缓存**：LRU 策略的预备语句缓存，减少 SQL 解析开销

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::any_driver::AnyBackend;

// ============================================================================
// 事务隔离级别
// ============================================================================

/// SQL 事务隔离级别。
///
/// 对应 SQL 标准的四种隔离级别，不同后端的 SQL 语法略有差异。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransactionIsolation {
    /// 读未提交（最低隔离级别，允许脏读）
    ReadUncommitted,
    /// 读已提交（禁止脏读，允许不可重复读）
    #[default]
    ReadCommitted,
    /// 可重复读（禁止不可重复读，允许幻读）
    RepeatableRead,
    /// 串行化（最高隔离级别，完全隔离）
    Serializable,
}

impl TransactionIsolation {
    /// 返回隔离级别的标准名称。
    pub fn name(&self) -> &'static str {
        match self {
            TransactionIsolation::ReadUncommitted => "READ UNCOMMITTED",
            TransactionIsolation::ReadCommitted => "READ COMMITTED",
            TransactionIsolation::RepeatableRead => "REPEATABLE READ",
            TransactionIsolation::Serializable => "SERIALIZABLE",
        }
    }

    /// 返回隔离级别的中文描述。
    pub fn description(&self) -> &'static str {
        match self {
            TransactionIsolation::ReadUncommitted => "读未提交",
            TransactionIsolation::ReadCommitted => "读已提交",
            TransactionIsolation::RepeatableRead => "可重复读",
            TransactionIsolation::Serializable => "串行化",
        }
    }

    /// 返回隔离级别的严格程度排序值（0=最低，3=最高）。
    pub fn strictness(&self) -> u8 {
        match self {
            TransactionIsolation::ReadUncommitted => 0,
            TransactionIsolation::ReadCommitted => 1,
            TransactionIsolation::RepeatableRead => 2,
            TransactionIsolation::Serializable => 3,
        }
    }

    /// 生成设置当前会话隔离级别的 SQL 语句。
    ///
    /// 不同后端语法略有差异：
    /// - MySQL: `SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED`
    /// - PostgreSQL: `SET SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL READ COMMITTED`
    /// - SQLite: 不支持（SQLite 始终使用 SERIALIZABLE），返回空字符串
    pub fn set_session_sql(&self, backend: AnyBackend) -> String {
        match backend {
            AnyBackend::MySql => {
                format!("SET SESSION TRANSACTION ISOLATION LEVEL {}", self.name())
            }
            AnyBackend::Postgres => {
                format!(
                    "SET SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL {}",
                    self.name()
                )
            }
            AnyBackend::Sqlite => {
                // SQLite 隐式使用 SERIALIZABLE，不支持设置
                String::new()
            }
        }
    }

    /// 生成设置下一事务隔离级别的 SQL 语句。
    ///
    /// - MySQL: `SET TRANSACTION ISOLATION LEVEL READ COMMITTED`
    /// - PostgreSQL: `SET TRANSACTION ISOLATION LEVEL READ COMMITTED`
    /// - SQLite: 不支持，返回空字符串
    pub fn set_transaction_sql(&self, backend: AnyBackend) -> String {
        match backend {
            AnyBackend::MySql => format!("SET TRANSACTION ISOLATION LEVEL {}", self.name()),
            AnyBackend::Postgres => format!("SET TRANSACTION ISOLATION LEVEL {}", self.name()),
            AnyBackend::Sqlite => String::new(),
        }
    }

    /// 生成查询当前隔离级别的 SQL 语句。
    ///
    /// - MySQL: `SELECT @@transaction_isolation`
    /// - PostgreSQL: `SHOW transaction_isolation`
    /// - SQLite: 不支持，返回空字符串
    pub fn query_sql(&self, backend: AnyBackend) -> String {
        match backend {
            AnyBackend::MySql => "SELECT @@transaction_isolation".to_string(),
            AnyBackend::Postgres => "SHOW transaction_isolation".to_string(),
            AnyBackend::Sqlite => String::new(),
        }
    }

    /// 从字符串解析隔离级别（不区分大小写）。
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        let upper = s.to_uppercase().replace('_', " ");
        match upper.as_str() {
            "READ UNCOMMITTED" => Some(TransactionIsolation::ReadUncommitted),
            "READ COMMITTED" => Some(TransactionIsolation::ReadCommitted),
            "REPEATABLE READ" => Some(TransactionIsolation::RepeatableRead),
            "SERIALIZABLE" => Some(TransactionIsolation::Serializable),
            _ => None,
        }
    }
}

impl std::fmt::Display for TransactionIsolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ============================================================================
// 连接池配置增强
// ============================================================================

/// 增强的连接池配置。
///
/// 提供 比 sqlx::PoolOptions 更丰富的配置项，包括健康检查、测试查询等。
#[derive(Debug, Clone)]
pub struct EnhancedPoolConfig {
    /// 最大连接数（默认 10）
    pub max_connections: u32,
    /// 最小空闲连接数（默认 0）
    pub min_idle: Option<u32>,
    /// 获取连接超时时间（默认 30 秒）
    pub acquire_timeout: Duration,
    /// 空闲连接超时时间（默认 600 秒）
    pub idle_timeout: Option<Duration>,
    /// 连接最大生存时间（默认 1800 秒）
    pub max_lifetime: Option<Duration>,
    /// 获取连接时是否执行测试查询（默认 false）
    pub test_on_acquire: bool,
    /// 测试查询 SQL（默认 "SELECT 1"）
    pub test_query: String,
    /// 连接池名称（用于日志和监控）
    pub pool_name: Option<String>,
}

impl Default for EnhancedPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_idle: None,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(600)),
            max_lifetime: Some(Duration::from_secs(1800)),
            test_on_acquire: false,
            test_query: "SELECT 1".to_string(),
            pool_name: None,
        }
    }
}

impl EnhancedPoolConfig {
    /// 创建新的配置构建器。
    pub fn builder() -> EnhancedPoolConfigBuilder {
        EnhancedPoolConfigBuilder::default()
    }

    /// 校验配置合法性。
    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections == 0 {
            return Err("max_connections 不能为 0".to_string());
        }
        if let Some(min) = self.min_idle {
            if min > self.max_connections {
                return Err(format!(
                    "min_idle ({}) 不能大于 max_connections ({})",
                    min, self.max_connections
                ));
            }
        }
        if self.acquire_timeout.is_zero() {
            return Err("acquire_timeout 不能为 0".to_string());
        }
        if self.test_query.is_empty() {
            return Err("test_query 不能为空".to_string());
        }
        Ok(())
    }

    /// 返回配置摘要信息。
    pub fn summary(&self) -> String {
        format!(
            "PoolConfig{{max={}, min_idle={:?}, timeout={}ms, test_on_acquire={}, name={:?}}}",
            self.max_connections,
            self.min_idle,
            self.acquire_timeout.as_millis(),
            self.test_on_acquire,
            self.pool_name
        )
    }
}

/// 增强连接池配置构建器。
#[derive(Debug, Clone, Default)]
pub struct EnhancedPoolConfigBuilder {
    config: EnhancedPoolConfig,
}

impl EnhancedPoolConfigBuilder {
    /// 设置最大连接数。
    pub fn max_connections(mut self, n: u32) -> Self {
        self.config.max_connections = n;
        self
    }

    /// 设置最小空闲连接数。
    pub fn min_idle(mut self, n: u32) -> Self {
        self.config.min_idle = Some(n);
        self
    }

    /// 设置获取连接超时时间（秒）。
    pub fn acquire_timeout_secs(mut self, secs: u64) -> Self {
        self.config.acquire_timeout = Duration::from_secs(secs);
        self
    }

    /// 设置获取连接超时时间（毫秒）。
    pub fn acquire_timeout_millis(mut self, millis: u64) -> Self {
        self.config.acquire_timeout = Duration::from_millis(millis);
        self
    }

    /// 设置空闲连接超时时间（秒）。
    pub fn idle_timeout_secs(mut self, secs: u64) -> Self {
        self.config.idle_timeout = Some(Duration::from_secs(secs));
        self
    }

    /// 设置连接最大生存时间（秒）。
    pub fn max_lifetime_secs(mut self, secs: u64) -> Self {
        self.config.max_lifetime = Some(Duration::from_secs(secs));
        self
    }

    /// 启用获取连接时的测试查询。
    pub fn test_on_acquire(mut self) -> Self {
        self.config.test_on_acquire = true;
        self
    }

    /// 设置测试查询 SQL。
    pub fn test_query(mut self, sql: &str) -> Self {
        self.config.test_query = sql.to_string();
        self
    }

    /// 设置连接池名称。
    pub fn name(mut self, name: &str) -> Self {
        self.config.pool_name = Some(name.to_string());
        self
    }

    /// 构建配置，执行校验。
    pub fn build(self) -> Result<EnhancedPoolConfig, String> {
        self.config.validate()?;
        Ok(self.config)
    }
}

// ============================================================================
// 预备语句缓存
// ============================================================================

/// 预备语句缓存条目。
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CacheEntry {
    /// 预备语句 ID 或名称
    statement_id: String,
    /// 创建时的访问序号
    created_seq: u64,
    /// 最后访问序号（用于 LRU 排序，单调递增）
    last_access_seq: u64,
    /// 命中次数
    hit_count: u64,
}

/// 预备语句缓存统计信息。
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// 缓存命中次数
    pub hits: u64,
    /// 缓存未命中次数
    pub misses: u64,
    /// 缓存驱逐次数
    pub evictions: u64,
    /// 当前缓存条目数
    pub size: usize,
    /// 最大缓存容量
    pub capacity: usize,
}

impl CacheStats {
    /// 计算缓存命中率（0.0 ~ 1.0）。
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }

    /// 返回统计信息摘要字符串。
    pub fn summary(&self) -> String {
        format!(
            "CacheStats{{hits={}, misses={}, evictions={}, size={}, capacity={}, hit_rate={:.2}%, capacity_utilization={:.2}%}}",
            self.hits,
            self.misses,
            self.evictions,
            self.size,
            self.capacity,
            self.hit_rate() * 100.0,
            self.capacity_utilization() * 100.0,
        )
    }

    /// 计算容量利用率（0.0 ~ 1.0）。
    pub fn capacity_utilization(&self) -> f64 {
        if self.capacity == 0 {
            return 0.0;
        }
        self.size as f64 / self.capacity as f64
    }

    /// 计算总访问次数。
    pub fn total_accesses(&self) -> u64 {
        self.hits + self.misses
    }
}

/// 预备语句缓存（LRU 策略）。
///
/// 缓存 SQL 语句到预备语句 ID 的映射，避免重复解析和编译 SQL。
/// 使用 LRU（Least Recently Used）策略在容量满时驱逐最久未使用的条目。
///
/// # 线程安全
///
/// 内部使用 `Mutex` 保护，可安全跨线程共享。
/// LRU 排序基于单调递增的原子计数器，不受系统时钟精度影响。
pub struct PreparedStatementCache {
    /// 缓存映射：SQL 哈希 → 缓存条目
    entries: Mutex<HashMap<u64, CacheEntry>>,
    /// 最大缓存容量
    capacity: usize,
    /// 统计信息
    stats: Mutex<CacheStats>,
    /// 单调递增的访问序号（用于 LRU 排序，避免时钟精度问题）
    access_seq: AtomicU64,
}

impl PreparedStatementCache {
    /// 创建新的预备语句缓存。
    ///
    /// # 参数
    ///
    /// - `capacity`: 最大缓存条目数（建议 100-1000）
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            entries: Mutex::new(HashMap::with_capacity(capacity)),
            capacity,
            stats: Mutex::new(CacheStats {
                hits: 0,
                misses: 0,
                evictions: 0,
                size: 0,
                capacity,
            }),
            access_seq: AtomicU64::new(0),
        }
    }

    /// 计算 SQL 语句的哈希值（使用 FNV-1a 算法，无需额外依赖）。
    fn hash_sql(sql: &str) -> u64 {
        // FNV-1a 64-bit
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET;
        for byte in sql.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    /// 获取下一个单调递增的访问序号。
    fn next_seq(&self) -> u64 {
        self.access_seq.fetch_add(1, Ordering::Relaxed)
    }

    /// 查询缓存中是否存在指定 SQL 的预备语句。
    ///
    /// 如果命中，更新最后访问序号并增加命中计数。
    pub fn get(&self, sql: &str) -> Option<String> {
        let hash = Self::hash_sql(sql);
        let seq = self.next_seq();

        let mut entries = self.entries.lock().ok()?;
        let mut stats = self.stats.lock().ok()?;

        if let Some(entry) = entries.get_mut(&hash) {
            entry.last_access_seq = seq;
            entry.hit_count += 1;
            stats.hits += 1;
            Some(entry.statement_id.clone())
        } else {
            stats.misses += 1;
            None
        }
    }

    /// 向缓存中插入预备语句。
    ///
    /// 如果缓存已满，驱逐最久未使用的条目（LRU）。
    pub fn put(&self, sql: &str, statement_id: &str) {
        let hash = Self::hash_sql(sql);
        let seq = self.next_seq();

        let mut entries = self.entries.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();

        // 如果已存在，更新
        if let Some(entry) = entries.get_mut(&hash) {
            entry.statement_id = statement_id.to_string();
            entry.last_access_seq = seq;
            return;
        }

        // 检查是否需要 LRU 驱逐
        if entries.len() >= self.capacity {
            // 找到 last_access_seq 最小的条目（最久未使用）
            if let Some(&evict_hash) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_access_seq)
                .map(|(k, _)| k)
            {
                entries.remove(&evict_hash);
                stats.evictions += 1;
            }
        }

        entries.insert(
            hash,
            CacheEntry {
                statement_id: statement_id.to_string(),
                created_seq: seq,
                last_access_seq: seq,
                hit_count: 0,
            },
        );
        stats.size = entries.len();
    }

    /// 从缓存中移除指定 SQL 的预备语句。
    pub fn remove(&self, sql: &str) -> bool {
        let hash = Self::hash_sql(sql);
        let mut entries = self.entries.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();
        let removed = entries.remove(&hash).is_some();
        if removed {
            stats.size = entries.len();
        }
        removed
    }

    /// 清空缓存。
    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();
        entries.clear();
        stats.size = 0;
    }

    /// 获取缓存统计信息。
    pub fn stats(&self) -> CacheStats {
        let stats = self.stats.lock().unwrap();
        stats.clone()
    }

    /// 获取缓存容量。
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 获取当前缓存条目数。
    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// 缓存是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 重置统计信息（不清空缓存条目）。
    pub fn reset_stats(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.hits = 0;
        stats.misses = 0;
        stats.evictions = 0;
    }
}

impl Default for PreparedStatementCache {
    fn default() -> Self {
        Self::new(256)
    }
}

impl std::fmt::Debug for PreparedStatementCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stats = self.stats();
        f.debug_struct("PreparedStatementCache")
            .field("capacity", &self.capacity)
            .field("size", &stats.size)
            .field("hits", &stats.hits)
            .field("misses", &stats.misses)
            .field("evictions", &stats.evictions)
            .finish()
    }
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- TransactionIsolation 测试 ----

    #[test]
    fn test_isolation_level_names() {
        assert_eq!(TransactionIsolation::ReadUncommitted.name(), "READ UNCOMMITTED");
        assert_eq!(TransactionIsolation::ReadCommitted.name(), "READ COMMITTED");
        assert_eq!(TransactionIsolation::RepeatableRead.name(), "REPEATABLE READ");
        assert_eq!(TransactionIsolation::Serializable.name(), "SERIALIZABLE");
    }

    #[test]
    fn test_isolation_level_descriptions() {
        assert_eq!(TransactionIsolation::ReadUncommitted.description(), "读未提交");
        assert_eq!(TransactionIsolation::ReadCommitted.description(), "读已提交");
        assert_eq!(TransactionIsolation::RepeatableRead.description(), "可重复读");
        assert_eq!(TransactionIsolation::Serializable.description(), "串行化");
    }

    #[test]
    fn test_isolation_level_strictness_order() {
        assert!(TransactionIsolation::ReadUncommitted.strictness() < TransactionIsolation::ReadCommitted.strictness());
        assert!(TransactionIsolation::ReadCommitted.strictness() < TransactionIsolation::RepeatableRead.strictness());
        assert!(TransactionIsolation::RepeatableRead.strictness() < TransactionIsolation::Serializable.strictness());
    }

    #[test]
    fn test_isolation_level_from_str() {
        assert_eq!(
            TransactionIsolation::from_str("READ COMMITTED"),
            Some(TransactionIsolation::ReadCommitted)
        );
        assert_eq!(
            TransactionIsolation::from_str("read committed"),
            Some(TransactionIsolation::ReadCommitted)
        );
        assert_eq!(
            TransactionIsolation::from_str("READ_COMMITTED"),
            Some(TransactionIsolation::ReadCommitted)
        );
        assert_eq!(
            TransactionIsolation::from_str("SERIALIZABLE"),
            Some(TransactionIsolation::Serializable)
        );
        assert_eq!(TransactionIsolation::from_str("UNKNOWN"), None);
    }

    #[test]
    fn test_isolation_level_set_session_sql_mysql() {
        let sql = TransactionIsolation::ReadCommitted.set_session_sql(AnyBackend::MySql);
        assert!(sql.contains("SET SESSION TRANSACTION ISOLATION LEVEL"));
        assert!(sql.contains("READ COMMITTED"));
    }

    #[test]
    fn test_isolation_level_set_session_sql_postgres() {
        let sql = TransactionIsolation::Serializable.set_session_sql(AnyBackend::Postgres);
        assert!(sql.contains("SET SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL"));
        assert!(sql.contains("SERIALIZABLE"));
    }

    #[test]
    fn test_isolation_level_set_session_sql_sqlite_empty() {
        let sql = TransactionIsolation::ReadCommitted.set_session_sql(AnyBackend::Sqlite);
        assert!(sql.is_empty(), "SQLite 不支持设置隔离级别");
    }

    #[test]
    fn test_isolation_level_set_transaction_sql() {
        let mysql_sql = TransactionIsolation::RepeatableRead.set_transaction_sql(AnyBackend::MySql);
        assert!(mysql_sql.contains("SET TRANSACTION ISOLATION LEVEL"));
        assert!(mysql_sql.contains("REPEATABLE READ"));

        let pg_sql = TransactionIsolation::RepeatableRead.set_transaction_sql(AnyBackend::Postgres);
        assert!(pg_sql.contains("SET TRANSACTION ISOLATION LEVEL"));

        let sqlite_sql = TransactionIsolation::RepeatableRead.set_transaction_sql(AnyBackend::Sqlite);
        assert!(sqlite_sql.is_empty());
    }

    #[test]
    fn test_isolation_level_query_sql() {
        assert_eq!(
            TransactionIsolation::ReadCommitted.query_sql(AnyBackend::MySql),
            "SELECT @@transaction_isolation"
        );
        assert_eq!(
            TransactionIsolation::ReadCommitted.query_sql(AnyBackend::Postgres),
            "SHOW transaction_isolation"
        );
        assert_eq!(
            TransactionIsolation::ReadCommitted.query_sql(AnyBackend::Sqlite),
            ""
        );
    }

    #[test]
    fn test_isolation_level_display() {
        let level = TransactionIsolation::Serializable;
        let s = format!("{}", level);
        assert_eq!(s, "SERIALIZABLE");
    }

    #[test]
    fn test_isolation_level_default() {
        let level = TransactionIsolation::default();
        assert_eq!(level, TransactionIsolation::ReadCommitted);
    }

    #[test]
    fn test_isolation_level_equality() {
        assert_eq!(TransactionIsolation::ReadCommitted, TransactionIsolation::ReadCommitted);
        assert_ne!(TransactionIsolation::ReadCommitted, TransactionIsolation::Serializable);
    }

    // ---- EnhancedPoolConfig 测试 ----

    #[test]
    fn test_pool_config_default() {
        let config = EnhancedPoolConfig::default();
        assert_eq!(config.max_connections, 10);
        assert!(config.min_idle.is_none());
        assert_eq!(config.acquire_timeout, Duration::from_secs(30));
        assert_eq!(config.test_query, "SELECT 1");
        assert!(!config.test_on_acquire);
    }

    #[test]
    fn test_pool_config_builder_basic() {
        let config = EnhancedPoolConfig::builder()
            .max_connections(20)
            .min_idle(5)
            .acquire_timeout_secs(60)
            .build()
            .unwrap();
        assert_eq!(config.max_connections, 20);
        assert_eq!(config.min_idle, Some(5));
        assert_eq!(config.acquire_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_pool_config_builder_test_on_acquire() {
        let config = EnhancedPoolConfig::builder()
            .test_on_acquire()
            .test_query("SELECT 1 FROM dual")
            .build()
            .unwrap();
        assert!(config.test_on_acquire);
        assert_eq!(config.test_query, "SELECT 1 FROM dual");
    }

    #[test]
    fn test_pool_config_builder_with_name() {
        let config = EnhancedPoolConfig::builder()
            .name("primary-pool")
            .build()
            .unwrap();
        assert_eq!(config.pool_name, Some("primary-pool".to_string()));
    }

    #[test]
    fn test_pool_config_validate_max_connections_zero() {
        let config = EnhancedPoolConfig {
            max_connections: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_pool_config_validate_min_idle_exceeds_max() {
        let config = EnhancedPoolConfig {
            max_connections: 5,
            min_idle: Some(10),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_pool_config_validate_timeout_zero() {
        let config = EnhancedPoolConfig {
            acquire_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_pool_config_validate_empty_test_query() {
        let config = EnhancedPoolConfig {
            test_query: "".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_pool_config_validate_valid() {
        let config = EnhancedPoolConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pool_config_summary() {
        let config = EnhancedPoolConfig::builder()
            .max_connections(15)
            .name("test-pool")
            .build()
            .unwrap();
        let summary = config.summary();
        assert!(summary.contains("max=15"));
        assert!(summary.contains("name=Some(\"test-pool\")"));
    }

    #[test]
    fn test_pool_config_builder_millis_timeout() {
        let config = EnhancedPoolConfig::builder()
            .acquire_timeout_millis(500)
            .build()
            .unwrap();
        assert_eq!(config.acquire_timeout, Duration::from_millis(500));
    }

    #[test]
    fn test_pool_config_builder_idle_and_lifetime() {
        let config = EnhancedPoolConfig::builder()
            .idle_timeout_secs(300)
            .max_lifetime_secs(900)
            .build()
            .unwrap();
        assert_eq!(config.idle_timeout, Some(Duration::from_secs(300)));
        assert_eq!(config.max_lifetime, Some(Duration::from_secs(900)));
    }

    // ---- PreparedStatementCache 测试 ----

    #[test]
    fn test_cache_basic_put_and_get() {
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT * FROM users WHERE id = ?", "stmt_1");

        let result = cache.get("SELECT * FROM users WHERE id = ?");
        assert_eq!(result, Some("stmt_1".to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let cache = PreparedStatementCache::new(10);
        let result = cache.get("SELECT * FROM nonexist");
        assert!(result.is_none());

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
    }

    #[test]
    fn test_cache_hit_increments_counter() {
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT 1", "stmt_1");

        cache.get("SELECT 1");
        cache.get("SELECT 1");
        cache.get("SELECT 1");

        let stats = cache.stats();
        assert_eq!(stats.hits, 3);
    }

    #[test]
    fn test_cache_remove() {
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT 1", "stmt_1");
        assert!(cache.remove("SELECT 1"));
        assert!(cache.get("SELECT 1").is_none());
    }

    #[test]
    fn test_cache_remove_nonexistent() {
        let cache = PreparedStatementCache::new(10);
        assert!(!cache.remove("SELECT 1"));
    }

    #[test]
    fn test_cache_clear() {
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT 1", "stmt_1");
        cache.put("SELECT 2", "stmt_2");
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = PreparedStatementCache::new(2);
        cache.put("sql_1", "stmt_1");
        cache.put("sql_2", "stmt_2");

        // 访问 sql_1 使其成为最近使用
        cache.get("sql_1");

        // 插入 sql_3，应驱逐最久未使用的 sql_2
        cache.put("sql_3", "stmt_3");

        assert!(cache.get("sql_1").is_some(), "sql_1 应被保留（最近使用）");
        assert!(cache.get("sql_2").is_none(), "sql_2 应被 LRU 驱逐");
        assert!(cache.get("sql_3").is_some(), "sql_3 应存在");

        let stats = cache.stats();
        assert!(stats.evictions >= 1, "应至少有 1 次驱逐");
    }

    #[test]
    fn test_cache_update_existing() {
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT 1", "stmt_1");
        cache.put("SELECT 1", "stmt_2"); // 更新

        let result = cache.get("SELECT 1");
        assert_eq!(result, Some("stmt_2".to_string()));

        // 更新不应增加条目数
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT 1", "stmt_1");

        // 3 次命中
        cache.get("SELECT 1");
        cache.get("SELECT 1");
        cache.get("SELECT 1");
        // 2 次未命中
        cache.get("SELECT 2");
        cache.get("SELECT 3");

        let stats = cache.stats();
        assert_eq!(stats.hits, 3);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.total_accesses(), 5);
        let expected_rate = 3.0 / 5.0;
        assert!((stats.hit_rate() - expected_rate).abs() < 0.001);
    }

    #[test]
    fn test_cache_stats_summary() {
        let cache = PreparedStatementCache::new(100);
        cache.put("SELECT 1", "stmt_1");
        cache.get("SELECT 1");

        let stats = cache.stats();
        let summary = stats.summary();
        assert!(summary.contains("hits=1"));
        assert!(summary.contains("capacity=100"));
        assert!(summary.contains("hit_rate="));
    }

    #[test]
    fn test_cache_capacity_utilization() {
        let cache = PreparedStatementCache::new(10);
        cache.put("sql_1", "stmt_1");
        cache.put("sql_2", "stmt_2");

        let stats = cache.stats();
        assert_eq!(stats.size, 2);
        assert_eq!(stats.capacity, 10);
        assert!((stats.capacity_utilization() - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_cache_reset_stats() {
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT 1", "stmt_1");
        cache.get("SELECT 1");
        cache.get("SELECT 2");

        cache.reset_stats();
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evictions, 0);
        // 条目不清空
        assert_eq!(stats.size, 1);
    }

    #[test]
    fn test_cache_default_capacity() {
        let cache = PreparedStatementCache::default();
        assert_eq!(cache.capacity(), 256);
    }

    #[test]
    fn test_cache_min_capacity_1() {
        let cache = PreparedStatementCache::new(0);
        assert_eq!(cache.capacity(), 1, "容量为 0 时应自动设为 1");
    }

    #[test]
    fn test_cache_debug_format() {
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT 1", "stmt_1");
        let debug_str = format!("{:?}", cache);
        assert!(debug_str.contains("PreparedStatementCache"));
        // debug_struct 使用 `: ` 作为键值分隔符
        assert!(debug_str.contains("capacity: 10"));
        assert!(debug_str.contains("size: 1"));
    }

    #[test]
    fn test_cache_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(PreparedStatementCache::new(100));
        let mut handles = Vec::new();

        for i in 0..4 {
            let c = cache.clone();
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    let sql = format!("SELECT {}", i * 10 + j);
                    c.put(&sql, &format!("stmt_{}", i * 10 + j));
                    c.get(&sql);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // 所有线程完成后，缓存应包含 40 个条目
        assert_eq!(cache.len(), 40);
        let stats = cache.stats();
        assert!(stats.hits >= 40, "每个 put 后立即 get 应产生 40 次命中");
    }

    #[test]
    fn test_cache_same_sql_different_whitespace_same_hash() {
        // 注意：当前实现基于字节级哈希，不同空格的 SQL 会被视为不同条目
        // 此测试验证行为符合预期（字节级哈希）
        let cache = PreparedStatementCache::new(10);
        cache.put("SELECT 1", "stmt_1");
        cache.put("SELECT  1", "stmt_2"); // 两个空格
        assert_eq!(cache.len(), 2, "不同空格的 SQL 应为不同条目");
    }
}
