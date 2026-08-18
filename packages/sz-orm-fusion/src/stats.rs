//! 融合统计报告（Fusion Stats）
//!
//! 收集和报告融合查询的运行时统计信息。
//! 包括查询分布、缓存命中率、数据源使用情况等。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// 融合统计信息
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FusionStats {
    pub total_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub primary_queries: u64,
    pub search_pushdowns: u64,
    pub degraded_queries: u64,
    pub failed_queries: u64,
    pub total_latency_ms: u64,
    pub max_latency_ms: u64,
    pub min_latency_ms: u64,
}

impl FusionStats {
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total > 0 {
            self.cache_hits as f64 / total as f64
        } else {
            0.0
        }
    }

    pub fn avg_latency_ms(&self) -> u64 {
        if self.total_queries > 0 {
            self.total_latency_ms / self.total_queries
        } else {
            0
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_queries > 0 {
            (self.total_queries - self.failed_queries) as f64 / self.total_queries as f64
        } else {
            0.0
        }
    }

    pub fn degradation_rate(&self) -> f64 {
        if self.total_queries > 0 {
            self.degraded_queries as f64 / self.total_queries as f64
        } else {
            0.0
        }
    }
}

/// 按数据源统计
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SourceStats {
    pub queries: u64,
    pub errors: u64,
    pub total_latency_ms: u64,
    pub rows_returned: u64,
}

impl SourceStats {
    pub fn avg_latency_ms(&self) -> u64 {
        if self.queries > 0 {
            self.total_latency_ms / self.queries
        } else {
            0
        }
    }

    pub fn error_rate(&self) -> f64 {
        if self.queries > 0 {
            self.errors as f64 / self.queries as f64
        } else {
            0.0
        }
    }
}

/// 按表统计
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TableStats {
    pub queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub avg_latency_ms: u64,
}

/// 融合统计收集器
pub struct FusionStatsCollector {
    overall: RwLock<FusionStats>,
    per_source: RwLock<HashMap<String, SourceStats>>,
    per_table: RwLock<HashMap<String, TableStats>>,
    start_time: Instant,
    query_count: AtomicU64,
}

impl FusionStatsCollector {
    pub fn new() -> Self {
        Self {
            overall: RwLock::new(FusionStats::default()),
            per_source: RwLock::new(HashMap::new()),
            per_table: RwLock::new(HashMap::new()),
            start_time: Instant::now(),
            query_count: AtomicU64::new(0),
        }
    }

