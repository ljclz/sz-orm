//! 性能 feature 命中率度量（v6.8.0 TASK W1-7）
//!
//! 追踪四大性能优化路径的命中/未命中次数，量化各 feature gate 的实际生效比例。
//! 四大路径：零拷贝、SIMD 向量化、io_uring 异步 IO、执行器优化。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 性能优化路径标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerfPath {
    /// 零拷贝结果集传输
    ZeroCopy,
    /// SIMD 向量化聚合
    Simd,
    /// io_uring 异步 IO
    IoUring,
    /// 执行器优化（并行/流水线）
    ExecutorOpt,
}

impl PerfPath {
    /// 返回 Prometheus 指标名前缀
    pub fn metric_name(&self) -> &'static str {
        match self {
            PerfPath::ZeroCopy => "sz_orm_perf_zero_copy",
            PerfPath::Simd => "sz_orm_perf_simd",
            PerfPath::IoUring => "sz_orm_perf_io_uring",
            PerfPath::ExecutorOpt => "sz_orm_perf_executor_opt",
        }
    }
}

/// 单条性能路径的命中/未命中计数
#[derive(Debug, Default)]
struct PathCounters {
    hit: AtomicU64,
    miss: AtomicU64,
}

impl PathCounters {
    fn record_hit(&self) {
        self.hit.fetch_add(1, Ordering::Relaxed);
    }

    fn record_miss(&self) {
        self.miss.fetch_add(1, Ordering::Relaxed);
    }

    fn hit(&self) -> u64 {
        self.hit.load(Ordering::Relaxed)
    }

    fn miss(&self) -> u64 {
        self.miss.load(Ordering::Relaxed)
    }

    fn total(&self) -> u64 {
        self.hit() + self.miss()
    }

    fn hit_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.hit() as f64 / total as f64
        }
    }
}

/// 性能 feature 命中率度量集合
///
/// 追踪四大性能优化路径的命中/未命中次数。
/// 线程安全（`AtomicU64`），可在多线程环境下并发记录。
#[derive(Debug, Default)]
pub struct PerfHitMetrics {
    zero_copy: PathCounters,
    simd: PathCounters,
    io_uring: PathCounters,
    executor_opt: PathCounters,
}

impl PerfHitMetrics {
    /// 创建空的度量集合
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建共享（`Arc`）实例
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// 记录指定路径命中
    pub fn record_hit(&self, path: PerfPath) {
        self.counters(path).record_hit();
    }

    /// 计算指定路径未命中
    pub fn record_miss(&self, path: PerfPath) {
        self.counters(path).record_miss();
    }

    /// 批量记录命中
    pub fn record_hits(&self, path: PerfPath, n: u64) {
        let c = self.counters(path);
        c.hit.fetch_add(n, Ordering::Relaxed);
    }

    /// 批量记录未命中
    pub fn record_misses(&self, path: PerfPath, n: u64) {
        let c = self.counters(path);
        c.miss.fetch_add(n, Ordering::Relaxed);
    }

    /// 获取指定路径的命中次数
    pub fn hit(&self, path: PerfPath) -> u64 {
        self.counters(path).hit()
    }

    /// 获取指定路径的未命中次数
    pub fn miss(&self, path: PerfPath) -> u64 {
        self.counters(path).miss()
    }

    /// 获取指定路径的总调用次数
    pub fn total(&self, path: PerfPath) -> u64 {
        self.counters(path).total()
    }

    /// 计算指定路径的命中率（0.0~1.0）
    pub fn hit_rate(&self, path: PerfPath) -> f64 {
        self.counters(path).hit_rate()
    }

    /// 渲染为 Prometheus text format
    pub fn render(&self) -> String {
        let paths = [
            PerfPath::ZeroCopy,
            PerfPath::Simd,
            PerfPath::IoUring,
            PerfPath::ExecutorOpt,
        ];
        let mut output = String::new();
        for path in &paths {
            let c = self.counters(*path);
            let name = path.metric_name();
            output.push_str(&format!("# HELP {}_hit Total hit count\n", name));
            output.push_str(&format!("# TYPE {}_hit counter\n", name));
            output.push_str(&format!("{}_hit {}\n", name, c.hit()));
            output.push_str(&format!("# HELP {}_miss Total miss count\n", name));
            output.push_str(&format!("# TYPE {}_miss counter\n", name));
            output.push_str(&format!("{}_miss {}\n", name, c.miss()));
            output.push_str(&format!("# HELP {}_hit_ratio Hit ratio (0~1)\n", name));
            output.push_str(&format!("# TYPE {}_hit_ratio gauge\n", name));
            output.push_str(&format!("{}_hit_ratio {}\n", name, c.hit_rate()));
        }
        output
    }

    fn counters(&self, path: PerfPath) -> &PathCounters {
        match path {
            PerfPath::ZeroCopy => &self.zero_copy,
            PerfPath::Simd => &self.simd,
            PerfPath::IoUring => &self.io_uring,
            PerfPath::ExecutorOpt => &self.executor_opt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_hit_miss_basic() {
        let m = PerfHitMetrics::new();
        m.record_hit(PerfPath::ZeroCopy);
        m.record_hit(PerfPath::ZeroCopy);
        m.record_miss(PerfPath::ZeroCopy);
        assert_eq!(m.hit(PerfPath::ZeroCopy), 2);
        assert_eq!(m.miss(PerfPath::ZeroCopy), 1);
        assert_eq!(m.total(PerfPath::ZeroCopy), 3);
    }

    #[test]
    fn hit_rate_calculation() {
        let m = PerfHitMetrics::new();
        m.record_hits(PerfPath::Simd, 7);
        m.record_misses(PerfPath::Simd, 3);
        assert!((m.hit_rate(PerfPath::Simd) - 0.7).abs() < 0.001);
    }

    #[test]
    fn empty_metrics_zero_rate() {
        let m = PerfHitMetrics::new();
        assert_eq!(m.hit_rate(PerfPath::IoUring), 0.0);
        assert_eq!(m.total(PerfPath::IoUring), 0);
    }

    #[test]
    fn all_four_paths_independent() {
        let m = PerfHitMetrics::new();
        m.record_hit(PerfPath::ZeroCopy);
        m.record_hit(PerfPath::Simd);
        m.record_hit(PerfPath::IoUring);
        m.record_hit(PerfPath::ExecutorOpt);
        assert_eq!(m.hit(PerfPath::ZeroCopy), 1);
        assert_eq!(m.hit(PerfPath::Simd), 1);
        assert_eq!(m.hit(PerfPath::IoUring), 1);
        assert_eq!(m.hit(PerfPath::ExecutorOpt), 1);
    }

    #[test]
    fn render_prometheus_format() {
        let m = PerfHitMetrics::new();
        m.record_hit(PerfPath::ZeroCopy);
        m.record_miss(PerfPath::ZeroCopy);
        let output = m.render();
        assert!(output.contains("sz_orm_perf_zero_copy_hit 1"));
        assert!(output.contains("sz_orm_perf_zero_copy_miss 1"));
        assert!(output.contains("sz_orm_perf_zero_copy_hit_ratio 0.5"));
    }

    #[test]
    fn shared_arc_thread_safe() {
        let m = PerfHitMetrics::shared();
        let m2 = m.clone();
        std::thread::spawn(move || {
            for _ in 0..100 {
                m2.record_hit(PerfPath::ExecutorOpt);
            }
        })
        .join()
        .unwrap();
        assert_eq!(m.hit(PerfPath::ExecutorOpt), 100);
    }
}
