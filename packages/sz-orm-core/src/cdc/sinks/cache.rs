//! CDC 缓存失效 Sink（v6.8.0 CDC-CACHE-01）
//!
//! 将 CDC 变更事件转化为对应缓存 key 的失效指令，
//! 通过 `DistCacheGateway::invalidate` 失效分布式缓存中的对应条目。

use std::sync::Arc;

use async_trait::async_trait;

use super::super::checkpoint::CdcError;
use super::super::dispatcher::CdcSink;
use super::super::event::{ChangeEvent, ChangeEventType};
use crate::dist_cache_cluster::DistCacheGateway;

/// 缓存 key 提取策略
///
/// 从变更事件中提取需要失效的缓存 key。
#[derive(Debug, Clone)]
pub enum CacheKeyStrategy {
    /// 基于 `table:primary_key` 格式
    TablePk {
        /// 主键列名
        pk_column: String,
    },
    /// 基于 `table:*` 通配（整表失效）
    TableWildcard,
    /// 自定义提取函数（通过事件字段名指定 key 组成）
    Composite {
        /// 参与组合的列名列表
        columns: Vec<String>,
    },
}

impl CacheKeyStrategy {
    /// 从变更事件提取缓存 key 列表
    pub fn extract_keys(&self, event: &ChangeEvent) -> Vec<String> {
        match self {
            CacheKeyStrategy::TablePk { pk_column } => {
                if let Some(pk) = event.row_data.get(pk_column) {
                    vec![format!(
                        "{}:{}",
                        event.source_table,
                        json_value_to_string(pk)
                    )]
                } else {
                    vec![]
                }
            }
            CacheKeyStrategy::TableWildcard => {
                vec![format!("{}:*", event.source_table)]
            }
            CacheKeyStrategy::Composite { columns } => {
                let mut parts = vec![event.source_table.clone()];
                for col in columns {
                    if let Some(val) = event.row_data.get(col) {
                        parts.push(json_value_to_string(val));
                    }
                }
                vec![parts.join(":")]
            }
        }
    }
}

fn json_value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// CDC 缓存失效 Sink
///
/// 接收变更事件，提取缓存 key，调用 `DistCacheGateway::invalidate` 失效对应缓存。
pub struct CacheInvalidationSink {
    gateway: Arc<DistCacheGateway>,
    strategy: CacheKeyStrategy,
    invalidated_count: std::sync::atomic::AtomicU64,
}

