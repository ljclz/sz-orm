//! v3.2.0 查询计划缓存
//!
//! 缓存 SQL 解析结果（AST）与查询优化结果，相同 SQL 模板第二次起跳过解析/优化（≤1μs）。
//! 与 L2Cache（数据缓存）职责分离：L2Cache 缓存查询结果数据，PlanCache 缓存查询计划。
//!
//! 特性：
//! - SQL 归一化（忽略空白/注释/参数顺序，参数值替换为占位符）
//! - xxHash 64bit 键生成（无碰撞差分测试验证）
//! - 双缓存（parse_cache + optimize_cache）
//! - LRU 淘汰（arena 双向链表 O(1)）
//! - 表级精确失效（table_index 索引）
//! - 命中率统计（原子计数器无锁）

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::hash::Hasher;
use twox_hash::XxHash64;

// ─── SqlNormalizer ───────────────────────────────────────────────

/// SQL 归一化器
///
/// 将 SQL 文本归一化为标准形式：忽略大小写差异、空白差异、参数值差异。
/// 相同语义不同写法的 SQL 归一化后产生相同文本，用于缓存键生成。
pub struct SqlNormalizer;

impl SqlNormalizer {
    /// 归一化 SQL 文本
    ///
    /// 返回归一化后的 SQL 文本。参数值（如 WHERE id = 42 中的 42）
    /// 不会被特殊处理——调用方应使用参数化查询（WHERE id = ?），
    /// 归一化仅处理大小写和空白差异。
    pub fn normalize(sql: &str) -> String {
        let dialect = GenericDialect {};
        match Parser::parse_sql(&dialect, sql) {
            Ok(statements) => {
                if statements.is_empty() {
                    return sql.trim().to_lowercase();
                }
                let normalized: Vec<String> =
                    statements.iter().map(|stmt| stmt.to_string()).collect();
                normalized.join("; ")
            }
            Err(_) => {
                let trimmed: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
                trimmed.to_lowercase()
            }
        }
    }

    /// 从 SQL 中提取依赖的表名列表
    ///
    /// 遍历 AST 提取所有表引用（FROM / JOIN / INTO / UPDATE 等）。
    pub fn extract_tables(sql: &str) -> Vec<String> {
        let dialect = GenericDialect {};
        let mut tables = Vec::new();

        if let Ok(statements) = Parser::parse_sql(&dialect, sql) {
            for stmt in &statements {
                Self::extract_tables_from_stmt(stmt, &mut tables);
            }
        }

        if tables.is_empty() {
            Self::extract_tables_from_str(sql, &mut tables);
        }

        tables.sort();
        tables.dedup();
        tables
    }

    fn extract_tables_from_stmt(stmt: &Statement, tables: &mut Vec<String>) {
        use sqlparser::ast::SetExpr;

        match stmt {
            Statement::Query(query) => {
                if let SetExpr::Select(select) = &*query.body {
                    for from in &select.from {
                        Self::extract_table_factor(&from.relation, tables);
                        for join in &from.joins {
                            Self::extract_table_factor(&join.relation, tables);
                        }
                    }
                }
            }
            Statement::Insert(_) => {}
            Statement::Update { table, .. } => {
                Self::extract_table_factor(&table.relation, tables);
            }
            Statement::Delete(delete) => {
                for table_name in &delete.tables {
                    let full_name = table_name
                        .0
                        .iter()
                        .map(|i| i.value.clone())
                        .collect::<Vec<_>>()
                        .join(".");
                    tables.push(full_name);
                }
            }
            _ => {}
        }
    }

    fn extract_table_factor(factor: &sqlparser::ast::TableFactor, tables: &mut Vec<String>) {
        use sqlparser::ast::TableFactor;
        if let TableFactor::Table { name, .. } = factor {
            let full_name = name
                .0
                .iter()
                .map(|i| i.value.clone())
                .collect::<Vec<_>>()
                .join(".");
            tables.push(full_name);
        }
    }

    fn extract_tables_from_str(sql: &str, tables: &mut Vec<String>) {
        let lower = sql.to_lowercase();
        for keyword in ["into ", "from ", "update ", "join "] {
            let mut search_pos = 0;
            while let Some(pos) = lower[search_pos..].find(keyword) {
                let abs_pos = search_pos + pos;
                let rest = &sql[abs_pos + keyword.len()..];
                let table: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                    .collect();
                if !table.is_empty() && !table.chars().all(|c| c.is_numeric()) {
                    tables.push(table);
                }
                search_pos = abs_pos + keyword.len();
            }
        }
    }
}

// ─── PlanCacheKey ────────────────────────────────────────────────

/// 查询计划缓存键
///
/// 由归一化 SQL 的 xxHash 64bit 哈希值 + 归一化 SQL 文本组成。
/// 哈希值用于快速查找，SQL 文本用于二次校验（防哈希碰撞）。
#[derive(Debug, Clone)]
pub struct PlanCacheKey {
    /// xxHash 64bit 哈希值
    pub hash: u64,
    /// 归一化 SQL 文本（用于碰撞校验）
    pub sql_normalized: String,
}

impl PlanCacheKey {
    /// 从原始 SQL 生成缓存键
    ///
    /// 1. 归一化 SQL（忽略大小写/空白差异）
    /// 2. 计算归一化 SQL 的 xxHash 64bit 哈希
    pub fn from_sql(sql: &str) -> Self {
        let sql_normalized = SqlNormalizer::normalize(sql);
        let mut h = XxHash64::with_seed(0);
        h.write(sql_normalized.as_bytes());
        let hash = h.finish();
        Self {
            hash,
            sql_normalized,
        }
    }
}

impl PartialEq for PlanCacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.sql_normalized == other.sql_normalized
    }
}

impl Eq for PlanCacheKey {}

impl std::hash::Hash for PlanCacheKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

// ─── PlanCacheEntry ──────────────────────────────────────────────

/// 查询计划缓存条目
///
/// 存储解析后的 AST 和/或优化后的查询分析结果。
pub struct PlanCacheEntry {
    /// 解析后的 AST（parse_cache 条目）
    pub ast: Option<Arc<Statement>>,
    /// 优化后的查询分析（optimize_cache 条目）
    pub analysis: Option<Arc<String>>,
    /// 创建时间
    pub created_at: Instant,
    /// 依赖的表名列表（用于表级失效）
    pub tables: Vec<String>,
    /// TTL（可选，过期自动失效）
    pub ttl: Option<Duration>,
}

impl PlanCacheEntry {
    /// 检查条目是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            self.created_at.elapsed() >= ttl
        } else {
            false
        }
    }

    /// 估算计划大小（字节）
    ///
    /// AST 和 analysis 的近似内存占用。
    pub fn sql_normalized_len(&self) -> usize {
        let ast_len = self.ast.as_ref().map(|a| a.to_string().len()).unwrap_or(0);
        let analysis_len = self
            .analysis
            .as_ref()
            .map(|a| a.as_str().len())
            .unwrap_or(0);
        ast_len + analysis_len + self.tables.iter().map(|t| t.len()).sum::<usize>()
    }
}

// ─── PlanCacheStats ──────────────────────────────────────────────

/// 查询计划缓存统计
///
/// 原子计数器无锁统计命中/未命中/淘汰次数。
pub struct PlanCacheStats {
    parse_hits: AtomicU64,
    parse_misses: AtomicU64,
    optimize_hits: AtomicU64,
    optimize_misses: AtomicU64,
    evictions: AtomicU64,
}

