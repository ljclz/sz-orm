//! 慢查询日志分析器
//!
//! 提供 [`SlowQueryAnalyzer`] 解析慢查询日志，提取慢查询样本、
//! 统计分布、识别Top-N慢查询等。

use std::collections::HashMap;
use std::fmt;

/// 慢查询日志条目
#[derive(Debug, Clone)]
pub struct SlowQueryEntry {
    /// 时间戳（Unix 毫秒）
    pub timestamp: u64,
    /// SQL 文本
    pub sql: String,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
    /// 锁等待时间（毫秒）
    pub lock_wait_ms: u64,
    /// 返回行数
    pub rows_examined: u64,
    /// 实际返回行数
    pub rows_returned: u64,
    /// 使用的数据库
    pub database: String,
    /// 客户端主机
    pub host: String,
    /// 用户名
    pub user: String,
    /// 查询ID
    pub query_id: u64,
}

impl SlowQueryEntry {
    /// 创建新条目
    #[must_use]
    pub fn new(sql: &str, elapsed_ms: u64, timestamp: u64) -> Self {
        Self {
            timestamp,
            sql: sql.to_string(),
            elapsed_ms,
            lock_wait_ms: 0,
            rows_examined: 0,
            rows_returned: 0,
            database: String::new(),
            host: String::new(),
            user: String::new(),
            query_id: 0,
        }
    }

    /// 设置锁等待时间
    #[must_use]
    pub fn with_lock_wait(mut self, ms: u64) -> Self {
        self.lock_wait_ms = ms;
        self
    }

    /// 设置检查行数
    #[must_use]
    pub fn with_rows_examined(mut self, rows: u64) -> Self {
        self.rows_examined = rows;
        self
    }

    /// 设置返回行数
    #[must_use]
    pub fn with_rows_returned(mut self, rows: u64) -> Self {
        self.rows_returned = rows;
        self
    }

    /// 设置数据库
    #[must_use]
    pub fn with_database(mut self, db: &str) -> Self {
        self.database = db.to_string();
        self
    }

    /// 设置客户端信息
    #[must_use]
    pub fn with_client(mut self, host: &str, user: &str) -> Self {
        self.host = host.to_string();
        self.user = user.to_string();
        self
    }

    /// 设置查询ID
    #[must_use]
    pub fn with_query_id(mut self, id: u64) -> Self {
        self.query_id = id;
        self
    }

    /// 行扫描效率（rows_returned / rows_examined）
    #[must_use]
    pub fn scan_efficiency(&self) -> f64 {
        if self.rows_examined == 0 {
            return 1.0;
        }
        self.rows_returned as f64 / self.rows_examined as f64
    }

    /// 是否为低效扫描
    #[must_use]
    pub fn is_inefficient_scan(&self) -> bool {
        self.rows_examined > 0 && self.scan_efficiency() < 0.01
    }

    /// 是否受锁阻塞
    #[must_use]
    pub fn is_lock_blocked(&self) -> bool {
        self.lock_wait_ms > 0 && self.lock_wait_ms > self.elapsed_ms / 2
    }
}

impl fmt::Display for SlowQueryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SlowQuery(id={}, {}ms, rows={}/{})",
            self.query_id, self.elapsed_ms, self.rows_returned, self.rows_examined
        )
    }
}

/// 慢查询统计
#[derive(Debug, Clone)]
pub struct SlowQueryStats {
    /// 慢查询总数
    pub count: u64,
    /// 总耗时
    pub total_time_ms: u64,
    /// 最大耗时
    pub max_time_ms: u64,
    /// 最小耗时
    pub min_time_ms: u64,
    /// 平均耗时
    pub avg_time_ms: f64,
    /// 中位数耗时
    pub median_time_ms: f64,
    /// P95 耗时
    pub p95_time_ms: f64,
    /// P99 耗时
    pub p99_time_ms: f64,
    /// 总锁等待时间
    pub total_lock_wait_ms: u64,
    /// 总检查行数
    pub total_rows_examined: u64,
    /// 总返回行数
    pub total_rows_returned: u64,
    /// 低效扫描数
    pub inefficient_scan_count: u64,
    /// 锁阻塞数
    pub lock_blocked_count: u64,
}

