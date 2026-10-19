//! 异步流水线并行度自适应（v10.10.0）
//!
//! 根据运行时负载信号动态调整异步流水线并行度：
//! - 高负载降级避免过载
//! - 低负载提升充分利用资源
//! - 含平滑因子（滑动平均）避免震荡
//!
//! 复用既有 perf-accel feature gate。

use std::collections::VecDeque;

/// 负载信号（v10.10.0）
///
/// 描述运行时负载的多维度信号。
#[derive(Debug, Clone)]
pub struct LoadSignal {
    /// CPU 利用率（0.0-1.0）
    pub cpu_utilization: f64,
    /// 队列深度（待处理任务数）
    pub queue_depth: u32,
    /// IO 等待比率（0.0-1.0）
    pub io_wait_ratio: f64,
}

impl LoadSignal {
    /// 创建负载信号
    pub fn new(cpu_utilization: f64, queue_depth: u32, io_wait_ratio: f64) -> Self {
        Self {
            cpu_utilization: cpu_utilization.clamp(0.0, 1.0),
            queue_depth,
            io_wait_ratio: io_wait_ratio.clamp(0.0, 1.0),
        }
    }

    /// 计算综合负载值（0.0-1.0）
    ///
    /// 权重：CPU 50% + IO 等待 30% + 队列深度 20%（队列深度归一化到 0-1，阈值 100）
    pub fn composite_load(&self) -> f64 {
        let queue_normalized = (self.queue_depth as f64 / 100.0).min(1.0);
        self.cpu_utilization * 0.5 + self.io_wait_ratio * 0.3 + queue_normalized * 0.2
    }
}

/// 流水线并行度自适应适配器（v10.10.0）
///
/// 根据负载信号动态调整并行度，含平滑因子避免震荡。
pub struct PipelineParallelismAdapter {
    /// 最小并行度
    min_parallelism: usize,
    /// 最大并行度
    max_parallelism: usize,
    /// 当前并行度
    current_parallelism: usize,
    /// 平滑因子（0.0-1.0，越大越平滑）
    smoothing_factor: f64,
    /// 负载历史（保留最近 N 条）
    load_history: VecDeque<LoadSignal>,
    /// 平滑后的负载值
    smoothed_load: f64,
    /// 历史窗口大小
    window_size: usize,
}

impl PipelineParallelismAdapter {
    /// 创建并行度适配器，初始并行度 = (min+max)/2
    pub fn new(min: usize, max: usize) -> Self {
        let initial = (min + max) / 2;
        Self {
            min_parallelism: min,
            max_parallelism: max,
            current_parallelism: initial,
            smoothing_factor: 0.7,
            load_history: VecDeque::with_capacity(32),
            smoothed_load: 0.5,
            window_size: 32,
        }
    }

    /// 设置平滑因子（0.0-1.0）
    pub fn with_smoothing_factor(mut self, factor: f64) -> Self {
        self.smoothing_factor = factor.clamp(0.0, 1.0);
        self
    }

    /// 设置历史窗口大小
    pub fn with_window_size(mut self, size: usize) -> Self {
        self.window_size = size;
        self.load_history.reserve(size);
        self
    }

    /// 当前并行度
    pub fn current_parallelism(&self) -> usize {
        self.current_parallelism
    }

    /// 记录负载信号
    pub fn record_load_signal(&mut self, signal: LoadSignal) {
        if self.load_history.len() >= self.window_size {
            self.load_history.pop_front();
        }
        self.load_history.push_back(signal);
    }

    /// 根据滑动平均负载信号动态调整并行度
    ///
    /// 高负载（>0.7）降级，低负载（<0.3）提升，中间不变。
    /// 平滑因子避免震荡：smoothed = factor * old + (1-factor) * current
    /// 返回调整后的并行度
    pub fn adjust_parallelism(&mut self) -> usize {
        if self.load_history.is_empty() {
            return self.current_parallelism;
        }

        let avg_load: f64 = self
            .load_history
            .iter()
            .map(|s| s.composite_load())
            .sum::<f64>()
            / self.load_history.len() as f64;

        self.smoothed_load = self.smoothing_factor * self.smoothed_load
            + (1.0 - self.smoothing_factor) * avg_load;

        let load = self.smoothed_load;
        if load > 0.7 {
            let decrease = ((load - 0.7) * 10.0).ceil() as usize;
            self.current_parallelism =
                self.current_parallelism.saturating_sub(decrease).max(self.min_parallelism);
        } else if load < 0.3 {
            let increase = ((0.3 - load) * 10.0).ceil() as usize;
            self.current_parallelism = (self.current_parallelism + increase).min(self.max_parallelism);
        }

        self.current_parallelism
    }

