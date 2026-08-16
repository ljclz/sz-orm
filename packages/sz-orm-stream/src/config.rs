//! 流式结果集配置与策略枚举

use serde::{Deserialize, Serialize};

use sz_orm_core::DbType;

/// 分页策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaginationStrategy {
    /// keyset 分页（WHERE key > last_key ORDER BY key LIMIT batch）
    Keyset,
    /// OFFSET 分页（LIMIT batch OFFSET n）
    LimitOffset,
    /// 服务端游标（DECLARE CURSOR + FETCH）
    ServerCursor,
}

impl PaginationStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            PaginationStrategy::Keyset => "keyset",
            PaginationStrategy::LimitOffset => "limit-offset",
            PaginationStrategy::ServerCursor => "server-cursor",
        }
    }
}

/// 排序方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OrderDirection {
    /// 升序（默认）
    #[default]
    Asc,
    /// 降序
    Desc,
}

impl OrderDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderDirection::Asc => "asc",
            OrderDirection::Desc => "desc",
        }
    }
}

/// 流式结果集配置
#[derive(Debug, Clone)]
pub struct StreamResultSetConfig {
    /// 批次大小（默认 1000）
    pub batch_size: usize,
    /// 背压阈值（默认 10000）
    pub backpressure_threshold: usize,
    /// 分页策略
    pub pagination_strategy: PaginationStrategy,
    /// keyset 列名（Keyset 策略时必须设置）
    pub keyset_column: Option<String>,
    /// 排序方向
    pub order_direction: OrderDirection,
    /// 数据库类型
    pub db_type: DbType,
}

impl Default for StreamResultSetConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            backpressure_threshold: 10000,
            pagination_strategy: PaginationStrategy::LimitOffset,
            keyset_column: None,
            order_direction: OrderDirection::Asc,
            db_type: DbType::PostgreSQL,
        }
    }
}

impl StreamResultSetConfig {
    pub fn new(db_type: DbType) -> Self {
        Self {
            db_type,
            ..Self::default()
        }
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    pub fn with_backpressure_threshold(mut self, threshold: usize) -> Self {
        self.backpressure_threshold = threshold;
        self
    }

    pub fn with_pagination_strategy(mut self, strategy: PaginationStrategy) -> Self {
        self.pagination_strategy = strategy;
        self
    }

    pub fn with_keyset_column(mut self, column: impl Into<String>) -> Self {
        self.keyset_column = Some(column.into());
        self
    }

    pub fn with_order_direction(mut self, direction: OrderDirection) -> Self {
        self.order_direction = direction;
        self
    }

    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), String> {
        if self.batch_size == 0 {
            return Err("batch_size must be > 0".into());
        }
        if self.pagination_strategy == PaginationStrategy::Keyset && self.keyset_column.is_none() {
            return Err("keyset pagination requires keyset_column".into());
        }
        Ok(())
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    pub fn backpressure_threshold(&self) -> usize {
        self.backpressure_threshold
    }

    pub fn db_type(&self) -> DbType {
        self.db_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL);
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.backpressure_threshold, 10000);
        assert_eq!(config.pagination_strategy, PaginationStrategy::LimitOffset);
        assert_eq!(config.order_direction, OrderDirection::Asc);
    }

    #[test]
    fn config_builder_chain() {
        let config = StreamResultSetConfig::new(DbType::MySQL)
            .with_batch_size(500)
            .with_backpressure_threshold(5000)
            .with_pagination_strategy(PaginationStrategy::Keyset)
            .with_keyset_column("id")
            .with_order_direction(OrderDirection::Desc);
        assert_eq!(config.batch_size, 500);
        assert_eq!(config.backpressure_threshold, 5000);
        assert_eq!(config.pagination_strategy, PaginationStrategy::Keyset);
        assert_eq!(config.keyset_column.as_deref(), Some("id"));
        assert_eq!(config.order_direction, OrderDirection::Desc);
    }

    #[test]
    fn config_validate_keyset_requires_column() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL)
            .with_pagination_strategy(PaginationStrategy::Keyset);
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_validate_keyset_with_column() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL)
            .with_pagination_strategy(PaginationStrategy::Keyset)
            .with_keyset_column("id");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_validate_limit_offset_ok() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn pagination_strategy_serde() {
        let s = serde_json::to_string(&PaginationStrategy::Keyset).unwrap();
        let d: PaginationStrategy = serde_json::from_str(&s).unwrap();
        assert_eq!(d, PaginationStrategy::Keyset);
    }

    #[test]
    fn order_direction_serde() {
        let s = serde_json::to_string(&OrderDirection::Desc).unwrap();
        let d: OrderDirection = serde_json::from_str(&s).unwrap();
        assert_eq!(d, OrderDirection::Desc);
    }

    #[test]
    fn batch_size_zero_clamped() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL).with_batch_size(0);
        assert_eq!(config.batch_size, 1);
    }

    #[test]
    fn test_pagination_strategy_as_str() {
        assert_eq!(PaginationStrategy::Keyset.as_str(), "keyset");
        assert_eq!(PaginationStrategy::LimitOffset.as_str(), "limit-offset");
        assert_eq!(PaginationStrategy::ServerCursor.as_str(), "server-cursor");
    }

    #[test]
    fn test_order_direction_as_str() {
        assert_eq!(OrderDirection::Asc.as_str(), "asc");
        assert_eq!(OrderDirection::Desc.as_str(), "desc");
    }

    #[test]
    fn test_config_getters() {
        let config = StreamResultSetConfig::new(DbType::MySQL)
            .with_batch_size(200)
            .with_backpressure_threshold(2000);
        assert_eq!(config.batch_size(), 200);
        assert_eq!(config.backpressure_threshold(), 2000);
        assert_eq!(config.db_type(), DbType::MySQL);
    }
}
