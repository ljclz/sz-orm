//! CDC 增量同步协调器（`db-fusion-v2` feature）
//!
//! [`CdcSyncCoordinator`] 复用既有 `DialectCapturer` + `DownstreamSink` + `distribute_to_all`，
//! 主库变更自动同步到缓存/搜索索引下游。
//!
//! 复用标注：
//! - `DialectCapturer` `packages/sz-orm-queue/src/cdc/capturer.rs:12`
//! - `DownstreamSink` `packages/sz-orm-queue/src/cdc/downstream.rs:12`
//! - `distribute_to_all` `packages/sz-orm-queue/src/cdc/downstream.rs:178`
//! - `CdcCheckpoint` `packages/sz-orm-queue/src/cdc/mod.rs:68`
//! - `apply_masking` `packages/sz-orm-queue/src/cdc/masking.rs:11`

use std::sync::Arc;

use futures::StreamExt;
use sz_orm_queue::cdc::capturer::DialectCapturer;
use sz_orm_queue::cdc::downstream::{distribute_to_all, DownstreamSink};
use sz_orm_queue::cdc::masking::apply_masking;
use sz_orm_queue::cdc::{CdcCheckpoint, CdcError, MaskingRuleMap};

/// CDC 同步结果
#[derive(Debug, Clone)]
pub struct SyncOutcome {
    /// 已处理事件数
    pub events_processed: u64,
    /// 已跳过事件数（脱敏失败等）
    pub events_skipped: u64,
    /// 是否降级到 TTL 兜底
    pub degraded_to_ttl: bool,
    /// 告警信息
    pub warnings: Vec<String>,
}

/// CDC 同步协调器
///
/// 串联 `DialectCapturer` → 变更事件脱敏 → `distribute_to_all` 下游分发，
/// 支持断点续传，捕获失败降级为 TTL 兜底。
pub struct CdcSyncCoordinator {
    capturer: Arc<dyn DialectCapturer>,
    sinks: Vec<Box<dyn DownstreamSink>>,
    checkpoint: Option<CdcCheckpoint>,
    masking_rules: Option<MaskingRuleMap>,
}

impl CdcSyncCoordinator {
    /// 创建 CDC 同步协调器
    pub fn new(capturer: Arc<dyn DialectCapturer>, sinks: Vec<Box<dyn DownstreamSink>>) -> Self {
        Self {
            capturer,
            sinks,
            checkpoint: None,
            masking_rules: None,
        }
    }

    /// 设置断点续传位置
    pub fn with_checkpoint(mut self, checkpoint: CdcCheckpoint) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    /// 设置脱敏规则
    pub fn with_masking(mut self, rules: MaskingRuleMap) -> Self {
        self.masking_rules = Some(rules);
        self
    }

    /// 启动 CDC 同步
    ///
    /// 流程：`DialectCapturer::start_capture` → 流式接收 ChangeEvent →
    /// 变更事件脱敏 → `distribute_to_all` 分发到下游。
    ///
    /// 捕获失败（WAL/binlog 未配置）降级为 TTL 过期兜底，告警"CDC capture failed, relying on TTL"。
    pub async fn start_sync(&self) -> Result<SyncOutcome, CdcError> {
        let mut outcome = SyncOutcome {
            events_processed: 0,
            events_skipped: 0,
            degraded_to_ttl: false,
            warnings: Vec::new(),
        };

        let mut stream = match self.capturer.start_capture(self.checkpoint.clone()).await {
            Ok(s) => s,
            Err(e) => {
                outcome.degraded_to_ttl = true;
                outcome
                    .warnings
                    .push(format!("CDC capture failed, relying on TTL: {e}"));
                return Ok(outcome);
            }
        };

        while let Some(mut event) = stream.next().await {
            if let Some(rules) = &self.masking_rules {
                apply_masking(&mut event, rules);
            }
            let results = distribute_to_all(&self.sinks, &event).await;
            let all_ok = results.iter().all(|r| r.is_ok());
            if all_ok {
                outcome.events_processed += 1;
            } else {
                outcome.events_skipped += 1;
                let failed: Vec<&str> = results
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| r.is_err().then_some(self.sinks[i].name()))
                    .collect();
                outcome.warnings.push(format!(
                    "CDC event distribution partially failed: {failed:?}"
                ));
            }
        }

        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::pin::Pin;
    use sz_orm_queue::cdc::capturer::DialectCapturer;
    use sz_orm_queue::cdc::downstream::InMemorySink;
    use sz_orm_queue::cdc::{ChangeEvent, ChangeOp, CheckpointPosition, DbType};

