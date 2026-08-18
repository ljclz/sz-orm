//! 流式结果集分页器
//!
//! 提供 OFFSET 分页与游标分页的统一抽象，
//! 支持自动遍历所有页面。

use serde_json::Value;

use crate::config::{OrderDirection, PaginationStrategy};

/// 分页状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaginationState {
    /// 初始状态
    Initial,
    /// 正在分页
    Paging,
    /// 已完成
    Done,
}

impl PaginationState {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            PaginationState::Initial => "initial",
            PaginationState::Paging => "paging",
            PaginationState::Done => "done",
        }
    }

    /// 是否已完成
    pub fn is_done(&self) -> bool {
        matches!(self, PaginationState::Done)
    }
}

/// 流式分页器配置
#[derive(Debug, Clone)]
pub struct StreamPaginatorConfig {
    /// 批次大小
    pub batch_size: usize,
    /// 最大页数（0 = 无限制）
    pub max_pages: usize,
    /// 分页策略
    pub strategy: PaginationStrategy,
    /// 排序方向
    pub order_direction: OrderDirection,
}

impl Default for StreamPaginatorConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            max_pages: 0,
            strategy: PaginationStrategy::LimitOffset,
            order_direction: OrderDirection::Asc,
        }
    }
}

impl StreamPaginatorConfig {
    /// 创建配置
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch_size: batch_size.max(1),
            ..Self::default()
        }
    }

    /// 设置最大页数
    pub fn with_max_pages(mut self, max_pages: usize) -> Self {
        self.max_pages = max_pages;
        self
    }

    /// 设置分页策略
    pub fn with_strategy(mut self, strategy: PaginationStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// 设置排序方向
    pub fn with_order_direction(mut self, direction: OrderDirection) -> Self {
        self.order_direction = direction;
        self
    }

    /// 校验配置
    pub fn validate(&self) -> Result<(), String> {
        if self.batch_size == 0 {
            return Err("batch_size must be > 0".to_string());
        }
        Ok(())
    }
}

/// 流式分页器
///
/// 维护分页状态，逐页获取数据。
pub struct StreamPaginator {
    config: StreamPaginatorConfig,
    state: PaginationState,
    current_page: usize,
    current_offset: u64,
    total_fetched: u64,
    last_key: Option<Value>,
    keyset_column: Option<String>,
}

