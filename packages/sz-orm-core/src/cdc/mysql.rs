//! MySQL Binlog 变更捕获（v6.8.0）
//!
//! 通过 `sqlx` 连接 MySQL，使用 `SHOW BINLOG EVENTS` 获取 binlog 事件列表。
//! 事件按位点有序返回，支持断点续传。

use tokio::sync::mpsc;

use super::checkpoint::{CdcError, SharedCheckpointStore};
use super::event::{ChangeEvent, ChangeEventType, ChangePosition};

/// MySQL CDC 配置
#[derive(Debug, Clone)]
pub struct MysqlCdcConfig {
    /// 数据库连接字符串
    pub connection_string: String,
    /// 起始 binlog 文件名
    pub start_filename: String,
    /// 起始位点
    pub start_position: u64,
    /// 监听的表（空表示所有表）
    pub tables: Vec<String>,
}

impl MysqlCdcConfig {
    /// 创建配置
    pub fn new(connection_string: &str) -> Self {
        Self {
            connection_string: connection_string.to_string(),
            start_filename: String::new(),
            start_position: 0,
            tables: Vec::new(),
        }
    }

    /// 设置起始位点
    pub fn with_start_position(mut self, filename: &str, position: u64) -> Self {
        self.start_filename = filename.to_string();
        self.start_position = position;
        self
    }

    /// 设置监听的表
    pub fn with_tables(mut self, tables: Vec<String>) -> Self {
        self.tables = tables;
        self
    }
}

/// MySQL Binlog 捕获器
pub struct MysqlBinlogCapturer {
    config: MysqlCdcConfig,
    checkpoint: SharedCheckpointStore,
}

impl MysqlBinlogCapturer {
    /// 创建捕获器
    pub fn new(config: MysqlCdcConfig, checkpoint: SharedCheckpointStore) -> Self {
        Self { config, checkpoint }
    }

    /// 启动捕获，返回事件接收端
    pub async fn start(&self) -> Result<mpsc::Receiver<ChangeEvent>, CdcError> {
        let (tx, rx) = mpsc::channel(1024);

        let start_pos = self.resolve_start_position().await?;

        let config = self.config.clone();
        let checkpoint = self.checkpoint.clone();

        tokio::spawn(async move {
            let _ = Self::capture_loop(config, checkpoint, tx, start_pos).await;
        });

        Ok(rx)
    }

    /// 解析起始位点：优先从 checkpoint 恢复，否则用配置
    async fn resolve_start_position(&self) -> Result<ChangePosition, CdcError> {
        if let Some(saved) = self.checkpoint.load_checkpoint().await? {
            return Ok(saved);
        }
        Ok(ChangePosition::MysqlBinlog {
            filename: self.config.start_filename.clone(),
            position: self.config.start_position,
        })
    }

