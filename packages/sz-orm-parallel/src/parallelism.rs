//! 并行度控制（Parallelism Control）
//!
//! 动态调整并行度，基于系统负载和任务特性。
//! 适用于需要自适应调节并发数的场景。

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// 并行度控制策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ParallelismStrategy {
    /// 固定并行度
    Fixed,
    /// 自适应（基于 CPU 核心数）
    Adaptive,
    /// 基于负载
    LoadBased,
    /// 渐进式（逐步增加）
    Progressive,
}

impl ParallelismStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParallelismStrategy::Fixed => "fixed",
            ParallelismStrategy::Adaptive => "adaptive",
            ParallelismStrategy::LoadBased => "load_based",
            ParallelismStrategy::Progressive => "progressive",
        }
    }
}

/// 并行度控制器
pub struct ParallelismControl {
    strategy: ParallelismStrategy,
    min_parallelism: usize,
    max_parallelism: usize,
    current: AtomicUsize,
    cpu_cores: usize,
    total_adjustments: AtomicU64,
    last_adjustment: RwLock<Option<Instant>>,
    adjustment_cooldown: Duration,
}

impl ParallelismControl {
    /// 创建并行度控制器
    pub fn new(strategy: ParallelismStrategy, min: usize, max: usize) -> Self {
        let cpu_cores = num_cpus();
        let initial = match strategy {
            ParallelismStrategy::Fixed => min,
            ParallelismStrategy::Adaptive => cpu_cores,
            ParallelismStrategy::LoadBased => cpu_cores,
            ParallelismStrategy::Progressive => min,
        };
        Self {
            strategy,
            min_parallelism: min,
            max_parallelism: max,
            current: AtomicUsize::new(initial.clamp(min, max)),
            cpu_cores,
            total_adjustments: AtomicU64::new(0),
            last_adjustment: RwLock::new(None),
            adjustment_cooldown: Duration::from_secs(1),
        }
    }

    /// 设置调整冷却时间
    pub fn with_cooldown(mut self, cooldown: Duration) -> Self {
        self.adjustment_cooldown = cooldown;
        self
    }

    /// 获取当前并行度
    pub fn current(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    /// 获取策略
    pub fn strategy(&self) -> ParallelismStrategy {
        self.strategy
    }

    /// 最小并行度
    pub fn min_parallelism(&self) -> usize {
        self.min_parallelism
    }

    /// 最大并行度
    pub fn max_parallelism(&self) -> usize {
        self.max_parallelism
    }

    /// CPU 核心数
    pub fn cpu_cores(&self) -> usize {
        self.cpu_cores
    }

    /// 手动设置并行度
    pub fn set_parallelism(&self, value: usize) {
        let clamped = value.clamp(self.min_parallelism, self.max_parallelism);
        self.current.store(clamped, Ordering::Relaxed);
        self.total_adjustments.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut last) = self.last_adjustment.write() {
            *last = Some(Instant::now());
        }
    }

    /// 增加并行度
    pub fn increase(&self) -> usize {
        let current = self.current.load(Ordering::Relaxed);
        let new = (current + 1).min(self.max_parallelism);
        self.current.store(new, Ordering::Relaxed);
        self.total_adjustments.fetch_add(1, Ordering::Relaxed);
        new
    }

    /// 减少并行度
    pub fn decrease(&self) -> usize {
        let current = self.current.load(Ordering::Relaxed);
        let new = current.saturating_sub(1).max(self.min_parallelism);
        self.current.store(new, Ordering::Relaxed);
        self.total_adjustments.fetch_add(1, Ordering::Relaxed);
        new
    }

    /// 基于负载调整并行度
    ///
    /// - `load`：当前负载（0.0~1.0）
    pub fn adjust_by_load(&self, load: f64) -> usize {
        if !self.can_adjust() {
            return self.current();
        }
        let load = load.clamp(0.0, 1.0);
        let target = if load < 0.3 {
            self.max_parallelism
        } else if load < 0.7 {
            self.cpu_cores
        } else {
            self.min_parallelism
        };
        self.set_parallelism(target);
        target
    }

    /// 渐进式增加
    pub fn progressive_increase(&self) -> usize {
        if !self.can_adjust() {
            return self.current();
        }
        let current = self.current.load(Ordering::Relaxed);
        let step = ((self.max_parallelism - current) / 4).max(1);
        let new = (current + step).min(self.max_parallelism);
        self.current.store(new, Ordering::Relaxed);
        self.total_adjustments.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut last) = self.last_adjustment.write() {
            *last = Some(Instant::now());
        }
        new
    }

    /// 渐进式减少
    pub fn progressive_decrease(&self) -> usize {
        if !self.can_adjust() {
            return self.current();
        }
        let current = self.current.load(Ordering::Relaxed);
        let step = ((current - self.min_parallelism) / 4).max(1);
        let new = current.saturating_sub(step).max(self.min_parallelism);
        self.current.store(new, Ordering::Relaxed);
        self.total_adjustments.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut last) = self.last_adjustment.write() {
            *last = Some(Instant::now());
        }
        new
    }

    fn can_adjust(&self) -> bool {
        match self.last_adjustment.read().ok().and_then(|r| *r) {
            None => true,
            Some(last) => last.elapsed() >= self.adjustment_cooldown,
        }
    }

    /// 总调整次数
    pub fn total_adjustments(&self) -> u64 {
        self.total_adjustments.load(Ordering::Relaxed)
    }

    /// 重置到初始值
    pub fn reset(&self) {
        let initial = match self.strategy {
            ParallelismStrategy::Fixed => self.min_parallelism,
            ParallelismStrategy::Adaptive => self.cpu_cores,
            ParallelismStrategy::LoadBased => self.cpu_cores,
            ParallelismStrategy::Progressive => self.min_parallelism,
        };
        self.current.store(
            initial.clamp(self.min_parallelism, self.max_parallelism),
            Ordering::Relaxed,
        );
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// 并行度统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct ParallelismStats {
    pub current: usize,
    pub min: usize,
    pub max: usize,
    pub strategy: ParallelismStrategy,
    pub cpu_cores: usize,
    pub total_adjustments: u64,
}

