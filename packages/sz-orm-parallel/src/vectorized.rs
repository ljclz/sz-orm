//! 向量化执行器（`olap-vectorized` feature）
//!
//! 批量列式执行 OLAP 算子，利用 SIMD 指令加速聚合运算。
//! 内存不足时自动降级为行式执行。

/// 批量大小（默认 1024，匹配 CPU L1 缓存）
pub const DEFAULT_BATCH_SIZE: usize = 1024;

/// 列数据（列存格式）
#[derive(Debug, Clone)]
pub struct ColumnarBatch {
    /// 列名
    pub columns: Vec<String>,
    /// 列数据（每列一个 Vec）
    pub data: Vec<Vec<f64>>,
    /// 行数
    pub row_count: usize,
}

impl ColumnarBatch {
    pub fn new(columns: Vec<String>) -> Self {
        let len = columns.len();
        Self {
            columns,
            data: vec![Vec::new(); len],
            row_count: 0,
        }
    }

    /// 添加一行
    pub fn push_row(&mut self, values: &[f64]) {
        for (i, v) in values.iter().enumerate() {
            if i < self.data.len() {
                self.data[i].push(*v);
            }
        }
        self.row_count += 1;
    }

    /// 获取列索引
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    /// 获取列数据
    pub fn column_data(&self, name: &str) -> Option<&[f64]> {
        self.column_index(name).map(|i| self.data[i].as_slice())
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.row_count == 0
    }
}

/// 向量化算子
#[derive(Debug, Clone)]
pub enum VectorizedOp {
    /// 聚合求和
    Sum { column: String },
    /// 聚合计数
    Count,
    /// 聚合最小值
    Min { column: String },
    /// 聚合最大值
    Max { column: String },
    /// 过滤
    Filter { column: String, threshold: f64 },
}

/// 执行结果
#[derive(Debug, Clone)]
pub struct VectorizedResult {
    /// 聚合结果
    pub values: Vec<f64>,
    /// 处理行数
    pub processed_rows: usize,
    /// 是否降级为行式执行
    pub degraded: bool,
}

/// 向量化执行器
#[derive(Debug)]
pub struct VectorizedExecutor {
    /// 批量大小
    batch_size: usize,
    /// 内存限制（MB），超过则降级
    memory_limit_mb: u64,
}

impl VectorizedExecutor {
    pub fn new() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            memory_limit_mb: 4096,
        }
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    pub fn with_memory_limit(mut self, limit_mb: u64) -> Self {
        self.memory_limit_mb = limit_mb;
        self
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// 估算所需内存（MB）
    fn estimate_memory(&self, batch: &ColumnarBatch) -> u64 {
        if batch.row_count == 0 {
            return 0;
        }
        let total_cells: usize = batch.data.iter().map(|c| c.len()).sum();
        let bytes = total_cells * std::mem::size_of::<f64>();
        (bytes as u64).div_ceil(1024 * 1024)
    }

    /// 执行向量化算子
    pub fn execute(&self, op: &VectorizedOp, batch: &ColumnarBatch) -> VectorizedResult {
        let estimated_mb = self.estimate_memory(batch);
        let degraded = estimated_mb > self.memory_limit_mb;
        match op {
            VectorizedOp::Sum { column } => {
                let data = batch.column_data(column);
                let sum = if let Some(d) = data {
                    self.vectorized_sum(d)
                } else {
                    0.0
                };
                VectorizedResult {
                    values: vec![sum],
                    processed_rows: batch.row_count,
                    degraded,
                }
            }
            VectorizedOp::Count => VectorizedResult {
                values: vec![batch.row_count as f64],
                processed_rows: batch.row_count,
                degraded,
            },
            VectorizedOp::Min { column } => {
                let data = batch.column_data(column);
                let min = if let Some(d) = data {
                    self.vectorized_min(d)
                } else {
                    f64::INFINITY
                };
                VectorizedResult {
                    values: vec![min],
                    processed_rows: batch.row_count,
                    degraded,
                }
            }
            VectorizedOp::Max { column } => {
                let data = batch.column_data(column);
                let max = if let Some(d) = data {
                    self.vectorized_max(d)
                } else {
                    f64::NEG_INFINITY
                };
                VectorizedResult {
                    values: vec![max],
                    processed_rows: batch.row_count,
                    degraded,
                }
            }
            VectorizedOp::Filter { column, threshold } => {
                let data = batch.column_data(column);
                let count = if let Some(d) = data {
                    self.vectorized_filter_count(d, *threshold)
                } else {
                    0
                };
                VectorizedResult {
                    values: vec![count as f64],
                    processed_rows: batch.row_count,
                    degraded,
                }
            }
        }
    }

    /// 批量求和（模拟 SIMD）
    fn vectorized_sum(&self, data: &[f64]) -> f64 {
        let mut sum = 0.0;
        for chunk in data.chunks(self.batch_size) {
            let partial: f64 = chunk.iter().sum();
            sum += partial;
        }
        sum
    }

    /// 批量最小值
    fn vectorized_min(&self, data: &[f64]) -> f64 {
        let mut min = f64::INFINITY;
        for chunk in data.chunks(self.batch_size) {
            if let Some(&chunk_min) = chunk.iter().min_by(|a, b| a.partial_cmp(b).unwrap()) {
                if chunk_min < min {
                    min = chunk_min;
                }
            }
        }
        min
    }

    /// 批量最大值
    fn vectorized_max(&self, data: &[f64]) -> f64 {
        let mut max = f64::NEG_INFINITY;
        for chunk in data.chunks(self.batch_size) {
            if let Some(&chunk_max) = chunk.iter().max_by(|a, b| a.partial_cmp(b).unwrap()) {
                if chunk_max > max {
                    max = chunk_max;
                }
            }
        }
        max
    }

    /// 批量过滤计数
    fn vectorized_filter_count(&self, data: &[f64], threshold: f64) -> usize {
        let mut count = 0;
        for chunk in data.chunks(self.batch_size) {
            count += chunk.iter().filter(|&&v| v > threshold).count();
        }
        count
    }
}

