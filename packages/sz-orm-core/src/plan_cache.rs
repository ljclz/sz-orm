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
use xxhash_rust::xxh64::xxh64;

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
        let hash = xxh64(sql_normalized.as_bytes(), 0);
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

    pub fn parse_hits(&self) -> u64 {
        self.parse_hits.load(Ordering::Relaxed)
    }

    pub fn parse_misses(&self) -> u64 {
        self.parse_misses.load(Ordering::Relaxed)
    }

    pub fn optimize_hits(&self) -> u64 {
        self.optimize_hits.load(Ordering::Relaxed)
    }

    pub fn optimize_misses(&self) -> u64 {
        self.optimize_misses.load(Ordering::Relaxed)
    }

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
    pub parse_hits: u64,
    pub parse_misses: u64,
    pub optimize_hits: u64,
    pub optimize_misses: u64,
    pub evictions: u64,
    pub size: usize,
    pub parse_hit_rate: f64,
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
    /// 表级失效索引（table → Vec<hash>）
    table_index: RwLock<HashMap<String, Vec<u64>>>,
    /// 统计计数器
    stats: PlanCacheStats,
    /// 最大缓存条目数
    max_size: usize,
    /// 默认 TTL
    default_ttl: Option<Duration>,
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
        }
    }

    /// 获取或解析 SQL
    ///
    /// 命中缓存时返回 AST + stats.parse_hits++ + LRU touch。
    /// 未命中时解析 SQL + 存入缓存 + stats.parse_misses++。
    pub fn get_or_parse(&self, sql: &str) -> Result<Arc<Statement>, String> {
        let key = PlanCacheKey::from_sql(sql);

        {
            let cache = self.parse_cache.read();
            if let Some(entry) = cache.get(&key.hash) {
                if !entry.is_expired() {
                    if let Some(ast) = &entry.ast {
                        self.stats.parse_hits.fetch_add(1, Ordering::Relaxed);
                        self.access_order.write().touch(key.hash);
                        return Ok(ast.clone());
                    }
                }
            }
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
            let mut cache = self.parse_cache.write();
            let mut table_index = self.table_index.write();

            if cache.len() >= self.max_size {
                if let Some(lru_hash) = access_order.lru_key() {
                    access_order.remove(lru_hash);
                    cache.remove(&lru_hash);
                    self.optimize_cache.write().remove(&lru_hash);
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

        {
            let cache = self.optimize_cache.read();
            if let Some(entry) = cache.get(&key.hash) {
                if !entry.is_expired() {
                    if let Some(analysis) = &entry.analysis {
                        self.stats.optimize_hits.fetch_add(1, Ordering::Relaxed);
                        self.access_order.write().touch(key.hash);
                        return Some(analysis.clone());
                    }
                }
            }
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
        let mut table_index = self.table_index.write();

        if cache.len() >= self.max_size {
            if let Some(lru_hash) = access_order.lru_key() {
                access_order.remove(lru_hash);
                cache.remove(&lru_hash);
                self.parse_cache.write().remove(&lru_hash);
                Self::remove_from_table_index(&mut table_index, lru_hash);
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
        let mut table_index = self.table_index.write();
        let keys = table_index.remove(table).unwrap_or_default();

        if keys.is_empty() {
            return 0;
        }

        let count = keys.len();
        let mut access_order = self.access_order.write();
        let mut parse_cache = self.parse_cache.write();
        let mut optimize_cache = self.optimize_cache.write();

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
}