    pub fn record_query(
        &self,
        table: &str,
        sources: &[String],
        from_cache: bool,
        degraded: bool,
        latency_ms: u64,
        rows: u64,
        failed: bool,
    ) {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut overall) = self.overall.write() {
            overall.total_queries += 1;
            if from_cache {
                overall.cache_hits += 1;
            } else {
                overall.cache_misses += 1;
            }
            if degraded {
                overall.degraded_queries += 1;
            }
            if failed {
                overall.failed_queries += 1;
            }
            if sources.iter().any(|s| s == "primary") {
                overall.primary_queries += 1;
            }
            if sources.iter().any(|s| s == "search") {
                overall.search_pushdowns += 1;
            }
            overall.total_latency_ms += latency_ms;
            overall.max_latency_ms = overall.max_latency_ms.max(latency_ms);
            if overall.min_latency_ms == 0 || latency_ms < overall.min_latency_ms {
                overall.min_latency_ms = latency_ms;
            }
        }
        if let Ok(mut per_source) = self.per_source.write() {
            for source in sources {
                let stats = per_source
                    .entry(source.clone())
                    .or_insert_with(SourceStats::default);
                stats.queries += 1;
                stats.total_latency_ms += latency_ms;
                stats.rows_returned += rows;
                if failed {
                    stats.errors += 1;
                }
            }
        }
        if let Ok(mut per_table) = self.per_table.write() {
            let stats = per_table
                .entry(table.to_string())
                .or_insert_with(TableStats::default);
            stats.queries += 1;
            if from_cache {
                stats.cache_hits += 1;
            } else {
                stats.cache_misses += 1;
            }
            stats.avg_latency_ms = if stats.queries > 0 {
                (stats.avg_latency_ms * (stats.queries - 1) + latency_ms) / stats.queries
            } else {
                latency_ms
            };
        }
    }

    pub fn overall(&self) -> FusionStats {
        self.overall.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn per_source(&self) -> HashMap<String, SourceStats> {
        self.per_source
            .read()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    pub fn per_table(&self) -> HashMap<String, TableStats> {
        self.per_table.read().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn source_stats(&self, source: &str) -> Option<SourceStats> {
        self.per_source
            .read()
            .ok()
            .and_then(|m| m.get(source).cloned())
    }

    pub fn table_stats(&self, table: &str) -> Option<TableStats> {
        self.per_table
            .read()
            .ok()
            .and_then(|m| m.get(table).cloned())
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn query_count(&self) -> u64 {
        self.query_count.load(Ordering::Relaxed)
    }

    pub fn source_count(&self) -> usize {
        self.per_source.read().map(|m| m.len()).unwrap_or(0)
    }

    pub fn table_count(&self) -> usize {
        self.per_table.read().map(|m| m.len()).unwrap_or(0)
    }

    pub fn reset(&self) {
        if let Ok(mut overall) = self.overall.write() {
            *overall = FusionStats::default();
        }
        if let Ok(mut per_source) = self.per_source.write() {
            per_source.clear();
        }
        if let Ok(mut per_table) = self.per_table.write() {
            per_table.clear();
        }
        self.query_count.store(0, Ordering::Relaxed);
    }

    pub fn to_json(&self) -> serde_json::Value {
        let overall = self.overall();
        let per_source = self.per_source();
        let per_table = self.per_table();
        serde_json::json!({
            "overall": overall,
            "per_source": per_source,
            "per_table": per_table,
            "uptime_secs": self.uptime().as_secs(),
            "query_count": self.query_count(),
        })
    }

    pub fn to_report(&self) -> FusionReport {
        FusionReport {
            overall: self.overall(),
            per_source: self.per_source(),
            per_table: self.per_table(),
            uptime_secs: self.uptime().as_secs(),
            generated_at_ms: now_ms(),
        }
    }
}

impl Default for FusionStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// 融合报告
#[derive(Debug, Clone, serde::Serialize)]
pub struct FusionReport {
    pub overall: FusionStats,
    pub per_source: HashMap<String, SourceStats>,
    pub per_table: HashMap<String, TableStats>,
    pub uptime_secs: u64,
    pub generated_at_ms: i64,
}

impl FusionReport {
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn to_summary(&self) -> String {
        format!(
            "Fusion Report: {} queries, {:.1}% cache hit, {:.1}% success, avg {}ms latency",
            self.overall.total_queries,
            self.overall.cache_hit_rate() * 100.0,
            self.overall.success_rate() * 100.0,
            self.overall.avg_latency_ms()
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
    fn test_fusion_stats_cache_hit_rate() {
        let stats = FusionStats {
            cache_hits: 80,
            cache_misses: 20,
            ..Default::default()
        };
        assert!((stats.cache_hit_rate() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_fusion_stats_cache_hit_rate_empty() {
        let stats = FusionStats::default();
        assert_eq!(stats.cache_hit_rate(), 0.0);
    }

    #[test]
    fn test_fusion_stats_avg_latency() {
        let stats = FusionStats {
            total_queries: 10,
            total_latency_ms: 1000,
            ..Default::default()
        };
        assert_eq!(stats.avg_latency_ms(), 100);
    }

    #[test]
    fn test_fusion_stats_success_rate() {
        let stats = FusionStats {
            total_queries: 100,
            failed_queries: 5,
            ..Default::default()
        };
        assert!((stats.success_rate() - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_fusion_stats_degradation_rate() {
        let stats = FusionStats {
            total_queries: 100,
            degraded_queries: 10,
            ..Default::default()
        };
        assert!((stats.degradation_rate() - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_source_stats_avg_latency() {
        let stats = SourceStats {
            queries: 5,
            total_latency_ms: 250,
            ..Default::default()
        };
        assert_eq!(stats.avg_latency_ms(), 50);
    }

    #[test]
    fn test_source_stats_error_rate() {
        let stats = SourceStats {
            queries: 100,
            errors: 10,
            ..Default::default()
        };
        assert!((stats.error_rate() - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_collector_new() {
        let collector = FusionStatsCollector::new();
        assert_eq!(collector.query_count(), 0);
        assert_eq!(collector.source_count(), 0);
    }

    #[test]
    fn test_collector_record_query() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        let overall = collector.overall();
        assert_eq!(overall.total_queries, 1);
        assert_eq!(overall.primary_queries, 1);
    }

    #[test]
    fn test_collector_cache_hit() {
        let collector = FusionStatsCollector::new();
        collector.record_query("users", &["cache".to_string()], true, false, 10, 5, false);
        let overall = collector.overall();
        assert_eq!(overall.cache_hits, 1);
        assert!((overall.cache_hit_rate() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_collector_degraded() {
        let collector = FusionStatsCollector::new();
        collector.record_query("users", &["cache".to_string()], true, true, 10, 5, false);
        let overall = collector.overall();
        assert_eq!(overall.degraded_queries, 1);
    }

    #[test]
    fn test_collector_failed() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string()],
            false,
            false,
            100,
            0,
            true,
        );
        let overall = collector.overall();
        assert_eq!(overall.failed_queries, 1);
    }

    #[test]
    fn test_collector_per_source() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string(), "cache".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        let per_source = collector.per_source();
        assert_eq!(per_source.len(), 2);
        assert_eq!(per_source.get("primary").unwrap().queries, 1);
    }

    #[test]
    fn test_collector_per_table() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        collector.record_query(
            "orders",
            &["primary".to_string()],
            false,
            false,
            200,
            20,
            false,
        );
        let per_table = collector.per_table();
        assert_eq!(per_table.len(), 2);
    }

    #[test]
    fn test_collector_source_stats() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        let stats = collector.source_stats("primary").unwrap();
        assert_eq!(stats.queries, 1);
        assert_eq!(stats.rows_returned, 10);
    }

    #[test]
    fn test_collector_table_stats() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        let stats = collector.table_stats("users").unwrap();
        assert_eq!(stats.queries, 1);
    }

    #[test]
    fn test_collector_reset() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        collector.reset();
        assert_eq!(collector.query_count(), 0);
        assert_eq!(collector.overall().total_queries, 0);
    }

    #[test]
    fn test_collector_to_json() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        let json = collector.to_json();
        assert_eq!(json["query_count"], 1);
    }

    #[test]
    fn test_collector_to_report() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "users",
            &["primary".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        let report = collector.to_report();
        assert_eq!(report.overall.total_queries, 1);
    }

    #[test]
    fn test_report_to_json_string() {
        let report = FusionReport {
            overall: FusionStats::default(),
            per_source: HashMap::new(),
            per_table: HashMap::new(),
            uptime_secs: 0,
            generated_at_ms: 0,
        };
        let json = report.to_json_string();
        assert!(json.contains("overall"));
    }

    #[test]
    fn test_report_to_summary() {
        let report = FusionReport {
            overall: FusionStats {
                total_queries: 100,
                cache_hits: 80,
                cache_misses: 20,
                failed_queries: 5,
                total_latency_ms: 10000,
                ..Default::default()
            },
            per_source: HashMap::new(),
            per_table: HashMap::new(),
            uptime_secs: 60,
            generated_at_ms: 0,
        };
        let summary = report.to_summary();
        assert!(summary.contains("100 queries"));
    }

    #[test]
    fn test_collector_uptime() {
        let collector = FusionStatsCollector::new();
        std::thread::sleep(Duration::from_millis(10));
        assert!(collector.uptime().as_millis() >= 10);
    }

    #[test]
    fn test_collector_search_pushdown() {
        let collector = FusionStatsCollector::new();
        collector.record_query(
            "products",
            &["search".to_string(), "primary".to_string()],
            false,
            false,
            100,
            10,
            false,
        );
        let overall = collector.overall();
        assert_eq!(overall.search_pushdowns, 1);
    }

    #[test]
    fn test_collector_latency_tracking() {
        let collector = FusionStatsCollector::new();
        collector.record_query("t", &["s".to_string()], false, false, 100, 1, false);
        collector.record_query("t", &["s".to_string()], false, false, 200, 1, false);
        collector.record_query("t", &["s".to_string()], false, false, 50, 1, false);
        let overall = collector.overall();
        assert_eq!(overall.max_latency_ms, 200);
        assert_eq!(overall.min_latency_ms, 50);
        assert_eq!(overall.avg_latency_ms(), 116);
    }
}
