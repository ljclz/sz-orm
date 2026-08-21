//! 并行统计（Parallel Stats）
//!
//! 收集和报告并行查询的运行时统计信息。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// 并行查询统计
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ParallelStats {
    pub total_queries: u64,
    pub successful_queries: u64,
    pub failed_queries: u64,
    pub timed_out_queries: u64,
    pub total_tasks: u64,
    pub completed_tasks: u64,
    pub total_latency_ms: u64,
    pub max_latency_ms: u64,
    pub min_latency_ms: u64,
    pub total_rows: u64,
}

impl ParallelStats {
    pub fn success_rate(&self) -> f64 {
        if self.total_queries > 0 {
            self.successful_queries as f64 / self.total_queries as f64
        } else {
            0.0
        }
    }

    pub fn avg_latency_ms(&self) -> u64 {
        self.total_latency_ms
            .checked_div(self.total_queries)
            .unwrap_or(0)
    }

    pub fn task_completion_rate(&self) -> f64 {
        if self.total_tasks > 0 {
            self.completed_tasks as f64 / self.total_tasks as f64
        } else {
            0.0
        }
    }

    pub fn failure_rate(&self) -> f64 {
        if self.total_queries > 0 {
            self.failed_queries as f64 / self.total_queries as f64
        } else {
            0.0
        }
    }

    pub fn timeout_rate(&self) -> f64 {
        if self.total_queries > 0 {
            self.timed_out_queries as f64 / self.total_queries as f64
        } else {
            0.0
        }
    }
}

/// 按查询键统计
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct QueryKeyStats {
    pub count: u64,
    pub successes: u64,
    pub failures: u64,
    pub total_latency_ms: u64,
    pub total_rows: u64,
}

impl QueryKeyStats {
    pub fn success_rate(&self) -> f64 {
        if self.count > 0 {
            self.successes as f64 / self.count as f64
        } else {
            0.0
        }
    }

    pub fn avg_latency_ms(&self) -> u64 {
        self.total_latency_ms.checked_div(self.count).unwrap_or(0)
    }
}

/// 并行统计收集器
pub struct ParallelStatsCollector {
    overall: RwLock<ParallelStats>,
    per_key: RwLock<HashMap<String, QueryKeyStats>>,
    start_time: Instant,
    query_count: AtomicU64,
}

impl ParallelStatsCollector {
    pub fn new() -> Self {
        Self {
            overall: RwLock::new(ParallelStats::default()),
            per_key: RwLock::new(HashMap::new()),
            start_time: Instant::now(),
            query_count: AtomicU64::new(0),
        }
    }