impl PlanCacheStats {
    fn new() -> Self {
        Self {
            parse_hits: AtomicU64::new(0),
            parse_misses: AtomicU64::new(0),
            optimize_hits: AtomicU64::new(0),
            optimize_misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// 解析缓存命中次数
    pub fn parse_hits(&self) -> u64 {
        self.parse_hits.load(Ordering::Relaxed)
    }

    /// 解析缓存未命中次数
    pub fn parse_misses(&self) -> u64 {
        self.parse_misses.load(Ordering::Relaxed)
    }

    /// 优化缓存命中次数
    pub fn optimize_hits(&self) -> u64 {
        self.optimize_hits.load(Ordering::Relaxed)
    }

    /// 优化缓存未命中次数
    pub fn optimize_misses(&self) -> u64 {
        self.optimize_misses.load(Ordering::Relaxed)
    }

    /// LRU 淘汰次数
    pub fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }

    /// 解析缓存命中率（0.0 ~ 1.0）
    pub fn parse_hit_rate(&self) -> f64 {
        let hits = self.parse_hits();
        let misses = self.parse_misses();
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// 优化缓存命中率（0.0 ~ 1.0）
    pub fn optimize_hit_rate(&self) -> f64 {
        let hits = self.optimize_hits();
        let misses = self.optimize_misses();
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

impl Default for PlanCacheStats {
    fn default() -> Self {
        Self::new()
    }
}

// ─── PlanCacheStatsSnapshot ──────────────────────────────────────

/// 统计快照（用于读取一致性视图）
#[derive(Debug, Clone)]
pub struct PlanCacheStatsSnapshot {
    /// 解析缓存命中次数
    pub parse_hits: u64,
    /// 解析缓存未命中次数
    pub parse_misses: u64,
    /// 优化缓存命中次数
    pub optimize_hits: u64,
    /// 优化缓存未命中次数
    pub optimize_misses: u64,
    /// LRU 淘汰次数
    pub evictions: u64,
    /// 当前缓存条目数
    pub size: usize,
    /// 解析缓存命中率
    pub parse_hit_rate: f64,
    /// 优化缓存命中率
    pub optimize_hit_rate: f64,
}

// ─── LruOrder64 ──────────────────────────────────────────────────

/// u64 键的 LRU 双向链表（arena 实现，O(1) touch/remove/lru_key）
struct LruOrder64 {
    nodes: Vec<LruNode64>,
    free_list: Vec<usize>,
    index: HashMap<u64, usize>,
    head: Option<usize>,
    tail: Option<usize>,
}

struct LruNode64 {
    key: u64,
    prev: Option<usize>,
    next: Option<usize>,
}

impl LruOrder64 {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            free_list: Vec::new(),
            index: HashMap::new(),
            head: None,
            tail: None,
        }
    }

    fn touch(&mut self, key: u64) {
        if let Some(&idx) = self.index.get(&key) {
            self.unlink(idx);
            self.link_tail(idx);
        } else {
            let idx = self.alloc_node(key);
            self.link_tail(idx);
            self.index.insert(key, idx);
        }
    }

    fn remove(&mut self, key: u64) {
        if let Some(idx) = self.index.remove(&key) {
            self.unlink(idx);
            self.free_node(idx);
        }
    }

    fn lru_key(&self) -> Option<u64> {
        self.head.map(|idx| self.nodes[idx].key)
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.free_list.clear();
        self.index.clear();
        self.head = None;
        self.tail = None;
    }

    fn len(&self) -> usize {
        self.index.len()
    }

    fn alloc_node(&mut self, key: u64) -> usize {
        if let Some(idx) = self.free_list.pop() {
            self.nodes[idx] = LruNode64 {
                key,
                prev: None,
                next: None,
            };
            idx
        } else {
            self.nodes.push(LruNode64 {
                key,
                prev: None,
                next: None,
            });
            self.nodes.len() - 1
        }
    }

    fn free_node(&mut self, idx: usize) {
        self.free_list.push(idx);
    }

    fn unlink(&mut self, idx: usize) {
        let prev = self.nodes[idx].prev;
        let next = self.nodes[idx].next;
        match prev {
            Some(p) => self.nodes[p].next = next,
            None => self.head = next,
        }
        match next {
            Some(n) => self.nodes[n].prev = prev,
            None => self.tail = prev,
        }
        self.nodes[idx].prev = None;
        self.nodes[idx].next = None;
    }

    fn link_tail(&mut self, idx: usize) {
        match self.tail {
            Some(t) => {
                self.nodes[idx].prev = Some(t);
                self.nodes[t].next = Some(idx);
            }
            None => {
                self.head = Some(idx);
            }
        }
        self.tail = Some(idx);
    }
}

// ─── PlanCache ───────────────────────────────────────────────────

/// 查询计划缓存
///
/// 双缓存架构：
/// - `parse_cache`：SQL → AST（解析结果缓存）
/// - `optimize_cache`：SQL → 优化分析（优化结果缓存）
///
/// 锁顺序约定：parse_cache → optimize_cache → access_order → table_index → stats
/// （按此顺序加锁，避免死锁）
pub struct PlanCache {
    /// 解析缓存（hash → entry）
    parse_cache: RwLock<HashMap<u64, PlanCacheEntry>>,
    /// 优化缓存（hash → entry）
    optimize_cache: RwLock<HashMap<u64, PlanCacheEntry>>,
    /// LRU 访问顺序（arena 双向链表）
    access_order: RwLock<LruOrder64>,
    /// 表级失效索引（table → `Vec<hash>`）
    table_index: RwLock<HashMap<String, Vec<u64>>>,
    /// 统计计数器
    stats: PlanCacheStats,
    /// 最大缓存条目数
    max_size: usize,
    /// 默认 TTL
    default_ttl: Option<Duration>,
    /// v7.4.0：LRU-K 访问次数（K=2，记录每个 key 被访问的次数）
    access_counts: RwLock<HashMap<u64, u32>>,
    /// v7.4.0：LRU-K 的 K 值
    lru_k: u32,
    /// v7.4.0：自适应容量下限
    min_capacity: usize,
    /// v7.4.0：自适应容量上限
    max_capacity: usize,
    /// v10.12.0 T2.2：多策略淘汰器
    multi_strategy_evictor: Option<MultiStrategyEvictor>,
    /// v10.12.0 T2.3：命中率监控器
    hit_rate_monitor: Option<HitRateMonitor>,
}

impl PlanCache {
    /// 创建新的查询计划缓存
    ///
    /// - `max_size`：最大缓存条目数（LRU 淘汰）
    /// - `default_ttl`：默认 TTL（None 表示永不过期）
    pub fn new(max_size: usize, default_ttl: Option<Duration>) -> Self {
        Self {
            parse_cache: RwLock::new(HashMap::new()),
            optimize_cache: RwLock::new(HashMap::new()),
            access_order: RwLock::new(LruOrder64::new()),
            table_index: RwLock::new(HashMap::new()),
            stats: PlanCacheStats::new(),
            max_size,
            default_ttl,
            access_counts: RwLock::new(HashMap::new()),
            lru_k: 2,
            min_capacity: max_size / 4,
            max_capacity: max_size * 4,
            multi_strategy_evictor: None,
            hit_rate_monitor: None,
        }
    }

    /// v10.12.0 T2.2：创建带多策略淘汰器的查询计划缓存
    pub fn new_with_evictors(
        max_size: usize,
        default_ttl: Option<Duration>,
        ttl_config: TtlEvictConfig,
        size_limit_config: SizeLimitConfig,
    ) -> Self {
        Self {
            parse_cache: RwLock::new(HashMap::new()),
            optimize_cache: RwLock::new(HashMap::new()),
            access_order: RwLock::new(LruOrder64::new()),
            table_index: RwLock::new(HashMap::new()),
            stats: PlanCacheStats::new(),
            max_size,
            default_ttl,
            access_counts: RwLock::new(HashMap::new()),
            lru_k: 2,
            min_capacity: max_size / 4,
            max_capacity: max_size * 4,
            multi_strategy_evictor: Some(MultiStrategyEvictor::new(ttl_config, size_limit_config)),
            hit_rate_monitor: None,
        }
    }

    /// v10.12.0 T2.3：启用命中率监控
    pub fn with_hit_rate_monitor(mut self, monitor: HitRateMonitor) -> Self {
        self.hit_rate_monitor = Some(monitor);
        self
    }

    /// v10.12.0 T2.3：命中率监控器引用
    pub fn hit_rate_monitor(&self) -> Option<&HitRateMonitor> {
        self.hit_rate_monitor.as_ref()
    }

    /// v10.12.0 T2.2：主动 TTL 淘汰过期条目
    ///
    /// 周期性扫描缓存，主动淘汰过期条目。返回淘汰条目数。
    pub fn evict_expired(&self) -> usize {
        let evictor = match &self.multi_strategy_evictor {
            Some(e) => e,
            None => return 0,
        };
        if !evictor.should_ttl_evict() {
            return 0;
        }

        let mut access_order = self.access_order.write();
        let mut optimize_cache = self.optimize_cache.write();
        let mut parse_cache = self.parse_cache.write();
        let mut table_index = self.table_index.write();

        let expired_hashes: Vec<u64> = parse_cache
            .iter()
            .filter(|(_, e)| e.is_expired())
            .map(|(&k, _)| k)
            .collect();

        let count = expired_hashes.len();
        for &hash in &expired_hashes {
            access_order.remove(hash);
            parse_cache.remove(&hash);
            optimize_cache.remove(&hash);
            Self::remove_from_table_index(&mut table_index, hash);
        }
        self.stats
            .evictions
            .fetch_add(count as u64, Ordering::Relaxed);
        count
    }

    /// v10.12.0 T2.2：多策略淘汰器引用
    pub fn multi_strategy_evictor(&self) -> Option<&MultiStrategyEvictor> {
        self.multi_strategy_evictor.as_ref()
    }

    /// 获取或解析 SQL
    ///
    /// 命中缓存时返回 AST + stats.parse_hits++ + LRU touch。
    /// 未命中时解析 SQL + 存入缓存 + stats.parse_misses++。
    pub fn get_or_parse(&self, sql: &str) -> Result<Arc<Statement>, String> {
        let key = PlanCacheKey::from_sql(sql);

        // 锁序约定：不得在持有 parse_cache 锁时获取 access_order 锁
        // （未命中路径以相反顺序获取两把锁，否则并发命中/未命中会死锁）
        let hit = {
            let cache = self.parse_cache.read();
            match cache.get(&key.hash) {
                Some(entry) if !entry.is_expired() => entry.ast.clone(),
                _ => None,
            }
        };
        if let Some(ast) = hit {
            self.stats.parse_hits.fetch_add(1, Ordering::Relaxed);
            self.access_order.write().touch(key.hash);
            self.access_counts
                .write()
                .entry(key.hash)
                .and_modify(|c| *c += 1)
                .or_insert(1);
            if let Some(evictor) = &self.multi_strategy_evictor {
                evictor.access(&key.hash.to_string());
            }
            if let Some(monitor) = &self.hit_rate_monitor {
                monitor.record_hit_rate(1.0);
            }
            return Ok(ast);
        }

        self.stats.parse_misses.fetch_add(1, Ordering::Relaxed);
        if let Some(monitor) = &self.hit_rate_monitor {
            monitor.record_hit_rate(0.0);
        }

        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, sql).map_err(|e| e.to_string())?;
        if statements.is_empty() {
            return Err("empty SQL".to_string());
        }
        let ast = Arc::new(statements.into_iter().next().unwrap());
        let tables = SqlNormalizer::extract_tables(sql);

        {
            let mut access_order = self.access_order.write();
            // 统一锁序：access_order → optimize_cache → parse_cache → table_index
            let mut optimize_cache = self.optimize_cache.write();
            let mut cache = self.parse_cache.write();
            let mut table_index = self.table_index.write();

            if cache.len() >= self.max_size {
                let evict_hash = if let Some(evictor) = &self.multi_strategy_evictor {
                    let memory_bytes: usize = cache.values().map(|e| e.sql_normalized_len()).sum();
                    let candidates = evictor.evict(&cache, cache.len(), memory_bytes);
                    candidates
                        .first()
                        .copied()
                        .or_else(|| access_order.lru_key())
                } else {
                    access_order.lru_key()
                };
                if let Some(evict_hash) = evict_hash {
                    access_order.remove(evict_hash);
                    cache.remove(&evict_hash);
                    optimize_cache.remove(&evict_hash);
                    Self::remove_from_table_index(&mut table_index, evict_hash);
                    self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                }
            }

            let entry = PlanCacheEntry {
                ast: Some(ast.clone()),
                analysis: None,
                created_at: Instant::now(),
                tables: tables.clone(),
                ttl: self.default_ttl,
            };
            cache.insert(key.hash, entry);
            access_order.touch(key.hash);

            for table in &tables {
                table_index.entry(table.clone()).or_default().push(key.hash);
            }
        }

        Ok(ast)
    }

    /// 获取或优化 SQL
    ///
    /// 命中缓存时返回优化分析 + stats.optimize_hits++ + LRU touch。
    /// 未命中时返回 None + stats.optimize_misses++（调用方应执行优化后调用 `store_optimize`）。
    pub fn get_or_optimize(&self, sql: &str) -> Option<Arc<String>> {
        let key = PlanCacheKey::from_sql(sql);

        let hit = {
            let cache = self.optimize_cache.read();
            match cache.get(&key.hash) {
                Some(entry) if !entry.is_expired() => entry.analysis.clone(),
                _ => None,
            }
        };
        if let Some(analysis) = hit {
            self.stats.optimize_hits.fetch_add(1, Ordering::Relaxed);
            self.access_order.write().touch(key.hash);
            return Some(analysis);
        }

        self.stats.optimize_misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// 存储优化结果
    ///
    /// 在 `get_or_optimize` 返回 None 后，调用方执行优化并存储结果。
    pub fn store_optimize(&self, sql: &str, analysis: Arc<String>) {
        let key = PlanCacheKey::from_sql(sql);
        let tables = SqlNormalizer::extract_tables(sql);

        let mut access_order = self.access_order.write();
        let mut cache = self.optimize_cache.write();
        let mut parse_cache = self.parse_cache.write();
        let mut table_index = self.table_index.write();

        if cache.len() >= self.max_size {
            let evict_hash = if let Some(evictor) = &self.multi_strategy_evictor {
                let memory_bytes: usize = cache.values().map(|e| e.sql_normalized_len()).sum();
                let candidates = evictor.evict(&cache, cache.len(), memory_bytes);
                candidates
                    .first()
                    .copied()
                    .or_else(|| access_order.lru_key())
            } else {
                access_order.lru_key()
            };
            if let Some(evict_hash) = evict_hash {
                access_order.remove(evict_hash);
                cache.remove(&evict_hash);
                parse_cache.remove(&evict_hash);
                Self::remove_from_table_index(&mut table_index, evict_hash);
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }

        let entry = PlanCacheEntry {
            ast: None,
            analysis: Some(analysis),
            created_at: Instant::now(),
            tables: tables.clone(),
            ttl: self.default_ttl,
        };
        cache.insert(key.hash, entry);
        access_order.touch(key.hash);

        for table in &tables {
            table_index.entry(table.clone()).or_default().push(key.hash);
        }
    }

    /// 表级精确失效
    ///
    /// 失效所有引用指定表的缓存条目，返回失效条目数。
    pub fn invalidate_table(&self, table: &str) -> usize {
        // 统一锁序：access_order → optimize_cache → parse_cache → table_index
        let mut access_order = self.access_order.write();
        let mut optimize_cache = self.optimize_cache.write();
        let mut parse_cache = self.parse_cache.write();
        let mut table_index = self.table_index.write();
        let keys = table_index.remove(table).unwrap_or_default();

        if keys.is_empty() {
            return 0;
        }

        let count = keys.len();

        for &hash in &keys {
            access_order.remove(hash);
            parse_cache.remove(&hash);
            optimize_cache.remove(&hash);
        }

        for remaining_keys in table_index.values_mut() {
            remaining_keys.retain(|k| !keys.contains(k));
        }

        self.stats
            .evictions
            .fetch_add(count as u64, Ordering::Relaxed);
        count
    }

    /// 全量清空缓存
    pub fn invalidate_all(&self) {
        self.parse_cache.write().clear();
        self.optimize_cache.write().clear();
        self.access_order.write().clear();
        self.table_index.write().clear();
    }

    /// 获取统计快照
    pub fn stats(&self) -> PlanCacheStatsSnapshot {
        let size = self.access_order.read().len();
        PlanCacheStatsSnapshot {
            parse_hits: self.stats.parse_hits(),
            parse_misses: self.stats.parse_misses(),
            optimize_hits: self.stats.optimize_hits(),
            optimize_misses: self.stats.optimize_misses(),
            evictions: self.stats.evictions(),
            size,
            parse_hit_rate: self.stats.parse_hit_rate(),
            optimize_hit_rate: self.stats.optimize_hit_rate(),
        }
    }

    /// 当前缓存大小
    pub fn size(&self) -> usize {
        self.access_order.read().len()
    }

    fn remove_from_table_index(table_index: &mut HashMap<String, Vec<u64>>, hash: u64) {
        for keys in table_index.values_mut() {
            keys.retain(|k| *k != hash);
        }
    }
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new(1024, None)
    }
}

// ============================================================================
// v7.3.0 任务 1.5：参数类型指纹 + PlanCacheConfig + eviction_count
// ============================================================================

/// 查询计划缓存配置（v7.3.0）
#[derive(Debug, Clone)]
pub struct PlanCacheConfig {
    /// 最大缓存条目数
    pub capacity: usize,
    /// TTL（默认 5 分钟）
    pub ttl: Duration,
    /// 指纹是否纳入参数类型
    pub include_param_types: bool,
}

impl Default for PlanCacheConfig {
    fn default() -> Self {
        Self {
            capacity: 256,
            ttl: Duration::from_secs(300),
            include_param_types: true,
        }
    }
}

impl PlanCacheConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置容量
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// 设置 TTL
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// 设置是否纳入参数类型
    pub fn with_param_types(mut self, include: bool) -> Self {
        self.include_param_types = include;
        self
    }
}

/// 生成包含参数类型的查询指纹（v7.3.0）
///
/// 使用 xxHash64 对 SQL + 参数类型生成指纹。
/// 相同 SQL 不同参数类型生成不同指纹，避免计划缓存错误命中。
///
/// # 生产调用点
///
/// `packages/sz-orm-core/src/plan_cache.rs` `fingerprint_with_types` 函数。
pub fn fingerprint_with_types(sql: &str, param_types: &[crate::DbType]) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(sql.as_bytes());
    for pt in param_types {
        hasher.write(&[*pt as u8]);
    }
    hasher.finish()
}

