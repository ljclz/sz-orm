//! CDC 事件分发器（v6.8.0）

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::checkpoint::CdcError;
use super::event::ChangeEvent;

/// CDC 事件 sink trait
#[async_trait]
pub trait CdcSink: Send + Sync {
    /// 处理变更事件
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError>;
}

/// CDC 事件分发器
pub struct CdcEventDispatcher {
    sinks: Vec<Arc<dyn CdcSink>>,
    backpressure_capacity: usize,
}

impl CdcEventDispatcher {
    /// 创建分发器
    pub fn new(sinks: Vec<Arc<dyn CdcSink>>, capacity: usize) -> Self {
        Self {
            sinks,
            backpressure_capacity: capacity,
        }
    }

    /// 分发事件到所有 sink
    pub async fn dispatch(&self, event: ChangeEvent) -> Result<(), CdcError> {
        for sink in &self.sinks {
            sink.handle(&event).await?;
        }
        Ok(())
    }

    /// 批量分发事件
    pub async fn dispatch_batch(&self, events: Vec<ChangeEvent>) -> Result<usize, CdcError> {
        let mut count = 0;
        for event in events {
            self.dispatch(event).await?;
            count += 1;
        }
        Ok(count)
    }

    /// 严格分发：所有 sink 确认后才推进位点（v6.8.0 CDC-RESUME-SEC）
    ///
    /// 任一 sink 失败时**不推进位点**，重启后从上次确认位点重发未确认事件。
    /// 返回 `Ok` 表示所有 sink 已确认，可推进位点；返回 `Err` 表示有 sink 未确认。
    pub async fn dispatch_and_confirm(
        &self,
        event: ChangeEvent,
        checkpoint: &super::checkpoint::SharedCheckpointStore,
    ) -> Result<bool, CdcError> {
        match self.dispatch(event.clone()).await {
            Ok(()) => {
                checkpoint
                    .save_checkpoint_with_retry(&event.position, 3)
                    .await?;
                Ok(true)
            }
            Err(e) => Err(e),
        }
    }

    /// 启动事件流消费（从 mpsc::Receiver 读取并分发）
    pub async fn run(&self, mut receiver: mpsc::Receiver<ChangeEvent>) -> Result<usize, CdcError> {
        let mut count = 0;
        while let Some(event) = receiver.recv().await {
            self.dispatch(event).await?;
            count += 1;
        }
        Ok(count)
    }

    /// sink 数量
    pub fn sink_count(&self) -> usize {
        self.sinks.len()
    }

    /// 背压容量
    pub fn backpressure_capacity(&self) -> usize {
        self.backpressure_capacity
    }
}

/// 内存 sink（测试/缓冲用）
pub struct MemorySink {
    events: parking_lot::RwLock<Vec<ChangeEvent>>,
}

impl MemorySink {
    /// 创建空 sink
    pub fn new() -> Self {
        Self {
            events: parking_lot::RwLock::new(Vec::new()),
        }
    }

    /// 获取已接收事件
    pub fn events(&self) -> Vec<ChangeEvent> {
        self.events.read().clone()
    }

    /// 已接收事件数
    pub fn count(&self) -> usize {
        self.events.read().len()
    }
}

impl Default for MemorySink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CdcSink for MemorySink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        self.events.write().push(event.clone());
        Ok(())
    }
}

/// 表名过滤 sink（只处理指定表的事件）
pub struct TableFilterSink {
    table: String,
    inner: Arc<dyn CdcSink>,
}

impl TableFilterSink {
    /// 创建过滤 sink
    pub fn new(table: &str, inner: Arc<dyn CdcSink>) -> Self {
        Self {
            table: table.to_string(),
            inner,
        }
    }
}

#[async_trait]
impl CdcSink for TableFilterSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        if event.source_table == self.table {
            self.inner.handle(event).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::event::ChangeEventType;
    use super::*;

    fn make_event(table: &str, pos: u64) -> ChangeEvent {
        ChangeEvent::new(
            ChangeEventType::Insert,
            "test_db",
            table,
            serde_json::json!({"id": pos}),
            super::super::event::ChangePosition::MysqlBinlog {
                filename: "bin.001".to_string(),
                position: pos,
            },
            1000 + pos,
        )
    }

    #[tokio::test]
    async fn dispatch_single_sink() {
        let sink = Arc::new(MemorySink::new());
        let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);
        let event = make_event("users", 1);
        dispatcher.dispatch(event).await.unwrap();
        assert_eq!(sink.count(), 1);
    }

    #[tokio::test]
    async fn dispatch_multiple_sinks() {
        let sink1 = Arc::new(MemorySink::new());
        let sink2 = Arc::new(MemorySink::new());
        let dispatcher = CdcEventDispatcher::new(vec![sink1.clone(), sink2.clone()], 100);
        let event = make_event("users", 1);
        dispatcher.dispatch(event).await.unwrap();
        assert_eq!(sink1.count(), 1);
        assert_eq!(sink2.count(), 1);
    }

    #[tokio::test]
    async fn dispatch_batch_events() {
        let sink = Arc::new(MemorySink::new());
        let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);
        let events: Vec<_> = (0..10).map(|i| make_event("users", i)).collect();
        let count = dispatcher.dispatch_batch(events).await.unwrap();
        assert_eq!(count, 10);
        assert_eq!(sink.count(), 10);
    }

    #[tokio::test]
    async fn table_filter_sink() {
        let inner = Arc::new(MemorySink::new());
        let filter = TableFilterSink::new("users", inner.clone());
        let users_event = make_event("users", 1);
        let orders_event = make_event("orders", 2);

        filter.handle(&users_event).await.unwrap();
        filter.handle(&orders_event).await.unwrap();
        assert_eq!(inner.count(), 1);
    }

    #[tokio::test]
    async fn dispatcher_run_from_channel() {
        let sink = Arc::new(MemorySink::new());
        let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);
        let (tx, rx) = mpsc::channel(100);

        for i in 0..5 {
            tx.send(make_event("users", i)).await.unwrap();
        }
        drop(tx);

        let count = dispatcher.run(rx).await.unwrap();
        assert_eq!(count, 5);
        assert_eq!(sink.count(), 5);
    }
}