impl CacheInvalidationSink {
    /// 创建缓存失效 Sink
    pub fn new(gateway: Arc<DistCacheGateway>, strategy: CacheKeyStrategy) -> Self {
        Self {
            gateway,
            strategy,
            invalidated_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 返回已失效的 key 总数
    pub fn invalidated_total(&self) -> u64 {
        self.invalidated_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 返回 gateway 引用
    pub fn gateway(&self) -> &DistCacheGateway {
        &self.gateway
    }
}

#[async_trait]
impl CdcSink for CacheInvalidationSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        let keys = self.strategy.extract_keys(event);
        if keys.is_empty() {
            return Ok(());
        }
        self.gateway.invalidate_batch(&keys);
        self.invalidated_count
            .fetch_add(keys.len() as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

/// 过滤型 Sink：仅处理指定表的变更事件
pub struct TableFilteredCacheSink {
    inner: CacheInvalidationSink,
    watched_tables: std::collections::HashSet<String>,
}

impl TableFilteredCacheSink {
    /// 创建表过滤缓存失效 Sink
    pub fn new(
        gateway: Arc<DistCacheGateway>,
        strategy: CacheKeyStrategy,
        watched_tables: Vec<String>,
    ) -> Self {
        Self {
            inner: CacheInvalidationSink::new(gateway, strategy),
            watched_tables: watched_tables.into_iter().collect(),
        }
    }

    /// 返回已失效的 key 总数
    pub fn invalidated_total(&self) -> u64 {
        self.inner.invalidated_total()
    }
}

#[async_trait]
impl CdcSink for TableFilteredCacheSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        if !self.watched_tables.contains(&event.source_table) {
            return Ok(());
        }
        self.inner.handle(event).await
    }
}

/// 仅处理指定事件类型的缓存失效 Sink
pub struct EventTypeFilteredCacheSink {
    inner: CacheInvalidationSink,
    watched_types: std::collections::HashSet<ChangeEventType>,
}

impl EventTypeFilteredCacheSink {
    /// 创建事件类型过滤缓存失效 Sink
    pub fn new(
        gateway: Arc<DistCacheGateway>,
        strategy: CacheKeyStrategy,
        watched_types: Vec<ChangeEventType>,
    ) -> Self {
        Self {
            inner: CacheInvalidationSink::new(gateway, strategy),
            watched_types: watched_types.into_iter().collect(),
        }
    }

    /// 返回已失效的 key 总数
    pub fn invalidated_total(&self) -> u64 {
        self.inner.invalidated_total()
    }
}

#[async_trait]
impl CdcSink for EventTypeFilteredCacheSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        if !self.watched_types.contains(&event.event_type) {
            return Ok(());
        }
        self.inner.handle(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};
    use crate::dist_cache_cluster::DistCacheGateway;
    use serde_json::json;

    fn make_event(table: &str, pk_val: &str, event_type: ChangeEventType) -> ChangeEvent {
        ChangeEvent::new(
            event_type,
            "test_db",
            table,
            json!({ "id": pk_val, "name": "test" }),
            ChangePosition::MysqlBinlog {
                filename: "bin.000001".to_string(),
                position: 100,
            },
            1000,
        )
    }

    #[tokio::test]
    async fn cache_sink_invalidates_key_on_update() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = CacheInvalidationSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "id".to_string(),
            },
        );
        let event = make_event("users", "42", ChangeEventType::Update);
        sink.handle(&event).await.unwrap();
        assert!(gateway.is_invalidated("users:42"));
        assert_eq!(sink.invalidated_total(), 1);
    }

    #[tokio::test]
    async fn cache_sink_invalidates_key_on_insert() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = CacheInvalidationSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "id".to_string(),
            },
        );
        let event = make_event("orders", "99", ChangeEventType::Insert);
        sink.handle(&event).await.unwrap();
        assert!(gateway.is_invalidated("orders:99"));
    }

    #[tokio::test]
    async fn cache_sink_invalidates_key_on_delete() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = CacheInvalidationSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "id".to_string(),
            },
        );
        let event = make_event("products", "7", ChangeEventType::Delete);
        sink.handle(&event).await.unwrap();
        assert!(gateway.is_invalidated("products:7"));
    }

    #[tokio::test]
    async fn wildcard_strategy_invalidates_entire_table() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = CacheInvalidationSink::new(gateway.clone(), CacheKeyStrategy::TableWildcard);
        let event = make_event("users", "1", ChangeEventType::Update);
        sink.handle(&event).await.unwrap();
        assert!(gateway.is_invalidated("users:*"));
    }

    #[tokio::test]
    async fn composite_strategy_invalidates_composite_key() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = CacheInvalidationSink::new(
            gateway.clone(),
            CacheKeyStrategy::Composite {
                columns: vec!["id".to_string(), "name".to_string()],
            },
        );
        let event = make_event("users", "5", ChangeEventType::Update);
        sink.handle(&event).await.unwrap();
        assert!(gateway.is_invalidated("users:5:test"));
    }

    #[tokio::test]
    async fn table_filtered_sink_skips_unwatched_tables() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = TableFilteredCacheSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "id".to_string(),
            },
            vec!["users".to_string()],
        );
        let event = make_event("orders", "1", ChangeEventType::Update);
        sink.handle(&event).await.unwrap();
        assert!(!gateway.is_invalidated("orders:1"));
        assert_eq!(sink.invalidated_total(), 0);
    }

    #[tokio::test]
    async fn table_filtered_sink_processes_watched_tables() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = TableFilteredCacheSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "id".to_string(),
            },
            vec!["users".to_string()],
        );
        let event = make_event("users", "1", ChangeEventType::Update);
        sink.handle(&event).await.unwrap();
        assert!(gateway.is_invalidated("users:1"));
        assert_eq!(sink.invalidated_total(), 1);
    }

    #[tokio::test]
    async fn event_type_filtered_sink_only_processes_matching_types() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = EventTypeFilteredCacheSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "id".to_string(),
            },
            vec![ChangeEventType::Update],
        );
        let insert_event = make_event("users", "1", ChangeEventType::Insert);
        sink.handle(&insert_event).await.unwrap();
        assert!(!gateway.is_invalidated("users:1"));

        let update_event = make_event("users", "1", ChangeEventType::Update);
        sink.handle(&update_event).await.unwrap();
        assert!(gateway.is_invalidated("users:1"));
        assert_eq!(sink.invalidated_total(), 1);
    }

    #[tokio::test]
    async fn batch_events_invalidate_multiple_keys() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = CacheInvalidationSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "id".to_string(),
            },
        );
        for i in 1..=5 {
            let event = make_event("users", &i.to_string(), ChangeEventType::Update);
            sink.handle(&event).await.unwrap();
        }
        for i in 1..=5 {
            assert!(gateway.is_invalidated(&format!("users:{}", i)));
        }
        assert_eq!(sink.invalidated_total(), 5);
        assert_eq!(gateway.invalidated_count(), 5);
    }

    #[tokio::test]
    async fn missing_pk_column_produces_no_invalidation() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = CacheInvalidationSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "nonexistent".to_string(),
            },
        );
        let event = make_event("users", "1", ChangeEventType::Update);
        sink.handle(&event).await.unwrap();
        assert_eq!(sink.invalidated_total(), 0);
        assert_eq!(gateway.invalidated_count(), 0);
    }

    #[tokio::test]
    async fn clear_invalidation_allows_repopulation() {
        let gateway = Arc::new(DistCacheGateway::with_default());
        let sink = CacheInvalidationSink::new(
            gateway.clone(),
            CacheKeyStrategy::TablePk {
                pk_column: "id".to_string(),
            },
        );
        let event = make_event("users", "1", ChangeEventType::Update);
        sink.handle(&event).await.unwrap();
        assert!(gateway.is_invalidated("users:1"));
        gateway.clear_invalidation("users:1");
        assert!(!gateway.is_invalidated("users:1"));
    }
}