    fn make_event(txid: &str, table: &str) -> ChangeEvent {
        let mut after = HashMap::new();
        after.insert("name".to_string(), serde_json::Value::String("test".into()));
        ChangeEvent {
            op: ChangeOp::Insert,
            before: None,
            after: Some(after),
            timestamp: 0,
            transaction_id: txid.into(),
            table: table.into(),
            schema: "public".into(),
        }
    }

    /// 测试用有限流捕获器
    struct FiniteCapturer {
        events: Vec<ChangeEvent>,
    }

    impl FiniteCapturer {
        fn new(events: Vec<ChangeEvent>) -> Self {
            Self { events }
        }
    }

    #[async_trait]
    impl DialectCapturer for FiniteCapturer {
        async fn start_capture(
            &self,
            _checkpoint: Option<CdcCheckpoint>,
        ) -> Result<Pin<Box<dyn futures::Stream<Item = ChangeEvent> + Send>>, CdcError> {
            let events = self.events.clone();
            Ok(Box::pin(futures::stream::iter(events)))
        }

        fn dialect(&self) -> DbType {
            DbType::Postgres
        }
    }

    /// 始终失败的捕获器
    struct FailingCapturer;

    #[async_trait]
    impl DialectCapturer for FailingCapturer {
        async fn start_capture(
            &self,
            _checkpoint: Option<CdcCheckpoint>,
        ) -> Result<Pin<Box<dyn futures::Stream<Item = ChangeEvent> + Send>>, CdcError> {
            Err(CdcError::WalNotConfigured)
        }

        fn dialect(&self) -> DbType {
            DbType::Postgres
        }
    }

    #[tokio::test]
    async fn cdc_sync_distributes_events() {
        let capturer = Arc::new(FiniteCapturer::new(vec![
            make_event("tx-1", "users"),
            make_event("tx-2", "orders"),
        ]));
        let sink = Box::new(InMemorySink::new("test-sink".into()));
        let coord = CdcSyncCoordinator::new(capturer, vec![sink]);
        let outcome = coord.start_sync().await.unwrap();

        assert_eq!(outcome.events_processed, 2);
        assert_eq!(outcome.events_skipped, 0);
        assert!(!outcome.degraded_to_ttl);
    }

    #[tokio::test]
    async fn cdc_capture_failure_degrades_to_ttl() {
        let capturer = Arc::new(FailingCapturer);
        let sink = Box::new(InMemorySink::new("test-sink".into()));
        let coord = CdcSyncCoordinator::new(capturer, vec![sink]);
        let outcome = coord.start_sync().await.unwrap();

        assert!(outcome.degraded_to_ttl);
        assert!(outcome
            .warnings
            .iter()
            .any(|w| w.contains("CDC capture failed")));
        assert_eq!(outcome.events_processed, 0);
    }

    #[tokio::test]
    async fn cdc_checkpoint_resume() {
        let capturer = Arc::new(FiniteCapturer::new(vec![make_event("tx-3", "users")]));
        let sink = Box::new(InMemorySink::new("test-sink".into()));
        let checkpoint = CdcCheckpoint {
            dialect: DbType::Postgres,
            position: CheckpointPosition::WalLsn(12345),
            updated_at: 0,
        };
        let coord = CdcSyncCoordinator::new(capturer, vec![sink]).with_checkpoint(checkpoint);
        let outcome = coord.start_sync().await.unwrap();

        assert_eq!(outcome.events_processed, 1);
    }

    #[tokio::test]
    async fn cdc_masking_applied() {
        let mut event = make_event("tx-1", "users");
        event.after = Some({
            let mut row = HashMap::new();
            row.insert(
                "phone".to_string(),
                serde_json::Value::String("13800138000".into()),
            );
            row
        });

        let capturer = Arc::new(FiniteCapturer::new(vec![event]));
        let sink = Box::new(InMemorySink::new("test-sink".into()));
        let rules = sz_orm_queue::cdc::masking::build_masking_rules();
        let coord = CdcSyncCoordinator::new(capturer, vec![sink]).with_masking(rules);
        let outcome = coord.start_sync().await.unwrap();

        assert_eq!(outcome.events_processed, 1);
    }

    #[tokio::test]
    async fn cdc_empty_stream() {
        let capturer = Arc::new(FiniteCapturer::new(vec![]));
        let sink = Box::new(InMemorySink::new("test-sink".into()));
        let coord = CdcSyncCoordinator::new(capturer, vec![sink]);
        let outcome = coord.start_sync().await.unwrap();

        assert_eq!(outcome.events_processed, 0);
        assert!(!outcome.degraded_to_ttl);
    }
}