impl SlowQueryStats {
    /// 从条目列表计算统计
    #[must_use]
    pub fn from_entries(entries: &[SlowQueryEntry]) -> Self {
        if entries.is_empty() {
            return Self {
                count: 0,
                total_time_ms: 0,
                max_time_ms: 0,
                min_time_ms: 0,
                avg_time_ms: 0.0,
                median_time_ms: 0.0,
                p95_time_ms: 0.0,
                p99_time_ms: 0.0,
                total_lock_wait_ms: 0,
                total_rows_examined: 0,
                total_rows_returned: 0,
                inefficient_scan_count: 0,
                lock_blocked_count: 0,
            };
        }
        let count = entries.len() as u64;
        let mut times: Vec<u64> = entries.iter().map(|e| e.elapsed_ms).collect();
        times.sort_unstable();
        let total_time_ms: u64 = times.iter().sum();
        let max_time_ms = *times.last().unwrap();
        let min_time_ms = *times.first().unwrap();
        let avg_time_ms = total_time_ms as f64 / count as f64;
        let median_time_ms = Self::percentile(&times, 50.0);
        let p95_time_ms = Self::percentile(&times, 95.0);
        let p99_time_ms = Self::percentile(&times, 99.0);
        let total_lock_wait_ms: u64 = entries.iter().map(|e| e.lock_wait_ms).sum();
        let total_rows_examined: u64 = entries.iter().map(|e| e.rows_examined).sum();
        let total_rows_returned: u64 = entries.iter().map(|e| e.rows_returned).sum();
        let inefficient_scan_count =
            entries.iter().filter(|e| e.is_inefficient_scan()).count() as u64;
        let lock_blocked_count = entries.iter().filter(|e| e.is_lock_blocked()).count() as u64;
        Self {
            count,
            total_time_ms,
            max_time_ms,
            min_time_ms,
            avg_time_ms,
            median_time_ms,
            p95_time_ms,
            p99_time_ms,
            total_lock_wait_ms,
            total_rows_examined,
            total_rows_returned,
            inefficient_scan_count,
            lock_blocked_count,
        }
    }

    /// 计算百分位数
    fn percentile(sorted: &[u64], p: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)] as f64
    }
}

impl fmt::Display for SlowQueryStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SlowQueryStats(count={}, avg={:.1}ms, p95={:.1}ms, p99={:.1}ms)",
            self.count, self.avg_time_ms, self.p95_time_ms, self.p99_time_ms
        )
    }
}

/// 慢查询分析器
#[derive(Debug, Default)]
pub struct SlowQueryAnalyzer {
    /// 慢查询条目
    entries: Vec<SlowQueryEntry>,
    /// 慢查询阈值（毫秒）
    threshold_ms: u64,
    /// 最大保留条目数
    max_entries: usize,
}

impl SlowQueryAnalyzer {
    /// 创建新分析器
    #[must_use]
    pub fn new(threshold_ms: u64) -> Self {
        Self {
            entries: Vec::new(),
            threshold_ms,
            max_entries: 10_000,
        }
    }

    /// 设置最大保留条目数
    #[must_use]
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// 添加慢查询条目
    pub fn add(&mut self, entry: SlowQueryEntry) {
        if entry.elapsed_ms >= self.threshold_ms {
            self.entries.push(entry);
            if self.entries.len() > self.max_entries {
                self.entries.remove(0);
            }
        }
    }

    /// 批量添加
    pub fn add_all(&mut self, entries: Vec<SlowQueryEntry>) {
        for entry in entries {
            self.add(entry);
        }
    }

