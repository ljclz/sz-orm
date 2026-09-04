//! 流式操作符：过滤、映射、聚合
//!
//! 提供流式数据处理的常用操作符。

use std::collections::HashMap;

use serde_json::Value;

/// 流式过滤器
///
/// 按条件过滤流中的元素。
#[derive(Debug, Clone)]
pub struct StreamFilter {
    /// 字段名
    field: String,
    /// 过滤条件
    condition: FilterCondition,
}

/// 过滤条件
#[derive(Debug, Clone, PartialEq)]
pub enum FilterCondition {
    /// 等于
    Eq(Value),
    /// 不等于
    NotEq(Value),
    /// 大于
    Gt(Value),
    /// 大于等于
    Ge(Value),
    /// 小于
    Lt(Value),
    /// 小于等于
    Le(Value),
    /// 在集合中
    In(Vec<Value>),
    /// 不在集合中
    NotIn(Vec<Value>),
    /// 为空
    IsNull,
    /// 不为空
    IsNotNull,
    /// LIKE 模糊匹配
    Like(String),
}

impl StreamFilter {
    /// 创建过滤器
    pub fn new(field: impl Into<String>, condition: FilterCondition) -> Self {
        Self {
            field: field.into(),
            condition,
        }
    }

    /// 字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 条件引用
    pub fn condition(&self) -> &FilterCondition {
        &self.condition
    }

    /// 测试单行是否满足条件
    pub fn test(&self, row: &Value) -> bool {
        let field_value = row.get(&self.field).unwrap_or(&Value::Null);
        match &self.condition {
            FilterCondition::Eq(v) => field_value == v,
            FilterCondition::NotEq(v) => field_value != v,
            FilterCondition::Gt(v) => compare_values(field_value, v) == std::cmp::Ordering::Greater,
            FilterCondition::Ge(v) => {
                matches!(
                    compare_values(field_value, v),
                    std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
                )
            }
            FilterCondition::Lt(v) => compare_values(field_value, v) == std::cmp::Ordering::Less,
            FilterCondition::Le(v) => {
                matches!(
                    compare_values(field_value, v),
                    std::cmp::Ordering::Less | std::cmp::Ordering::Equal
                )
            }
            FilterCondition::In(values) => values.contains(field_value),
            FilterCondition::NotIn(values) => !values.contains(field_value),
            FilterCondition::IsNull => field_value == &Value::Null,
            FilterCondition::IsNotNull => field_value != &Value::Null,
            FilterCondition::Like(pattern) => like_match(field_value, pattern),
        }
    }

    /// 过滤一批行
    pub fn apply(&self, rows: &[Value]) -> Vec<Value> {
        rows.iter().filter(|r| self.test(r)).cloned().collect()
    }
}

/// 比较两个 JSON 值（仅支持数字和字符串）
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let af = a.as_f64().unwrap_or(0.0);
            let bf = b.as_f64().unwrap_or(0.0);
            af.partial_cmp(&bf).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Value::String(a), Value::String(b)) => a.cmp(b),
        _ => std::cmp::Ordering::Equal,
    }
}

/// LIKE 模式匹配（支持 % 通配符）
fn like_match(value: &Value, pattern: &str) -> bool {
    let s = match value {
        Value::String(s) => s.as_str(),
        _ => return false,
    };
    if pattern == "%" {
        return true;
    }
    if !pattern.contains('%') {
        return s == pattern;
    }
    let parts: Vec<&str> = pattern.split('%').collect();
    if parts.len() == 2 {
        let prefix = parts[0];
        let suffix = parts[1];
        s.starts_with(prefix) && s.ends_with(suffix) && s.len() >= prefix.len() + suffix.len()
    } else {
        s.contains(pattern.replace('%', "").as_str())
    }
}

/// 流式映射器
///
/// 对流中的元素执行字段映射/转换。
#[derive(Debug, Clone)]
pub struct StreamMapper {
    /// 映射规则：目标字段 -> 源字段
    mappings: HashMap<String, String>,
    /// 是否保留未映射字段
    keep_unmapped: bool,
}

