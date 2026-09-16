//! CDC 同步编排器
//!
//! 编排 CDC 管道：事件源 → 过滤 → 变换 → Sink 分发。
//! 支持断点续传、至少一次语义、多 Sink 并行分发。

use std::sync::Arc;

use tokio::sync::mpsc;

use sz_orm_core::cdc::checkpoint::{CdcCheckpointStore, CdcError, SharedCheckpointStore};
use sz_orm_core::cdc::dispatcher::{CdcEventDispatcher, CdcSink};
use sz_orm_core::cdc::event::ChangeEvent;

/// CDC 同步配置
#[derive(Debug, Clone)]
pub struct CdcSyncConfig {
    /// 背压容量
    pub backpressure_capacity: usize,
    /// 最大重试次数
    pub max_retries: u32,
    /// 批次大小
    pub batch_size: usize,
}

impl Default for CdcSyncConfig {
    fn default() -> Self {
        Self {
            backpressure_capacity: 1024,
            max_retries: 3,
            batch_size: 100,
        }
    }
}

/// CDC 同步协调器
pub struct CdcSyncCoordinator {
    dispatcher: CdcEventDispatcher,
    checkpoint: SharedCheckpointStore,
    config: CdcSyncConfig,
    events_processed: std::sync::atomic::AtomicU64,
    events_failed: std::sync::atomic::AtomicU64,
}

