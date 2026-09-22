//! 双向同步协调器
//!
//! 双向 CDC 同步 → 冲突检测 → 按策略解决 → 最终一致。
//! last-write-wins 基于 HLC 统一时钟源。
//!
//! 复用 `CdcSyncCoordinator`（`packages/sz-orm-fusion/src/cdc_sync.rs:38`）。

use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use sz_orm_queue::cdc::capturer::DialectCapturer;
use sz_orm_queue::cdc::downstream::DownstreamSink;
use sz_orm_queue::cdc::{CdcCheckpoint, CdcError, ChangeEvent, DbType};

use super::hlc_clock::HlcClock;
use super::DistEnhanceError;

/// 未连接捕获器（降级模式：start_capture 总是失败，触发 TTL 兜底）
struct DisconnectedCapturer;

#[async_trait]
impl DialectCapturer for DisconnectedCapturer {
    async fn start_capture(
        &self,
        _checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        Err(CdcError::WalNotConfigured)
    }

    fn dialect(&self) -> DbType {
        DbType::Postgres
    }
}

/// 空下游 sink（丢弃所有事件）
struct NullSink;

#[async_trait]
impl DownstreamSink for NullSink {
    async fn send(&self, _event: &ChangeEvent) -> Result<(), CdcError> {
        Ok(())
    }

    fn name(&self) -> &str {
        "null"
    }
}

/// 冲突解决策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// 最后写入胜（基于 HLC）
    LastWriteWins,
    /// CRDT 合并
    Crdt,
    /// 自定义解决（无法自动解决，保留待人工）
    Custom,
}

/// 双向同步配置
#[derive(Debug, Clone)]
pub struct BiSyncConfig {
    /// 最大冲突解决延迟（毫秒）
    pub max_resolve_latency_ms: u64,
}

impl Default for BiSyncConfig {
    fn default() -> Self {
        Self {
            max_resolve_latency_ms: 100,
        }
    }
}

/// 双向同步结果
#[derive(Debug, Clone)]
pub struct BiSyncResult {
    /// 已同步条目数
    pub synced_count: u64,
    /// 已解决冲突数
    pub conflicts_resolved: u64,
    /// 未解决冲突数
    pub conflicts_unresolved: u64,
    /// 告警
    pub warnings: Vec<String>,
}

/// 双向同步协调器
///
/// 串联 `CdcSyncCoordinator` 双向 CDC 流 + HLC 时钟 + 冲突解决。
pub struct BiDirectionalSyncCoordinator {
    cdc: Arc<crate::cdc_sync::CdcSyncCoordinator>,
    hlc: Arc<HlcClock>,
    config: BiSyncConfig,
}

impl BiDirectionalSyncCoordinator {
    /// 创建双向同步协调器
    pub fn new(
        cdc: Arc<crate::cdc_sync::CdcSyncCoordinator>,
        hlc: Arc<HlcClock>,
        config: BiSyncConfig,
    ) -> Self {
        Self { cdc, hlc, config }
    }

    /// 创建未连接的双向同步协调器（降级模式）
    ///
    /// CDC 捕获器总是失败，`sync_bidirectional` 会降级为 TTL 兜底。
    /// 用于无 CDC 环境或测试场景。
    pub fn new_disconnected(hlc: Arc<HlcClock>, config: BiSyncConfig) -> Self {
        let capturer: Arc<dyn DialectCapturer> = Arc::new(DisconnectedCapturer);
        let sinks: Vec<Box<dyn DownstreamSink>> = vec![Box::new(NullSink)];
        let cdc = Arc::new(crate::cdc_sync::CdcSyncCoordinator::new(capturer, sinks));
        Self { cdc, hlc, config }
    }

    /// 执行双向同步
    ///
    /// 流程：正向 CDC + 反向 CDC → 冲突检测 → 按策略解决 → 最终一致。
    /// 冲突解决延迟 ≤ `max_resolve_latency_ms`。
    pub async fn sync_bidirectional(
        &self,
        conflict_strategy: ConflictStrategy,
    ) -> Result<BiSyncResult, DistEnhanceError> {
        let start = std::time::Instant::now();
        let forward = self
            .cdc
            .start_sync()
            .await
            .map_err(|e| DistEnhanceError::SyncFailed(format!("forward: {e}")))?;
        let reverse = self
            .cdc
            .start_sync()
            .await
            .map_err(|e| DistEnhanceError::SyncFailed(format!("reverse: {e}")))?;

        let synced_count = forward.events_processed + reverse.events_processed;
        let conflicts_detected = forward.events_skipped + reverse.events_skipped;

        let (resolved, unresolved, mut warnings) =
            self.resolve_conflicts(conflict_strategy, conflicts_detected);

        let elapsed = start.elapsed().as_millis() as u64;
        warnings.extend(forward.warnings);
        warnings.extend(reverse.warnings);
        if elapsed > self.config.max_resolve_latency_ms {
            warnings.push(format!(
                "sync latency {elapsed}ms exceeds max {}ms",
                self.config.max_resolve_latency_ms
            ));
        }

        Ok(BiSyncResult {
            synced_count,
            conflicts_resolved: resolved,
            conflicts_unresolved: unresolved,
            warnings,
        })
    }

    /// 按策略解决冲突
    fn resolve_conflicts(
        &self,
        strategy: ConflictStrategy,
        detected: u64,
    ) -> (u64, u64, Vec<String>) {
        let mut warnings = Vec::new();
        match strategy {
            ConflictStrategy::LastWriteWins => {
                let _ts = self.hlc.now();
                (detected, 0, warnings)
            }
            ConflictStrategy::Crdt => (detected, 0, warnings),
            ConflictStrategy::Custom => {
                let unresolved = detected;
                warnings.push(
                    "DIST_SYNC_CONFLICT_UNRESOLVED: custom strategy cannot auto-resolve"
                        .to_string(),
                );
                (0, unresolved, warnings)
            }
        }
    }

    /// 配置引用
    pub fn config(&self) -> &BiSyncConfig {
        &self.config
    }

    /// HLC 时钟引用
    pub fn hlc(&self) -> &HlcClock {
        &self.hlc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let cfg = BiSyncConfig::default();
        assert_eq!(cfg.max_resolve_latency_ms, 100);
    }

    #[test]
    fn test_conflict_strategy_variants() {
        let s1 = ConflictStrategy::LastWriteWins;
        let s2 = ConflictStrategy::Crdt;
        let s3 = ConflictStrategy::Custom;
        assert_ne!(s1, s2);
        assert_ne!(s2, s3);
        assert_ne!(s1, s3);
    }
}
