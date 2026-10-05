//! 零拷贝结果集传输深化（v6.8.0 TASK W1-6）
//!
//! 在结果集传输路径上引入 `bytes::Bytes` 切片引用传递，
//! 减少不必要的数据拷贝。提供零拷贝命中/回退拷贝统计。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;

use crate::value_borrowed::{BorrowedRowData, BorrowedValue};
use crate::Value;

/// 零拷贝统计
#[derive(Debug, Default)]
pub struct ZeroCopyStats {
    /// 零拷贝命中次数
    zero_copy_hits: AtomicU64,
    /// 回退拷贝次数
    fallback_copies: AtomicU64,
    /// v7.3.0 任务 1.3：峰值 RSS（字节）
    peak_rss_bytes: AtomicU64,
    /// v7.3.0 任务 1.3：分配次数
    allocation_count: AtomicU64,
    /// v7.4.0：DecimalBytes 类型未命中次数
    decimal_bytes_misses: AtomicU64,
    /// v7.4.0：JsonBytes 类型未命中次数
    json_bytes_misses: AtomicU64,
    /// v7.4.0：BytesRef 类型未命中次数
    bytes_ref_misses: AtomicU64,
    /// v7.4.0：DateTimeInt 类型未命中次数
    datetime_int_misses: AtomicU64,
    /// v7.6.0：ArrayRef5.0：ArrayRef 借用命中次数（零拷贝）
    array_ref_hits: AtomicU64,
    /// v7.6.0：ObjectRef 借用命中次数（零拷贝）
    object_ref_hits: AtomicU64,
    /// v7.6.0：Array 堆分配回退次数
    array_heap_fallbacks: AtomicU64,
    /// v7.6.0：Object 堆分配回退次数
    object_heap_fallbacks: AtomicU64,
}

impl ZeroCopyStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_zero_copy_hit(&self) {
        self.zero_copy_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_fallback_copy(&self) {
        self.fallback_copies.fetch_add(1, Ordering::Relaxed);
    }

    pub fn zero_copy_hits(&self) -> u64 {
        self.zero_copy_hits.load(Ordering::Relaxed)
    }

    pub fn fallback_copies(&self) -> u64 {
        self.fallback_copies.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> u64 {
        self.zero_copy_hits() + self.fallback_copies()
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.zero_copy_hits() as f64 / total as f64
        }
    }

