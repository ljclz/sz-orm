//! CDC 标准变更事件（v6.8.0）

use serde::{Deserialize, Serialize};

/// 变更事件类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChangeEventType {
    /// 插入
    Insert,
    /// 更新
    Update,
    /// 删除
    Delete,
}

/// 变更位点
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangePosition {
    /// MySQL Binlog 位点
    MysqlBinlog {
        /// Binlog 文件名
        filename: String,
        /// 文件内偏移
        position: u64,
    },
    /// PostgreSQL WAL 位点
    PostgresWal {
        /// WAL LSN
        lsn: u64,
    },
    /// SQLite update-hook 序号
    SqliteHook {
        /// 递增序号
        seq: u64,
    },
}

impl ChangePosition {
    /// 返回位点单调递增的比较键
    pub fn order_key(&self) -> (String, u64) {
        match self {
            ChangePosition::MysqlBinlog { filename, position } => (filename.clone(), *position),
            ChangePosition::PostgresWal { lsn } => ("pg_wal".to_string(), *lsn),
            ChangePosition::SqliteHook { seq } => ("sqlite".to_string(), *seq),
        }
    }
}

/// CDC 标准变更事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// 事件唯一 ID
    pub event_id: String,
    /// 事件类型
    pub event_type: ChangeEventType,
    /// 源数据库名
    pub source_db: String,
    /// 源表名
    pub source_table: String,
    /// 行数据（JSON）
    pub row_data: serde_json::Value,
    /// 变更位点
    pub position: ChangePosition,
    /// 时间戳（毫秒）
    pub timestamp_ms: u64,
    /// 是否已脱敏
    pub masked: bool,
}

impl ChangeEvent {
    /// 创建新事件
    pub fn new(
        event_type: ChangeEventType,
        source_db: &str,
        source_table: &str,
        row_data: serde_json::Value,
        position: ChangePosition,
        timestamp_ms: u64,
    ) -> Self {
        let event_id = format!(
            "{}:{}:{}:{}",
            source_db,
            source_table,
            timestamp_ms,
            position.order_key().1
        );
        Self {
            event_id,
            event_type,
            source_db: source_db.to_string(),
            source_table: source_table.to_string(),
            row_data,
            position,
            timestamp_ms,
            masked: false,
        }
    }

    /// 标记为已脱敏
    pub fn with_masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_creation_insert() {
        let event = ChangeEvent::new(
            ChangeEventType::Insert,
            "my_db",
            "users",
            serde_json::json!({"id": 1, "name": "Alice"}),
            ChangePosition::MysqlBinlog {
                filename: "bin.000001".to_string(),
                position: 100,
            },
            1700000000000,
        );
        assert_eq!(event.event_type, ChangeEventType::Insert);
        assert_eq!(event.source_db, "my_db");
        assert_eq!(event.source_table, "users");
        assert!(!event.masked);
        assert!(!event.event_id.is_empty());
    }

    #[test]
    fn event_type_serde_roundtrip() {
        let et = ChangeEventType::Update;
        let json = serde_json::to_string(&et).unwrap();
        let de: ChangeEventType = serde_json::from_str(&json).unwrap();
        assert_eq!(et, de);
    }

    #[test]
    fn position_order_key_monotonic() {
        let p1 = ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position: 100,
        };
        let p2 = ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position: 200,
        };
        assert!(p1.order_key().1 < p2.order_key().1);
    }

    #[test]
    fn event_id_uniqueness() {
        let pos = ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position: 100,
        };
        let e1 = ChangeEvent::new(
            ChangeEventType::Insert,
            "db",
            "tbl",
            serde_json::json!({}),
            pos.clone(),
            1000,
        );
        let e2 = ChangeEvent::new(
            ChangeEventType::Insert,
            "db",
            "tbl",
            serde_json::json!({}),
            ChangePosition::MysqlBinlog {
                filename: "bin.000001".to_string(),
                position: 200,
            },
            1000,
        );
        assert_ne!(e1.event_id, e2.event_id);
    }

    #[test]
    fn event_with_masked_flag() {
        let event = ChangeEvent::new(
            ChangeEventType::Update,
            "db",
            "users",
            serde_json::json!({"phone": "13800138000"}),
            ChangePosition::MysqlBinlog {
                filename: "bin.000001".to_string(),
                position: 100,
            },
            1000,
        )
        .with_masked(true);
        assert!(event.masked);
    }

    #[test]
    fn position_variants() {
        let mysql_pos = ChangePosition::MysqlBinlog {
            filename: "bin.001".to_string(),
            position: 42,
        };
        let pg_pos = ChangePosition::PostgresWal { lsn: 12345 };
        let sqlite_pos = ChangePosition::SqliteHook { seq: 99 };

        assert_eq!(mysql_pos.order_key(), ("bin.001".to_string(), 42));
        assert_eq!(pg_pos.order_key(), ("pg_wal".to_string(), 12345));
        assert_eq!(sqlite_pos.order_key(), ("sqlite".to_string(), 99));
    }
}