impl StreamMapper {
    /// 创建空映射器
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
            keep_unmapped: false,
        }
    }

    /// 添加字段映射
    pub fn map(mut self, source: impl Into<String>, target: impl Into<String>) -> Self {
        self.mappings.insert(target.into(), source.into());
        self
    }

    /// 保留未映射字段
    pub fn keep_unmapped(mut self) -> Self {
        self.keep_unmapped = true;
        self
    }

    /// 映射单行
    pub fn apply(&self, row: &Value) -> Value {
        let obj = match row.as_object() {
            Some(o) => o,
            None => return row.clone(),
        };
        let mut result = serde_json::Map::new();
        for (target, source) in &self.mappings {
            if let Some(v) = obj.get(source) {
                result.insert(target.clone(), v.clone());
            }
        }
        if self.keep_unmapped {
            for (k, v) in obj {
                if !self.mappings.contains_key(k) {
                    result.insert(k.clone(), v.clone());
                }
            }
        }
        Value::Object(result)
    }

    /// 映射一批行
    pub fn apply_batch(&self, rows: &[Value]) -> Vec<Value> {
        rows.iter().map(|r| self.apply(r)).collect()
    }

    /// 映射规则数
    pub fn mapping_count(&self) -> usize {
        self.mappings.len()
    }
}

impl Default for StreamMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// 聚合函数类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateFunction {
    /// 计数
    Count,
    /// 求和
    Sum,
    /// 平均值
    Avg,
    /// 最小值
    Min,
    /// 最大值
    Max,
}

impl AggregateFunction {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            AggregateFunction::Count => "count",
            AggregateFunction::Sum => "sum",
            AggregateFunction::Avg => "avg",
            AggregateFunction::Min => "min",
            AggregateFunction::Max => "max",
        }
    }
}

/// 聚合结果
#[derive(Debug, Clone)]
pub struct AggregateResult {
    /// 聚合函数
    pub function: AggregateFunction,
    /// 字段名
    pub field: String,
    /// 结果值
    pub value: f64,
    /// 参与聚合的行数
    pub row_count: usize,
}

impl AggregateResult {
    /// 创建聚合结果
    pub fn new(
        function: AggregateFunction,
        field: impl Into<String>,
        value: f64,
        row_count: usize,
    ) -> Self {
        Self {
            function,
            field: field.into(),
            value,
            row_count,
        }
    }
}

/// 流式聚合器
///
/// 对流中的元素执行聚合计算。
#[derive(Debug, Clone)]
pub struct StreamAggregator {
    /// 聚合函数
    function: AggregateFunction,
    /// 聚合字段
    field: String,
    /// 累计值
    accumulator: f64,
    /// 行数
    row_count: usize,
}

impl StreamAggregator {
    /// 创建聚合器
    pub fn new(function: AggregateFunction, field: impl Into<String>) -> Self {
        Self {
            function,
            field: field.into(),
            accumulator: 0.0,
            row_count: 0,
        }
    }

    /// 添加单行
    pub fn add(&mut self, row: &Value) {
        self.row_count += 1;
        let v = self.extract_numeric(row);
        match self.function {
            AggregateFunction::Count => {
                // Count 不需要值
            }
            AggregateFunction::Sum | AggregateFunction::Avg => {
                self.accumulator += v;
            }
            AggregateFunction::Min => {
                if self.row_count == 1 || v < self.accumulator {
                    self.accumulator = v;
                }
            }
            AggregateFunction::Max => {
                if self.row_count == 1 || v > self.accumulator {
                    self.accumulator = v;
                }
            }
        }
    }

    /// 批量添加
    pub fn add_batch(&mut self, rows: &[Value]) {
        for row in rows {
            self.add(row);
        }
    }

    /// 提取数值
    fn extract_numeric(&self, row: &Value) -> f64 {
        match row.get(&self.field) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    /// 计算结果
    pub fn result(&self) -> AggregateResult {
        let value = match self.function {
            AggregateFunction::Count => self.row_count as f64,
            AggregateFunction::Sum => self.accumulator,
            AggregateFunction::Avg => {
                if self.row_count == 0 {
                    0.0
                } else {
                    self.accumulator / self.row_count as f64
                }
            }
            AggregateFunction::Min | AggregateFunction::Max => self.accumulator,
        };
        AggregateResult::new(self.function, self.field.clone(), value, self.row_count)
    }

    /// 重置聚合器
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
        self.row_count = 0;
    }