    /// 从日志文本解析（简化格式：每行一条 "elapsed_ms|sql"）
    pub fn parse_log(&mut self, log: &str) {
        for line in log.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((time_str, sql)) = line.split_once('|') {
                if let Ok(elapsed) = time_str.trim().parse::<u64>() {
                    self.add(SlowQueryEntry::new(sql.trim(), elapsed, 0));
                }
            }
        }
    }

    /// 获取所有条目
    #[must_use]
    pub fn entries(&self) -> &[SlowQueryEntry] {
        &self.entries
    }

    /// 条目数
    #[must_use]
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// 计算统计
    #[must_use]
    pub fn stats(&self) -> SlowQueryStats {
        SlowQueryStats::from_entries(&self.entries)
    }

    /// 获取Top-N最慢查询
    #[must_use]
    pub fn top_slowest(&self, n: usize) -> Vec<&SlowQueryEntry> {
        let mut entries: Vec<&SlowQueryEntry> = self.entries.iter().collect();
        entries.sort_by(|a, b| b.elapsed_ms.cmp(&a.elapsed_ms));
        entries.into_iter().take(n).collect()
    }

    /// 按数据库分组
    #[must_use]
    pub fn group_by_database(&self) -> HashMap<String, Vec<&SlowQueryEntry>> {
        let mut groups: HashMap<String, Vec<&SlowQueryEntry>> = HashMap::new();
        for entry in &self.entries {
            groups
                .entry(entry.database.clone())
                .or_default()
                .push(entry);
        }
        groups
    }

    /// 按用户分组
    #[must_use]
    pub fn group_by_user(&self) -> HashMap<String, Vec<&SlowQueryEntry>> {
        let mut groups: HashMap<String, Vec<&SlowQueryEntry>> = HashMap::new();
        for entry in &self.entries {
            groups.entry(entry.user.clone()).or_default().push(entry);
        }
        groups
    }

    /// 按SQL指纹分组（简化指纹：取前100字符）
    #[must_use]
    pub fn group_by_fingerprint(&self) -> HashMap<String, Vec<&SlowQueryEntry>> {
        let mut groups: HashMap<String, Vec<&SlowQueryEntry>> = HashMap::new();
        for entry in &self.entries {
            let fp: String = entry.sql.chars().take(100).collect();
            groups.entry(fp).or_default().push(entry);
        }
        groups
    }

    /// 获取低效扫描查询
    #[must_use]
    pub fn inefficient_scans(&self) -> Vec<&SlowQueryEntry> {
        self.entries
            .iter()
            .filter(|e| e.is_inefficient_scan())
            .collect()
    }

    /// 获取锁阻塞查询
    #[must_use]
    pub fn lock_blocked(&self) -> Vec<&SlowQueryEntry> {
        self.entries
            .iter()
            .filter(|e| e.is_lock_blocked())
            .collect()
    }

    /// 按时间范围过滤
    #[must_use]
    pub fn in_time_range(&self, start: u64, end: u64) -> Vec<&SlowQueryEntry> {
        self.entries
            .iter()
            .filter(|e| (start..=end).contains(&e.timestamp))
            .collect()
    }

    /// 清空
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 阈值
    #[must_use]
    pub fn threshold(&self) -> u64 {
        self.threshold_ms
    }
}

