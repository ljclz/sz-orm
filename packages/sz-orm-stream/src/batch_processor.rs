//! 流式批处理器
//!
//! 将流中的元素按批次处理，支持背压控制与错误处理。

use std::collections::VecDeque;

use serde_json::Value;

use crate::backpressure::AsyncBackpressureController;

/// 批处理结果
#[derive(Debug, Clone)]
pub struct BatchResult {
    /// 本批数据
    pub batch: Vec<Value>,
    /// 处理是否成功
    pub success: bool,
    /// 错误信息（失败时）
    pub error: Option<String>,
    /// 批次序号
    pub batch_index: usize,
}

impl BatchResult {
    /// 创建成功结果
    pub fn ok(batch: Vec<Value>, batch_index: usize) -> Self {
        Self {
            batch,
            success: true,
            error: None,
            batch_index,
        }
    }

    /// 创建失败结果
    pub fn err(error: String, batch_index: usize) -> Self {
        Self {
            batch: Vec::new(),
            success: false,
            error: Some(error),
            batch_index,
        }
    }

    /// 批次大小
    pub fn len(&self) -> usize {
        self.batch.len()
    }

    /// 是否为空批
    pub fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }
}

/// 批处理器配置
#[derive(Debug, Clone)]
pub struct BatchProcessorConfig {
    /// 批次大小
    pub batch_size: usize,
    /// 背压阈值
    pub backpressure_threshold: usize,
    /// 失败时是否继续
    pub continue_on_error: bool,
    /// 最大重试次数
    pub max_retries: usize,
}

impl Default for BatchProcessorConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            backpressure_threshold: 1000,
            continue_on_error: true,
            max_retries: 3,
        }
    }
}

impl BatchProcessorConfig {
    /// 创建配置
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch_size: batch_size.max(1),
            ..Self::default()
        }
    }

    /// 设置背压阈值
    pub fn with_backpressure(mut self, threshold: usize) -> Self {
        self.backpressure_threshold = threshold;
        self
    }

    /// 设置失败时是否继续
    pub fn continue_on_error(mut self, continue_on: bool) -> Self {
        self.continue_on_error = continue_on;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }
}

/// 流式批处理器
///
/// 缓冲输入元素，按批次输出。支持背压控制。
pub struct StreamBatchProcessor {
    config: BatchProcessorConfig,
    buffer: VecDeque<Value>,
    batch_index: usize,
    total_processed: usize,
    total_batches: usize,
    total_errors: usize,
    backpressure: AsyncBackpressureController,
}

impl StreamBatchProcessor {
    /// 创建批处理器
    pub fn new(config: BatchProcessorConfig) -> Self {
        let backpressure = AsyncBackpressureController::new(config.backpressure_threshold);
        Self {
            config,
            buffer: VecDeque::new(),
            batch_index: 0,
            total_processed: 0,
            total_batches: 0,
            total_errors: 0,
            backpressure,
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(BatchProcessorConfig::default())
    }

    /// 添加元素到缓冲区
    pub fn push(&mut self, item: Value) {
        self.buffer.push_back(item);
    }

    /// 批量添加元素
    pub fn push_batch(&mut self, items: Vec<Value>) {
        for item in items {
            self.buffer.push_back(item);
        }
    }

    /// 尝试取出一个批次
    ///
    /// 返回 `Some(BatchResult)` 表示有批次可处理，`None` 表示缓冲区为空。
    pub fn next_batch(&mut self) -> Option<Vec<Value>> {
        if self.buffer.is_empty() {
            return None;
        }
        let batch_size = self.config.batch_size.min(self.buffer.len());
        let batch: Vec<Value> = (0..batch_size)
            .filter_map(|_| self.buffer.pop_front())
            .collect();
        Some(batch)
    }

    /// 记录处理成功
    pub fn record_success(&mut self, batch_len: usize) -> BatchResult {
        let result = BatchResult::ok(Vec::new(), self.batch_index);
        self.batch_index += 1;
        self.total_batches += 1;
        self.total_processed += batch_len;
        self.backpressure.push();
        result
    }

    /// 记录处理失败
    pub fn record_failure(&mut self, error: String) -> BatchResult {
        let result = BatchResult::err(error, self.batch_index);
        self.batch_index += 1;
        self.total_batches += 1;
        self.total_errors += 1;
        result
    }

    /// 缓冲区大小
    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }

    /// 是否有满批
    pub fn has_full_batch(&self) -> bool {
        self.buffer.len() >= self.config.batch_size
    }

    /// 已处理批次数
    pub fn total_batches(&self) -> usize {
        self.total_batches
    }

    /// 已处理元素数
    pub fn total_processed(&self) -> usize {
        self.total_processed
    }

    /// 错误数
    pub fn total_errors(&self) -> usize {
        self.total_errors
    }

    /// 错误率
    pub fn error_rate(&self) -> f64 {
        if self.total_batches == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_batches as f64
        }
    }

    /// 背压控制器引用
    pub fn backpressure(&self) -> &AsyncBackpressureController {
        &self.backpressure
    }

    /// 配置引用
    pub fn config(&self) -> &BatchProcessorConfig {
        &self.config
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// 重置统计
    pub fn reset_stats(&mut self) {
        self.batch_index = 0;
        self.total_processed = 0;
        self.total_batches = 0;
        self.total_errors = 0;
    }
}

/// 批处理统计
#[derive(Debug, Clone, Default)]
pub struct BatchProcessingStats {
    /// 总批次数
    pub total_batches: usize,
    /// 总元素数
    pub total_items: usize,
    /// 成功批次数
    pub successful_batches: usize,
    /// 失败批次数
    pub failed_batches: usize,
    /// 总处理耗时（毫秒）
    pub total_elapsed_ms: u64,
}

impl BatchProcessingStats {
    /// 创建空统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一批处理结果
    pub fn record(&mut self, result: &BatchResult, elapsed_ms: u64) {
        self.total_batches += 1;
        self.total_items += result.len();
        self.total_elapsed_ms += elapsed_ms;
        if result.success {
            self.successful_batches += 1;
        } else {
            self.failed_batches += 1;
        }
    }

    /// 成功率
    pub fn success_rate(&self) -> f64 {
        if self.total_batches == 0 {
            0.0
        } else {
            self.successful_batches as f64 / self.total_batches as f64
        }
    }

    /// 平均批耗时（毫秒）
    pub fn avg_batch_elapsed_ms(&self) -> f64 {
        if self.total_batches == 0 {
            0.0
        } else {
            self.total_elapsed_ms as f64 / self.total_batches as f64
        }
    }