    /// 行数
    pub fn row_count(&self) -> usize {
        self.row_count
    }
}

/// 多字段聚合器
///
/// 同时对多个字段执行不同聚合函数。
#[derive(Debug, Clone)]
pub struct MultiAggregator {
    aggregators: Vec<StreamAggregator>,
}

impl MultiAggregator {
    /// 创建空聚合器
    pub fn new() -> Self {
        Self {
            aggregators: Vec::new(),
        }
    }

    /// 添加聚合字段
    pub fn add_field(mut self, function: AggregateFunction, field: impl Into<String>) -> Self {
        self.aggregators
            .push(StreamAggregator::new(function, field));
        self
    }

    /// 添加单行
    pub fn add(&mut self, row: &Value) {
        for agg in &mut self.aggregators {
            agg.add(row);
        }
    }

    /// 批量添加
    pub fn add_batch(&mut self, rows: &[Value]) {
        for row in rows {
            self.add(row);
        }
    }

    /// 所有聚合结果
    pub fn results(&self) -> Vec<AggregateResult> {
        self.aggregators.iter().map(|a| a.result()).collect()
    }

    /// 聚合器数
    pub fn count(&self) -> usize {
        self.aggregators.len()
    }

    /// 重置所有
    pub fn reset(&mut self) {
        for agg in &mut self.aggregators {
            agg.reset();
        }
    }
}

impl Default for MultiAggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// 排序方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    /// 升序
    Asc,
    /// 降序
    Desc,
}

impl SortDirection {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            SortDirection::Asc => "asc",
            SortDirection::Desc => "desc",
        }
    }
}

/// 流式排序器
///
/// 对一批数据按指定字段排序。
#[derive(Debug, Clone)]
pub struct StreamSorter {
    /// 排序字段
    field: String,
    /// 排序方向
    direction: SortDirection,
}

impl StreamSorter {
    /// 创建排序器
    pub fn new(field: impl Into<String>, direction: SortDirection) -> Self {
        Self {
            field: field.into(),
            direction,
        }
    }

    /// 排序字段
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 排序方向
    pub fn direction(&self) -> SortDirection {
        self.direction
    }

    /// 对一批数据排序
    pub fn sort(&self, rows: &mut [Value]) {
        let field = self.field.clone();
        match self.direction {
            SortDirection::Asc => rows.sort_by(|a, b| {
                compare_values(
                    a.get(&field).unwrap_or(&Value::Null),
                    b.get(&field).unwrap_or(&Value::Null),
                )
            }),
            SortDirection::Desc => rows.sort_by(|a, b| {
                compare_values(
                    b.get(&field).unwrap_or(&Value::Null),
                    a.get(&field).unwrap_or(&Value::Null),
                )
            }),
        }
    }

    /// 返回排序后的新向量
    pub fn apply(&self, rows: &[Value]) -> Vec<Value> {
        let mut result = rows.to_vec();
        self.sort(&mut result);
        result
    }
}

/// 流式去重器
///
/// 按指定字段去重，保留首次出现的行。
#[derive(Debug, Clone)]
pub struct StreamDeduplicator {
    /// 去重字段
    field: String,
}

impl StreamDeduplicator {
    /// 创建去重器
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
        }
    }

    /// 去重字段
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 对一批数据去重
    pub fn apply(&self, rows: &[Value]) -> Vec<Value> {
        let mut seen: Vec<Value> = Vec::new();
        let mut result = Vec::new();
        for row in rows {
            let key = row.get(&self.field).cloned().unwrap_or(Value::Null);
            if !seen.contains(&key) {
                seen.push(key);
                result.push(row.clone());
            }
        }
        result
    }

    /// 去重后的行数
    pub fn deduplicated_count(&self, rows: &[Value]) -> usize {
        self.apply(rows).len()
    }

    /// 被去除的行数
    pub fn removed_count(&self, rows: &[Value]) -> usize {
        rows.len() - self.deduplicated_count(rows)
    }
}

/// 流式限制器
///
/// 限制流中的元素数量（LIMIT 语义）。
#[derive(Debug, Clone)]
pub struct StreamLimiter {
    /// 最大数量
    limit: usize,
    /// 偏移量
    offset: usize,
}