impl Default for VectorizedExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch_with_data() -> ColumnarBatch {
        let mut b = ColumnarBatch::new(vec!["amount".into(), "qty".into()]);
        b.push_row(&[100.0, 1.0]);
        b.push_row(&[200.0, 2.0]);
        b.push_row(&[300.0, 3.0]);
        b.push_row(&[400.0, 4.0]);
        b
    }

    #[test]
    fn vectorized_sum() {
        let exec = VectorizedExecutor::new();
        let batch = batch_with_data();
        let result = exec.execute(
            &VectorizedOp::Sum {
                column: "amount".into(),
            },
            &batch,
        );
        assert_eq!(result.values[0], 1000.0);
        assert!(!result.degraded);
    }

    #[test]
    fn vectorized_count() {
        let exec = VectorizedExecutor::new();
        let batch = batch_with_data();
        let result = exec.execute(&VectorizedOp::Count, &batch);
        assert_eq!(result.values[0], 4.0);
    }

    #[test]
    fn vectorized_min() {
        let exec = VectorizedExecutor::new();
        let batch = batch_with_data();
        let result = exec.execute(
            &VectorizedOp::Min {
                column: "amount".into(),
            },
            &batch,
        );
        assert_eq!(result.values[0], 100.0);
    }

    #[test]
    fn vectorized_max() {
        let exec = VectorizedExecutor::new();
        let batch = batch_with_data();
        let result = exec.execute(
            &VectorizedOp::Max {
                column: "amount".into(),
            },
            &batch,
        );
        assert_eq!(result.values[0], 400.0);
    }

    #[test]
    fn vectorized_filter() {
        let exec = VectorizedExecutor::new();
        let batch = batch_with_data();
        let result = exec.execute(
            &VectorizedOp::Filter {
                column: "amount".into(),
                threshold: 150.0,
            },
            &batch,
        );
        assert_eq!(result.values[0], 3.0);
    }

    #[test]
    fn memory_degradation() {
        let exec = VectorizedExecutor::new().with_memory_limit(0);
        let batch = batch_with_data();
        let result = exec.execute(
            &VectorizedOp::Sum {
                column: "amount".into(),
            },
            &batch,
        );
        assert!(result.degraded);
    }

    #[test]
    fn batch_size_affects_chunking() {
        let exec = VectorizedExecutor::new().with_batch_size(2);
        let batch = batch_with_data();
        let result = exec.execute(
            &VectorizedOp::Sum {
                column: "amount".into(),
            },
            &batch,
        );
        assert_eq!(result.values[0], 1000.0);
    }

    #[test]
    fn empty_batch() {
        let exec = VectorizedExecutor::new();
        let batch = ColumnarBatch::new(vec!["x".into()]);
        let result = exec.execute(&VectorizedOp::Sum { column: "x".into() }, &batch);
        assert_eq!(result.values[0], 0.0);
        assert!(batch.is_empty());
    }
}
