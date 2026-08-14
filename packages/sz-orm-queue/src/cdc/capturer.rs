//! DialectCapturer trait + 各方言捕获器

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;

use super::{CdcCheckpoint, CdcConfig, CdcError, ChangeEvent, CheckpointPosition, DbType};

/// 方言捕获器 trait
#[async_trait]
pub trait DialectCapturer: Send + Sync {
    /// 启动捕获，返回变更事件流
    async fn start_capture(
        &self,
        checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError>;

    /// 方言类型
    fn dialect(&self) -> DbType;
}

/// PostgreSQL WAL 逻辑复制捕获器
///
/// ⚠️ 状态：协议级捕获未实现（需 PostgreSQL 复制协议 + 真实 DB 集成环境）。
/// 返回明确的 WalNotConfigured/CaptureError，不假装成功。
/// 真实可用的 CDC 请使用 [`PollingCapturer`]（轮询式）。
pub struct WalCapturer {
    config: CdcConfig,
    conn_string: String,
    slot_name: String,
    wal_level: String,
}

impl WalCapturer {
    pub fn new(
        config: CdcConfig,
        conn_string: String,
        slot_name: String,
        wal_level: String,
    ) -> Self {
        Self {
            config,
            conn_string,
            slot_name,
            wal_level,
        }
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }

    pub fn wal_level(&self) -> &str {
        &self.wal_level
    }
}

#[async_trait]
impl DialectCapturer for WalCapturer {
    async fn start_capture(
        &self,
        checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        if self.wal_level != "logical" {
            return Err(CdcError::WalNotConfigured);
        }
        let _ = (&self.conn_string, &self.slot_name, checkpoint);
        Err(CdcError::CaptureError {
            reason: "WAL capture requires live PostgreSQL connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Postgres
    }
}

/// MySQL binlog 捕获器
///
/// ⚠️ 状态：协议级捕获未实现（需 MySQL binlog 协议解析 + 真实 DB）。
/// 真实可用的 CDC 请使用 [`PollingCapturer`]。
pub struct BinlogCapturer {
    config: CdcConfig,
    conn_string: String,
    server_id: u32,
    binlog_enabled: bool,
}

impl BinlogCapturer {
    pub fn new(
        config: CdcConfig,
        conn_string: String,
        server_id: u32,
        binlog_enabled: bool,
    ) -> Self {
        Self {
            config,
            conn_string,
            server_id,
            binlog_enabled,
        }
    }

    pub fn binlog_enabled(&self) -> bool {
        self.binlog_enabled
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }
}

#[async_trait]
impl DialectCapturer for BinlogCapturer {
    async fn start_capture(
        &self,
        checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        if !self.binlog_enabled {
            return Err(CdcError::BinlogNotEnabled);
        }
        let _ = (&self.conn_string, self.server_id, checkpoint);
        Err(CdcError::CaptureError {
            reason: "binlog capture requires live MySQL connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Mysql
    }
}

/// SQLite 触发器捕获器
pub struct TriggerCapturer {
    config: CdcConfig,
    db_path: String,
}

impl TriggerCapturer {
    pub fn new(config: CdcConfig, db_path: String) -> Self {
        Self { config, db_path }
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }
}

#[async_trait]
impl DialectCapturer for TriggerCapturer {
    async fn start_capture(
        &self,
        _checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        let _ = &self.db_path;
        Err(CdcError::CaptureError {
            reason: "trigger capture requires live SQLite connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Sqlite
    }
}

/// Oracle LogMiner 捕获器
///
/// ⚠️ 状态：协议级捕获未实现。真实可用的 CDC 请使用 [`PollingCapturer`]。
pub struct LogMinerCapturer {
    config: CdcConfig,
    conn_string: String,
}

impl LogMinerCapturer {
    pub fn new(config: CdcConfig, conn_string: String) -> Self {
        Self {
            config,
            conn_string,
        }
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }
}

#[async_trait]
impl DialectCapturer for LogMinerCapturer {
    async fn start_capture(
        &self,
        _checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        let _ = &self.conn_string;
        Err(CdcError::CaptureError {
            reason: "LogMiner capture requires live Oracle connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Oracle
    }
}

/// MSSQL CDC 捕获器
pub struct MssqlCdcCapturer {
    config: CdcConfig,
    conn_string: String,
}

impl MssqlCdcCapturer {
    pub fn new(config: CdcConfig, conn_string: String) -> Self {
        Self {
            config,
            conn_string,
        }
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }
}

#[async_trait]
impl DialectCapturer for MssqlCdcCapturer {
    async fn start_capture(
        &self,
        _checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        let _ = &self.conn_string;
        Err(CdcError::CaptureError {
            reason: "MSSQL CDC capture requires live MSSQL connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Mssql
    }
}

/// 按方言构造捕获器
pub fn create_capturer(
    config: CdcConfig,
    conn_string: &str,
) -> Result<Box<dyn DialectCapturer>, CdcError> {
    match config.dialect {
        DbType::Postgres => Ok(Box::new(WalCapturer::new(
            config,
            conn_string.to_string(),
            "sz_orm_cdc_slot".to_string(),
            "logical".to_string(),
        ))),
        DbType::Mysql => Ok(Box::new(BinlogCapturer::new(
            config,
            conn_string.to_string(),
            1001,
            true,
        ))),
        DbType::Sqlite => Ok(Box::new(TriggerCapturer::new(
            config,
            conn_string.to_string(),
        ))),
        DbType::Oracle => Ok(Box::new(LogMinerCapturer::new(
            config,
            conn_string.to_string(),
        ))),
        DbType::Mssql => Ok(Box::new(MssqlCdcCapturer::new(
            config,
            conn_string.to_string(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::{CheckpointStoreConfig, DownstreamConfig};

    fn test_config(dialect: DbType) -> CdcConfig {
        CdcConfig {
            tables: vec!["users".to_string()],
            dialect,
            downstream: vec![DownstreamConfig::Kafka {
                topic: "cdc".to_string(),
            }],
            checkpoint_store: CheckpointStoreConfig::Memory,
            masking: None,
        }
    }

    #[test]
    fn test_wal_capturer_dialect() {
        let capturer = WalCapturer::new(
            test_config(DbType::Postgres),
            "host=localhost".to_string(),
            "slot1".to_string(),
            "logical".to_string(),
        );
        assert_eq!(capturer.dialect(), DbType::Postgres);
    }

    #[test]
    fn test_binlog_capturer_dialect() {
        let capturer = BinlogCapturer::new(
            test_config(DbType::Mysql),
            "host=localhost".to_string(),
            1001,
            true,
        );
        assert_eq!(capturer.dialect(), DbType::Mysql);
    }

    #[test]
    fn test_trigger_capturer_dialect() {
        let capturer = TriggerCapturer::new(test_config(DbType::Sqlite), "test.db".to_string());
        assert_eq!(capturer.dialect(), DbType::Sqlite);
    }

    #[test]
    fn test_logminer_capturer_dialect() {
        let capturer =
            LogMinerCapturer::new(test_config(DbType::Oracle), "oracle_conn".to_string());
        assert_eq!(capturer.dialect(), DbType::Oracle);
    }

    #[test]
    fn test_mssql_cdc_capturer_dialect() {
        let capturer = MssqlCdcCapturer::new(test_config(DbType::Mssql), "mssql_conn".to_string());
        assert_eq!(capturer.dialect(), DbType::Mssql);
    }

    #[tokio::test]
    async fn test_wal_not_configured_error() {
        let capturer = WalCapturer::new(
            test_config(DbType::Postgres),
            "host=localhost".to_string(),
            "slot1".to_string(),
            "replica".to_string(),
        );
        let result = capturer.start_capture(None).await;
        assert!(matches!(result, Err(CdcError::WalNotConfigured)));
    }

    #[tokio::test]
    async fn test_binlog_not_enabled_error() {
        let capturer = BinlogCapturer::new(
            test_config(DbType::Mysql),
            "host=localhost".to_string(),
            1001,
            false,
        );
        let result = capturer.start_capture(None).await;
        assert!(matches!(result, Err(CdcError::BinlogNotEnabled)));
    }

    #[test]
    fn test_create_capturer_postgres() {
        let capturer = create_capturer(test_config(DbType::Postgres), "conn").unwrap();
        assert_eq!(capturer.dialect(), DbType::Postgres);
    }

    #[test]
    fn test_create_capturer_all_dialects() {
        for dialect in [
            DbType::Postgres,
            DbType::Mysql,
            DbType::Sqlite,
            DbType::Oracle,
            DbType::Mssql,
        ] {
            let capturer = create_capturer(test_config(dialect), "conn").unwrap();
            assert_eq!(capturer.dialect(), dialect);
        }
    }
}

// ============================================================================
// PollingCapturer — 轮询式 CDC 捕获器（真实实现）
//
// 背景：协议级捕获（PostgreSQL WAL 复制协议 / MySQL binlog 协议 /
// Oracle LogMiner / MSSQL CDC）需要真实数据库服务器与数千行协议实现，
// 在库内无法完成（v4.7.0 审计 P-4 登记）。本组件提供**真实可工作的
// 轮询式 CDC**（生产级方案之一）：调用方持有真实 DB 连接并提供增量
// 轮询函数（如 `SELECT ... WHERE seq > checkpoint`），本组件负责
// 检查点推进、增量获取与事件流输出。
// ============================================================================

use std::collections::VecDeque;
use std::time::Duration;

/// 增量轮询函数：从 checkpoint 拉取新变更，返回 (事件, 推进后的 checkpoint)
///
/// 调用方实现（持有真实 DB 连接）：
/// ```ignore
/// let poll_fn = |cp: &CdcCheckpoint| -> Result<(Vec<ChangeEvent>, CdcCheckpoint), CdcError> {
///     // SELECT * FROM change_log WHERE seq > cp.position 的 seq
///     // 返回新事件 + 新 checkpoint（游标推进）
/// };
/// ```
pub type PollFn =
    dyn Fn(&CdcCheckpoint) -> Result<(Vec<ChangeEvent>, CdcCheckpoint), CdcError> + Send + Sync;

/// 轮询式 CDC 捕获器（真实实现）
///
/// 基于检查点游标的增量轮询，不依赖复制协议：
///   - `poll_once`：单次增量拉取（checkpoint 推进）
///   - `start_capture`：持续轮询事件流（按 poll_interval 间隔）
///
/// 适用场景：变更频率中等的生产 CDC（轮询间隔 ≥ 1s），
/// 高吞吐场景请使用协议级捕获（PostgreSQL WAL / MySQL binlog）。
pub struct PollingCapturer {
    dialect: DbType,
    poll_interval: Duration,
    poll_fn: Arc<PollFn>,
}

impl PollingCapturer {
    /// 创建轮询捕获器
    ///
    /// `poll_interval` 轮询间隔；`poll_fn` 增量拉取函数（见 [`PollFn`]）。
    pub fn new(dialect: DbType, poll_interval: Duration, poll_fn: Arc<PollFn>) -> Self {
        Self {
            dialect,
            poll_interval,
            poll_fn,
        }
    }

    /// 方言类型
    pub fn dialect(&self) -> DbType {
        self.dialect
    }

    /// 单次增量拉取：从 checkpoint 拉取新事件并推进检查点
    ///
    /// `checkpoint` 为 `None` 时从初始位置（seq=0）拉取。
    pub async fn poll_once(
        &self,
        checkpoint: Option<CdcCheckpoint>,
    ) -> Result<(Vec<ChangeEvent>, CdcCheckpoint), CdcError> {
        let initial = CdcCheckpoint {
            dialect: self.dialect,
            position: CheckpointPosition::TriggerSeq(0),
            updated_at: 0,
        };
        let cp = checkpoint.unwrap_or(initial);
        (self.poll_fn)(&cp)
    }

    /// 启动持续轮询，返回变更事件流
    ///
    /// 内部按 `poll_interval` 间隔调用 `poll_fn`，事件逐个产出；
    /// 轮询错误时流结束（调用方可重连后重新 `start_capture`）。
    pub async fn start_capture(
        &self,
        checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        let poll_fn = Arc::clone(&self.poll_fn);
        let interval = self.poll_interval;
        let dialect = self.dialect;
        let initial_cp = CdcCheckpoint {
            dialect,
            position: CheckpointPosition::TriggerSeq(0),
            updated_at: 0,
        };

        let stream = futures::stream::unfold(
            (VecDeque::new(), checkpoint.unwrap_or(initial_cp)),
            move |(mut pending, mut cp)| {
                let poll_fn = Arc::clone(&poll_fn);
                async move {
                    loop {
                        // 先消费已拉取的事件
                        if let Some(ev) = pending.pop_front() {
                            return Some((ev, (pending, cp)));
                        }
                        // 拉取新一批
                        match (poll_fn)(&cp) {
                            Ok((events, new_cp)) => {
                                cp = new_cp;
                                pending.extend(events);
                                if pending.is_empty() {
                                    tokio::time::sleep(interval).await;
                                    continue;
                                }
                            }
                            Err(_) => return None, // 轮询错误：结束流
                        }
                    }
                }
            },
        );
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod polling_tests {
    use super::*;
    use crate::cdc::{ChangeOp, Row};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// 内存"伪数据源"：模拟 DB 变更日志（seq 递增）
    struct FakeChangeSource {
        events: Mutex<VecDeque<ChangeEvent>>,
        next_seq: Mutex<u64>,
    }

    impl FakeChangeSource {
        fn new(events: Vec<ChangeEvent>) -> Self {
            Self {
                events: Mutex::new(events.into()),
                next_seq: Mutex::new(0),
            }
        }

        /// 轮询函数：从 checkpoint 的 seq 开始拉取新事件
        fn poll(&self, cp: &CdcCheckpoint) -> Result<(Vec<ChangeEvent>, CdcCheckpoint), CdcError> {
            let mut events = self.events.lock().unwrap();
            let mut next = self.next_seq.lock().unwrap();
            let mut batch = vec![];
            // 伪数据源：pop 全部剩余事件（checkpoint 语义由调用方按 seq 过滤）
            while let Some(ev) = events.pop_front() {
                batch.push(ev);
                *next += 1;
            }
            let new_cp = CdcCheckpoint {
                dialect: cp.dialect,
                position: CheckpointPosition::TriggerSeq(*next),
                updated_at: 0,
            };
            Ok((batch, new_cp))
        }
    }

    fn make_event(seq: u64, name: &str) -> ChangeEvent {
        let mut row = Row::new();
        row.insert("id".to_string(), serde_json::json!(seq as i64));
        row.insert("name".to_string(), serde_json::json!(name));
        ChangeEvent {
            op: ChangeOp::Insert,
            before: None,
            after: Some(row),
            timestamp: seq,
            transaction_id: format!("tx-{seq}"),
            table: "users".to_string(),
            schema: "public".to_string(),
        }
    }

    #[tokio::test]
    async fn test_polling_capturer_poll_once_incremental() {
        let source = std::sync::Arc::new(FakeChangeSource::new(vec![
            make_event(1, "alice"),
            make_event(2, "bob"),
            make_event(3, "carol"),
        ]));
        let poll_fn = {
            let s = Arc::clone(&source);
            Arc::new(move |cp: &CdcCheckpoint| s.poll(cp))
        };
        let capturer = PollingCapturer::new(DbType::Sqlite, Duration::from_millis(10), poll_fn);

        // 第一次拉取：3 条 + checkpoint 推进到 3
        let (batch1, cp1) = capturer.poll_once(None).await.unwrap();
        assert_eq!(batch1.len(), 3, "首次拉取应拿到全部 3 条");
        assert_eq!(cp1.position, CheckpointPosition::TriggerSeq(3));

        // 第二次拉取：无新事件（checkpoint 已推进）
        let (batch2, cp2) = capturer.poll_once(Some(cp1)).await.unwrap();
        assert_eq!(batch2.len(), 0, "增量拉取：checkpoint 后无新事件");
        assert_eq!(cp2.position, CheckpointPosition::TriggerSeq(3));
    }

    #[tokio::test]
    async fn test_polling_capturer_stream_output() {
        let source = std::sync::Arc::new(FakeChangeSource::new(vec![
            make_event(1, "alice"),
            make_event(2, "bob"),
        ]));
        let poll_fn = {
            let s = Arc::clone(&source);
            Arc::new(move |cp: &CdcCheckpoint| s.poll(cp))
        };
        let capturer = PollingCapturer::new(DbType::Sqlite, Duration::from_millis(10), poll_fn);

        // 持续轮询流：收集前 2 个事件
        let mut stream = capturer.start_capture(None).await.unwrap();
        let mut seen = vec![];
        for _ in 0..2 {
            if let Some(ev) = futures::StreamExt::next(&mut stream).await {
                seen.push(ev.table.clone());
            }
        }
        assert_eq!(seen, vec!["users".to_string(), "users".to_string()]);
    }

    #[test]
    fn test_polling_capturer_dialect() {
        let source = std::sync::Arc::new(FakeChangeSource::new(vec![]));
        let poll_fn = {
            let s = Arc::clone(&source);
            Arc::new(move |cp: &CdcCheckpoint| s.poll(cp))
        };
        let capturer = PollingCapturer::new(DbType::Postgres, Duration::from_secs(1), poll_fn);
        assert_eq!(capturer.dialect(), DbType::Postgres);
    }
}