    /// 捕获循环（通过 `SHOW BINLOG EVENTS` 轮询）
    async fn capture_loop(
        config: MysqlCdcConfig,
        checkpoint: SharedCheckpointStore,
        tx: mpsc::Sender<ChangeEvent>,
        start_pos: ChangePosition,
    ) -> Result<(), CdcError> {
        let pool = sqlx::MySqlPool::connect(&config.connection_string)
            .await
            .map_err(|e| CdcError::ConnectionError(e.to_string()))?;

        let (mut current_filename, mut current_position) = match start_pos {
            ChangePosition::MysqlBinlog { filename, position } => (filename, position),
            _ => return Err(CdcError::ConnectionError("invalid position type".into())),
        };

        let mut last_seen_pos = current_position;

        super::validate_identifier(&current_filename).map_err(CdcError::IoError)?;

        loop {
            let sql = format!(
                "SHOW BINLOG EVENTS IN '{}' FROM {} LIMIT 100",
                current_filename, current_position
            );

            let rows = sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                .fetch_all(&pool)
                .await
                .map_err(|e| CdcError::ConnectionError(e.to_string()))?;

            if rows.is_empty() {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            for row in rows {
                use sqlx::Row;
                let log_name: String = row
                    .try_get("Log_name")
                    .map_err(|e| CdcError::ConnectionError(e.to_string()))?;
                let pos: u64 = row
                    .try_get("Pos")
                    .map_err(|e| CdcError::ConnectionError(e.to_string()))?;
                let event_type: String = row
                    .try_get("Event_type")
                    .map_err(|e| CdcError::ConnectionError(e.to_string()))?;
                let info: String = row
                    .try_get("Info")
                    .map_err(|e| CdcError::ConnectionError(e.to_string()))?;

                if pos <= last_seen_pos {
                    continue;
                }

                if let Some(event) =
                    Self::parse_binlog_event(&log_name, pos, &event_type, &info, &config)
                {
                    if tx.send(event).await.is_err() {
                        return Ok(());
                    }
                }

                let new_pos = ChangePosition::MysqlBinlog {
                    filename: log_name.clone(),
                    position: pos,
                };
                checkpoint.save_checkpoint(&new_pos).await?;
                last_seen_pos = pos;
                if log_name != current_filename {
                    super::validate_identifier(&log_name).map_err(CdcError::IoError)?;
                    current_filename = log_name;
                }
                current_position = pos;
            }
        }
    }

    /// 解析 binlog 事件为标准变更事件
    fn parse_binlog_event(
        filename: &str,
        position: u64,
        event_type: &str,
        info: &str,
        config: &MysqlCdcConfig,
    ) -> Option<ChangeEvent> {
        let (change_type, table) = match event_type {
            "Write_rows" => (ChangeEventType::Insert, Self::extract_table(info)),
            "Update_rows" => (ChangeEventType::Update, Self::extract_table(info)),
            "Delete_rows" => (ChangeEventType::Delete, Self::extract_table(info)),
            _ => return None,
        };

        let table = table?;

        if !config.tables.is_empty() && !config.tables.contains(&table) {
            return None;
        }

        let timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;

        Some(ChangeEvent::new(
            change_type,
            "mysql",
            &table,
            serde_json::json!({"info": info}),
            ChangePosition::MysqlBinlog {
                filename: filename.to_string(),
                position,
            },
            timestamp_ms,
        ))
    }

    /// 从 binlog 事件 info 字段提取表名
    fn extract_table(info: &str) -> Option<String> {
        if info.starts_with("table_id:") {
            Some(format!("table_from_binlog_{}", info))
        } else {
            None
        }
    }

    /// 获取当前 binlog 位点
    pub async fn current_position(&self) -> Result<ChangePosition, CdcError> {
        self.checkpoint
            .load_checkpoint()
            .await?
            .ok_or_else(|| CdcError::ConnectionError("no checkpoint yet".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use super::super::checkpoint::CdcCheckpointStore;

    #[test]
    fn mysql_cdc_config_creation() {
        let config = MysqlCdcConfig::new("mysql://root:test123@127.0.0.1/sz_orm_test")
            .with_start_position("bin.000001", 100)
            .with_tables(vec!["users".to_string(), "orders".to_string()]);

        assert_eq!(config.start_filename, "bin.000001");
        assert_eq!(config.start_position, 100);
        assert_eq!(config.tables.len(), 2);
    }

    #[test]
    fn mysql_binlog_capturer_creation() {
        let config = MysqlCdcConfig::new("mysql://root:test123@127.0.0.1/sz_orm_test");
        let checkpoint = Arc::new(CdcCheckpointStore::in_memory());
        let capturer = MysqlBinlogCapturer::new(config, checkpoint);
        assert!(capturer.config.tables.is_empty());
    }

    #[test]
    fn parse_binlog_event_insert() {
        let config = MysqlCdcConfig::new("mysql://localhost");
        let event = MysqlBinlogCapturer::parse_binlog_event(
            "bin.000001",
            100,
            "Write_rows",
            "table_id: 42 flags: STMT_END_F",
            &config,
        );
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event_type, ChangeEventType::Insert);
    }

    #[test]
    fn parse_binlog_event_skip_non_row_events() {
        let config = MysqlCdcConfig::new("mysql://localhost");
        let event = MysqlBinlogCapturer::parse_binlog_event(
            "bin.000001",
            100,
            "Format_desc",
            "Server ver: 8.0",
            &config,
        );
        assert!(event.is_none());
    }

    #[test]
    fn parse_binlog_event_table_filter() {
        let config =
            MysqlCdcConfig::new("mysql://localhost").with_tables(vec!["users".to_string()]);
        let event = MysqlBinlogCapturer::parse_binlog_event(
            "bin.000001",
            100,
            "Write_rows",
            "table_id: 42",
            &config,
        );
        assert!(event.is_none());
    }
}