impl CdcSyncCoordinator {
    /// 创建协调器
    pub fn new(
        sinks: Vec<Arc<dyn CdcSink>>,
        checkpoint: SharedCheckpointStore,
        config: CdcSyncConfig,
    ) -> Self {
        let dispatcher = CdcEventDispatcher::new(sinks, config.backpressure_capacity);
        Self {
            dispatcher,
            checkpoint,
            config,
            events_processed: std::sync::atomic::AtomicU64::new(0),
            events_failed: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 处理单个事件（至少一次语义）
    pub async fn process_event(&self, event: ChangeEvent) -> Result<bool, CdcError> {
        for attempt in 0..=self.config.max_retries {
            match self
                .dispatcher
                .dispatch_and_confirm(event.clone(), &self.checkpoint)
                .await
            {
                Ok(confirmed) => {
                    self.events_processed
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Ok(confirmed);
                }
                Err(CdcError::Backpressure) if attempt < self.config.max_retries => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10 * 2u64.pow(attempt)))
                        .await;
                    continue;
                }
                Err(e) => {
                    self.events_failed
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Err(e);
                }
            }
        }
        Err(CdcError::Backpressure)
    }

    /// 批量处理事件
    pub async fn process_batch(&self, events: Vec<ChangeEvent>) -> Result<usize, CdcError> {
        let total = events.len();
        let result = self.dispatcher.dispatch_batch(events).await;
        match result {
            Ok(count) => {
                self.events_processed
                    .fetch_add(count as u64, std::sync::atomic::Ordering::Relaxed);
                Ok(count)
            }
            Err(e) => {
                self.events_failed
                    .fetch_add(total as u64, std::sync::atomic::Ordering::Relaxed);
                Err(e)
            }
        }
    }

    /// 从检查点恢复
    pub async fn resume_from_checkpoint(&self) -> Result<Option<String>, CdcError> {
        let position = self.checkpoint.resume_from().await?;
        Ok(position.map(|p| format!("{:?}", p)))
    }

    /// 保存检查点
    pub async fn save_checkpoint(
        &self,
        position: &sz_orm_core::cdc::event::ChangePosition,
    ) -> Result<(), CdcError> {
        self.checkpoint
            .save_checkpoint_with_retry(position, self.config.max_retries)
            .await
    }

    /// 运行事件管道（从 channel 消费）
    pub async fn run_pipeline(
        &self,
        receiver: mpsc::Receiver<ChangeEvent>,
    ) -> Result<usize, CdcError> {
        self.dispatcher.run(receiver).await
    }

    /// 已处理事件数
    pub fn events_processed(&self) -> u64 {
        self.events_processed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 失败事件数
    pub fn events_failed(&self) -> u64 {
        self.events_failed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Sink 数量
    pub fn sink_count(&self) -> usize {
        self.dispatcher.sink_count()
    }
}

/// 创建内存检查点存储
pub fn create_memory_checkpoint() -> SharedCheckpointStore {
    Arc::new(CdcCheckpointStore::in_memory())
}

/// 创建文件检查点存储
pub fn create_file_checkpoint(path: std::path::PathBuf) -> SharedCheckpointStore {
    Arc::new(CdcCheckpointStore::file(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_core::cdc::dispatcher::MemorySink;
    use sz_orm_core::cdc::event::{ChangeEventType, ChangePosition};

    fn make_event(id: &str, table: &str) -> ChangeEvent {
        ChangeEvent::new(
            ChangeEventType::Insert,
            "db",
            table,
            serde_json::json!({"id": id}),
            ChangePosition::SqliteHook { seq: 1 },
            1000,
        )
    }

    fn make_coordinator(sinks: Vec<Arc<dyn CdcSink>>) -> CdcSyncCoordinator {
        CdcSyncCoordinator::new(sinks, create_memory_checkpoint(), CdcSyncConfig::default())
    }

    #[tokio::test]
    async fn test_process_single_event() {
        let sink = Arc::new(MemorySink::new());
        let coord = make_coordinator(vec![sink]);
        let event = make_event("1", "users");
        coord.process_event(event).await.unwrap();
        assert_eq!(coord.events_processed(), 1);
    }

    #[tokio::test]
    async fn test_process_batch_events() {
        let sink = Arc::new(MemorySink::new());
        let coord = make_coordinator(vec![sink]);
        let events = vec![
            make_event("1", "users"),
            make_event("2", "users"),
            make_event("3", "users"),
        ];
        let count = coord.process_batch(events).await.unwrap();
        assert_eq!(count, 3);
        assert_eq!(coord.events_processed(), 3);
    }

    #[tokio::test]
    async fn test_resume_from_empty_checkpoint() {
        let sink = Arc::new(MemorySink::new());
        let coord = make_coordinator(vec![sink]);
        let result = coord.resume_from_checkpoint().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_save_and_resume_checkpoint() {
        let sink = Arc::new(MemorySink::new());
        let coord = make_coordinator(vec![sink]);
        let pos = ChangePosition::SqliteHook { seq: 42 };
        coord.save_checkpoint(&pos).await.unwrap();
        let resumed = coord.resume_from_checkpoint().await.unwrap();
        assert!(resumed.is_some());
    }

    #[tokio::test]
    async fn test_multiple_sinks_parallel() {
        let sink1 = Arc::new(MemorySink::new());
        let sink2 = Arc::new(MemorySink::new());
        let coord = make_coordinator(vec![sink1, sink2]);
        let event = make_event("1", "users");
        coord.process_event(event).await.unwrap();
        assert_eq!(coord.sink_count(), 2);
    }

    #[tokio::test]
    async fn test_at_least_once_retry_on_backpressure() {
        struct BackpressureSink {
            attempt: std::sync::atomic::AtomicU32,
        }
        #[async_trait::async_trait]
        impl CdcSink for BackpressureSink {
            async fn handle(&self, _event: &ChangeEvent) -> Result<(), CdcError> {
                let n = self
                    .attempt
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n < 2 {
                    Err(CdcError::Backpressure)
                } else {
                    Ok(())
                }
            }
        }
        let sink = Arc::new(BackpressureSink {
            attempt: std::sync::atomic::AtomicU32::new(0),
        });
        let coord = make_coordinator(vec![sink]);
        let event = make_event("1", "users");
        let result = coord.process_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_failure_count_tracking() {
        struct FailingSink;
        #[async_trait::async_trait]
        impl CdcSink for FailingSink {
            async fn handle(&self, _: &ChangeEvent) -> Result<(), CdcError> {
                Err(CdcError::SinkError("fail".into()))
            }
        }
        let sink = Arc::new(FailingSink);
        let coord = make_coordinator(vec![sink]);
        let event = make_event("1", "users");
        let _ = coord.process_event(event).await;
        assert_eq!(coord.events_failed(), 1);
    }

    #[tokio::test]
    async fn test_checkpoint_clear_and_resume() {
        let checkpoint = create_memory_checkpoint();
        let pos = ChangePosition::SqliteHook { seq: 10 };
        checkpoint.save_checkpoint(&pos).await.unwrap();
        checkpoint.clear().await.unwrap();
        let result = checkpoint.resume_from().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_config_custom_values() {
        let sink = Arc::new(MemorySink::new());
        let config = CdcSyncConfig {
            backpressure_capacity: 512,
            max_retries: 5,
            batch_size: 50,
        };
        let coord = CdcSyncCoordinator::new(vec![sink], create_memory_checkpoint(), config);
        assert_eq!(coord.sink_count(), 1);
    }

    #[tokio::test]
    async fn test_empty_batch() {
        let sink = Arc::new(MemorySink::new());
        let coord = make_coordinator(vec![sink]);
        let count = coord.process_batch(vec![]).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_file_checkpoint_create() {
        let path = std::path::PathBuf::from("test_cdc_checkpoint.json");
        let checkpoint = create_file_checkpoint(path.clone());
        let pos = ChangePosition::SqliteHook { seq: 1 };
        checkpoint.save_checkpoint(&pos).await.unwrap();
        let _ = std::fs::remove_file(&path);
    }
}
