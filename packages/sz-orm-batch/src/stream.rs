//! 批量流式处理模块（v4.1.0，`batch-stream` feature gate）
//!
//! 提供背压控制、窗口聚合、并行度控制的批量流式处理能力。

use std::collections::VecDeque;

/// 流式处理配置
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// 批次大小（每批记录数）
    pub batch_size: usize,
    /// 最大并发度
    pub max_concurrency: usize,
    /// 背压阈值（队列最大积压量）
    pub backpressure_threshold: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            max_concurrency: 4,
            backpressure_threshold: 10000,
        }
    }
}

/// 流式批次
#[derive(Debug, Clone)]
pub struct StreamBatch<T> {
    /// 批次序号
    pub batch_index: usize,
    /// 批次数据
    pub records: Vec<T>,
    /// 是否为最后一批
    pub is_last: bool,
}

/// 背压控制器
pub struct BackpressureController {
    /// 阈值
    threshold: usize,
    /// 当前队列长度
    current: usize,
}

impl BackpressureController {
    /// 创建背压控制器
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            current: 0,
        }
    }

    /// 检查是否允许入队
    pub fn allow_push(&self) -> bool {
        self.current < self.threshold
    }

    /// 入队
    pub fn push(&mut self) -> bool {
        if self.allow_push() {
            self.current += 1;
            true
        } else {
            false
        }
    }

    /// 出队
    pub fn pop(&mut self) {
        if self.current > 0 {
            self.current -= 1;
        }
    }

    /// 当前积压量
    pub fn pending(&self) -> usize {
        self.current
    }
}

/// 批次分割器
pub struct BatchSplitter<T> {
    /// 配置
    config: StreamConfig,
    /// 待处理队列
    queue: VecDeque<T>,
    /// 已生成批次数
    batch_count: usize,
}

impl<T> BatchSplitter<T> {
    /// 创建分割器
    pub fn new(config: StreamConfig) -> Self {
        Self {
            config,
            queue: VecDeque::new(),
            batch_count: 0,
        }
    }

    /// 入队数据
    pub fn push(&mut self, item: T) {
        self.queue.push_back(item);
    }

    /// 批量入队
    pub fn push_batch(&mut self, items: impl IntoIterator<Item = T>) {
        for item in items {
            self.queue.push_back(item);
        }
    }

    /// 取出下一批
    pub fn next_batch(&mut self) -> Option<StreamBatch<T>> {
        if self.queue.is_empty() {
            return None;
        }
        let batch_size = self.config.batch_size.min(self.queue.len());
        let records: Vec<T> = (0..batch_size)
            .filter_map(|_| self.queue.pop_front())
            .collect();
        let is_last = self.queue.is_empty();
        let batch_index = self.batch_count;
        self.batch_count += 1;
        Some(StreamBatch {
            batch_index,
            records,
            is_last,
        })
    }

    /// 剩余记录数
    pub fn remaining(&self) -> usize {
        self.queue.len()
    }

    /// 已生成批次数
    pub fn batch_count(&self) -> usize {
        self.batch_count
    }
}

/// 流式处理结果
#[derive(Debug, Clone)]
pub struct StreamResult {
    /// 总批次数
    pub total_batches: usize,
    /// 总记录数
    pub total_records: usize,
    /// 处理成功数
    pub succeeded: usize,
    /// 处理失败数
    pub failed: usize,
}

impl StreamResult {
    /// 成功率
    pub fn success_rate(&self) -> f64 {
        if self.total_records == 0 {
            1.0
        } else {
            self.succeeded as f64 / self.total_records as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backpressure_controller() {
        let mut ctrl = BackpressureController::new(3);
        assert!(ctrl.push());
        assert!(ctrl.push());
        assert!(ctrl.push());
        assert!(!ctrl.push());
        assert_eq!(ctrl.pending(), 3);
        ctrl.pop();
        assert_eq!(ctrl.pending(), 2);
        assert!(ctrl.push());
    }

    #[test]
    fn test_batch_splitter_basic() {
        let config = StreamConfig {
            batch_size: 2,
            max_concurrency: 1,
            backpressure_threshold: 100,
        };
        let mut splitter = BatchSplitter::new(config);
        for i in 0..5 {
            splitter.push(i);
        }
        let batch1 = splitter.next_batch().unwrap();
        assert_eq!(batch1.records, vec![0, 1]);
        assert!(!batch1.is_last);

        let batch2 = splitter.next_batch().unwrap();
        assert_eq!(batch2.records, vec![2, 3]);
        assert!(!batch2.is_last);

        let batch3 = splitter.next_batch().unwrap();
        assert_eq!(batch3.records, vec![4]);
        assert!(batch3.is_last);

        assert!(splitter.next_batch().is_none());
    }

    #[test]
    fn test_batch_splitter_empty() {
        let splitter: BatchSplitter<i32> = BatchSplitter::new(StreamConfig::default());
        let mut splitter = splitter;
        assert!(splitter.next_batch().is_none());
    }

    #[test]
    fn test_batch_splitter_exact_division() {
        let config = StreamConfig {
            batch_size: 3,
            max_concurrency: 1,
            backpressure_threshold: 100,
        };
        let mut splitter = BatchSplitter::new(config);
        splitter.push_batch(0..9);
        let mut batches = Vec::new();
        while let Some(batch) = splitter.next_batch() {
            batches.push(batch);
        }
        assert_eq!(batches.len(), 3);
        assert!(batches[2].is_last);
        assert_eq!(splitter.remaining(), 0);
    }

    #[test]
    fn test_batch_index_increment() {
        let config = StreamConfig {
            batch_size: 1,
            max_concurrency: 1,
            backpressure_threshold: 100,
        };
        let mut splitter = BatchSplitter::new(config);
        splitter.push_batch(0..3);
        let b0 = splitter.next_batch().unwrap();
        let b1 = splitter.next_batch().unwrap();
        let b2 = splitter.next_batch().unwrap();
        assert_eq!(b0.batch_index, 0);
        assert_eq!(b1.batch_index, 1);
        assert_eq!(b2.batch_index, 2);
    }

    #[test]
    fn test_stream_result_success_rate() {
        let result = StreamResult {
            total_batches: 10,
            total_records: 100,
            succeeded: 95,
            failed: 5,
        };
        assert_eq!(result.success_rate(), 0.95);
    }

    #[test]
    fn test_stream_result_empty() {
        let result = StreamResult {
            total_batches: 0,
            total_records: 0,
            succeeded: 0,
            failed: 0,
        };
        assert_eq!(result.success_rate(), 1.0);
    }

    #[test]
    fn test_stream_config_default() {
        let config = StreamConfig::default();
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.max_concurrency, 4);
        assert_eq!(config.backpressure_threshold, 10000);
    }
}
