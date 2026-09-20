//! SLI 采集器

use std::sync::Arc;

use parking_lot::RwLock;

use super::types::{RequestResult, SliMetrics};

/// SLI 采集器
pub struct SliCollector {
    results: Arc<RwLock<Vec<RequestResult>>>,
}

impl SliCollector {
    pub fn new() -> Self {
        Self {
            results: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn collect(&self, result: RequestResult) {
        self.results.write().push(result);
    }

    pub fn calculate_sli(&self) -> SliMetrics {
        let results = self.results.read();
        if results.is_empty() {
            return SliMetrics {
                timestamp: 0,
                availability: 0.0,
                latency_ms: 0.0,
                throughput: 0.0,
                correctness: 0.0,
            };
        }

        let total = results.len() as f64;
        let success_count = results.iter().filter(|r| r.success).count() as f64;
        let avg_latency: f64 = results.iter().map(|r| r.latency_ms).sum::<f64>() / total;
        let last_ts = results.last().map(|r| r.timestamp).unwrap_or(0);

        SliMetrics {
            timestamp: last_ts,
            availability: success_count / total,
            latency_ms: avg_latency,
            throughput: total,
            correctness: success_count / total,
        }
    }

    pub fn export_prometheus(&self) -> String {
        let sli = self.calculate_sli();
        format!(
            "sz_orm_sli_availability {}\nsz_orm_sli_latency_ms {}\nsz_orm_sli_throughput {}\nsz_orm_slo_availability {}\n",
            sli.availability, sli.latency_ms, sli.throughput, sli.availability
        )
    }

    pub fn count(&self) -> usize {
        self.results.read().len()
    }
}

impl Default for SliCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_and_calculate() {
        let collector = SliCollector::new();
        collector.collect(RequestResult {
            success: true,
            latency_ms: 10.0,
            timestamp: 1,
        });
        collector.collect(RequestResult {
            success: true,
            latency_ms: 20.0,
            timestamp: 2,
        });
        collector.collect(RequestResult {
            success: false,
            latency_ms: 30.0,
            timestamp: 3,
        });

        let sli = collector.calculate_sli();
        assert_eq!(sli.availability, 2.0 / 3.0);
        assert_eq!(sli.latency_ms, 20.0);
    }

    #[test]
    fn test_empty_collector() {
        let collector = SliCollector::new();
        let sli = collector.calculate_sli();
        assert_eq!(sli.availability, 0.0);
    }

    #[test]
    fn test_export_prometheus() {
        let collector = SliCollector::new();
        collector.collect(RequestResult {
            success: true,
            latency_ms: 10.0,
            timestamp: 1,
        });
        let output = collector.export_prometheus();
        assert!(output.contains("sz_orm_sli_availability"));
        assert!(output.contains("sz_orm_slo_availability"));
    }
}