impl StreamPaginator {
    /// 创建分页器
    pub fn new(config: StreamPaginatorConfig) -> Self {
        Self {
            config,
            state: PaginationState::Initial,
            current_page: 0,
            current_offset: 0,
            total_fetched: 0,
            last_key: None,
            keyset_column: None,
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(StreamPaginatorConfig::default())
    }

    /// 设置 keyset 列名
    pub fn with_keyset_column(mut self, column: impl Into<String>) -> Self {
        self.keyset_column = Some(column.into());
        self
    }

    /// 当前页码（0 起）
    pub fn current_page(&self) -> usize {
        self.current_page
    }

    /// 当前偏移量
    pub fn current_offset(&self) -> u64 {
        self.current_offset
    }

    /// 已获取总数
    pub fn total_fetched(&self) -> u64 {
        self.total_fetched
    }

    /// 分页状态
    pub fn state(&self) -> PaginationState {
        self.state
    }

    /// 是否已完成
    pub fn is_done(&self) -> bool {
        self.state.is_done()
    }

    /// 批次大小
    pub fn batch_size(&self) -> usize {
        self.config.batch_size
    }

    /// 生成下一页的 OFFSET
    pub fn next_offset(&self) -> u64 {
        self.current_offset
    }

    /// 生成下一页的 LIMIT
    pub fn next_limit(&self) -> usize {
        self.config.batch_size
    }

    /// 生成下一页 SQL
    pub fn build_page_sql(&self, base_sql: &str) -> String {
        match self.config.strategy {
            PaginationStrategy::Keyset => self.build_keyset_sql(base_sql),
            PaginationStrategy::LimitOffset | PaginationStrategy::ServerCursor => {
                format!(
                    "{} LIMIT {} OFFSET {}",
                    base_sql, self.config.batch_size, self.current_offset
                )
            }
        }
    }

    fn build_keyset_sql(&self, base_sql: &str) -> String {
        let col = self.keyset_column.as_deref().unwrap_or("id");
        let order = match self.config.order_direction {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        };
        match &self.last_key {
            None => format!(
                "{base_sql} ORDER BY {col} {order} LIMIT {}",
                self.config.batch_size
            ),
            Some(key) => {
                let key_str = format_key(key);
                let cmp = match self.config.order_direction {
                    OrderDirection::Asc => ">",
                    OrderDirection::Desc => "<",
                };
                format!(
                    "{base_sql} WHERE {col} {cmp} {key_str} ORDER BY {col} {order} LIMIT {}",
                    self.config.batch_size
                )
            }
        }
    }

    /// 记录本页结果，推进分页状态
    ///
    /// 返回 `true` 表示还有更多页，`false` 表示已完成。
    pub fn advance(&mut self, batch: &[Value]) -> bool {
        let batch_len = batch.len();

        // 检查是否达到最大页数
        if self.config.max_pages > 0 && self.current_page >= self.config.max_pages {
            self.state = PaginationState::Done;
            return false;
        }

        // 空批次表示结束
        if batch_len == 0 {
            self.state = PaginationState::Done;
            return false;
        }

        self.current_page += 1;
        self.total_fetched += batch_len as u64;

        // keyset 分页：更新 last_key
        if self.config.strategy == PaginationStrategy::Keyset {
            if let Some(last_row) = batch.last() {
                if let Some(obj) = last_row.as_object() {
                    if let Some(col) = &self.keyset_column {
                        self.last_key = obj.get(col).cloned();
                    }
                }
            }
        }

        // OFFSET 分页：推进偏移量
        if self.config.strategy != PaginationStrategy::Keyset {
            self.current_offset += batch_len as u64;
        }

        // 不足一批次表示最后一页
        if batch_len < self.config.batch_size {
            self.state = PaginationState::Done;
            return false;
        }

        self.state = PaginationState::Paging;
        true
    }

    /// 重置分页器
    pub fn reset(&mut self) {
        self.state = PaginationState::Initial;
        self.current_page = 0;
        self.current_offset = 0;
        self.total_fetched = 0;
        self.last_key = None;
    }

    /// 配置引用
    pub fn config(&self) -> &StreamPaginatorConfig {
        &self.config
    }
}

fn format_key(key: &Value) -> String {
    match key {
        Value::Null => "NULL".to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Bool(b) => b.to_string(),
        n @ Value::Number(_) => n.to_string(),
        _ => key.to_string(),
    }
}

/// 分页统计
#[derive(Debug, Clone, Default)]
pub struct PaginationStats {
    /// 总页数
    pub total_pages: usize,
    /// 总行数
    pub total_rows: u64,
    /// 总耗时（毫秒）
    pub total_elapsed_ms: u64,
    /// 最大页耗时（毫秒）
    pub max_page_elapsed_ms: u64,
    /// 最小页耗时（毫秒）
    pub min_page_elapsed_ms: u64,
}

impl PaginationStats {
    /// 创建空统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一页
    pub fn record_page(&mut self, row_count: u64, elapsed_ms: u64) {
        self.total_pages += 1;
        self.total_rows += row_count;
        self.total_elapsed_ms += elapsed_ms;
        if elapsed_ms > self.max_page_elapsed_ms {
            self.max_page_elapsed_ms = elapsed_ms;
        }
        if self.min_page_elapsed_ms == 0 || elapsed_ms < self.min_page_elapsed_ms {
            self.min_page_elapsed_ms = elapsed_ms;
        }
    }