impl PlanCache {
    /// v7.3.0 任务 1.5：淘汰数原子计数器
    pub fn eviction_count(&self) -> u64 {
        self.stats.evictions.load(Ordering::Relaxed)
    }

    /// v7.3.0 任务 1.5：从 PlanCacheConfig 创建缓存
    pub fn with_config(config: &PlanCacheConfig) -> Self {
        Self::new(config.capacity, Some(config.ttl))
    }

    /// v7.3.0 任务 1.5：带参数类型指纹的 get_or_parse
    ///
    /// 相同 SQL 不同参数类型生成不同指纹，避免错误命中。
    pub fn get_or_parse_with_types(
        &self,
        sql: &str,
        param_types: &[crate::DbType],
    ) -> Result<Arc<Statement>, String> {
        let type_fingerprint = fingerprint_with_types(sql, param_types);

        let hit = {
            let cache = self.parse_cache.read();
            match cache.get(&type_fingerprint) {
                Some(entry) if !entry.is_expired() => entry.ast.clone(),
                _ => None,
            }
        };
        if let Some(ast) = hit {
            self.stats.parse_hits.fetch_add(1, Ordering::Relaxed);
            self.access_order.write().touch(type_fingerprint);
            return Ok(ast);
        }

        self.stats.parse_misses.fetch_add(1, Ordering::Relaxed);

        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, sql).map_err(|e| e.to_string())?;
        if statements.is_empty() {
            return Err("empty SQL".to_string());
        }
        let ast = Arc::new(statements.into_iter().next().unwrap());
        let tables = SqlNormalizer::extract_tables(sql);

        {
            let mut access_order = self.access_order.write();
            let mut optimize_cache = self.optimize_cache.write();
            let mut cache = self.parse_cache.write();
            let mut table_index = self.table_index.write();

            if cache.len() >= self.max_size {
                if let Some(lru_hash) = access_order.lru_key() {
                    access_order.remove(lru_hash);
                    cache.remove(&lru_hash);
                    optimize_cache.remove(&lru_hash);
                    Self::remove_from_table_index(&mut table_index, lru_hash);
                    self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                }
            }

            let entry = PlanCacheEntry {
                ast: Some(ast.clone()),
                analysis: None,
                created_at: Instant::now(),
                tables: tables.clone(),
                ttl: self.default_ttl,
            };
            cache.insert(type_fingerprint, entry);
            access_order.touch(type_fingerprint);

            for table in &tables {
                table_index
                    .entry(table.clone())
                    .or_default()
                    .push(type_fingerprint);
            }
        }