    /// 平均批大小
    pub fn avg_batch_size(&self) -> f64 {
        if self.total_batches == 0 {
            0.0
        } else {
            self.total_items as f64 / self.total_batches as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- BatchResult tests ---

    #[test]
    fn batch_result_ok() {
        let result = BatchResult::ok(vec![json!(1), json!(2)], 0);
        assert!(result.success);
        assert!(result.error.is_none());
        assert_eq!(result.len(), 2);
        assert!(!result.is_empty());
    }

    #[test]
    fn batch_result_err() {
        let result = BatchResult::err("db error".to_string(), 1);
        assert!(!result.success);
        assert_eq!(result.error, Some("db error".to_string()));
        assert_eq!(result.len(), 0);
        assert!(result.is_empty());
    }

    #[test]
    fn batch_result_batch_index() {
        let r1 = BatchResult::ok(vec![], 0);
        let r2 = BatchResult::ok(vec![], 1);
        assert_eq!(r1.batch_index, 0);
        assert_eq!(r2.batch_index, 1);
    }

    // --- BatchProcessorConfig tests ---

    #[test]
    fn config_default() {
        let c = BatchProcessorConfig::default();
        assert_eq!(c.batch_size, 100);
        assert_eq!(c.backpressure_threshold, 1000);
        assert!(c.continue_on_error);
        assert_eq!(c.max_retries, 3);
    }

    #[test]
    fn config_new_clamps_batch_size() {
        let c = BatchProcessorConfig::new(0);
        assert_eq!(c.batch_size, 1);
    }

    #[test]
    fn config_with_backpressure() {
        let c = BatchProcessorConfig::new(10).with_backpressure(500);
        assert_eq!(c.backpressure_threshold, 500);
    }

    #[test]
    fn config_continue_on_error() {
        let c = BatchProcessorConfig::new(10).continue_on_error(false);
        assert!(!c.continue_on_error);
    }

    #[test]
    fn config_with_max_retries() {
        let c = BatchProcessorConfig::new(10).with_max_retries(5);
        assert_eq!(c.max_retries, 5);
    }

    // --- StreamBatchProcessor tests ---

    #[test]
    fn processor_empty_buffer() {
        let mut p = StreamBatchProcessor::with_defaults();
        assert_eq!(p.buffer_size(), 0);
        assert!(p.next_batch().is_none());
    }

    #[test]
    fn processor_push_and_next_batch() {
        let mut p = StreamBatchProcessor::new(BatchProcessorConfig::new(2));
        p.push(json!(1));
        p.push(json!(2));
        p.push(json!(3));
        let batch = p.next_batch().unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(p.buffer_size(), 1);
    }

    #[test]
    fn processor_push_batch() {
        let mut p = StreamBatchProcessor::new(BatchProcessorConfig::new(3));
        p.push_batch(vec![json!(1), json!(2), json!(3), json!(4)]);
        assert_eq!(p.buffer_size(), 4);
    }

    #[test]
    fn processor_has_full_batch() {
        let mut p = StreamBatchProcessor::new(BatchProcessorConfig::new(2));
        p.push(json!(1));
        assert!(!p.has_full_batch());
        p.push(json!(2));
        assert!(p.has_full_batch());
    }

    #[test]
    fn processor_record_success() {
        let mut p = StreamBatchProcessor::with_defaults();
        p.record_success(10);
        assert_eq!(p.total_batches(), 1);
        assert_eq!(p.total_processed(), 10);
        assert_eq!(p.total_errors(), 0);
    }

    #[test]
    fn processor_record_failure() {
        let mut p = StreamBatchProcessor::with_defaults();
        p.record_failure("error".to_string());
        assert_eq!(p.total_batches(), 1);
        assert_eq!(p.total_errors(), 1);
    }

    #[test]
    fn processor_error_rate() {
        let mut p = StreamBatchProcessor::with_defaults();
        p.record_success(10);
        p.record_failure("err".to_string());
        assert!((p.error_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn processor_error_rate_empty() {
        let p = StreamBatchProcessor::with_defaults();
        assert_eq!(p.error_rate(), 0.0);
    }

    #[test]
    fn processor_clear() {
        let mut p = StreamBatchProcessor::with_defaults();
        p.push(json!(1));
        p.push(json!(2));
        p.clear();
        assert_eq!(p.buffer_size(), 0);
    }

    #[test]
    fn processor_reset_stats() {
        let mut p = StreamBatchProcessor::with_defaults();
        p.record_success(10);
        p.record_failure("err".to_string());
        p.reset_stats();
        assert_eq!(p.total_batches(), 0);
        assert_eq!(p.total_processed(), 0);
        assert_eq!(p.total_errors(), 0);
    }

    #[test]
    fn processor_config_ref() {
        let p = StreamBatchProcessor::new(BatchProcessorConfig::new(50));
        assert_eq!(p.config().batch_size, 50);
    }

    #[test]
    fn processor_backpressure_ref() {
        let p = StreamBatchProcessor::with_defaults();
        assert!(p.backpressure().try_allow_push());
    }

    #[test]
    fn processor_multiple_batches() {
        let mut p = StreamBatchProcessor::new(BatchProcessorConfig::new(2));
        for i in 0..6 {
            p.push(json!(i));
        }
        let mut batches = 0;
        while let Some(batch) = p.next_batch() {
            batches += 1;
            p.record_success(batch.len());
        }
        assert_eq!(batches, 3);
        assert_eq!(p.total_processed(), 6);
    }

    #[test]
    fn processor_partial_batch() {
        let mut p = StreamBatchProcessor::new(BatchProcessorConfig::new(10));
        p.push(json!(1));
        p.push(json!(2));
        let batch = p.next_batch().unwrap();
        assert_eq!(batch.len(), 2);
    }

    // --- BatchProcessingStats tests ---

    #[test]
    fn stats_new_empty() {
        let s = BatchProcessingStats::new();
        assert_eq!(s.total_batches, 0);
        assert_eq!(s.success_rate(), 0.0);
    }

    #[test]
    fn stats_record_success() {
        let mut s = BatchProcessingStats::new();
        let result = BatchResult::ok(vec![json!(1), json!(2)], 0);
        s.record(&result, 100);
        assert_eq!(s.total_batches, 1);
        assert_eq!(s.total_items, 2);
        assert_eq!(s.successful_batches, 1);
    }

    #[test]
    fn stats_record_failure() {
        let mut s = BatchProcessingStats::new();
        let result = BatchResult::err("error".to_string(), 0);
        s.record(&result, 50);
        assert_eq!(s.failed_batches, 1);
    }

    #[test]
    fn stats_success_rate() {
        let mut s = BatchProcessingStats::new();
        s.record(&BatchResult::ok(vec![], 0), 10);
        s.record(&BatchResult::ok(vec![], 1), 10);
        s.record(&BatchResult::err("err".to_string(), 2), 10);
        assert!((s.success_rate() - (2.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn stats_avg_batch_elapsed() {
        let mut s = BatchProcessingStats::new();
        s.record(&BatchResult::ok(vec![], 0), 100);
        s.record(&BatchResult::ok(vec![], 1), 300);
        assert!((s.avg_batch_elapsed_ms() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn stats_avg_batch_size() {
        let mut s = BatchProcessingStats::new();
        s.record(&BatchResult::ok(vec![json!(1)], 0), 10);
        s.record(&BatchResult::ok(vec![json!(1), json!(2), json!(3)], 1), 10);
        assert!((s.avg_batch_size() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn stats_all_failures() {
        let mut s = BatchProcessingStats::new();
        s.record(&BatchResult::err("e1".to_string(), 0), 10);
        s.record(&BatchResult::err("e2".to_string(), 1), 10);
        assert_eq!(s.success_rate(), 0.0);
    }

    #[test]
    fn stats_all_success() {
        let mut s = BatchProcessingStats::new();
        s.record(&BatchResult::ok(vec![json!(1)], 0), 10);
        s.record(&BatchResult::ok(vec![json!(2)], 1), 10);
        assert!((s.success_rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn processor_full_workflow() {
        let mut p = StreamBatchProcessor::new(BatchProcessorConfig::new(2));
        p.push_batch(vec![json!(1), json!(2), json!(3), json!(4), json!(5)]);
        let mut total = 0;
        while let Some(batch) = p.next_batch() {
            total += batch.len();
            p.record_success(batch.len());
        }
        assert_eq!(total, 5);
        assert_eq!(p.total_processed(), 5);
        assert_eq!(p.total_errors(), 0);
    }

    #[test]
    fn processor_error_rate_all_success() {
        let mut p = StreamBatchProcessor::with_defaults();
        p.record_success(10);
        p.record_success(20);
        assert!((p.error_rate() - 0.0).abs() < 1e-9);
    }
}
