//! PostgreSQL WAL 变更捕获（v6.8.0）
//!
//! 通过 `sqlx` 连接 PostgreSQL，使用 `pg_logical_slot_get_changes` 获取逻辑复制槽中的变更事件。
//! 事件按 WAL LSN 有序返回，支持断点续传。

use tokio::sync::mpsc;

use super::checkpoint::{CdcError, SharedCheckpointStore};
use super::event::{ChangeEvent, ChangeEventType, ChangePosition};

/// PostgreSQL CDC 配置
#[derive(Debug, Clone)]
pub struct PostgresCdcConfig {
    /// 数据库连接字符串
    pub connection_string: String,
    /// 逻辑复制槽名
    pub slot_name: String,
    /// 起始 WAL LSN
    pub start_lsn: u64,
    /// 监听的表（空表示所有表）
    pub tables: Vec<String>,
}

impl PostgresCdcConfig {
    /// 创建配置
    pub fn new(connection_string: &str, slot_name: &str) -> Self {
        Self {
            connection_string: connection_string.to_string(),
            slot_name: slot_name.to_string(),
            start_lsn: 0,
            tables: Vec::new(),
        }
    }

    /// 设置起始 LSN
    pub fn with_start_lsn(mut self, lsn: u64) -> Self {
        self.start_lsn = lsn;
        self
    }

    /// 设置监听的表
    pub fn with_tables(mut self, tables: Vec<String>) -> Self {
        self.tables = tables;
        self
    }
}

/// PostgreSQL WAL 捕获器
pub struct PostgresWalCapturer {
    config: PostgresCdcConfig,
    checkpoint: SharedCheckpointStore,
}

impl PostgresWalCapturer {
    /// 创建捕获器
    pub fn new(config: PostgresCdcConfig, checkpoint: SharedCheckpointStore) -> Self {
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

    /// 解析起始位点
    async fn resolve_start_position(&self) -> Result<ChangePosition, CdcError> {
        if let Some(saved) = self.checkpoint.load_checkpoint().await? {
            return Ok(saved);
        }
        Ok(ChangePosition::PostgresWal {
            lsn: self.config.start_lsn,
        })
    }

    /// 捕获循环（通过 `pg_logical_slot_get_changes` 轮询）
    async fn capture_loop(
        config: PostgresCdcConfig,
        checkpoint: SharedCheckpointStore,
        tx: mpsc::Sender<ChangeEvent>,
        start_pos: ChangePosition,
    ) -> Result<(), CdcError> {
        let pool = sqlx::PgPool::connect(&config.connection_string)
            .await
            .map_err(|e| CdcError::ConnectionError(e.to_string()))?;

        super::validate_identifier(&config.slot_name)
            .map_err(|e| CdcError::ConnectionError(format!("slot_name 非法: {}", e)))?;

        let start_lsn = match start_pos {
            ChangePosition::PostgresWal { lsn } => lsn,
            _ => return Err(CdcError::ConnectionError("invalid position type".into())),
        };

        let mut current_lsn = start_lsn;

        loop {
            let sql = format!(
                "SELECT lsn, data FROM pg_logical_slot_get_changes('{}', NULL, 100)",
                config.slot_name
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
                let lsn_str: String = row
                    .try_get("lsn")
                    .map_err(|e| CdcError::ConnectionError(e.to_string()))?;
                let data: String = row
                    .try_get("data")
                    .map_err(|e| CdcError::ConnectionError(e.to_string()))?;

                let lsn = Self::parse_lsn(&lsn_str)?;

                if lsn <= current_lsn {
                    continue;
                }

                if let Some(event) = Self::parse_wal_event(lsn, &data, &config) {
                    if tx.send(event).await.is_err() {
                        return Ok(());
                    }
                }

                let new_pos = ChangePosition::PostgresWal { lsn };
                checkpoint.save_checkpoint(&new_pos).await?;
                current_lsn = lsn;
            }
        }
    }

    /// 解析 LSN 字符串（如 "0/16B3748"）为 u64
    fn parse_lsn(lsn_str: &str) -> Result<u64, CdcError> {
        let parts: Vec<&str> = lsn_str.split('/').collect();
        if parts.len() != 2 {
            return Err(CdcError::ConnectionError(format!(
                "invalid LSN format: {}",
                lsn_str
            )));
        }
        let high = u64::from_str_radix(parts[0], 16)
            .map_err(|e| CdcError::ConnectionError(e.to_string()))?;
        let low = u64::from_str_radix(parts[1], 16)
            .map_err(|e| CdcError::ConnectionError(e.to_string()))?;
        Ok((high << 32) | low)
    }

    /// 解析 WAL 事件数据为标准变更事件
    fn parse_wal_event(lsn: u64, data: &str, config: &PostgresCdcConfig) -> Option<ChangeEvent> {
        let (change_type, table) = if data.contains("INSERT:") {
            (ChangeEventType::Insert, Self::extract_table(data))
        } else if data.contains("UPDATE:") {
            (ChangeEventType::Update, Self::extract_table(data))
        } else if data.contains("DELETE:") {
            (ChangeEventType::Delete, Self::extract_table(data))
        } else {
            return None;
        };

        let table = table?;

        if !config.tables.is_empty() && !config.tables.contains(&table) {
            return None;
        }

        let timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;

        Some(ChangeEvent::new(
            change_type,
            "postgres",
            &table,
            serde_json::json!({"data": data}),
            ChangePosition::PostgresWal { lsn },
            timestamp_ms,
        ))
    }

    /// 从 WAL 事件数据提取表名
    fn extract_table(data: &str) -> Option<String> {
        for keyword in ["INSERT:", "UPDATE:", "DELETE:"] {
            if let Some(pos) = data.find(keyword) {
                let rest = &data[pos + keyword.len()..];
                let rest = rest.trim_start();
                if let Some(colon_pos) = rest.find(':') {
                    let table = rest[..colon_pos].trim();
                    if !table.is_empty() {
                        return Some(table.to_string());
                    }
                }
            }
        }
        None
    }

    /// 获取当前 WAL 位点
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
    fn postgres_cdc_config_creation() {
        let config = PostgresCdcConfig::new(
            "postgres://postgres:test123@127.0.0.1/sz_orm_test",
            "test_slot",
        )
        .with_start_lsn(12345)
        .with_tables(vec!["users".to_string()]);

        assert_eq!(config.slot_name, "test_slot");
        assert_eq!(config.start_lsn, 12345);
        assert_eq!(config.tables.len(), 1);
    }

    #[test]
    fn postgres_wal_capturer_creation() {
        let config = PostgresCdcConfig::new(
            "postgres://postgres:test123@127.0.0.1/sz_orm_test",
            "test_slot",
        );
        let checkpoint = Arc::new(CdcCheckpointStore::in_memory());
        let capturer = PostgresWalCapturer::new(config, checkpoint);
        assert_eq!(capturer.config.slot_name, "test_slot");
    }

    #[test]
    fn parse_lsn_valid() {
        let lsn = PostgresWalCapturer::parse_lsn("0/16B3748").unwrap();
        assert!(lsn > 0);
    }

    #[test]
    fn parse_lsn_zero() {
        let lsn = PostgresWalCapturer::parse_lsn("0/0").unwrap();
        assert_eq!(lsn, 0);
    }

    #[test]
    fn parse_lsn_invalid() {
        let result = PostgresWalCapturer::parse_lsn("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn parse_wal_event_insert() {
        let config = PostgresCdcConfig::new("postgres://localhost", "slot");
        let event = PostgresWalCapturer::parse_wal_event(
            12345,
            "INSERT: public.users: {\"id\": 1, \"name\": \"Alice\"}",
            &config,
        );
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event_type, ChangeEventType::Insert);
        assert_eq!(event.source_table, "public.users");
    }

    #[test]
    fn parse_wal_event_update() {
        let config = PostgresCdcConfig::new("postgres://localhost", "slot");
        let event = PostgresWalCapturer::parse_wal_event(
            12346,
            "UPDATE: public.orders: {\"id\": 1, \"total\": 100}",
            &config,
        );
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event_type, ChangeEventType::Update);
    }