        Ok(ast)
    }

    /// v7.4.0 任务 3.3：获取 key 的访问次数（LRU-K 统计）
    pub fn access_count(&self, hash: u64) -> u32 {
        self.access_counts.read().get(&hash).copied().unwrap_or(0)
    }

    /// v7.4.0 任务 3.3：LRU-K 的 K 值
    pub fn lru_k(&self) -> u32 {
        self.lru_k
    }

    /// v7.4.0 任务 3.3：解析缓存命中率
    pub fn parse_hit_rate(&self) -> f64 {
        let hits = self.stats.parse_hits.load(Ordering::Relaxed);
        let misses = self.stats.parse_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// v7.4.0 任务 3.3：自适应容量调整
    ///
    /// 根据命中率动态调整缓存大小：
    /// - 命中率 > 80%：扩容（上限 max_capacity）
    /// - 命中率 < 30%：缩容（下限 min_capacity）
    /// - 其他：保持不变
    pub fn adjust_capacity(&mut self) -> bool {
        let hit_rate = self.parse_hit_rate();
        let old_size = self.max_size;
        if hit_rate > 0.8 && self.max_size < self.max_capacity {
            self.max_size = (self.max_size * 3 / 2).min(self.max_capacity);
        } else if hit_rate < 0.3 && self.max_size > self.min_capacity {
            self.max_size = (self.max_size * 2 / 3).max(self.min_capacity);
        }
        self.max_size != old_size
    }

    /// v7.4.0 任务 3.3：当前最大容量
    pub fn capacity(&self) -> usize {
        self.max_size
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_normalizer_basic() {
        let sql1 = "SELECT * FROM users WHERE id = ?";
        let sql2 = "select * from users where id = ?";
        let n1 = SqlNormalizer::normalize(sql1);
        let n2 = SqlNormalizer::normalize(sql2);
        assert_eq!(n1, n2, "大小写差异应归一化");
    }

    #[test]
    fn test_sql_normalizer_whitespace() {
        let sql1 = "SELECT   *   FROM   users";
        let sql2 = "SELECT * FROM users";
        let n1 = SqlNormalizer::normalize(sql1);
        let n2 = SqlNormalizer::normalize(sql2);
        assert_eq!(n1, n2, "空白差异应归一化");
    }

    #[test]
    fn test_sql_normalizer_different_semantics() {
        let sql1 = "SELECT * FROM users WHERE id = ?";
        let sql2 = "SELECT * FROM orders WHERE id = ?";
        let n1 = SqlNormalizer::normalize(sql1);
        let n2 = SqlNormalizer::normalize(sql2);
        assert_ne!(n1, n2, "不同表名应产生不同归一化");
    }

    #[test]
    fn test_sql_normalizer_parse_error_fallback() {
        let sql = "this is not valid sql !!!";
        let normalized = SqlNormalizer::normalize(sql);
        assert!(!normalized.is_empty());
    }

    #[test]
    fn test_extract_tables_select() {
        let sql = "SELECT * FROM users JOIN orders ON users.id = orders.user_id";
        let tables = SqlNormalizer::extract_tables(sql);
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_extract_tables_insert() {
        let sql = "INSERT INTO products (name) VALUES (?)";
        let tables = SqlNormalizer::extract_tables(sql);
        assert!(tables.contains(&"products".to_string()));
    }

    #[test]
    fn test_extract_tables_update() {
        let sql = "UPDATE products SET name = ? WHERE id = ?";
        let tables = SqlNormalizer::extract_tables(sql);
        assert!(tables.contains(&"products".to_string()));
    }

    #[test]
    fn test_extract_tables_delete() {
        let sql = "DELETE FROM products WHERE id = ?";
        let tables = SqlNormalizer::extract_tables(sql);
        assert!(
            tables.iter().any(|t| t.contains("products")),
            "应包含 products 表，实际: {:?}",
            tables
        );
    }

    #[test]
    fn test_plan_cache_key_same_sql() {
        let k1 = PlanCacheKey::from_sql("SELECT * FROM users WHERE id = ?");
        let k2 = PlanCacheKey::from_sql("select * from users where id = ?");
        assert_eq!(k1.hash, k2.hash, "相同 SQL 模板应产生相同 hash");
    }

    #[test]
    fn test_plan_cache_key_different_sql() {
        let k1 = PlanCacheKey::from_sql("SELECT * FROM users");
        let k2 = PlanCacheKey::from_sql("SELECT * FROM orders");
        assert_ne!(k1.hash, k2.hash, "不同 SQL 应产生不同 hash");
    }

    #[test]
    fn test_plan_cache_key_no_sensitive_data() {
        let key = PlanCacheKey::from_sql("SELECT * FROM users WHERE password = ?");
        assert!(
            !key.sql_normalized.contains("secret123"),
            "参数化查询缓存键不应包含参数值"
        );
    }

    #[test]
    fn test_plan_cache_new() {
        let cache = PlanCache::new(1024, None);
        assert_eq!(cache.size(), 0);
        let stats = cache.stats();
        assert_eq!(stats.size, 0);
        assert_eq!(stats.parse_hits, 0);
        assert_eq!(stats.parse_misses, 0);
    }

    #[test]
    fn test_plan_cache_default() {
        let cache = PlanCache::default();
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn test_get_or_parse_hit() {
        let cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users WHERE id = ?";

        let ast1 = cache.get_or_parse(sql).expect("parse");
        assert_eq!(cache.stats().parse_misses, 1, "首次应 miss");

        let ast2 = cache.get_or_parse(sql).expect("parse");
        assert_eq!(cache.stats().parse_hits, 1, "第二次应 hit");
        assert_eq!(cache.stats().parse_misses, 1, "misses 不变");

        assert!(Arc::ptr_eq(&ast1, &ast2), "命中应返回相同 Arc");
    }

    #[test]
    fn test_get_or_parse_different_params_same_template() {
        let cache = PlanCache::new(100, None);
        let sql1 = "SELECT * FROM users WHERE id = ?";
        let sql2 = "select * from users where id = ?";

        cache.get_or_parse(sql1).expect("parse");
        cache.get_or_parse(sql2).expect("parse");

        assert_eq!(cache.stats().parse_hits, 1, "相同模板不同写法应命中");
        assert_eq!(cache.stats().parse_misses, 1);
    }

    #[test]
    fn test_get_or_parse_different_sql() {
        let cache = PlanCache::new(100, None);
        cache.get_or_parse("SELECT * FROM users").expect("parse");
        cache.get_or_parse("SELECT * FROM orders").expect("parse");

        assert_eq!(cache.stats().parse_misses, 2, "不同 SQL 应各 miss 一次");
        assert_eq!(cache.stats().parse_hits, 0);
    }

    #[test]
    fn test_get_or_optimize_miss_then_store_then_hit() {
        let cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users WHERE id = ?";

        assert!(cache.get_or_optimize(sql).is_none(), "首次应 miss");
        assert_eq!(cache.stats().optimize_misses, 1);

        cache.store_optimize(sql, Arc::new("optimized plan".to_string()));

        let result = cache.get_or_optimize(sql);
        assert!(result.is_some(), "存储后应 hit");
        assert_eq!(cache.stats().optimize_hits, 1);
        assert_eq!(*result.unwrap().as_ref(), "optimized plan");
    }

    #[test]
    fn test_invalidate_table_precise() {
        let cache = PlanCache::new(100, None);
        cache.get_or_parse("SELECT * FROM users").expect("parse");
        cache.get_or_parse("SELECT * FROM orders").expect("parse");
        assert_eq!(cache.size(), 2);

        let evicted = cache.invalidate_table("users");
        assert_eq!(evicted, 1, "应失效 1 条");
        assert_eq!(cache.size(), 1, "应剩余 1 条");

        let stats = cache.stats();
        assert!(stats.parse_hits == 0, "orders 缓存应不受影响");
        cache.get_or_parse("SELECT * FROM orders").expect("parse");
        assert_eq!(cache.stats().parse_hits, 1, "orders 应命中缓存");
    }

    #[test]
    fn test_invalidate_table_nonexistent() {
        let cache = PlanCache::new(100, None);
        cache.get_or_parse("SELECT * FROM users").expect("parse");
        let evicted = cache.invalidate_table("nonexistent");
        assert_eq!(evicted, 0, "不存在的表应返回 0");
        assert_eq!(cache.size(), 1, "缓存不应受影响");
    }

    #[test]
    fn test_invalidate_all() {
        let cache = PlanCache::new(100, None);
        cache.get_or_parse("SELECT * FROM users").expect("parse");
        cache.get_or_parse("SELECT * FROM orders").expect("parse");
        assert_eq!(cache.size(), 2);

        cache.invalidate_all();
        assert_eq!(cache.size(), 0, "全量清空后 size 应为 0");
    }

    #[test]
    fn test_lru_eviction() {
        let cache = PlanCache::new(3, None);
        cache.get_or_parse("SELECT * FROM t1").expect("parse");
        cache.get_or_parse("SELECT * FROM t2").expect("parse");
        cache.get_or_parse("SELECT * FROM t3").expect("parse");
        assert_eq!(cache.size(), 3);

        cache.get_or_parse("SELECT * FROM t4").expect("parse");
        assert_eq!(cache.size(), 3, "max_size=3 应保持 3 条");
        assert!(cache.stats().evictions >= 1, "应有淘汰");

        cache.get_or_parse("SELECT * FROM t1").expect("parse");
        assert!(cache.stats().parse_misses >= 4, "t1 被淘汰后应重新 miss");
    }

    #[test]
    fn test_lru_eviction_max_size_1() {
        let cache = PlanCache::new(1, None);
        cache.get_or_parse("SELECT * FROM t1").expect("parse");
        assert_eq!(cache.size(), 1);

        cache.get_or_parse("SELECT * FROM t2").expect("parse");
        assert_eq!(cache.size(), 1, "max_size=1 应保持 1 条");

        cache.get_or_parse("SELECT * FROM t1").expect("parse");
        assert!(cache.stats().parse_misses >= 3, "t1 应被淘汰后重新 miss");
    }

    #[test]
    fn test_stats_hit_rate() {
        let cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users";

        cache.get_or_parse(sql).expect("parse");
        cache.get_or_parse(sql).expect("parse");
        cache.get_or_parse(sql).expect("parse");

        let stats = cache.stats();
        assert_eq!(stats.parse_hits, 2);
        assert_eq!(stats.parse_misses, 1);
        assert!((stats.parse_hit_rate - (2.0 / 3.0)).abs() < 0.001);
    }

    #[test]
    fn test_stats_hit_rate_empty() {
        let cache = PlanCache::new(100, None);
        let stats = cache.stats();
        assert_eq!(stats.parse_hit_rate, 0.0, "空缓存命中率应为 0.0");
        assert_eq!(stats.optimize_hit_rate, 0.0);
    }

    #[test]
    fn test_ttl_expiration() {
        let cache = PlanCache::new(100, Some(Duration::from_nanos(1)));
        let sql = "SELECT * FROM users";

        cache.get_or_parse(sql).expect("parse");
        std::thread::sleep(Duration::from_millis(10));

        cache.get_or_parse(sql).expect("parse");
        assert!(cache.stats().parse_misses >= 2, "TTL 过期后应重新 miss");
    }

    #[test]
    fn test_plan_cache_concurrent_same_sql() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(PlanCache::new(100, None));
        let sql = "SELECT * FROM users WHERE id = ?";
        let mut handles = Vec::new();

        for _ in 0..10 {
            let cache = cache.clone();
            handles.push(thread::spawn(move || {
                cache.get_or_parse(sql).expect("parse");
            }));
        }

        for h in handles {
            h.join().expect("thread");
        }

        assert!(cache.size() >= 1, "并发后应至少有 1 条缓存");
        assert!(
            cache.stats().parse_misses + cache.stats().parse_hits >= 10,
            "应有 10 次访问记录"
        );
    }

    // ─── T26c 新增测试（覆盖率 74.12% → ≥90%） ─────────────────────

    #[test]
    fn t26pc_normalize_empty_sql() {
        let normalized = SqlNormalizer::normalize("");
        assert!(normalized.is_empty(), "空 SQL 归一化应为空");
    }

    #[test]
    fn t26pc_normalize_multi_statement() {
        let sql = "SELECT * FROM a; SELECT * FROM b";
        let normalized = SqlNormalizer::normalize(sql);
        assert!(normalized.contains("SELECT"), "多语句归一化应包含 SELECT");
    }

    #[test]
    fn t26pc_extract_tables_no_table_fallback() {
        let tables = SqlNormalizer::extract_tables("this is not valid sql");
        assert!(tables.is_empty(), "无表 SQL 应返回空列表");
    }

    #[test]
    fn t26pc_extract_tables_complex_join() {
        let sql =
            "SELECT * FROM users u JOIN orders o ON u.id = o.uid JOIN items i ON o.id = i.oid";
        let tables = SqlNormalizer::extract_tables(sql);
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
        assert!(tables.contains(&"items".to_string()));
    }

    #[test]
    fn t26pc_plan_cache_key_eq_trait() {
        let k1 = PlanCacheKey::from_sql("SELECT * FROM users");
        let k2 = PlanCacheKey::from_sql("select * from users");
        assert_eq!(k1, k2, "相同 SQL 不同大小写应相等");
    }

    #[test]
    fn t26pc_plan_cache_key_hash_trait() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let k1 = PlanCacheKey::from_sql("SELECT * FROM users");
        let k2 = PlanCacheKey::from_sql("SELECT * FROM users");
        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        k1.hash(&mut h1);
        k2.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish(), "相同 key 应产生相同 hash");
    }

    #[test]
    fn t26pc_plan_cache_entry_no_ttl_not_expired() {
        let entry = PlanCacheEntry {
            ast: None,
            analysis: None,
            created_at: Instant::now(),
            tables: vec![],
            ttl: None,
        };
        assert!(!entry.is_expired(), "无 TTL 不应过期");
    }

    #[test]
    fn t26pc_plan_cache_entry_with_ttl_expired() {
        let entry = PlanCacheEntry {
            ast: None,
            analysis: None,
            created_at: Instant::now(),
            tables: vec![],
            ttl: Some(Duration::from_nanos(1)),
        };
        std::thread::sleep(Duration::from_millis(5));
        assert!(entry.is_expired(), "TTL 过期应返回 true");
    }

    #[test]
    fn t26pc_plan_cache_stats_default() {
        let stats = PlanCacheStats::default();
        assert_eq!(stats.parse_hits(), 0);
        assert_eq!(stats.parse_misses(), 0);
        assert_eq!(stats.optimize_hits(), 0);
        assert_eq!(stats.optimize_misses(), 0);
        assert_eq!(stats.evictions(), 0);
        assert_eq!(stats.parse_hit_rate(), 0.0);
        assert_eq!(stats.optimize_hit_rate(), 0.0);
    }

    #[test]
    fn t26pc_plan_cache_stats_optimize_rates() {
        let cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users";
        assert!(cache.get_or_optimize(sql).is_none());
        cache.store_optimize(sql, Arc::new("plan".to_string()));
        assert!(cache.get_or_optimize(sql).is_some());
        let stats = cache.stats();
        assert_eq!(stats.optimize_hits, 1);
        assert_eq!(stats.optimize_misses, 1);
        assert!((stats.optimize_hit_rate - 0.5).abs() < 0.001);
    }

    #[test]
    fn t26pc_plan_cache_config_default_values() {
        let config = PlanCacheConfig::default();
        assert_eq!(config.capacity, 256);
        assert_eq!(config.ttl, Duration::from_secs(300));
        assert!(config.include_param_types);
        let config2 = PlanCacheConfig::new();
        assert_eq!(config2.capacity, 256);
    }

    #[test]
    fn t26pc_plan_cache_config_builders() {
        let config = PlanCacheConfig::new()
            .with_capacity(512)
            .with_ttl(Duration::from_secs(600))
            .with_param_types(false);
        assert_eq!(config.capacity, 512);
        assert_eq!(config.ttl, Duration::from_secs(600));
        assert!(!config.include_param_types);
    }

    #[test]
    fn t26pc_fingerprint_with_types_same() {
        let types = [crate::DbType::MySQL, crate::DbType::Sqlite];
        let f1 = fingerprint_with_types("SELECT * FROM users", &types);
        let f2 = fingerprint_with_types("SELECT * FROM users", &types);
        assert_eq!(f1, f2, "相同 SQL + 相同类型应产生相同指纹");
    }

    #[test]
    fn t26pc_fingerprint_with_types_differ() {
        let sql = "SELECT * FROM users";
        let f1 = fingerprint_with_types(sql, &[crate::DbType::MySQL]);
        let f2 = fingerprint_with_types(sql, &[crate::DbType::PostgreSQL]);
        assert_ne!(f1, f2, "不同参数类型应产生不同指纹");
    }

    #[test]
    fn t26pc_fingerprint_with_types_empty() {
        let f1 = fingerprint_with_types("SELECT 1", &[]);
        let f2 = fingerprint_with_types("SELECT 1", &[]);
        assert_eq!(f1, f2);
    }

    #[test]
    fn t26pc_eviction_count_method() {
        let cache = PlanCache::new(2, None);
        cache.get_or_parse("SELECT * FROM t1").expect("parse");
        cache.get_or_parse("SELECT * FROM t2").expect("parse");
        cache.get_or_parse("SELECT * FROM t3").expect("parse");
        assert!(cache.eviction_count() >= 1, "应有淘汰计数");
    }

    #[test]
    fn t26pc_with_config_construction() {
        let config = PlanCacheConfig::new().with_capacity(64);
        let cache = PlanCache::with_config(&config);
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.capacity(), 64);
    }

    #[test]
    fn t26pc_get_or_parse_with_types_miss_then_hit() {
        let cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users WHERE id = ?";
        let types = [crate::DbType::MySQL];
        let ast1 = cache.get_or_parse_with_types(sql, &types).expect("parse");
        assert_eq!(cache.stats().parse_misses, 1);
        let ast2 = cache.get_or_parse_with_types(sql, &types).expect("parse");
        assert_eq!(cache.stats().parse_hits, 1);
        assert!(Arc::ptr_eq(&ast1, &ast2), "命中应返回相同 Arc");
    }

    #[test]
    fn t26pc_get_or_parse_with_types_different_types() {
        let cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users WHERE id = ?";
        cache
            .get_or_parse_with_types(sql, &[crate::DbType::MySQL])
            .expect("parse");
        cache
            .get_or_parse_with_types(sql, &[crate::DbType::PostgreSQL])
            .expect("parse");
        assert_eq!(cache.stats().parse_misses, 2, "不同类型应各 miss 一次");
        assert_eq!(cache.stats().parse_hits, 0);
    }

    #[test]
    fn t26pc_access_count_tracking() {
        let cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users";
        cache.get_or_parse(sql).expect("parse");
        cache.get_or_parse(sql).expect("parse");
        cache.get_or_parse(sql).expect("parse");
        let key = PlanCacheKey::from_sql(sql);
        assert_eq!(cache.access_count(key.hash), 2, "应记录 2 次命中访问");
    }

    #[test]
    fn t26pc_access_count_zero_for_unknown() {
        let cache = PlanCache::new(100, None);
        assert_eq!(cache.access_count(999999), 0, "未知 key 应返回 0");
    }

    #[test]
    fn t26pc_lru_k_value() {
        let cache = PlanCache::new(100, None);
        assert_eq!(cache.lru_k(), 2, "默认 LRU-K 的 K 应为 2");
    }

    #[test]
    fn t26pc_parse_hit_rate_method() {
        let cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users";
        cache.get_or_parse(sql).expect("parse");
        cache.get_or_parse(sql).expect("parse");
        let rate = cache.parse_hit_rate();
        assert!((rate - 0.5).abs() < 0.001, "1 hit / 2 total = 0.5");
    }

    #[test]
    fn t26pc_parse_hit_rate_empty() {
        let cache = PlanCache::new(100, None);
        assert_eq!(cache.parse_hit_rate(), 0.0);
    }

    #[test]
    fn t26pc_adjust_capacity_expand() {
        let mut cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users";
        cache.get_or_parse(sql).expect("parse");
        for _ in 0..5 {
            cache.get_or_parse(sql).expect("parse");
        }
        let old_cap = cache.capacity();
        let changed = cache.adjust_capacity();
        assert!(changed, "命中率 5/6>0.8 应扩容");
        assert!(cache.capacity() > old_cap);
    }

    #[test]
    fn t26pc_adjust_capacity_shrink() {
        let mut cache = PlanCache::new(100, None);
        for i in 1..=10u32 {
            let sql = format!("SELECT * FROM t{}", i);
            cache.get_or_parse(&sql).expect("parse");
        }
        let old_cap = cache.capacity();
        let changed = cache.adjust_capacity();
        assert!(changed, "命中率 0/10<0.3 应缩容");
        assert!(cache.capacity() < old_cap);
    }

    #[test]
    fn t26pc_adjust_capacity_noop() {
        let mut cache = PlanCache::new(100, None);
        let sql = "SELECT * FROM users";
        cache.get_or_parse(sql).expect("parse");
        cache.get_or_parse(sql).expect("parse");
        let changed = cache.adjust_capacity();
        assert!(!changed, "命中率 0.5 在 [0.3,0.8] 区间不应调整");
    }

    #[test]
    fn t26pc_capacity_method() {
        let cache = PlanCache::new(42, None);
        assert_eq!(cache.capacity(), 42);
    }

    #[test]
    fn t26pc_lru_k_replacer_new_and_access() {
        let mut replacer = LruKReplacer::new(2, 4);
        assert!(replacer.is_empty());
        replacer.access(1);
        assert!(!replacer.is_empty());
        assert_eq!(replacer.len(), 1);
    }

    #[test]
    fn t26pc_lru_k_replacer_evict() {
        let mut replacer = LruKReplacer::new(2, 2);
        replacer.access(1);
        replacer.access(2);
        let evicted = replacer.evict();
        assert!(evicted.is_some(), "满容量应能淘汰");
        assert_eq!(replacer.len(), 1);
    }

    #[test]
    fn t26pc_lru_k_replacer_evict_under_capacity() {
        let mut replacer = LruKReplacer::new(2, 4);
        replacer.access(1);
        assert!(replacer.evict().is_none(), "未满容量应返回 None");
    }

    #[test]
    fn t26pc_lru_k_replacer_hit_rate() {
        let mut replacer = LruKReplacer::new(2, 4);
        replacer.access(1);
        replacer.access(1);
        replacer.access(2);
        let rate = replacer.hit_rate();
        assert!(rate > 0.0, "应有命中率");
    }

    #[test]
    fn t26pc_lru_k_replacer_hit_rate_empty() {
        let replacer = LruKReplacer::new(2, 4);
        assert_eq!(replacer.hit_rate(), 0.0);
    }

    #[test]
    fn t26pc_lru_k_replacer_access_trims_history() {
        let mut replacer = LruKReplacer::new(2, 4);
        for _ in 0..5 {
            replacer.access(1);
        }
        // K=2，历史应只保留最近 2 次
        assert_eq!(replacer.len(), 1);
    }

    #[test]
    fn t26pc_adaptive_cache_new() {
        let acc = AdaptiveCacheCapacity::new(10, 100);
        assert_eq!(acc.min(), 10);
        assert_eq!(acc.max(), 100);
        assert_eq!(acc.current(), 10);
    }

    #[test]
    fn t26pc_adaptive_cache_adapt_low_rate_expand() {
        let mut acc = AdaptiveCacheCapacity::new(10, 1000);
        acc.record_hit_rate(0.5);
        let changed = acc.adapt();
        assert!(changed, "低命中率 0.5<0.85 应扩容");
        assert!(acc.current() > 10);
    }

    #[test]
    fn t26pc_adaptive_cache_adapt_empty() {
        let mut acc = AdaptiveCacheCapacity::new(10, 100);
        let changed = acc.adapt();
        assert!(!changed, "空窗口不应调整");
        assert_eq!(acc.current(), 10);
    }

    #[test]
    fn t26pc_adaptive_cache_adapt_at_max() {
        let mut acc = AdaptiveCacheCapacity::new(10, 10);
        acc.record_hit_rate(0.5);
        let changed = acc.adapt();
        assert!(!changed, "已达 max 不应扩容");
        assert_eq!(acc.current(), 10);
    }

    #[test]
    fn t26pc_adaptive_cache_record_window_evicts_old() {
        let mut acc = AdaptiveCacheCapacity::new(10, 1000);
        for _ in 0..150 {
            acc.record_hit_rate(0.5);
        }
        // 窗口大小 100，应只保留最近 100 条
        let changed = acc.adapt();
        assert!(changed, "窗口满后仍应能调整");
    }

    #[test]
    fn t26pc_store_optimize_with_eviction() {
        let cache = PlanCache::new(2, None);
        cache.store_optimize("SELECT * FROM t1", Arc::new("p1".to_string()));
        cache.store_optimize("SELECT * FROM t2", Arc::new("p2".to_string()));
        cache.store_optimize("SELECT * FROM t3", Arc::new("p3".to_string()));
        assert!(cache.stats().evictions >= 1, "应触发淘汰");
    }

    #[test]
    fn t26pc_invalidate_table_shared_table() {
        let cache = PlanCache::new(100, None);
        cache
            .get_or_parse("SELECT * FROM users JOIN orders ON users.id = orders.uid")
            .expect("parse");
        cache.get_or_parse("SELECT * FROM users").expect("parse");
        assert_eq!(cache.size(), 2);
        let evicted = cache.invalidate_table("users");
        assert_eq!(evicted, 2, "应失效 2 条引用 users 的条目");
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn t26pc_get_or_optimize_ttl_expired() {
        let cache = PlanCache::new(100, Some(Duration::from_nanos(1)));
        let sql = "SELECT * FROM users";
        assert!(cache.get_or_optimize(sql).is_none());
        cache.store_optimize(sql, Arc::new("plan".to_string()));
        std::thread::sleep(Duration::from_millis(5));
        assert!(cache.get_or_optimize(sql).is_none(), "TTL 过期后应 miss");
    }

    #[test]
    fn t26pc_get_or_parse_with_types_ttl_expired() {
        let cache = PlanCache::new(100, Some(Duration::from_nanos(1)));
        let sql = "SELECT * FROM users";
        let types = [crate::DbType::MySQL];
        cache.get_or_parse_with_types(sql, &types).expect("parse");
        std::thread::sleep(Duration::from_millis(5));
        cache.get_or_parse_with_types(sql, &types).expect("parse");
        assert!(cache.stats().parse_misses >= 2, "TTL 过期后应重新 miss");
    }

    #[test]
    fn t26pc_invalidate_all_clears_stats_size() {
        let cache = PlanCache::new(100, None);
        cache.get_or_parse("SELECT * FROM users").expect("parse");
        cache.store_optimize("SELECT * FROM users", Arc::new("p".to_string()));
        assert_eq!(cache.size(), 1);
        cache.invalidate_all();
        assert_eq!(cache.size(), 0);
    }
}
/// LRU-K 替换器：基于最近 K 次访问时间戳淘汰最久未访问的键
#[derive(Debug, Clone)]
pub struct LruKReplacer {
    k: usize,
    capacity: usize,
    access_history: std::collections::HashMap<u64, Vec<std::time::Instant>>,
}