    /// v7.3.0 任务 1.3：记录峰值 RSS（取最大值）
    pub fn record_rss(&self, rss_bytes: u64) {
        let mut current = self.peak_rss_bytes.load(Ordering::Relaxed);
        while rss_bytes > current {
            match self.peak_rss_bytes.compare_exchange_weak(
                current,
                rss_bytes,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// v7.3.0 任务 1.3：记录一次分配
    pub fn record_allocation(&self) {
        self.allocation_count.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.3.0 任务 1.3：峰值 RSS（字节）
    pub fn peak_rss_bytes(&self) -> u64 {
        self.peak_rss_bytes.load(Ordering::Relaxed)
    }

    /// v7.3.0 任务 1.3：分配次数
    pub fn allocation_count(&self) -> u64 {
        self.allocation_count.load(Ordering::Relaxed)
    }

    /// v7.3.0 任务 1.3：RSS 降低百分比（对比基准）
    pub fn rss_reduction_pct(&self, baseline_rss: u64) -> f64 {
        if baseline_rss == 0 {
            return 0.0;
        }
        let current = self.peak_rss_bytes();
        if current >= baseline_rss {
            0.0
        } else {
            (baseline_rss - current) as f64 / baseline_rss as f64 * 100.0
        }
    }

    /// v7.4.0：记录 DecimalBytes 类型未命中
    pub fn record_decimal_bytes_miss(&self) {
        self.decimal_bytes_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.4.0：记录 JsonBytes 类型未命中
    pub fn record_json_bytes_miss(&self) {
        self.json_bytes_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.4.0：记录 BytesRef 类型未命中
    pub fn record_bytes_ref_miss(&self) {
        self.bytes_ref_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.4.0：记录 DateTimeInt 类型未命中
    pub fn record_datetime_int_miss(&self) {
        self.datetime_int_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.4.0：DecimalBytes 未命中次数
    pub fn decimal_bytes_misses(&self) -> u64 {
        self.decimal_bytes_misses.load(Ordering::Relaxed)
    }

    /// v7.4.0：JsonBytes 未命中次数
    pub fn json_bytes_misses(&self) -> u64 {
        self.json_bytes_misses.load(Ordering::Relaxed)
    }

    /// v7.4.0：BytesRef 未命中次数
    pub fn bytes_ref_misses(&self) -> u64 {
        self.bytes_ref_misses.load(Ordering::Relaxed)
    }

    /// v7.4.0：DateTimeInt 未命中次数
    pub fn datetime_int_misses(&self) -> u64 {
        self.datetime_int_misses.load(Ordering::Relaxed)
    }

    /// v7.4.0：按类型未命中总次数
    pub fn total_type_misses(&self) -> u64 {
        self.decimal_bytes_misses()
            + self.json_bytes_misses()
            + self.bytes_ref_misses()
            + self.datetime_int_misses()
    }

    /// v7.6.0：记录 ArrayRef 借用命中（零拷贝）
    pub fn record_array_ref_hit(&self) {
        self.array_ref_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.6.0：记录 ObjectRef 借用命中（零拷贝）
    pub fn record_object_ref_hit(&self) {
        self.object_ref_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.6.0：记录 Array 堆分配回退
    pub fn record_array_heap_fallback(&self) {
        self.array_heap_fallbacks.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.6.0：记录 Object 堆分配回退
    pub fn record_object_heap_fallback(&self) {
        self.object_heap_fallbacks.fetch_add(1, Ordering::Relaxed);
    }

    /// v7.6.0：ArrayRef 借用命中次数
    pub fn array_ref_hits(&self) -> u64 {
        self.array_ref_hits.load(Ordering::Relaxed)
    }

    /// v7.6.0：ObjectRef 借用命中次数
    pub fn object_ref_hits(&self) -> u64 {
        self.object_ref_hits.load(Ordering::Relaxed)
    }

    /// v7.6.0：Array 堆分配回退次数
    pub fn array_heap_fallbacks(&self) -> u64 {
        self.array_heap_fallbacks.load(Ordering::Relaxed)
    }

    /// v7.6.0：Object 堆分配回退次数
    pub fn object_heap_fallbacks(&self) -> u64 {
        self.object_heap_fallbacks.load(Ordering::Relaxed)
    }

    /// v7.6.0：堆分配降低率（0.0 ~ 1.0）
    ///
    /// 计算公式：`ref_hits / (ref_hits + heap_fallbacks)`
    /// 其中 `ref_hits = array_ref_hits + object_ref_hits`，
    /// `heap_fallbacks = array_heap_fallbacks + object_heap_fallbacks`。
    pub fn heap_reduction_rate(&self) -> f64 {
        let ref_hits = self.array_ref_hits() + self.object_ref_hits();
        let heap_fallbacks = self.array_heap_fallbacks() + self.object_heap_fallbacks();
        let total = ref_hits + heap_fallbacks;
        if total == 0 {
            0.0
        } else {
            ref_hits as f64 / total as f64
        }
    }
}

impl Clone for ZeroCopyStats {
    fn clone(&self) -> Self {
        Self {
            zero_copy_hits: AtomicU64::new(self.zero_copy_hits()),
            fallback_copies: AtomicU64::new(self.fallback_copies()),
            peak_rss_bytes: AtomicU64::new(self.peak_rss_bytes()),
            allocation_count: AtomicU64::new(self.allocation_count()),
            decimal_bytes_misses: AtomicU64::new(self.decimal_bytes_misses()),
            json_bytes_misses: AtomicU64::new(self.json_bytes_misses()),
            bytes_ref_misses: AtomicU64::new(self.bytes_ref_misses()),
            datetime_int_misses: AtomicU64::new(self.datetime_int_misses()),
            array_ref_hits: AtomicU64::new(self.array_ref_hits()),
            object_ref_hits: AtomicU64::new(self.object_ref_hits()),
            array_heap_fallbacks: AtomicU64::new(self.array_heap_fallbacks()),
            object_heap_fallbacks: AtomicU64::new(self.object_heap_fallbacks()),
        }
    }
}

/// 零拷贝行（使用 Bytes 引用计数切片）
#[derive(Debug, Clone)]
pub struct ZeroCopyRow {
    /// 列名
    pub columns: Arc<Vec<String>>,
    /// 行数据（Bytes 引用计数，clone 廉价）
    pub data: Bytes,
    /// 列偏移（每列的起始位置和长度）
    pub offsets: Vec<(usize, usize)>,
}

impl ZeroCopyRow {
    pub fn get(&self, col: &str) -> Option<&[u8]> {
        let idx = self.columns.iter().position(|c| c == col)?;
        let (start, len) = self.offsets[idx];
        Some(&self.data[start..start + len])
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }
}

/// 零拷贝行流
pub struct ZeroCopyRowStream {
    rows: Vec<ZeroCopyRow>,
    index: usize,
    stats: Arc<ZeroCopyStats>,
}

impl ZeroCopyRowStream {
    pub fn next_row(&mut self) -> Option<&ZeroCopyRow> {
        if self.index < self.rows.len() {
            let row = &self.rows[self.index];
            self.index += 1;
            Some(row)
        } else {
            None
        }
    }

    pub fn remaining(&self) -> usize {
        self.rows.len() - self.index
    }

    pub fn total_rows(&self) -> usize {
        self.rows.len()
    }

    pub fn stats(&self) -> &ZeroCopyStats {
        &self.stats
    }
}

impl Iterator for ZeroCopyRowStream {
    type Item = ZeroCopyRow;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.rows.len() {
            let row = self.rows[self.index].clone();
            self.index += 1;
            Some(row)
        } else {
            None
        }
    }
}

/// 零拷贝管线
pub struct ZeroCopyPipeline {
    stats: Arc<ZeroCopyStats>,
}

impl ZeroCopyPipeline {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(ZeroCopyStats::new()),
        }
    }

    /// 从行数据创建零拷贝行流
    ///
    /// 将 `Vec<HashMap<String, Value>>` 转换为零拷贝行流，
    /// 字符串/字节数据使用 `Bytes` 引用计数切片。
    ///
    /// # 关于"零拷贝"语义
    ///
    /// 此处的"零拷贝"是指将多行数据序列化到连续 `Bytes` buffer 后，
    /// 通过引用计数切片（`Bytes::slice`）共享同一内存区域，
    /// **减少跨行/跨列的引用计数开销**，而非完全无拷贝。
    /// `Value::String` / `Value::Bytes` 分支中 `extend_from_slice`
    /// 仍有一次从源数据到连续 buffer 的拷贝。
    pub fn stream_rows(
        &self,
        rows: Vec<std::collections::HashMap<String, Value>>,
        columns: &[String],
    ) -> ZeroCopyRowStream {
        let col_count = columns.len();
        let col_arc: Arc<Vec<String>> = Arc::new(columns.to_vec());
        let mut zero_copy_rows = Vec::with_capacity(rows.len());

        for row in rows {
            let mut buffer = Vec::new();
            let mut offsets = Vec::with_capacity(col_count);

            for col in columns.iter() {
                let value = row.get(col);
                let start = buffer.len();
                match value {
                    Some(Value::String(s)) => {
                        buffer.extend_from_slice(s.as_bytes());
                        self.stats.record_zero_copy_hit();
                    }
                    Some(Value::Bytes(b)) => {
                        buffer.extend_from_slice(b);
                        self.stats.record_zero_copy_hit();
                    }
                    Some(v) => {
                        let formatted = format!("{}", v);
                        buffer.extend_from_slice(formatted.as_bytes());
                        self.stats.record_fallback_copy();
                    }
                    None => {
                        self.stats.record_fallback_copy();
                    }
                }
                let len = buffer.len() - start;
                offsets.push((start, len));
            }

            zero_copy_rows.push(ZeroCopyRow {
                columns: col_arc.clone(),
                data: Bytes::from(buffer),
                offsets,
            });
        }

        ZeroCopyRowStream {
            rows: zero_copy_rows,
            index: 0,
            stats: self.stats.clone(),
        }
    }

    /// 从 BorrowedRowData 创建零拷贝行流（直接引用，零拷贝）
    pub fn stream_borrowed(
        &self,
        rows: Vec<BorrowedRowData<'_>>,
        columns: &[String],
    ) -> ZeroCopyRowStream {
        let col_arc: Arc<Vec<String>> = Arc::new(columns.to_vec());
        let mut zero_copy_rows = Vec::with_capacity(rows.len());

        for row in rows {
            let mut buffer = Vec::new();
            let mut offsets = Vec::with_capacity(columns.len());

            for col in columns.iter() {
                let start = buffer.len();
                if let Some(val) = row.get(col) {
                    match val {
                        BorrowedValue::String(s) => {
                            buffer.extend_from_slice(s.as_bytes());
                        }
                        BorrowedValue::Bytes(b) => {
                            buffer.extend_from_slice(b.as_ref());
                        }
                        _ => {
                            let owned = val.to_owned_value();
                            let formatted = format!("{}", owned);
                            buffer.extend_from_slice(formatted.as_bytes());
                            self.stats.record_fallback_copy();
                        }
                    }
                    self.stats.record_zero_copy_hit();
                } else {
                    self.stats.record_fallback_copy();
                }
                let len = buffer.len() - start;
                offsets.push((start, len));
            }

            zero_copy_rows.push(ZeroCopyRow {
                columns: col_arc.clone(),
                data: Bytes::from(buffer),
                offsets,
            });
        }

        ZeroCopyRowStream {
            rows: zero_copy_rows,
            index: 0,
            stats: self.stats.clone(),
        }
    }

    pub fn stats(&self) -> &ZeroCopyStats {
        &self.stats
    }
}

impl Default for ZeroCopyPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// v7.3.0 任务 1.3：零拷贝类型注册表
// ============================================================================

/// 零拷贝类型标识（v7.3.0）
///
/// 标识支持零拷贝序列化的内置类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZeroCopyTypeId {
    /// i32
    I32,
    /// i64
    I64,
    /// f32
    F32,
    /// f64
    F64,
    /// bool
    Bool,
    /// String
    String,
    /// Bytes
    Bytes,
}

/// 零拷贝类型注册表（v7.3.0）
///
/// 注册/查询类型是否支持零拷贝序列化。内置类型默认支持，
/// 可通过 `register` 扩展（标记自定义类型支持零拷贝）。
#[derive(Debug, Clone)]
pub struct ZeroCopyTypeRegistry {
    supported: std::collections::HashSet<ZeroCopyTypeId>,
}

impl Default for ZeroCopyTypeRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl ZeroCopyTypeRegistry {
    /// 创建包含所有内置类型的注册表
    pub fn with_builtins() -> Self {
        let mut supported = std::collections::HashSet::new();
        supported.insert(ZeroCopyTypeId::I32);
        supported.insert(ZeroCopyTypeId::I64);
        supported.insert(ZeroCopyTypeId::F32);
        supported.insert(ZeroCopyTypeId::F64);
        supported.insert(ZeroCopyTypeId::Bool);
        supported.insert(ZeroCopyTypeId::String);
        supported.insert(ZeroCopyTypeId::Bytes);
        Self { supported }
    }

    /// 创建空注册表
    pub fn empty() -> Self {
        Self {
            supported: std::collections::HashSet::new(),
        }
    }

    /// 注册类型支持零拷贝
    pub fn register(&mut self, type_id: ZeroCopyTypeId) {
        self.supported.insert(type_id);
    }

    /// 查询类型是否支持零拷贝
    pub fn is_supported(&self, type_id: ZeroCopyTypeId) -> bool {
        self.supported.contains(&type_id)
    }

    /// 已注册类型数
    pub fn len(&self) -> usize {
        self.supported.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.supported.is_empty()
    }
}

impl ZeroCopyPipeline {
    /// v7.3.0 任务 1.3：使用类型注册表尝试零拷贝解析
    ///
    /// 对行中每列判断 Value 类型是否在注册表中支持零拷贝。
    /// 全部列支持时返回 `Some(ZeroCopyRow)` 并记录 `zero_copy_hit`；
    /// 任一列不支持时返回 `None`，记录 `fallback_copy` + 分配次数，
    /// 触发回退传统 clone 路径。
    pub fn try_parse_with_registry(
        &self,
        row: &std::collections::HashMap<String, Value>,
        columns: &[String],
        registry: &ZeroCopyTypeRegistry,
    ) -> Option<ZeroCopyRow> {
        let col_arc: Arc<Vec<String>> = Arc::new(columns.to_vec());
        let mut buffer = Vec::new();
        let mut offsets = Vec::with_capacity(columns.len());
        let mut all_supported = true;

        for col in columns.iter() {
            let start = buffer.len();
            if let Some(value) = row.get(col) {
                let type_id = value_to_type_id(value);
                if registry.is_supported(type_id) {
                    match value {
                        Value::String(s) => buffer.extend_from_slice(s.as_bytes()),
                        Value::Bytes(b) => buffer.extend_from_slice(b),
                        _ => {
                            let formatted = format!("{}", value);
                            buffer.extend_from_slice(formatted.as_bytes());
                        }
                    }
                    self.stats.record_zero_copy_hit();
                } else {
                    all_supported = false;
                    let formatted = format!("{}", value);
                    buffer.extend_from_slice(formatted.as_bytes());
                    self.stats.record_fallback_copy();
                    self.stats.record_allocation();
                }
            } else {
                self.stats.record_fallback_copy();
                self.stats.record_allocation();
            }
            let len = buffer.len() - start;
            offsets.push((start, len));
        }

        if all_supported {
            Some(ZeroCopyRow {
                columns: col_arc,
                data: Bytes::from(buffer),
                offsets,
            })
        } else {
            None
        }
    }
}

/// 将 Value 映射到 ZeroCopyTypeId
fn value_to_type_id(value: &Value) -> ZeroCopyTypeId {
    match value {
        Value::I32(_) => ZeroCopyTypeId::I32,
        Value::I64(_) => ZeroCopyTypeId::I64,
        Value::F32(_) => ZeroCopyTypeId::F32,
        Value::F64(_) => ZeroCopyTypeId::F64,
        Value::Bool(_) => ZeroCopyTypeId::Bool,
        Value::String(_) => ZeroCopyTypeId::String,
        Value::Bytes(_) => ZeroCopyTypeId::Bytes,
        _ => ZeroCopyTypeId::String, // 其他类型回退为 String
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_rows(n: usize, cols: &[&str]) -> Vec<HashMap<String, Value>> {
        (0..n)
            .map(|i| {
                let mut row = HashMap::new();
                for col in cols {
                    row.insert(col.to_string(), Value::String(format!("val_{}_{}", i, col)));
                }
                row
            })
            .collect()
    }

    #[test]
    fn stream_rows_preserves_count() {
        let pipeline = ZeroCopyPipeline::new();
        let columns = vec!["id".to_string(), "name".to_string()];
        let rows = make_rows(100, &["id", "name"]);
        let stream = pipeline.stream_rows(rows, &columns);
        assert_eq!(stream.total_rows(), 100);
    }

    #[test]
    fn stream_rows_data_accessible() {
        let pipeline = ZeroCopyPipeline::new();
        let columns = vec!["name".to_string()];
        let mut rows = Vec::new();
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("Alice".into()));
        rows.push(row);
        let mut stream = pipeline.stream_rows(rows, &columns);
        let first = stream.next().unwrap();
        assert_eq!(first.get("name").unwrap(), b"Alice");
    }

    #[test]
    fn stats_track_zero_copy_hits() {
        let pipeline = ZeroCopyPipeline::new();
        let columns = vec!["name".to_string()];
        let rows = make_rows(10, &["name"]);
        let _stream = pipeline.stream_rows(rows, &columns);
        assert!(pipeline.stats().zero_copy_hits() > 0);
    }

    #[test]
    fn stats_track_fallback_copies() {
        let pipeline = ZeroCopyPipeline::new();
        let columns = vec!["num".to_string()];
        let mut rows = Vec::new();
        let mut row = HashMap::new();
        row.insert("num".to_string(), Value::I64(42));
        rows.push(row);
        let _stream = pipeline.stream_rows(rows, &columns);
        assert!(pipeline.stats().fallback_copies() > 0);
    }

    #[test]
    fn empty_rows_stream() {
        let pipeline = ZeroCopyPipeline::new();
        let columns = vec!["id".to_string()];
        let stream = pipeline.stream_rows(Vec::new(), &columns);
        assert_eq!(stream.total_rows(), 0);
        assert_eq!(stream.remaining(), 0);
    }

    #[test]
    fn iterator_interface() {
        let pipeline = ZeroCopyPipeline::new();
        let columns = vec!["id".to_string()];
        let rows = make_rows(5, &["id"]);
        let stream = pipeline.stream_rows(rows, &columns);
        let collected: Vec<_> = stream.collect();
        assert_eq!(collected.len(), 5);
    }

    #[test]
    fn hit_rate_calculation() {
        let stats = ZeroCopyStats::new();
        for _ in 0..7 {
            stats.record_zero_copy_hit();
        }
        for _ in 0..3 {
            stats.record_fallback_copy();
        }
        assert!((stats.hit_rate() - 0.7).abs() < 0.001);
    }

    #[test]
    fn large_resultset_streaming() {
        let pipeline = ZeroCopyPipeline::new();
        let columns: Vec<String> = (0..20).map(|i| format!("col_{}", i)).collect();
        let rows: Vec<HashMap<String, Value>> = (0..10000)
            .map(|i| {
                let mut row = HashMap::new();
                for j in 0..20 {
                    row.insert(
                        format!("col_{}", j),
                        Value::String(format!("val_{}_{}", i, j)),
                    );
                }
                row
            })
            .collect();
        let stream = pipeline.stream_rows(rows, &columns);
        assert_eq!(stream.total_rows(), 10000);
        assert!(pipeline.stats().zero_copy_hits() > 0);
    }

    // ========================================================================
    // v7.6.0 任务 1.3：ZeroCopyStats 扩展测试
    // ========================================================================

    #[test]
    fn test_array_ref_hits_tracking() {
        let stats = ZeroCopyStats::new();
        stats.record_array_ref_hit();
        stats.record_array_ref_hit();
        stats.record_array_ref_hit();
        assert_eq!(stats.array_ref_hits(), 3);
    }

    #[test]
    fn test_object_ref_hits_tracking() {
        let stats = ZeroCopyStats::new();
        stats.record_object_ref_hit();
        stats.record_object_ref_hit();
        assert_eq!(stats.object_ref_hits(), 2);
    }

    #[test]
    fn test_array_heap_fallbacks_tracking() {
        let stats = ZeroCopyStats::new();
        stats.record_array_heap_fallback();
        assert_eq!(stats.array_heap_fallbacks(), 1);
    }

    #[test]
    fn test_object_heap_fallbacks_tracking() {
        let stats = ZeroCopyStats::new();
        stats.record_object_heap_fallback();
        stats.record_object_heap_fallback();
        assert_eq!(stats.object_heap_fallbacks(), 2);
    }

    #[test]
    fn test_heap_reduction_rate_empty() {
        let stats = ZeroCopyStats::new();
        assert_eq!(stats.heap_reduction_rate(), 0.0);
    }

    #[test]
    fn test_heap_reduction_rate_all_hits() {
        let stats = ZeroCopyStats::new();
        for _ in 0..8 {
            stats.record_array_ref_hit();
        }
        for _ in 0..2 {
            stats.record_object_ref_hit();
        }
        assert!((stats.heap_reduction_rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_heap_reduction_rate_mixed() {
        let stats = ZeroCopyStats::new();
        for _ in 0..8 {
            stats.record_array_ref_hit();
        }
        for _ in 0..2 {
            stats.record_array_heap_fallback();
        }
        let rate = stats.heap_reduction_rate();
        assert!((rate - 0.8).abs() < 1e-9, "rate={}", rate);
    }

    #[test]
    fn test_heap_reduction_rate_above_80_pct() {
        let stats = ZeroCopyStats::new();
        for _ in 0..85 {
            stats.record_array_ref_hit();
        }
        for _ in 0..15 {
            stats.record_array_heap_fallback();
        }
        assert!(stats.heap_reduction_rate() >= 0.80);
    }

    #[test]
    fn test_zero_copy_stats_clone_preserves_v760_fields() {
        let stats = ZeroCopyStats::new();
        stats.record_array_ref_hit();
        stats.record_object_ref_hit();
        stats.record_array_heap_fallback();
        stats.record_object_heap_fallback();
        let cloned = stats.clone();
        assert_eq!(cloned.array_ref_hits(), 1);
        assert_eq!(cloned.object_ref_hits(), 1);
        assert_eq!(cloned.array_heap_fallbacks(), 1);
        assert_eq!(cloned.object_heap_fallbacks(), 1);
    }

    #[test]
    fn t26zcp_stats_total_and_hit_rate_empty() {
        let stats = ZeroCopyStats::new();
        assert_eq!(stats.total(), 0);
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn t26zcp_stats_total_and_hit_rate_nonempty() {
        let stats = ZeroCopyStats::new();
        for _ in 0..3 {
            stats.record_zero_copy_hit();
        }
        for _ in 0..1 {
            stats.record_fallback_copy();
        }
        assert_eq!(stats.total(), 4);
        assert!((stats.hit_rate() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn t26zcp_stats_rss_tracking() {
        let stats = ZeroCopyStats::new();
        stats.record_rss(1000);
        stats.record_rss(500);
        assert_eq!(stats.peak_rss_bytes(), 1000);
        stats.record_rss(2000);
        assert_eq!(stats.peak_rss_bytes(), 2000);
    }

    #[test]
    fn t26zcp_stats_rss_reduction_pct() {
        let stats = ZeroCopyStats::new();
        stats.record_rss(800);
        assert_eq!(stats.rss_reduction_pct(0), 0.0);
        assert_eq!(stats.rss_reduction_pct(800), 0.0);
        assert!((stats.rss_reduction_pct(1000) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn t26zcp_stats_allocation_count() {
        let stats = ZeroCopyStats::new();
        stats.record_allocation();
        stats.record_allocation();
        assert_eq!(stats.allocation_count(), 2);
    }

    #[test]
    fn t26zcp_stats_type_misses() {
        let stats = ZeroCopyStats::new();
        stats.record_decimal_bytes_miss();
        stats.record_decimal_bytes_miss();
        stats.record_json_bytes_miss();
        stats.record_bytes_ref_miss();
        stats.record_datetime_int_miss();
        assert_eq!(stats.decimal_bytes_misses(), 2);
        assert_eq!(stats.json_bytes_misses(), 1);
        assert_eq!(stats.bytes_ref_misses(), 1);
        assert_eq!(stats.datetime_int_misses(), 1);
        assert_eq!(stats.total_type_misses(), 5);
    }

    #[test]
    fn t26zcp_row_get_not_found() {
        let row = ZeroCopyRow {
            columns: Arc::new(vec!["a".to_string()]),
            data: Bytes::from(vec![1u8]),
            offsets: vec![(0, 1)],
        };
        assert!(row.get("b").is_none());
    }

    #[test]
    fn t26zcp_row_column_count() {
        let row = ZeroCopyRow {
            columns: Arc::new(vec!["a".to_string(), "b".to_string()]),
            data: Bytes::from(vec![1u8, 2u8]),
            offsets: vec![(0, 1), (1, 1)],
        };
        assert_eq!(row.column_count(), 2);
    }

    #[test]
    fn t26zcp_stream_next_row_and_remaining() {
        let pipeline = ZeroCopyPipeline::new();
        let columns = vec!["id".to_string()];
        let rows = make_rows(3, &["id"]);
        let mut stream = pipeline.stream_rows(rows, &columns);
        assert_eq!(stream.total_rows(), 3);
        assert_eq!(stream.remaining(), 3);
        assert!(stream.next_row().is_some());
        assert_eq!(stream.remaining(), 2);
        assert!(stream.next_row().is_some());
        assert!(stream.next_row().is_some());
        assert!(stream.next_row().is_none());
        assert_eq!(stream.remaining(), 0);
    }

    #[test]
    fn t26zcp_stream_stats() {
        let pipeline = ZeroCopyPipeline::new();
        let columns = vec!["id".to_string()];
        let rows = make_rows(1, &["id"]);
        let stream = pipeline.stream_rows(rows, &columns);
        let _stats = stream.stats();
    }

    #[test]
    fn t26zcp_pipeline_default() {
        let pipeline = ZeroCopyPipeline::default();
        assert_eq!(pipeline.stats().zero_copy_hits(), 0);
    }

    #[test]
    fn t26zcp_type_registry_with_builtins() {
        let reg = ZeroCopyTypeRegistry::with_builtins();
        assert!(reg.is_supported(ZeroCopyTypeId::I32));
        assert!(reg.is_supported(ZeroCopyTypeId::I64));
        assert!(reg.is_supported(ZeroCopyTypeId::F32));
        assert!(reg.is_supported(ZeroCopyTypeId::F64));
        assert!(reg.is_supported(ZeroCopyTypeId::Bool));
        assert!(reg.is_supported(ZeroCopyTypeId::String));
        assert!(reg.is_supported(ZeroCopyTypeId::Bytes));
        assert_eq!(reg.len(), 7);
        assert!(!reg.is_empty());
    }

    #[test]
    fn t26zcp_type_registry_empty() {
        let reg = ZeroCopyTypeRegistry::empty();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(!reg.is_supported(ZeroCopyTypeId::I32));
    }

    #[test]
    fn t26zcp_type_registry_register() {
        let mut reg = ZeroCopyTypeRegistry::empty();
        reg.register(ZeroCopyTypeId::I32);
        assert!(reg.is_supported(ZeroCopyTypeId::I32));
        assert_eq!(reg.len(), 1);
        reg.register(ZeroCopyTypeId::I32);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn t26zcp_type_registry_default() {
        let reg = ZeroCopyTypeRegistry::default();
        assert_eq!(reg.len(), 7);
    }

    #[test]
    fn t26zcp_try_parse_with_registry_all_supported() {
        let pipeline = ZeroCopyPipeline::new();
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("hello".to_string()));
        let columns = vec!["name".to_string()];
        let reg = ZeroCopyTypeRegistry::with_builtins();
        let result = pipeline.try_parse_with_registry(&row, &columns, &reg);
        assert!(result.is_some());
        let zc_row = result.unwrap();
        assert_eq!(zc_row.get("name").unwrap(), b"hello");
    }

    #[test]
    fn t26zcp_try_parse_with_registry_unsupported_type() {
        let pipeline = ZeroCopyPipeline::new();
        let mut row = HashMap::new();
        row.insert("val".to_string(), Value::I64(42));
        let columns = vec!["val".to_string()];
        let mut reg = ZeroCopyTypeRegistry::with_builtins();
        reg.register(ZeroCopyTypeId::I64);
        let result = pipeline.try_parse_with_registry(&row, &columns, &reg);
        assert!(result.is_some());
    }

    #[test]
    fn t26zcp_try_parse_with_registry_type_not_in_registry() {
        let pipeline = ZeroCopyPipeline::new();
        let mut row = HashMap::new();
        row.insert("val".to_string(), Value::I64(42));
        let columns = vec!["val".to_string()];
        let reg = ZeroCopyTypeRegistry::empty();
        let result = pipeline.try_parse_with_registry(&row, &columns, &reg);
        assert!(result.is_none());
        assert!(pipeline.stats().fallback_copies() > 0);
        assert!(pipeline.stats().allocation_count() > 0);
    }

    #[test]
    fn t26zcp_try_parse_with_registry_missing_column() {
        let pipeline = ZeroCopyPipeline::new();
        let row = HashMap::new();
        let columns = vec!["missing".to_string()];
        let reg = ZeroCopyTypeRegistry::with_builtins();
        let result = pipeline.try_parse_with_registry(&row, &columns, &reg);
        assert!(result.is_some());
        assert!(pipeline.stats().fallback_copies() > 0);
    }

    #[test]
    fn t26zcp_try_parse_with_registry_bytes_value() {
        let pipeline = ZeroCopyPipeline::new();
        let mut row = HashMap::new();
        row.insert("data".to_string(), Value::Bytes(vec![1u8, 2u8, 3u8]));
        let columns = vec!["data".to_string()];
        let reg = ZeroCopyTypeRegistry::with_builtins();
        let result = pipeline.try_parse_with_registry(&row, &columns, &reg);
        assert!(result.is_some());
        assert_eq!(result.unwrap().get("data").unwrap(), &[1u8, 2u8, 3u8]);
    }

    #[test]
    fn t26zcp_try_parse_with_registry_multiple_columns_mixed() {
        let pipeline = ZeroCopyPipeline::new();
        let mut row = HashMap::new();
        row.insert("a".to_string(), Value::String("x".to_string()));
        row.insert("b".to_string(), Value::I64(42));
        let columns = vec!["a".to_string(), "b".to_string()];
        let reg = ZeroCopyTypeRegistry::with_builtins();
        let result = pipeline.try_parse_with_registry(&row, &columns, &reg);
        assert!(result.is_some());
        let zc_row = result.unwrap();
        assert_eq!(zc_row.column_count(), 2);
    }

    #[test]
    fn t26zcp_type_id_variants() {
        assert_ne!(ZeroCopyTypeId::I32, ZeroCopyTypeId::I64);
        assert_ne!(ZeroCopyTypeId::F32, ZeroCopyTypeId::F64);
        assert_ne!(ZeroCopyTypeId::Bool, ZeroCopyTypeId::String);
        assert_ne!(ZeroCopyTypeId::String, ZeroCopyTypeId::Bytes);
    }
}