    /// 平均页耗时（毫秒）
    pub fn avg_page_elapsed_ms(&self) -> f64 {
        if self.total_pages == 0 {
            0.0
        } else {
            self.total_elapsed_ms as f64 / self.total_pages as f64
        }
    }

    /// 平均页行数
    pub fn avg_page_rows(&self) -> f64 {
        if self.total_pages == 0 {
            0.0
        } else {
            self.total_rows as f64 / self.total_pages as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- PaginationState tests ---

    #[test]
    fn state_as_str() {
        assert_eq!(PaginationState::Initial.as_str(), "initial");
        assert_eq!(PaginationState::Paging.as_str(), "paging");
        assert_eq!(PaginationState::Done.as_str(), "done");
    }

    #[test]
    fn state_is_done() {
        assert!(!PaginationState::Initial.is_done());
        assert!(!PaginationState::Paging.is_done());
        assert!(PaginationState::Done.is_done());
    }

    // --- StreamPaginatorConfig tests ---

    #[test]
    fn config_default() {
        let c = StreamPaginatorConfig::default();
        assert_eq!(c.batch_size, 1000);
        assert_eq!(c.max_pages, 0);
        assert_eq!(c.strategy, PaginationStrategy::LimitOffset);
    }

    #[test]
    fn config_new_clamps_batch_size() {
        let c = StreamPaginatorConfig::new(0);
        assert_eq!(c.batch_size, 1);
    }

    #[test]
    fn config_with_max_pages() {
        let c = StreamPaginatorConfig::new(100).with_max_pages(5);
        assert_eq!(c.max_pages, 5);
    }

    #[test]
    fn config_with_strategy() {
        let c = StreamPaginatorConfig::new(100).with_strategy(PaginationStrategy::Keyset);
        assert_eq!(c.strategy, PaginationStrategy::Keyset);
    }

    #[test]
    fn config_with_order_direction() {
        let c = StreamPaginatorConfig::new(100).with_order_direction(OrderDirection::Desc);
        assert_eq!(c.order_direction, OrderDirection::Desc);
    }

    #[test]
    fn config_validate_ok() {
        let c = StreamPaginatorConfig::new(100);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn config_validate_zero_batch() {
        let c = StreamPaginatorConfig {
            batch_size: 0,
            ..StreamPaginatorConfig::default()
        };
        assert!(c.validate().is_err());
    }

    // --- StreamPaginator tests ---

    #[test]
    fn paginator_initial_state() {
        let p = StreamPaginator::with_defaults();
        assert_eq!(p.state(), PaginationState::Initial);
        assert_eq!(p.current_page(), 0);
        assert_eq!(p.current_offset(), 0);
        assert_eq!(p.total_fetched(), 0);
        assert!(!p.is_done());
    }

    #[test]
    fn paginator_build_first_page_sql() {
        let p = StreamPaginator::with_defaults();
        let sql = p.build_page_sql("SELECT * FROM t");
        assert!(sql.contains("LIMIT 1000"));
        assert!(sql.contains("OFFSET 0"));
    }

    #[test]
    fn paginator_advance_full_batch() {
        let mut p = StreamPaginator::new(StreamPaginatorConfig::new(10));
        let batch: Vec<Value> = (0..10).map(|i| json!({"id": i})).collect();
        let has_more = p.advance(&batch);
        assert!(has_more);
        assert_eq!(p.current_page(), 1);
        assert_eq!(p.total_fetched(), 10);
        assert_eq!(p.current_offset(), 10);
    }

    #[test]
    fn paginator_advance_partial_batch_done() {
        let mut p = StreamPaginator::new(StreamPaginatorConfig::new(10));
        let batch: Vec<Value> = (0..5).map(|i| json!({"id": i})).collect();
        let has_more = p.advance(&batch);
        assert!(!has_more);
        assert!(p.is_done());
    }

    #[test]
    fn paginator_advance_empty_batch_done() {
        let mut p = StreamPaginator::new(StreamPaginatorConfig::new(10));
        let has_more = p.advance(&[]);
        assert!(!has_more);
        assert!(p.is_done());
    }

    #[test]
    fn paginator_max_pages_limit() {
        let config = StreamPaginatorConfig::new(10).with_max_pages(2);
        let mut p = StreamPaginator::new(config);
        let batch: Vec<Value> = (0..10).map(|i| json!({"id": i})).collect();
        p.advance(&batch);
        p.advance(&batch);
        let has_more = p.advance(&batch);
        assert!(!has_more);
        assert!(p.is_done());
    }

    #[test]
    fn paginator_reset() {
        let mut p = StreamPaginator::new(StreamPaginatorConfig::new(10));
        let batch: Vec<Value> = (0..10).map(|i| json!({"id": i})).collect();
        p.advance(&batch);
        p.reset();
        assert_eq!(p.state(), PaginationState::Initial);
        assert_eq!(p.current_page(), 0);
        assert_eq!(p.total_fetched(), 0);
    }

    #[test]
    fn paginator_keyset_sql_first_page() {
        let config = StreamPaginatorConfig::new(10).with_strategy(PaginationStrategy::Keyset);
        let p = StreamPaginator::new(config).with_keyset_column("id");
        let sql = p.build_page_sql("SELECT * FROM t");
        assert!(sql.contains("ORDER BY id ASC"));
        assert!(sql.contains("LIMIT 10"));
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn paginator_keyset_sql_second_page() {
        let config = StreamPaginatorConfig::new(10).with_strategy(PaginationStrategy::Keyset);
        let mut p = StreamPaginator::new(config).with_keyset_column("id");
        let batch: Vec<Value> = (0..10).map(|i| json!({"id": i + 100})).collect();
        p.advance(&batch);
        let sql = p.build_page_sql("SELECT * FROM t");
        assert!(sql.contains("WHERE id > 109"));
    }

    #[test]
    fn paginator_desc_order_sql() {
        let config = StreamPaginatorConfig::new(10)
            .with_strategy(PaginationStrategy::Keyset)
            .with_order_direction(OrderDirection::Desc);
        let p = StreamPaginator::new(config).with_keyset_column("id");
        let sql = p.build_page_sql("SELECT * FROM t");
        assert!(sql.contains("ORDER BY id DESC"));
    }

    #[test]
    fn paginator_next_offset_and_limit() {
        let mut p = StreamPaginator::new(StreamPaginatorConfig::new(50));
        assert_eq!(p.next_offset(), 0);
        assert_eq!(p.next_limit(), 50);
        let batch: Vec<Value> = (0..50).map(|i| json!({"id": i})).collect();
        p.advance(&batch);
        assert_eq!(p.next_offset(), 50);
    }

    #[test]
    fn paginator_config_ref() {
        let p = StreamPaginator::with_defaults();
        assert_eq!(p.config().batch_size, 1000);
    }

    #[test]
    fn paginator_batch_size_getter() {
        let p = StreamPaginator::new(StreamPaginatorConfig::new(25));
        assert_eq!(p.batch_size(), 25);
    }

    #[test]
    fn paginator_multiple_pages() {
        let mut p = StreamPaginator::new(StreamPaginatorConfig::new(10));
        for page in 0..3 {
            let batch: Vec<Value> = (0..10).map(|i| json!({"id": page * 10 + i})).collect();
            let has_more = p.advance(&batch);
            if page < 2 {
                assert!(has_more);
            }
        }
        assert_eq!(p.current_page(), 3);
        assert_eq!(p.total_fetched(), 30);
    }

    // --- PaginationStats tests ---

    #[test]
    fn stats_new_empty() {
        let s = PaginationStats::new();
        assert_eq!(s.total_pages, 0);
        assert_eq!(s.total_rows, 0);
        assert_eq!(s.avg_page_elapsed_ms(), 0.0);
    }

    #[test]
    fn stats_record_page() {
        let mut s = PaginationStats::new();
        s.record_page(100, 50);
        s.record_page(200, 100);
        assert_eq!(s.total_pages, 2);
        assert_eq!(s.total_rows, 300);
        assert_eq!(s.total_elapsed_ms, 150);
    }

    #[test]
    fn stats_avg_page_elapsed() {
        let mut s = PaginationStats::new();
        s.record_page(10, 100);
        s.record_page(10, 300);
        assert!((s.avg_page_elapsed_ms() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn stats_avg_page_rows() {
        let mut s = PaginationStats::new();
        s.record_page(100, 10);
        s.record_page(200, 10);
        assert!((s.avg_page_rows() - 150.0).abs() < 1e-9);
    }

    #[test]
    fn stats_max_min_elapsed() {
        let mut s = PaginationStats::new();
        s.record_page(10, 100);
        s.record_page(10, 50);
        s.record_page(10, 200);
        assert_eq!(s.max_page_elapsed_ms, 200);
        assert_eq!(s.min_page_elapsed_ms, 50);
    }

    #[test]
    fn stats_single_page() {
        let mut s = PaginationStats::new();
        s.record_page(100, 50);
        assert_eq!(s.max_page_elapsed_ms, 50);
        assert_eq!(s.min_page_elapsed_ms, 50);
    }

    #[test]
    fn stats_empty_avg() {
        let s = PaginationStats::new();
        assert_eq!(s.avg_page_rows(), 0.0);
    }

    #[test]
    fn paginator_keyset_desc_second_page() {
        let config = StreamPaginatorConfig::new(10)
            .with_strategy(PaginationStrategy::Keyset)
            .with_order_direction(OrderDirection::Desc);
        let mut p = StreamPaginator::new(config).with_keyset_column("id");
        let batch: Vec<Value> = (0..10).map(|i| json!({"id": 100 - i})).collect();
        p.advance(&batch);
        let sql = p.build_page_sql("SELECT * FROM t");
        assert!(sql.contains("WHERE id < 91"));
    }

    #[test]
    fn paginator_state_transitions() {
        let mut p = StreamPaginator::new(StreamPaginatorConfig::new(10));
        assert_eq!(p.state(), PaginationState::Initial);
        let batch: Vec<Value> = (0..10).map(|i| json!({"id": i})).collect();
        p.advance(&batch);
        assert_eq!(p.state(), PaginationState::Paging);
        let small_batch: Vec<Value> = (0..3).map(|i| json!({"id": i})).collect();
        p.advance(&small_batch);
        assert_eq!(p.state(), PaginationState::Done);
    }

    #[test]
    fn paginator_total_fetched_after_multiple_pages() {
        let mut p = StreamPaginator::new(StreamPaginatorConfig::new(10));
        for _ in 0..3 {
            let batch: Vec<Value> = (0..10).map(|i| json!({"id": i})).collect();
            p.advance(&batch);
        }
        assert_eq!(p.total_fetched(), 30);
        assert_eq!(p.current_page(), 3);
    }

    #[test]
    fn paginator_keyset_uses_last_row_key() {
        let config = StreamPaginatorConfig::new(10).with_strategy(PaginationStrategy::Keyset);
        let mut p = StreamPaginator::new(config).with_keyset_column("id");
        let batch: Vec<Value> = vec![
            json!({"id": 1, "name": "a"}),
            json!({"id": 2, "name": "b"}),
            json!({"id": 3, "name": "c"}),
        ];
        p.advance(&batch);
        let sql = p.build_page_sql("SELECT * FROM t");
        assert!(sql.contains("WHERE id > 3"));
    }

    #[test]
    fn stats_default_is_empty() {
        let s = PaginationStats::default();
        assert_eq!(s.total_pages, 0);
    }
}