impl LruKReplacer {
    /// 创建 K 近邻 LRU 替换器，容量为 `capacity`
    pub fn new(k: usize, capacity: usize) -> Self {
        Self {
            k,
            capacity,
            access_history: std::collections::HashMap::new(),
        }
    }

    /// 记录键的访问时间戳
    pub fn access(&mut self, key: u64) {
        let history = self.access_history.entry(key).or_default();
        history.push(std::time::Instant::now());
        if history.len() > self.k {
            history.remove(0);
        }
    }

    /// 淘汰并返回最久未访问的键，未满容量时返回 None
    pub fn evict(&mut self) -> Option<u64> {
        if self.access_history.len() < self.capacity {
            return None;
        }
        let mut oldest_key: Option<u64> = None;
        let mut oldest_time: Option<std::time::Instant> = None;

        for (&key, history) in &self.access_history {
            let ref_time = if history.len() >= self.k {
                history[0]
            } else {
                std::time::Instant::now()
            };
            if oldest_time.is_none() || ref_time < oldest_time.unwrap() {
                oldest_time = Some(ref_time);
                oldest_key = Some(key);
            }
        }

        if let Some(key) = oldest_key {
            self.access_history.remove(&key);
        }
        oldest_key
    }

    /// 返回命中率（访问次数 ≥ K 的键占比）
    pub fn hit_rate(&self) -> f64 {
        let total: usize = self.access_history.values().map(|h| h.len()).sum();
        if total == 0 {
            return 0.0;
        }
        let hits: usize = self
            .access_history
            .values()
            .map(|h| if h.len() >= self.k { 1 } else { 0 })
            .sum();
        hits as f64 / self.access_history.len().max(1) as f64
    }