impl StreamLimiter {
    /// 创建限制器
    pub fn new(limit: usize) -> Self {
        Self { limit, offset: 0 }
    }

    /// 设置偏移量
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// 最大数量
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// 偏移量
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// 应用限制
    pub fn apply(&self, rows: &[Value]) -> Vec<Value> {
        let start = self.offset.min(rows.len());
        let end = (start + self.limit).min(rows.len());
        rows[start..end].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- FilterCondition tests ---

    #[test]
    fn filter_eq() {
        let f = StreamFilter::new("status", FilterCondition::Eq(json!("active")));
        assert!(f.test(&json!({"status": "active"})));
        assert!(!f.test(&json!({"status": "inactive"})));
    }

    #[test]
    fn filter_not_eq() {
        let f = StreamFilter::new("status", FilterCondition::NotEq(json!("active")));
        assert!(!f.test(&json!({"status": "active"})));
        assert!(f.test(&json!({"status": "inactive"})));
    }

    #[test]
    fn filter_gt() {
        let f = StreamFilter::new("age", FilterCondition::Gt(json!(18)));
        assert!(f.test(&json!({"age": 25})));
        assert!(!f.test(&json!({"age": 18})));
        assert!(!f.test(&json!({"age": 10})));
    }

    #[test]
    fn filter_ge() {
        let f = StreamFilter::new("age", FilterCondition::Ge(json!(18)));
        assert!(f.test(&json!({"age": 18})));
        assert!(f.test(&json!({"age": 25})));
        assert!(!f.test(&json!({"age": 10})));
    }

    #[test]
    fn filter_lt() {
        let f = StreamFilter::new("age", FilterCondition::Lt(json!(18)));
        assert!(f.test(&json!({"age": 10})));
        assert!(!f.test(&json!({"age": 18})));
    }

    #[test]
    fn filter_le() {
        let f = StreamFilter::new("age", FilterCondition::Le(json!(18)));
        assert!(f.test(&json!({"age": 18})));
        assert!(f.test(&json!({"age": 10})));
        assert!(!f.test(&json!({"age": 25})));
    }

    #[test]
    fn filter_in() {
        let f = StreamFilter::new(
            "status",
            FilterCondition::In(vec![json!("active"), json!("pending")]),
        );
        assert!(f.test(&json!({"status": "active"})));
        assert!(f.test(&json!({"status": "pending"})));
        assert!(!f.test(&json!({"status": "closed"})));
    }

    #[test]
    fn filter_not_in() {
        let f = StreamFilter::new(
            "status",
            FilterCondition::NotIn(vec![json!("active"), json!("pending")]),
        );
        assert!(!f.test(&json!({"status": "active"})));
        assert!(f.test(&json!({"status": "closed"})));
    }

    #[test]
    fn filter_is_null() {
        let f = StreamFilter::new("deleted_at", FilterCondition::IsNull);
        assert!(f.test(&json!({"deleted_at": null})));
        assert!(!f.test(&json!({"deleted_at": "2024-01-01"})));
    }

    #[test]
    fn filter_is_not_null() {
        let f = StreamFilter::new("deleted_at", FilterCondition::IsNotNull);
        assert!(!f.test(&json!({"deleted_at": null})));
        assert!(f.test(&json!({"deleted_at": "2024-01-01"})));
    }

    #[test]
    fn filter_like() {
        let f = StreamFilter::new("name", FilterCondition::Like("John%".to_string()));
        assert!(f.test(&json!({"name": "Johnson"})));
        assert!(!f.test(&json!({"name": "Mary"})));
    }

    #[test]
    fn filter_apply_batch() {
        let f = StreamFilter::new("age", FilterCondition::Gt(json!(18)));
        let rows = vec![json!({"age": 25}), json!({"age": 10}), json!({"age": 30})];
        let filtered = f.apply(&rows);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_field_getter() {
        let f = StreamFilter::new("status", FilterCondition::Eq(json!("active")));
        assert_eq!(f.field(), "status");
    }

    #[test]
    fn filter_missing_field() {
        let f = StreamFilter::new("status", FilterCondition::Eq(json!("active")));
        assert!(!f.test(&json!({"name": "test"})));
    }

    // --- StreamMapper tests ---

    #[test]
    fn mapper_empty() {
        let m = StreamMapper::new();
        let row = json!({"a": 1, "b": 2});
        let result = m.apply(&row);
        assert!(result.as_object().unwrap().is_empty());
    }

    #[test]
    fn mapper_single_field() {
        let m = StreamMapper::new().map("a", "x");
        let row = json!({"a": 1, "b": 2});
        let result = m.apply(&row);
        assert_eq!(result["x"], json!(1));
    }

    #[test]
    fn mapper_multiple_fields() {
        let m = StreamMapper::new().map("a", "x").map("b", "y");
        let row = json!({"a": 1, "b": 2});
        let result = m.apply(&row);
        assert_eq!(result["x"], json!(1));
        assert_eq!(result["y"], json!(2));
    }

    #[test]
    fn mapper_keep_unmapped() {
        let m = StreamMapper::new().map("a", "x").keep_unmapped();
        let row = json!({"a": 1, "b": 2});
        let result = m.apply(&row);
        assert_eq!(result["x"], json!(1));
        assert_eq!(result["b"], json!(2));
    }

    #[test]
    fn mapper_apply_batch() {
        let m = StreamMapper::new().map("a", "x");
        let rows = vec![json!({"a": 1}), json!({"a": 2})];
        let result = m.apply_batch(&rows);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["x"], json!(1));
    }

    #[test]
    fn mapper_mapping_count() {
        let m = StreamMapper::new().map("a", "x").map("b", "y");
        assert_eq!(m.mapping_count(), 2);
    }

    #[test]
    fn mapper_default() {
        let m = StreamMapper::default();
        assert_eq!(m.mapping_count(), 0);
    }

    #[test]
    fn mapper_non_object_passthrough() {
        let m = StreamMapper::new().map("a", "x");
        let result = m.apply(&json!(42));
        assert_eq!(result, json!(42));
    }

    // --- AggregateFunction tests ---

    #[test]
    fn aggregate_function_as_str() {
        assert_eq!(AggregateFunction::Count.as_str(), "count");
        assert_eq!(AggregateFunction::Sum.as_str(), "sum");
        assert_eq!(AggregateFunction::Avg.as_str(), "avg");
        assert_eq!(AggregateFunction::Min.as_str(), "min");
        assert_eq!(AggregateFunction::Max.as_str(), "max");
    }

    // --- StreamAggregator tests ---

    #[test]
    fn aggregator_count() {
        let mut agg = StreamAggregator::new(AggregateFunction::Count, "id");
        agg.add(&json!({"id": 1}));
        agg.add(&json!({"id": 2}));
        let result = agg.result();
        assert_eq!(result.value, 2.0);
    }

    #[test]
    fn aggregator_sum() {
        let mut agg = StreamAggregator::new(AggregateFunction::Sum, "amount");
        agg.add(&json!({"amount": 100}));
        agg.add(&json!({"amount": 200}));
        let result = agg.result();
        assert!((result.value - 300.0).abs() < 1e-9);
    }

    #[test]
    fn aggregator_avg() {
        let mut agg = StreamAggregator::new(AggregateFunction::Avg, "amount");
        agg.add(&json!({"amount": 100}));
        agg.add(&json!({"amount": 200}));
        agg.add(&json!({"amount": 300}));
        let result = agg.result();
        assert!((result.value - 200.0).abs() < 1e-9);
    }

    #[test]
    fn aggregator_min() {
        let mut agg = StreamAggregator::new(AggregateFunction::Min, "amount");
        agg.add(&json!({"amount": 100}));
        agg.add(&json!({"amount": 50}));
        agg.add(&json!({"amount": 200}));
        let result = agg.result();
        assert!((result.value - 50.0).abs() < 1e-9);
    }

    #[test]
    fn aggregator_max() {
        let mut agg = StreamAggregator::new(AggregateFunction::Max, "amount");
        agg.add(&json!({"amount": 100}));
        agg.add(&json!({"amount": 200}));
        agg.add(&json!({"amount": 50}));
        let result = agg.result();
        assert!((result.value - 200.0).abs() < 1e-9);
    }

    #[test]
    fn aggregator_add_batch() {
        let mut agg = StreamAggregator::new(AggregateFunction::Sum, "amount");
        let rows = vec![json!({"amount": 100}), json!({"amount": 200})];
        agg.add_batch(&rows);
        let result = agg.result();
        assert!((result.value - 300.0).abs() < 1e-9);
    }

    #[test]
    fn aggregator_reset() {
        let mut agg = StreamAggregator::new(AggregateFunction::Sum, "amount");
        agg.add(&json!({"amount": 100}));
        agg.reset();
        let result = agg.result();
        assert_eq!(result.value, 0.0);
        assert_eq!(result.row_count, 0);
    }

    #[test]
    fn aggregator_empty_avg() {
        let agg = StreamAggregator::new(AggregateFunction::Avg, "amount");
        let result = agg.result();
        assert_eq!(result.value, 0.0);
    }

    #[test]
    fn aggregator_row_count() {
        let mut agg = StreamAggregator::new(AggregateFunction::Count, "id");
        agg.add(&json!({"id": 1}));
        agg.add(&json!({"id": 2}));
        assert_eq!(agg.row_count(), 2);
    }

    // --- MultiAggregator tests ---

    #[test]
    fn multi_aggregator_empty() {
        let m = MultiAggregator::new();
        assert_eq!(m.count(), 0);
        assert!(m.results().is_empty());
    }

    #[test]
    fn multi_aggregator_multiple_fields() {
        let mut m = MultiAggregator::new()
            .add_field(AggregateFunction::Sum, "amount")
            .add_field(AggregateFunction::Count, "id");
        m.add(&json!({"id": 1, "amount": 100}));
        m.add(&json!({"id": 2, "amount": 200}));
        let results = m.results();
        assert_eq!(results.len(), 2);
        assert!((results[0].value - 300.0).abs() < 1e-9);
        assert_eq!(results[1].value, 2.0);
    }

    #[test]
    fn multi_aggregator_add_batch() {
        let mut m = MultiAggregator::new().add_field(AggregateFunction::Sum, "amount");
        let rows = vec![json!({"amount": 100}), json!({"amount": 200})];
        m.add_batch(&rows);
        let results = m.results();
        assert!((results[0].value - 300.0).abs() < 1e-9);
    }

    #[test]
    fn multi_aggregator_reset() {
        let mut m = MultiAggregator::new().add_field(AggregateFunction::Sum, "amount");
        m.add(&json!({"amount": 100}));
        m.reset();
        let results = m.results();
        assert_eq!(results[0].value, 0.0);
    }

    #[test]
    fn multi_aggregator_default() {
        let m = MultiAggregator::default();
        assert_eq!(m.count(), 0);
    }

    #[test]
    fn multi_aggregator_count() {
        let m = MultiAggregator::new()
            .add_field(AggregateFunction::Sum, "a")
            .add_field(AggregateFunction::Max, "b");
        assert_eq!(m.count(), 2);
    }

    // --- SortDirection tests ---

    #[test]
    fn sort_direction_as_str() {
        assert_eq!(SortDirection::Asc.as_str(), "asc");
        assert_eq!(SortDirection::Desc.as_str(), "desc");
    }

    // --- StreamSorter tests ---

    #[test]
    fn sorter_asc() {
        let s = StreamSorter::new("age", SortDirection::Asc);
        let mut rows = vec![json!({"age": 30}), json!({"age": 10}), json!({"age": 20})];
        s.sort(&mut rows);
        assert_eq!(rows[0]["age"], json!(10));
        assert_eq!(rows[1]["age"], json!(20));
        assert_eq!(rows[2]["age"], json!(30));
    }

    #[test]
    fn sorter_desc() {
        let s = StreamSorter::new("age", SortDirection::Desc);
        let mut rows = vec![json!({"age": 10}), json!({"age": 30}), json!({"age": 20})];
        s.sort(&mut rows);
        assert_eq!(rows[0]["age"], json!(30));
        assert_eq!(rows[1]["age"], json!(20));
        assert_eq!(rows[2]["age"], json!(10));
    }

    #[test]
    fn sorter_apply() {
        let s = StreamSorter::new("name", SortDirection::Asc);
        let rows = vec![
            json!({"name": "c"}),
            json!({"name": "a"}),
            json!({"name": "b"}),
        ];
        let sorted = s.apply(&rows);
        assert_eq!(sorted[0]["name"], json!("a"));
    }

    #[test]
    fn sorter_field_getter() {
        let s = StreamSorter::new("age", SortDirection::Asc);
        assert_eq!(s.field(), "age");
    }

    #[test]
    fn sorter_direction_getter() {
        let s = StreamSorter::new("age", SortDirection::Desc);
        assert_eq!(s.direction(), SortDirection::Desc);
    }

    #[test]
    fn sorter_empty() {
        let s = StreamSorter::new("age", SortDirection::Asc);
        let mut rows: Vec<Value> = vec![];
        s.sort(&mut rows);
        assert!(rows.is_empty());
    }

    // --- StreamDeduplicator tests ---

    #[test]
    fn deduplicator_basic() {
        let d = StreamDeduplicator::new("id");
        let rows = vec![
            json!({"id": 1, "name": "a"}),
            json!({"id": 2, "name": "b"}),
            json!({"id": 1, "name": "c"}),
        ];
        let result = d.apply(&rows);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn deduplicator_no_duplicates() {
        let d = StreamDeduplicator::new("id");
        let rows = vec![json!({"id": 1}), json!({"id": 2}), json!({"id": 3})];
        let result = d.apply(&rows);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn deduplicator_all_duplicates() {
        let d = StreamDeduplicator::new("id");
        let rows = vec![json!({"id": 1}), json!({"id": 1}), json!({"id": 1})];
        let result = d.apply(&rows);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn deduplicator_field_getter() {
        let d = StreamDeduplicator::new("user_id");
        assert_eq!(d.field(), "user_id");
    }

    #[test]
    fn deduplicator_deduplicated_count() {
        let d = StreamDeduplicator::new("id");
        let rows = vec![json!({"id": 1}), json!({"id": 1}), json!({"id": 2})];
        assert_eq!(d.deduplicated_count(&rows), 2);
    }

    #[test]
    fn deduplicator_removed_count() {
        let d = StreamDeduplicator::new("id");
        let rows = vec![json!({"id": 1}), json!({"id": 1}), json!({"id": 2})];
        assert_eq!(d.removed_count(&rows), 1);
    }

    #[test]
    fn deduplicator_empty() {
        let d = StreamDeduplicator::new("id");
        let result = d.apply(&[]);
        assert!(result.is_empty());
    }

    // --- StreamLimiter tests ---

    #[test]
    fn limiter_basic() {
        let l = StreamLimiter::new(2);
        let rows = vec![json!(1), json!(2), json!(3), json!(4)];
        let result = l.apply(&rows);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn limiter_with_offset() {
        let l = StreamLimiter::new(2).with_offset(1);
        let rows = vec![json!(1), json!(2), json!(3), json!(4)];
        let result = l.apply(&rows);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], json!(2));
    }

    #[test]
    fn limiter_limit_getter() {
        let l = StreamLimiter::new(10);
        assert_eq!(l.limit(), 10);
    }

    #[test]
    fn limiter_offset_getter() {
        let l = StreamLimiter::new(10).with_offset(5);
        assert_eq!(l.offset(), 5);
    }

    #[test]
    fn limiter_exceeds_rows() {
        let l = StreamLimiter::new(100);
        let rows = vec![json!(1), json!(2)];
        let result = l.apply(&rows);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn limiter_zero_limit() {
        let l = StreamLimiter::new(0);
        let rows = vec![json!(1), json!(2)];
        let result = l.apply(&rows);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn limiter_offset_exceeds() {
        let l = StreamLimiter::new(10).with_offset(100);
        let rows = vec![json!(1), json!(2)];
        let result = l.apply(&rows);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn limiter_empty() {
        let l = StreamLimiter::new(10);
        let result = l.apply(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn sorter_single_element() {
        let s = StreamSorter::new("age", SortDirection::Asc);
        let mut rows = vec![json!({"age": 42})];
        s.sort(&mut rows);
        assert_eq!(rows[0]["age"], json!(42));
    }

    #[test]
    fn deduplicator_preserves_first() {
        let d = StreamDeduplicator::new("id");
        let rows = vec![
            json!({"id": 1, "name": "first"}),
            json!({"id": 1, "name": "second"}),
        ];
        let result = d.apply(&rows);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], json!("first"));
    }
}