impl fmt::Display for SlowQueryAnalyzer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SlowQueryAnalyzer(count={}, threshold={}ms)",
            self.count(),
            self.threshold_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slow_query_entry_new() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000);
        assert_eq!(e.sql, "SELECT 1");
        assert_eq!(e.elapsed_ms, 100);
        assert_eq!(e.timestamp, 1000);
    }

    #[test]
    fn test_with_lock_wait() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_lock_wait(50);
        assert_eq!(e.lock_wait_ms, 50);
    }

    #[test]
    fn test_with_rows_examined() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_rows_examined(1000);
        assert_eq!(e.rows_examined, 1000);
    }

    #[test]
    fn test_with_rows_returned() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_rows_returned(10);
        assert_eq!(e.rows_returned, 10);
    }

    #[test]
    fn test_with_database() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_database("mydb");
        assert_eq!(e.database, "mydb");
    }

    #[test]
    fn test_with_client() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_client("host1", "user1");
        assert_eq!(e.host, "host1");
        assert_eq!(e.user, "user1");
    }

    #[test]
    fn test_with_query_id() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_query_id(42);
        assert_eq!(e.query_id, 42);
    }

    #[test]
    fn test_scan_efficiency_no_examined() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000);
        assert!((e.scan_efficiency() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_scan_efficiency_with_data() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000)
            .with_rows_examined(1000)
            .with_rows_returned(10);
        assert!((e.scan_efficiency() - 0.01).abs() < 1e-10);
    }

    #[test]
    fn test_is_inefficient_scan() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000)
            .with_rows_examined(10000)
            .with_rows_returned(10);
        assert!(e.is_inefficient_scan());
    }

    #[test]
    fn test_is_not_inefficient_scan() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000)
            .with_rows_examined(100)
            .with_rows_returned(50);
        assert!(!e.is_inefficient_scan());
    }

    #[test]
    fn test_is_lock_blocked() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_lock_wait(80);
        assert!(e.is_lock_blocked());
    }

    #[test]
    fn test_is_not_lock_blocked() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_lock_wait(10);
        assert!(!e.is_lock_blocked());
    }

    #[test]
    fn test_entry_display() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000).with_query_id(5);
        let s = format!("{}", e);
        assert!(s.contains("SlowQuery"));
    }

    #[test]
    fn test_stats_empty() {
        let stats = SlowQueryStats::from_entries(&[]);
        assert_eq!(stats.count, 0);
    }

    #[test]
    fn test_stats_single() {
        let e = SlowQueryEntry::new("SELECT 1", 100, 1000);
        let stats = SlowQueryStats::from_entries(&[e]);
        assert_eq!(stats.count, 1);
        assert_eq!(stats.total_time_ms, 100);
        assert_eq!(stats.max_time_ms, 100);
        assert_eq!(stats.min_time_ms, 100);
    }

    #[test]
    fn test_stats_multiple() {
        let entries = vec![
            SlowQueryEntry::new("q1", 100, 1000),
            SlowQueryEntry::new("q2", 200, 2000),
            SlowQueryEntry::new("q3", 300, 3000),
        ];
        let stats = SlowQueryStats::from_entries(&entries);
        assert_eq!(stats.count, 3);
        assert_eq!(stats.total_time_ms, 600);
        assert_eq!(stats.max_time_ms, 300);
        assert_eq!(stats.min_time_ms, 100);
        assert!((stats.avg_time_ms - 200.0).abs() < 1e-10);
    }

    #[test]
    fn test_stats_with_inefficient() {
        let entries = vec![
            SlowQueryEntry::new("q1", 100, 1000)
                .with_rows_examined(10000)
                .with_rows_returned(10),
            SlowQueryEntry::new("q2", 200, 2000),
        ];
        let stats = SlowQueryStats::from_entries(&entries);
        assert_eq!(stats.inefficient_scan_count, 1);
    }

    #[test]
    fn test_stats_display() {
        let stats = SlowQueryStats::from_entries(&[SlowQueryEntry::new("q1", 100, 1000)]);
        let s = format!("{}", stats);
        assert!(s.contains("SlowQueryStats"));
    }

    #[test]
    fn test_analyzer_new() {
        let a = SlowQueryAnalyzer::new(100);
        assert_eq!(a.threshold(), 100);
        assert_eq!(a.count(), 0);
    }

    #[test]
    fn test_analyzer_with_max_entries() {
        let a = SlowQueryAnalyzer::new(100).with_max_entries(50);
        assert_eq!(a.count(), 0);
    }

    #[test]
    fn test_analyzer_add() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new("SELECT 1", 200, 1000));
        assert_eq!(a.count(), 1);
    }

    #[test]
    fn test_analyzer_add_below_threshold() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new("SELECT 1", 50, 1000));
        assert_eq!(a.count(), 0);
    }

    #[test]
    fn test_analyzer_add_all() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add_all(vec![
            SlowQueryEntry::new("q1", 200, 1000),
            SlowQueryEntry::new("q2", 300, 2000),
        ]);
        assert_eq!(a.count(), 2);
    }

    #[test]
    fn test_analyzer_parse_log() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.parse_log("200|SELECT * FROM t\n300|SELECT * FROM u\n# comment\n");
        assert_eq!(a.count(), 2);
    }

    #[test]
    fn test_analyzer_parse_log_empty() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.parse_log("");
        assert_eq!(a.count(), 0);
    }

    #[test]
    fn test_analyzer_parse_log_with_comments() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.parse_log("# comment\n# another\n");
        assert_eq!(a.count(), 0);
    }

    #[test]
    fn test_analyzer_top_slowest() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add_all(vec![
            SlowQueryEntry::new("q1", 200, 1000),
            SlowQueryEntry::new("q2", 500, 2000),
            SlowQueryEntry::new("q3", 300, 3000),
        ]);
        let top = a.top_slowest(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].elapsed_ms, 500);
    }

    #[test]
    fn test_analyzer_group_by_database() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new("q1", 200, 1000).with_database("db1"));
        a.add(SlowQueryEntry::new("q2", 200, 1000).with_database("db2"));
        a.add(SlowQueryEntry::new("q3", 200, 1000).with_database("db1"));
        let groups = a.group_by_database();
        assert_eq!(groups.get("db1").unwrap().len(), 2);
        assert_eq!(groups.get("db2").unwrap().len(), 1);
    }

    #[test]
    fn test_analyzer_group_by_user() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new("q1", 200, 1000).with_client("h1", "u1"));
        a.add(SlowQueryEntry::new("q2", 200, 1000).with_client("h2", "u2"));
        let groups = a.group_by_user();
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_analyzer_group_by_fingerprint() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new(
            "SELECT * FROM t WHERE id = ?",
            200,
            1000,
        ));
        a.add(SlowQueryEntry::new(
            "SELECT * FROM t WHERE id = ?",
            200,
            1000,
        ));
        let groups = a.group_by_fingerprint();
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn test_analyzer_inefficient_scans() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(
            SlowQueryEntry::new("q1", 200, 1000)
                .with_rows_examined(10000)
                .with_rows_returned(10),
        );
        a.add(SlowQueryEntry::new("q2", 200, 1000));
        let inefficient = a.inefficient_scans();
        assert_eq!(inefficient.len(), 1);
    }

    #[test]
    fn test_analyzer_lock_blocked() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new("q1", 200, 1000).with_lock_wait(180));
        a.add(SlowQueryEntry::new("q2", 200, 1000).with_lock_wait(10));
        let blocked = a.lock_blocked();
        assert_eq!(blocked.len(), 1);
    }

    #[test]
    fn test_analyzer_in_time_range() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new("q1", 200, 1000));
        a.add(SlowQueryEntry::new("q2", 200, 2000));
        a.add(SlowQueryEntry::new("q3", 200, 3000));
        let filtered = a.in_time_range(1500, 3500);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_analyzer_clear() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new("q1", 200, 1000));
        a.clear();
        assert_eq!(a.count(), 0);
    }

    #[test]
    fn test_analyzer_stats() {
        let mut a = SlowQueryAnalyzer::new(100);
        a.add(SlowQueryEntry::new("q1", 200, 1000));
        a.add(SlowQueryEntry::new("q2", 300, 2000));
        let stats = a.stats();
        assert_eq!(stats.count, 2);
    }

    #[test]
    fn test_analyzer_display() {
        let a = SlowQueryAnalyzer::new(100);
        let s = format!("{}", a);
        assert!(s.contains("SlowQueryAnalyzer"));
    }

    #[test]
    fn test_analyzer_max_entries_eviction() {
        let mut a = SlowQueryAnalyzer::new(100).with_max_entries(2);
        a.add(SlowQueryEntry::new("q1", 200, 1000));
        a.add(SlowQueryEntry::new("q2", 200, 1000));
        a.add(SlowQueryEntry::new("q3", 200, 1000));
        assert_eq!(a.count(), 2);
    }

    #[test]
    fn test_percentile_calculation() {
        let times = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        let p50 = SlowQueryStats::percentile(&times, 50.0);
        assert!((p50 - 50.0).abs() < 1e-10 || (p50 - 60.0).abs() < 1e-10);
    }

    #[test]
    fn test_percentile_empty() {
        let times: Vec<u64> = vec![];
        assert!((SlowQueryStats::percentile(&times, 50.0) - 0.0).abs() < 1e-10);
    }
}