    /// 返回当前追踪的键数量
    pub fn len(&self) -> usize {
        self.access_history.len()
    }

    /// 返回是否为空
    pub fn is_empty(&self) -> bool {
        self.access_history.is_empty()
    }
}

/// 自适应缓存容量：根据命中率窗口动态调整容量
#[derive(Debug, Clone)]
pub struct AdaptiveCacheCapacity {
    min: usize,
    max: usize,
    current: usize,
    hit_rate_window: std::collections::VecDeque<f64>,
    window_size: usize,
}

impl AdaptiveCacheCapacity {
    /// 创建自适应容量控制器，范围 [`min`, `max`]
    pub fn new(min: usize, max: usize) -> Self {
        Self {
            min,
            max,
            current: min,
            hit_rate_window: std::collections::VecDeque::new(),
            window_size: 100,
        }
    }

    /// 记录命中率采样到滑动窗口
    pub fn record_hit_rate(&mut self, rate: f64) {
        if self.hit_rate_window.len() >= self.window_size {
            self.hit_rate_window.pop_front();
        }
        self.hit_rate_window.push_back(rate);
    }

    /// 根据平均命中率调整容量，返回是否发生变化
    pub fn adapt(&mut self) -> bool {
        if self.hit_rate_window.is_empty() {
            return false;
        }
        let avg: f64 = self.hit_rate_window.iter().sum::<f64>() / self.hit_rate_window.len() as f64;
        let old = self.current;
        if avg < 0.85 {
            self.current = (self.current as f64 * 1.5) as usize;
        } else if avg > 0.95 {
            self.current = (self.current as f64 * 0.8) as usize;
        }
        self.current = self.current.clamp(self.min, self.max);
        self.current != old
    }

    /// 返回当前容量
    pub fn current(&self) -> usize {
        self.current
    }

    /// 返回最小容量
    pub fn min(&self) -> usize {
        self.min
    }

    /// 返回最大容量
    pub fn max(&self) -> usize {
        self.max
    }
}
// ============================================================================
// v10.12.0 T2.1：LFU 频率追踪
// ============================================================================

/// LFU 频率追踪器
///
/// 追踪每个缓存键的访问频率，支持周期衰减防止溢出。
/// 用于多策略淘汰中的 LFU 策略：低频键优先淘汰。
pub struct LfuTracker {
    frequencies: parking_lot::RwLock<HashMap<String, u64>>,
    total_accesses: AtomicU64,
    decay_factor: u64,
}

impl LfuTracker {
    /// 创建 LFU 频率追踪器
    pub fn new() -> Self {
        Self {
            frequencies: parking_lot::RwLock::new(HashMap::new()),
            total_accesses: AtomicU64::new(0),
            decay_factor: 2,
        }
    }

    /// 记录一次键访问（频率 +1）
    pub fn access(&self, key: &str) {
        self.total_accesses.fetch_add(1, Ordering::Relaxed);
        let mut freqs = self.frequencies.write();
        *freqs.entry(key.to_string()).or_insert(0) += 1;
    }

    /// 获取键的当前频率
    pub fn frequency(&self, key: &str) -> u64 {
        self.frequencies.read().get(key).copied().unwrap_or(0)
    }

    /// 周期衰减：所有频率除以 decay_factor，防止计数器溢出
    pub fn decay(&self) {
        let mut freqs = self.frequencies.write();
        for freq in freqs.values_mut() {
            *freq /= self.decay_factor;
        }
    }

    /// 返回频率最低的 N 个键（淘汰候选）
    pub fn evict_candidates(&self, n: usize) -> Vec<String> {
        let freqs = self.frequencies.read();
        let mut entries: Vec<(&String, &u64)> = freqs.iter().collect();
        entries.sort_by_key(|(_, freq)| *freq);
        entries.iter().take(n).map(|(k, _)| k.to_string()).collect()
    }

    /// 移除键
    pub fn remove(&self, key: &str) {
        self.frequencies.write().remove(key);
    }

    /// 追踪的键数量
    pub fn len(&self) -> usize {
        self.frequencies.read().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.frequencies.read().is_empty()
    }

    /// 总访问次数
    pub fn total_accesses(&self) -> u64 {
        self.total_accesses.load(Ordering::Relaxed)
    }
}

impl Default for LfuTracker {
    fn default() -> Self {
        Self::new()
    }
}
// ============================================================================
// v10.12.0 T2.2：多策略淘汰（TTL → 大小限制 → LFU → LRU）
// ============================================================================

/// TTL 淘汰配置
#[derive(Debug, Clone)]
pub struct TtlEvictConfig {
    /// 默认 TTL（条目超时自动淘汰）
    pub default_ttl: Duration,
    /// 主动淘汰检查间隔
    pub evict_interval: Duration,
}

impl Default for TtlEvictConfig {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_secs(300),
            evict_interval: Duration::from_secs(60),
        }
    }
}

/// TTL 主动周期淘汰器
///
/// 周期性扫描缓存，主动淘汰过期条目（非仅 get 时惰性判定）。
pub struct TtlEvictor {
    config: TtlEvictConfig,
    last_evict_time: parking_lot::Mutex<Instant>,
}

impl TtlEvictor {
    /// 创建 TTL 淘汰器
    pub fn new(config: TtlEvictConfig) -> Self {
        Self {
            config,
            last_evict_time: parking_lot::Mutex::new(Instant::now()),
        }
    }

    /// 检查是否到达淘汰周期
    pub fn should_evict(&self) -> bool {
        let last = self.last_evict_time.lock();
        last.elapsed() >= self.config.evict_interval
    }