    pub fn record_query(
        &self,
        query_key: Option<&str>,
        success: bool,
        timed_out: bool,
        latency_ms: u64,
        rows: u64,
    ) {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut overall) = self.overall.write() {
            overall.total_queries += 1;
            if success {
                overall.successful_queries += 1;
            } else if timed_out {
                overall.timed_out_queries += 1;
            } else {
                overall.failed_queries += 1;
            }
            overall.total_latency_ms += latency_ms;
            overall.max_latency_ms = overall.max_latency_ms.max(latency_ms);
            if overall.min_latency_ms == 0 || latency_ms < overall.min_latency_ms {
                overall.min_latency_ms = latency_ms;
            }
            overall.total_rows += rows;
        }
        if let Some(key) = query_key {
            if let Ok(mut per_key) = self.per_key.write() {
                let stats = per_key
                    .entry(key.to_string())
                    .or_insert_with(QueryKeyStats::default);
                stats.count += 1;
                if success {
                    stats.successes += 1;
                } else {
                    stats.failures += 1;
                }
                stats.total_latency_ms += latency_ms;
                stats.total_rows += rows;
            }
        }
    }

    pub fn record_task(&self, completed: bool) {
        if let Ok(mut overall) = self.overall.write() {
            overall.total_tasks += 1;
            if completed {
                overall.completed_tasks += 1;
            }
        }
    }

    pub fn overall(&self) -> ParallelStats {
        self.overall.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn per_key(&self) -> HashMap<String, QueryKeyStats> {
        self.per_key.read().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn key_stats(&self, key: &str) -> Option<QueryKeyStats> {
        self.per_key.read().ok().and_then(|m| m.get(key).cloned())
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn query_count(&self) -> u64 {
        self.query_count.load(Ordering::Relaxed)
    }

    pub fn key_count(&self) -> usize {
        self.per_key.read().map(|m| m.len()).unwrap_or(0)
    }

    pub fn reset(&self) {
        if let Ok(mut overall) = self.overall.write() {
            *overall = ParallelStats::default();
        }
        if let Ok(mut per_key) = self.per_key.write() {
            per_key.clear();
        }
        self.query_count.store(0, Ordering::Relaxed);
    }

    pub fn to_json(&self) -> serde_json::Value {
        let overall = self.overall();
        let per_key = self.per_key();
        serde_json::json!({
            "overall": overall,
            "per_key": per_key,
            "uptime_secs": self.uptime().as_secs(),
            "query_count": self.query_count(),
        })
    }

    pub fn to_report(&self) -> ParallelReport {
        ParallelReport {
            overall: self.overall(),
            per_key: self.per_key(),
            uptime_secs: self.uptime().as_secs(),
            generated_at_ms: now_ms(),
        }
    }
}

impl Default for ParallelStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// 并行报告
#[derive(Debug, Clone, serde::Serialize)]
pub struct ParallelReport {
    pub overall: ParallelStats,
    pub per_key: HashMap<String, QueryKeyStats>,
    pub uptime_secs: u64,
    pub generated_at_ms: i64,
}

impl ParallelReport {
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn to_summary(&self) -> String {
        format!(
            "Parallel Report: {} queries, {:.1}% success, avg {}ms, {} keys",
            self.overall.total_queries,
            self.overall.success_rate() * 100.0,
            self.overall.avg_latency_ms(),
            self.per_key.len()
        )
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_stats_success_rate() {
        let stats = ParallelStats {
            total_queries: 100,
            successful_queries: 95,
            ..Default::default()
        };
        assert!((stats.success_rate() - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_parallel_stats_avg_latency() {
        let stats = ParallelStats {
            total_queries: 10,
            total_latency_ms: 500,
            ..Default::default()
        };
        assert_eq!(stats.avg_latency_ms(), 50);
    }

    #[test]
    fn test_parallel_stats_task_completion_rate() {
        let stats = ParallelStats {
            total_tasks: 100,
            completed_tasks: 80,
            ..Default::default()
        };
        assert!((stats.task_completion_rate() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_parallel_stats_failure_rate() {
        let stats = ParallelStats {
            total_queries: 100,
            failed_queries: 5,
            ..Default::default()
        };
        assert!((stats.failure_rate() - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_parallel_stats_timeout_rate() {
        let stats = ParallelStats {
            total_queries: 100,
            timed_out_queries: 3,
            ..Default::default()
        };
        assert!((stats.timeout_rate() - 0.03).abs() < 0.001);
    }

    #[test]
    fn test_collector_new() {
        let collector = ParallelStatsCollector::new();
        assert_eq!(collector.query_count(), 0);
    }

    #[test]
    fn test_collector_record_success() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), true, false, 100, 10);
        let overall = collector.overall();
        assert_eq!(overall.total_queries, 1);
        assert_eq!(overall.successful_queries, 1);
    }

    #[test]
    fn test_collector_record_failure() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), false, false, 100, 0);
        let overall = collector.overall();
        assert_eq!(overall.failed_queries, 1);
    }

    #[test]
    fn test_collector_record_timeout() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), false, true, 100, 0);
        let overall = collector.overall();
        assert_eq!(overall.timed_out_queries, 1);
    }

    #[test]
    fn test_collector_record_task() {
        let collector = ParallelStatsCollector::new();
        collector.record_task(true);
        collector.record_task(false);
        collector.record_task(true);
        let overall = collector.overall();
        assert_eq!(overall.total_tasks, 3);
        assert_eq!(overall.completed_tasks, 2);
    }

    #[test]
    fn test_collector_per_key() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), true, false, 100, 10);
        collector.record_query(Some("q1"), true, false, 200, 20);
        collector.record_query(Some("q2"), true, false, 50, 5);
        let per_key = collector.per_key();
        assert_eq!(per_key.len(), 2);
        assert_eq!(per_key.get("q1").unwrap().count, 2);
    }

    #[test]
    fn test_collector_key_stats() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), true, false, 100, 10);
        let stats = collector.key_stats("q1").unwrap();
        assert_eq!(stats.count, 1);
        assert_eq!(stats.total_rows, 10);
    }

    #[test]
    fn test_collector_reset() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), true, false, 100, 10);
        collector.reset();
        assert_eq!(collector.query_count(), 0);
        assert_eq!(collector.overall().total_queries, 0);
    }

    #[test]
    fn test_collector_to_json() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), true, false, 100, 10);
        let json = collector.to_json();
        assert_eq!(json["query_count"], 1);
    }

    #[test]
    fn test_collector_to_report() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), true, false, 100, 10);
        let report = collector.to_report();
        assert_eq!(report.overall.total_queries, 1);
    }

    #[test]
    fn test_report_to_summary() {
        let report = ParallelReport {
            overall: ParallelStats {
                total_queries: 100,
                successful_queries: 95,
                total_latency_ms: 10000,
                ..Default::default()
            },
            per_key: HashMap::new(),
            uptime_secs: 60,
            generated_at_ms: 0,
        };
        let summary = report.to_summary();
        assert!(summary.contains("100 queries"));
    }

    #[test]
    fn test_report_to_json_string() {
        let report = ParallelReport {
            overall: ParallelStats::default(),
            per_key: HashMap::new(),
            uptime_secs: 0,
            generated_at_ms: 0,
        };
        let json = report.to_json_string();
        assert!(json.contains("overall"));
    }

    #[test]
    fn test_collector_uptime() {
        let collector = ParallelStatsCollector::new();
        std::thread::sleep(Duration::from_millis(10));
        assert!(collector.uptime().as_millis() >= 10);
    }

    #[test]
    fn test_collector_key_count() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(Some("q1"), true, false, 100, 10);
        collector.record_query(Some("q2"), true, false, 100, 10);
        assert_eq!(collector.key_count(), 2);
    }

    #[test]
    fn test_collector_no_key() {
        let collector = ParallelStatsCollector::new();
        collector.record_query(None, true, false, 100, 10);
        assert_eq!(collector.key_count(), 0);
        assert_eq!(collector.overall().total_queries, 1);
    }

    #[test]
    fn test_query_key_stats_success_rate() {
        let stats = QueryKeyStats {
            count: 100,
            successes: 90,
            ..Default::default()
        };
        assert!((stats.success_rate() - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_query_key_stats_avg_latency() {
        let stats = QueryKeyStats {
            count: 5,
            total_latency_ms: 250,
            ..Default::default()
        };
        assert_eq!(stats.avg_latency_ms(), 50);
    }
}