    #[test]
    fn parse_wal_event_delete() {
        let config = PostgresCdcConfig::new("postgres://localhost", "slot");
        let event = PostgresWalCapturer::parse_wal_event(
            12347,
            "DELETE: public.users: {\"id\": 1}",
            &config,
        );
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event_type, ChangeEventType::Delete);
    }

    #[test]
    fn parse_wal_event_skip_unknown() {
        let config = PostgresCdcConfig::new("postgres://localhost", "slot");
        let event = PostgresWalCapturer::parse_wal_event(12348, "BEGIN:", &config);
        assert!(event.is_none());
    }

    #[test]
    fn parse_wal_event_table_filter() {
        let config = PostgresCdcConfig::new("postgres://localhost", "slot")
            .with_tables(vec!["public.users".to_string()]);
        let event = PostgresWalCapturer::parse_wal_event(
            12349,
            "INSERT: public.orders: {\"id\": 1}",
            &config,
        );
        assert!(event.is_none());
    }

    #[test]
    fn extract_table_from_wal_data() {
        assert_eq!(
            PostgresWalCapturer::extract_table("INSERT: public.users: {}"),
            Some("public.users".to_string())
        );
        assert_eq!(
            PostgresWalCapturer::extract_table("UPDATE: public.orders: {}"),
            Some("public.orders".to_string())
        );
        assert_eq!(
            PostgresWalCapturer::extract_table("DELETE: public.users: {}"),
            Some("public.users".to_string())
        );
        assert_eq!(PostgresWalCapturer::extract_table("BEGIN:"), None);
    }

    #[test]
    fn postgres_wal_position_ordering() {
        let p1 = ChangePosition::PostgresWal { lsn: 100 };
        let p2 = ChangePosition::PostgresWal { lsn: 200 };
        assert!(p1.order_key().1 < p2.order_key().1);
    }
}