    /// 执行 TTL 淘汰，返回被淘汰的键列表
    pub fn evict_expired(&self, entries: &HashMap<u64, PlanCacheEntry>) -> Vec<u64> {
        let mut evicted = Vec::new();
        for (&hash, entry) in entries {
            if entry.is_expired() {
                evicted.push(hash);
            }
        }
        *self.last_evict_time.lock() = Instant::now();
        evicted
    }

    /// 默认 TTL
    pub fn default_ttl(&self) -> Duration {
        self.config.default_ttl
    }

    /// 淘汰间隔
    pub fn evict_interval(&self) -> Duration {
        self.config.evict_interval
    }
}

/// 大小限制淘汰配置
#[derive(Debug, Clone)]
pub struct SizeLimitConfig {
    /// 最大条目数
    pub max_entries: usize,
    /// 最大内存占用（字节）
    pub max_memory_bytes: usize,
}

impl Default for SizeLimitConfig {
    fn default() -> Self {
        Self {
            max_entries: 1024,
            max_memory_bytes: 64 * 1024 * 1024,
        }
    }
}

/// 大小限制淘汰器
///
/// 按内存占用/综合权重淘汰：超容量时按 EvictWeight 权重淘汰最低权重条目。
pub struct SizeLimitEvictor {
    config: SizeLimitConfig,
}

impl SizeLimitEvictor {
    /// 创建大小限制淘汰器
    pub fn new(config: SizeLimitConfig) -> Self {
        Self { config }
    }

    /// 检查是否超容量
    pub fn is_over_limit(&self, entry_count: usize, memory_bytes: usize) -> bool {
        entry_count > self.config.max_entries || memory_bytes > self.config.max_memory_bytes
    }

    /// 按权重选择淘汰候选：返回权重最低的键
    pub fn select_evict_candidate(&self, weights: &HashMap<u64, EvictWeight>) -> Option<u64> {
        weights
            .iter()
            .min_by_key(|(_, w)| w.composite_score())
            .map(|(&k, _)| k)
    }

    /// 最大条目数
    pub fn max_entries(&self) -> usize {
        self.config.max_entries
    }

    /// 最大内存字节
    pub fn max_memory_bytes(&self) -> usize {
        self.config.max_memory_bytes
    }
}

/// 综合淘汰权重
///
/// 权重 = LFU 频率 + TTL 剩余比例 + 计划大小因子。
/// 权重越低越优先淘汰。
#[derive(Debug, Clone)]
pub struct EvictWeight {
    /// LFU 访问频率
    pub lfu_freq: u64,
    /// TTL 剩余比例（0.0~1.0，1.0 = 刚创建，0.0 = 即将过期）
    pub ttl_remaining_ratio: f64,
    /// 计划大小因子（字节，越小越优先淘汰）
    pub plan_size_bytes: usize,
}

impl EvictWeight {
    /// 创建淘汰权重
    pub fn new(lfu_freq: u64, ttl_remaining_ratio: f64, plan_size_bytes: usize) -> Self {
        Self {
            lfu_freq,
            ttl_remaining_ratio: ttl_remaining_ratio.clamp(0.0, 1.0),
            plan_size_bytes,
        }
    }

    /// 综合评分（越低越优先淘汰）
    ///
    /// 评分 = lfu_freq * 10 + ttl_remaining_ratio * 100 + plan_size_bytes / 1024
    /// 频率权重最高（热点不淘汰），TTL 剩余次之（快过期优先淘汰），大小最低。
    pub fn composite_score(&self) -> i64 {
        let freq_score = self.lfu_freq as i64 * 10;
        let ttl_score = (self.ttl_remaining_ratio * 100.0) as i64;
        let size_score = (self.plan_size_bytes / 1024) as i64;
        freq_score + ttl_score + size_score
    }
}

/// 多策略叠加淘汰器
///
/// 淘汰优先级：TTL 过期 → 大小限制 → LFU → LRU
pub struct MultiStrategyEvictor {
    ttl_evictor: TtlEvictor,
    size_limit_evictor: SizeLimitEvictor,
    lfu_tracker: LfuTracker,
}

impl MultiStrategyEvictor {
    /// 创建多策略淘汰器
    pub fn new(ttl_config: TtlEvictConfig, size_limit_config: SizeLimitConfig) -> Self {
        Self {
            ttl_evictor: TtlEvictor::new(ttl_config),
            size_limit_evictor: SizeLimitEvictor::new(size_limit_config),
            lfu_tracker: LfuTracker::new(),
        }
    }

    /// 多策略淘汰：按优先级依次检查，返回被淘汰的键
    ///
    /// 1. TTL 过期优先：主动淘汰所有过期条目
    /// 2. 大小限制：超容量时按权重淘汰
    /// 3. LFU：低频优先
    /// 4. LRU：最久未访问优先（由调用方 access_order 处理）
    pub fn evict(
        &self,
        entries: &HashMap<u64, PlanCacheEntry>,
        entry_count: usize,
        memory_bytes: usize,
    ) -> Vec<u64> {
        let mut evicted = Vec::new();

        // 策略 1：TTL 过期优先
        let ttl_expired = self.ttl_evictor.evict_expired(entries);
        if !ttl_expired.is_empty() {
            evicted.extend(ttl_expired);
            return evicted;
        }

        // 策略 2：大小限制
        if self
            .size_limit_evictor
            .is_over_limit(entry_count, memory_bytes)
        {
            let weights: HashMap<u64, EvictWeight> = entries
                .iter()
                .map(|(&k, e)| {
                    let freq = self.lfu_tracker.frequency(&k.to_string());
                    let ttl_remaining = e
                        .ttl
                        .map(|ttl| {
                            let elapsed = e.created_at.elapsed();
                            if elapsed >= ttl {
                                0.0
                            } else {
                                1.0 - (elapsed.as_secs_f64() / ttl.as_secs_f64())
                            }
                        })
                        .unwrap_or(1.0);
                    let plan_size = e.sql_normalized_len();
                    (k, EvictWeight::new(freq, ttl_remaining, plan_size))
                })
                .collect();

            if let Some(candidate) = self.size_limit_evictor.select_evict_candidate(&weights) {
                evicted.push(candidate);
                return evicted;
            }
        }

        // 策略 3：LFU 低频优先
        if entry_count > 0 {
            let candidates = self.lfu_tracker.evict_candidates(1);
            if let Some(key) = candidates.first() {
                if let Ok(hash) = key.parse::<u64>() {
                    evicted.push(hash);
                    return evicted;
                }
            }
        }

        evicted
    }

    /// 记录键访问（LFU 追踪）
    pub fn access(&self, key: &str) {
        self.lfu_tracker.access(key);
    }

    /// 频率衰减
    pub fn decay(&self) {
        self.lfu_tracker.decay();
    }

    /// 检查是否到达 TTL 淘汰周期
    pub fn should_ttl_evict(&self) -> bool {
        self.ttl_evictor.should_evict()
    }

    /// TTL 淘汰器引用
    pub fn ttl_evictor(&self) -> &TtlEvictor {
        &self.ttl_evictor
    }

    /// 大小限制淘汰器引用
    pub fn size_limit_evictor(&self) -> &SizeLimitEvictor {
        &self.size_limit_evictor
    }

    /// LFU 追踪器引用
    pub fn lfu_tracker(&self) -> &LfuTracker {
        &self.lfu_tracker
    }
}
// ============================================================================
// v10.12.0 T2.3：命中率监控与自动调优
// ============================================================================

use std::collections::VecDeque;

/// 命中率调优配置
#[derive(Debug, Clone)]
pub struct HitRateTuningConfig {
    /// 低命中率阈值（低于此值触发调优）
    pub low_hit_threshold: f64,
    /// 监控窗口（窗口外的数据不计入）
    pub monitor_window: Duration,
    /// 冷启动豁免期（此期间内不调优）
    pub cold_start_exempt_duration: Duration,
}

impl Default for HitRateTuningConfig {
    fn default() -> Self {
        Self {
            low_hit_threshold: 0.5,
            monitor_window: Duration::from_secs(60),
            cold_start_exempt_duration: Duration::from_secs(300),
        }
    }
}

/// 调优动作
#[derive(Debug, Clone, PartialEq)]
pub enum TuningAction {
    /// 扩容缓存
    ExpandCapacity {
        /// 原容量
        from: usize,
        /// 新容量
        to: usize,
    },
    /// 调整淘汰策略
    AdjustStrategy {
        /// 原策略
        from: String,
        /// 新策略
        to: String,
    },
    /// 优化 SQL 归一化
    OptimizeNormalization,
}

/// 命中率分析结果
#[derive(Debug, Clone, PartialEq)]
pub enum HitRateIssue {
    /// 容量不足（命中率低 + 淘汰频繁）
    CapacityInsufficient,
    /// 淘汰过激（命中率低 + 淘汰次数高）
    EvictionTooAggressive,
    /// SQL 未归一化（命中率低 + 淘汰少 → 不同 SQL 模板过多）
    SqlNotNormalized,
    /// 无问题
    NoIssue,
}

/// 命中率分析器
///
/// 分析命中率低的原因：容量不足/淘汰过激/SQL 未归一化。
pub struct HitRateAnalyzer;

impl HitRateAnalyzer {
    /// 创建命中率分析器
    pub fn new() -> Self {
        Self
    }

    /// 分析命中率低的原因
    ///
    /// - `avg_hit_rate`：平均命中率
    /// - `eviction_count`：淘汰次数
    /// - `cache_size`：当前缓存大小
    /// - `unique_sql_count`：不同 SQL 模板数
    pub fn analyze(
        &self,
        avg_hit_rate: f64,
        eviction_count: u64,
        cache_size: usize,
        unique_sql_count: usize,
    ) -> HitRateIssue {
        if avg_hit_rate >= 0.5 {
            return HitRateIssue::NoIssue;
        }

        if unique_sql_count > cache_size * 2 {
            return HitRateIssue::SqlNotNormalized;
        }

        if eviction_count > 100 && eviction_count > cache_size as u64 {
            return HitRateIssue::EvictionTooAggressive;
        }

        HitRateIssue::CapacityInsufficient
    }
}

impl Default for HitRateAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// 命中率调优器
///
/// 根据分析结果生成调优动作。
pub struct HitRateTuner;

impl HitRateTuner {
    /// 创建调优器
    pub fn new() -> Self {
        Self
    }

    /// 生成调优动作
    pub fn tune(&self, issue: &HitRateIssue, current_capacity: usize) -> Option<TuningAction> {
        match issue {
            HitRateIssue::CapacityInsufficient => {
                let new_capacity = current_capacity * 2;
                Some(TuningAction::ExpandCapacity {
                    from: current_capacity,
                    to: new_capacity,
                })
            }
            HitRateIssue::EvictionTooAggressive => Some(TuningAction::AdjustStrategy {
                from: "LFU".to_string(),
                to: "LRU".to_string(),
            }),
            HitRateIssue::SqlNotNormalized => Some(TuningAction::OptimizeNormalization),
            HitRateIssue::NoIssue => None,
        }
    }
}