impl ParallelismControl {
    pub fn stats(&self) -> ParallelismStats {
        ParallelismStats {
            current: self.current(),
            min: self.min_parallelism,
            max: self.max_parallelism,
            strategy: self.strategy,
            cpu_cores: self.cpu_cores,
            total_adjustments: self.total_adjustments(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallelism_strategy_as_str() {
        assert_eq!(ParallelismStrategy::Fixed.as_str(), "fixed");
        assert_eq!(ParallelismStrategy::Adaptive.as_str(), "adaptive");
        assert_eq!(ParallelismStrategy::LoadBased.as_str(), "load_based");
        assert_eq!(ParallelismStrategy::Progressive.as_str(), "progressive");
    }

    #[test]
    fn test_parallelism_control_fixed() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 4, 16);
        assert_eq!(ctrl.current(), 4);
        assert_eq!(ctrl.strategy(), ParallelismStrategy::Fixed);
    }

    #[test]
    fn test_parallelism_control_adaptive() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Adaptive, 1, 100);
        assert_eq!(ctrl.current(), ctrl.cpu_cores());
    }

    #[test]
    fn test_parallelism_control_set() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 1, 100);
        ctrl.set_parallelism(8);
        assert_eq!(ctrl.current(), 8);
    }

    #[test]
    fn test_parallelism_control_set_clamped() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 4, 16);
        ctrl.set_parallelism(2);
        assert_eq!(ctrl.current(), 4);
        ctrl.set_parallelism(100);
        assert_eq!(ctrl.current(), 16);
    }

    #[test]
    fn test_parallelism_control_increase() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 1, 10);
        ctrl.set_parallelism(5);
        let new = ctrl.increase();
        assert_eq!(new, 6);
        assert_eq!(ctrl.current(), 6);
    }

    #[test]
    fn test_parallelism_control_increase_max() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 1, 5);
        ctrl.set_parallelism(5);
        let new = ctrl.increase();
        assert_eq!(new, 5);
    }

    #[test]
    fn test_parallelism_control_decrease() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 1, 10);
        ctrl.set_parallelism(5);
        let new = ctrl.decrease();
        assert_eq!(new, 4);
        assert_eq!(ctrl.current(), 4);
    }

    #[test]
    fn test_parallelism_control_decrease_min() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 5, 10);
        ctrl.set_parallelism(5);
        let new = ctrl.decrease();
        assert_eq!(new, 5);
    }

    #[test]
    fn test_parallelism_control_adjust_by_load_low() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::LoadBased, 1, 100);
        let new = ctrl.adjust_by_load(0.1);
        assert_eq!(new, 100);
    }

    #[test]
    fn test_parallelism_control_adjust_by_load_high() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::LoadBased, 1, 100);
        let new = ctrl.adjust_by_load(0.9);
        assert_eq!(new, 1);
    }

    #[test]
    fn test_parallelism_control_adjust_by_load_medium() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::LoadBased, 1, 100);
        let new = ctrl.adjust_by_load(0.5);
        assert_eq!(new, ctrl.cpu_cores());
    }

    #[test]
    fn test_parallelism_control_progressive_increase() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Progressive, 1, 100)
            .with_cooldown(Duration::from_millis(0));
        ctrl.set_parallelism(10);
        let new = ctrl.progressive_increase();
        assert!(new > 10);
    }

    #[test]
    fn test_parallelism_control_progressive_decrease() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Progressive, 1, 100)
            .with_cooldown(Duration::from_millis(0));
        ctrl.set_parallelism(50);
        let new = ctrl.progressive_decrease();
        assert!(new < 50);
    }

    #[test]
    fn test_parallelism_control_total_adjustments() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 1, 10);
        ctrl.set_parallelism(5);
        ctrl.increase();
        ctrl.decrease();
        assert!(ctrl.total_adjustments() >= 3);
    }

    #[test]
    fn test_parallelism_control_reset() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 4, 16);
        ctrl.set_parallelism(10);
        ctrl.reset();
        assert_eq!(ctrl.current(), 4);
    }

    #[test]
    fn test_parallelism_control_min_max() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 4, 16);
        assert_eq!(ctrl.min_parallelism(), 4);
        assert_eq!(ctrl.max_parallelism(), 16);
    }

    #[test]
    fn test_parallelism_control_cpu_cores() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 1, 100);
        assert!(ctrl.cpu_cores() >= 1);
    }

    #[test]
    fn test_parallelism_stats() {
        let ctrl = ParallelismControl::new(ParallelismStrategy::Fixed, 4, 16);
        ctrl.set_parallelism(8);
        let stats = ctrl.stats();
        assert_eq!(stats.current, 8);
        assert_eq!(stats.min, 4);
        assert_eq!(stats.max, 16);
    }
}