    /// 平滑后的负载值
    pub fn smoothed_load(&self) -> f64 {
        self.smoothed_load
    }

    /// 负载历史记录数
    pub fn history_len(&self) -> usize {
        self.load_history.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_signal_composite_load() {
        let signal = LoadSignal::new(0.8, 50, 0.2);
        let load = signal.composite_load();
        assert!(load > 0.0 && load <= 1.0, "综合负载应在 0-1 范围内");
        assert!((load - 0.56).abs() < 0.01, "0.8*0.5 + 0.2*0.3 + 0.5*0.2 = 0.56");
    }

    #[test]
    fn test_load_signal_clamping() {
        let signal = LoadSignal::new(1.5, 200, -0.1);
        assert_eq!(signal.cpu_utilization, 1.0);
        assert_eq!(signal.io_wait_ratio, 0.0);
    }

    #[test]
    fn test_pipeline_parallelism_initial() {
        let adapter = PipelineParallelismAdapter::new(2, 16);
        assert_eq!(adapter.current_parallelism(), 9, "初始并行度 = (2+16)/2 = 9");
    }

    #[test]
    fn test_pipeline_parallelism_high_load_decreases() {
        let mut adapter = PipelineParallelismAdapter::new(2, 16).with_smoothing_factor(0.0);
        for _ in 0..5 {
            adapter.record_load_signal(LoadSignal::new(0.95, 80, 0.9));
        }
        let initial = adapter.current_parallelism();
        let adjusted = adapter.adjust_parallelism();
        assert!(
            adjusted < initial,
            "高负载时并行度应降级: initial={}, adjusted={}",
            initial,
            adjusted
        );
    }

    #[test]
    fn test_pipeline_parallelism_low_load_increases() {
        let mut adapter = PipelineParallelismAdapter::new(2, 16).with_smoothing_factor(0.0);
        adapter.record_load_signal(LoadSignal::new(0.9, 80, 0.9));
        adapter.adjust_parallelism();

        for _ in 0..10 {
            adapter.record_load_signal(LoadSignal::new(0.1, 5, 0.05));
        }
        let before_low = adapter.current_parallelism();
        let adjusted = adapter.adjust_parallelism();
        assert!(
            adjusted > before_low,
            "低负载时并行度应提升: before={}, adjusted={}",
            before_low,
            adjusted
        );
    }

    #[test]
    fn test_pipeline_parallelism_stays_in_bounds() {
        let mut adapter = PipelineParallelismAdapter::new(4, 12).with_smoothing_factor(0.0);

        for _ in 0..20 {
            adapter.record_load_signal(LoadSignal::new(0.99, 200, 0.99));
            let adjusted = adapter.adjust_parallelism();
            assert!(
                (4..=12).contains(&adjusted),
                "并行度应在 [4, 12] 范围内: got {}",
                adjusted
            );
        }

        for _ in 0..20 {
            adapter.record_load_signal(LoadSignal::new(0.01, 0, 0.0));
            let adjusted = adapter.adjust_parallelism();
            assert!(
                (4..=12).contains(&adjusted),
                "并行度应在 [4, 12] 范围内: got {}",
                adjusted
            );
        }
    }

    #[test]
    fn test_pipeline_parallelism_smoothing_avoids_oscillation() {
        let mut adapter = PipelineParallelismAdapter::new(2, 16).with_smoothing_factor(0.9);

        let mut results = Vec::new();
        for i in 0..20 {
            let load = if i % 2 == 0 { 0.9 } else { 0.1 };
            adapter.record_load_signal(LoadSignal::new(load, 50, load));
            results.push(adapter.adjust_parallelism());
        }

        let changes: usize = results
            .windows(2)
            .map(|w| if w[0] != w[1] { 1 } else { 0 })
            .sum();
        assert!(
            changes < 10,
            "平滑因子 0.9 应减少震荡，变化次数 {} 应 < 10",
            changes
        );
    }

    #[test]
    fn test_pipeline_parallelism_empty_history_no_change() {
        let mut adapter = PipelineParallelismAdapter::new(2, 16);
        let initial = adapter.current_parallelism();
        let adjusted = adapter.adjust_parallelism();
        assert_eq!(
            adjusted, initial,
            "无负载历史时并行度不应变化"
        );
    }

    #[test]
    fn test_pipeline_parallelism_window_size() {
        let mut adapter = PipelineParallelismAdapter::new(2, 16).with_window_size(5);
        for _ in 0..10 {
            adapter.record_load_signal(LoadSignal::new(0.5, 10, 0.5));
        }
        assert_eq!(adapter.history_len(), 5, "窗口大小 5 应只保留最近 5 条");
    }
}