impl Default for HitRateTuner {
    fn default() -> Self {
        Self::new()
    }
}

/// 命中率监控器
///
/// 命中率监控闭环：命中率持续低于阈值 → 自动分析原因 → 自动调整参数。
/// 冷启动期豁免（命中率自然低）。
pub struct HitRateMonitor {
    config: HitRateTuningConfig,
    hit_rate_history: parking_lot::Mutex<VecDeque<(Instant, f64)>>,
    cold_start_end: parking_lot::Mutex<Option<Instant>>,
    analyzer: HitRateAnalyzer,
    tuner: HitRateTuner,
}

impl HitRateMonitor {
    /// 创建命中率监控器
    pub fn new(config: HitRateTuningConfig) -> Self {
        let cold_start_end = Instant::now() + config.cold_start_exempt_duration;
        Self {
            config,
            hit_rate_history: parking_lot::Mutex::new(VecDeque::new()),
            cold_start_end: parking_lot::Mutex::new(Some(cold_start_end)),
            analyzer: HitRateAnalyzer::new(),
            tuner: HitRateTuner::new(),
        }
    }

    /// 记算当前命中率
    pub fn current_hit_rate(&self) -> f64 {
        let history = self.hit_rate_history.lock();
        if history.is_empty() {
            return 0.0;
        }
        let sum: f64 = history.iter().map(|(_, r)| r).sum();
        sum / history.len() as f64
    }

    /// 记录命中率
    pub fn record_hit_rate(&self, rate: f64) {
        let now = Instant::now();
        let mut history = self.hit_rate_history.lock();
        history.push_back((now, rate.clamp(0.0, 1.0)));

        let cutoff = now - self.config.monitor_window;
        while let Some(&(t, _)) = history.front() {
            if t < cutoff {
                history.pop_front();
            } else {
                break;
            }
        }
    }

    /// 检查并调优
    ///
    /// 返回调优动作（如果有）。冷启动期内不调优。
    pub fn check_and_tune(
        &self,
        eviction_count: u64,
        cache_size: usize,
        unique_sql_count: usize,
    ) -> Option<TuningAction> {
        let cold_start = self.cold_start_end.lock();
        if let Some(end) = *cold_start {
            if Instant::now() < end {
                return None;
            }
        }
        drop(cold_start);

        let avg_hit_rate = self.current_hit_rate();
        if avg_hit_rate >= self.config.low_hit_threshold {
            return None;
        }

        let issue =
            self.analyzer
                .analyze(avg_hit_rate, eviction_count, cache_size, unique_sql_count);
        self.tuner.tune(&issue, cache_size)
    }

    /// 是否在冷启动期
    pub fn is_cold_start(&self) -> bool {
        let cold_start = self.cold_start_end.lock();
        if let Some(end) = *cold_start {
            Instant::now() < end
        } else {
            false
        }
    }

    /// 历史记录数
    pub fn history_len(&self) -> usize {
        self.hit_rate_history.lock().len()
    }

    /// 配置引用
    pub fn config(&self) -> &HitRateTuningConfig {
        &self.config
    }

    /// 强制结束冷启动期（用于测试）
    pub fn end_cold_start(&self) {
        *self.cold_start_end.lock() = None;
    }
}
// ============================================================================
// v10.12.0 T2.5：计划缓存预热与冷启动加速
// ============================================================================

/// 热点 SQL 历史数据
///
/// 记录历史热点 SQL 及其访问频率，用于冷启动时预加载计划缓存。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HotSqlHistory {
    /// 热点 SQL 列表（归一化后）
    pub hot_sqls: Vec<String>,
    /// 对应的访问频率
    pub access_freqs: Vec<u64>,
    /// 最后更新时间（Unix 秒）
    pub last_updated: u64,
}

impl HotSqlHistory {
    /// 创建空的历史
    pub fn new() -> Self {
        Self {
            hot_sqls: Vec::new(),
            access_freqs: Vec::new(),
            last_updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// 添加热点 SQL
    pub fn add(&mut self, sql: impl Into<String>, freq: u64) {
        self.hot_sqls.push(sql.into());
        self.access_freqs.push(freq);
    }

    /// 衰减历史数据
    ///
    /// 衰减因子 0.5：访问频率减半。
    pub fn decay(&self, decay_factor: f64) -> HotSqlHistory {
        HotSqlHistory {
            hot_sqls: self.hot_sqls.clone(),
            access_freqs: self
                .access_freqs
                .iter()
                .map(|&f| (f as f64 * decay_factor) as u64)
                .collect(),
            last_updated: self.last_updated,
        }
    }

    /// 合并另一份历史（去重 + 频率累加）
    pub fn merge(&self, other: &HotSqlHistory) -> HotSqlHistory {
        let mut sqls = self.hot_sqls.clone();
        let mut freqs = self.access_freqs.clone();

        for (i, sql) in other.hot_sqls.iter().enumerate() {
            if let Some(pos) = sqls.iter().position(|s| s == sql) {
                freqs[pos] += other.access_freqs[i];
            } else {
                sqls.push(sql.clone());
                freqs.push(other.access_freqs[i]);
            }
        }

        HotSqlHistory {
            hot_sqls: sqls,
            access_freqs: freqs,
            last_updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// 过滤低频 SQL
    pub fn filter_low_freq(&self, min_freq: u64) -> HotSqlHistory {
        let mut sqls = Vec::new();
        let mut freqs = Vec::new();
        for (i, sql) in self.hot_sqls.iter().enumerate() {
            if self.access_freqs[i] >= min_freq {
                sqls.push(sql.clone());
                freqs.push(self.access_freqs[i]);
            }
        }
        HotSqlHistory {
            hot_sqls: sqls,
            access_freqs: freqs,
            last_updated: self.last_updated,
        }
    }

    /// TopK 选取（按访问频率降序取前 K）
    pub fn topk(&self, k: usize) -> Vec<(String, u64)> {
        let mut indices: Vec<usize> = (0..self.hot_sqls.len()).collect();
        indices.sort_by(|&a, &b| self.access_freqs[b].cmp(&self.access_freqs[a]));
        indices.truncate(k);
        indices
            .into_iter()
            .map(|i| (self.hot_sqls[i].clone(), self.access_freqs[i]))
            .collect()
    }

    /// 条目数
    pub fn len(&self) -> usize {
        self.hot_sqls.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.hot_sqls.is_empty()
    }
}

impl Default for HotSqlHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// 计划缓存预热配置
#[derive(Debug, Clone)]
pub struct PlanPrewarmConfig {
    /// TopK 热点 SQL 选取数
    pub topk: usize,
    /// 衰减因子（0.0~1.0）
    pub decay_factor: f64,
    /// 历史数据文件路径
    pub history_path: std::path::PathBuf,
}

impl Default for PlanPrewarmConfig {
    fn default() -> Self {
        Self {
            topk: 100,
            decay_factor: 0.5,
            history_path: std::path::PathBuf::from("hot_sql_history.json"),
        }
    }
}

/// 计划缓存预热驱动器
///
/// 冷启动时按历史热点 SQL 指纹预加载计划缓存，
/// 避免冷启动期解析优化开销突增。
/// 预热失败跳过不影响启动。
pub struct PlanCachePrewarm {
    config: PlanPrewarmConfig,
    /// 已加载的历史
    history: std::sync::Mutex<Option<HotSqlHistory>>,
    /// 预热次数统计
    prewarm_count: AtomicU64,
}

impl std::fmt::Debug for PlanCachePrewarm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanCachePrewarm")
            .field("config", &self.config)
            .field("prewarm_count", &self.prewarm_count.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl PlanCachePrewarm {
    /// 创建计划缓存预热驱动器
    pub fn new(config: PlanPrewarmConfig) -> Self {
        Self {
            config,
            history: std::sync::Mutex::new(None),
            prewarm_count: AtomicU64::new(0),
        }
    }

    /// 加载热点 SQL 历史
    ///
    /// 文件不存在或损坏时返回 None。
    pub fn load_hot_sql_history(&self) -> Option<HotSqlHistory> {
        if !self.config.history_path.exists() {
            return None;
        }

        match std::fs::read_to_string(&self.config.history_path) {
            Ok(content) => match serde_json::from_str::<HotSqlHistory>(&content) {
                Ok(history) => {
                    *self.history.lock().unwrap() = Some(history.clone());
                    Some(history)
                }
                Err(e) => {
                    tracing::warn!(
                        path = ?self.config.history_path,
                        error = %e,
                        "热点 SQL 历史文件损坏，跳过预热"
                    );
                    None
                }
            },
            Err(e) => {
                tracing::warn!(
                    path = ?self.config.history_path,
                    error = %e,
                    "热点 SQL 历史文件读取失败，跳过预热"
                );
                None
            }
        }
    }

    /// 预热计划缓存
    ///
    /// 按历史热点 SQL 的 TopK 预加载计划缓存。
    /// 返回预加载的条目数。
    pub fn prewarm_plan_cache(&self, cache: &PlanCache) -> usize {
        self.prewarm_count.fetch_add(1, Ordering::Relaxed);

        let history = match self.load_hot_sql_history() {
            Some(h) => h,
            None => return 0,
        };

        let topk = history.topk(self.config.topk);
        let mut loaded = 0;

        for (sql, _freq) in &topk {
            if cache.get_or_parse(sql).is_ok() {
                loaded += 1;
            }
        }

        loaded
    }

    /// 保存热点 SQL 历史
    pub fn save_hot_sql_history(&self, history: &HotSqlHistory) -> Result<(), String> {
        if let Some(parent) = self.config.history_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| format!("创建历史目录失败: {e}"))?;
            }
        }

        let content =
            serde_json::to_string(history).map_err(|e| format!("序列化历史数据失败: {e}"))?;

        std::fs::write(&self.config.history_path, content)
            .map_err(|e| format!("写入历史文件失败: {e}"))?;

        *self.history.lock().unwrap() = Some(history.clone());
        Ok(())
    }

    /// 配置引用
    pub fn config(&self) -> &PlanPrewarmConfig {
        &self.config
    }

    /// 预热次数
    pub fn prewarm_count(&self) -> u64 {
        self.prewarm_count.load(Ordering::Relaxed)
    }

    /// 获取已加载的历史
    pub fn cached_history(&self) -> Option<HotSqlHistory> {
        self.history.lock().unwrap().clone()
    }
}
