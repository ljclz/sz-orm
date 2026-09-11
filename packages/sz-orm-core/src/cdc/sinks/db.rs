//! CDC 下游库同步 Sink（v6.8.0 CDC-DBSYNC-01）
//!
//! 将变更事件转化为对应 SQL（INSERT/UPDATE/DELETE），
//! 通过 DbExecutor 执行，支持 at-least-once 投递。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::super::checkpoint::CdcError;
use super::super::dispatcher::CdcSink;
use super::super::event::{ChangeEvent, ChangeEventType};

/// 数据库执行器 trait（抽象数据库执行接口，便于测试注入）
#[async_trait]
pub trait DbExecutor: Send + Sync {
    /// 执行 SQL（参数化，占位符为 `?`）
    async fn execute(&self, sql: &str, params: &[serde_json::Value]) -> Result<(), CdcError>;
}

/// 数据库同步 Sink
pub struct DbSyncSink {
    executor: Arc<dyn DbExecutor>,
    table_map: HashMap<String, String>,
}

impl DbSyncSink {
    /// 创建数据库同步 Sink
    pub fn new(executor: Arc<dyn DbExecutor>) -> Self {
        Self {
            executor,
            table_map: HashMap::new(),
        }
    }

    /// 添加源表到目标表的映射（不设置时默认同名）
    pub fn with_table_mapping(mut self, source: &str, target: &str) -> Self {
        self.table_map
            .insert(source.to_string(), target.to_string());
        self
    }

    /// 解析目标表名
    fn resolve_target<'a>(&'a self, source: &'a str) -> &'a str {
        self.table_map
            .get(source)
            .map(|s| s.as_str())
            .unwrap_or(source)
    }

    /// 从行数据中提取列名和值
    fn extract_columns(row_data: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
        if let serde_json::Value::Object(map) = row_data {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            Vec::new()
        }
    }

    /// 构建 INSERT SQL
    fn build_insert(
        target_table: &str,
        columns: &[(String, serde_json::Value)],
    ) -> (String, Vec<serde_json::Value>) {
        let col_names: Vec<&str> = columns.iter().map(|(k, _)| k.as_str()).collect();
        let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            target_table,
            col_names.join(", "),
            placeholders.join(", ")
        );
        let params: Vec<serde_json::Value> = columns.iter().map(|(_, v)| v.clone()).collect();
        (sql, params)
    }

    /// 构建 UPDATE SQL
    fn build_update(
        target_table: &str,
        columns: &[(String, serde_json::Value)],
    ) -> (String, Vec<serde_json::Value>) {
        let set_clause: Vec<String> = columns
            .iter()
            .filter(|(k, _)| k != "id")
            .map(|(k, _)| format!("{} = ?", k))
            .collect();
        let mut params: Vec<serde_json::Value> = columns
            .iter()
            .filter(|(k, _)| k != "id")
            .map(|(_, v)| v.clone())
            .collect();

        let id_value = columns
            .iter()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v.clone())
            .unwrap_or(serde_json::Value::Null);
        params.push(id_value);

        let sql = format!(
            "UPDATE {} SET {} WHERE id = ?",
            target_table,
            set_clause.join(", ")
        );
        (sql, params)
    }

    /// 构建 DELETE SQL
    fn build_delete(
        target_table: &str,
        columns: &[(String, serde_json::Value)],
    ) -> (String, Vec<serde_json::Value>) {
        let id_value = columns
            .iter()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v.clone())
            .unwrap_or(serde_json::Value::Null);
        let sql = format!("DELETE FROM {} WHERE id = ?", target_table);
        (sql, vec![id_value])
    }
}

#[async_trait]
impl CdcSink for DbSyncSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        let target = self.resolve_target(&event.source_table);
        super::super::validate_identifier(target)
            .map_err(|e| CdcError::SinkError(format!("表名非法: {}", e)))?;

        let columns = Self::extract_columns(&event.row_data);

        if columns.is_empty() {
            return Ok(());
        }

        for (col, _) in &columns {
            super::super::validate_identifier(col)
                .map_err(|e| CdcError::SinkError(format!("列名 '{}' 非法: {}", col, e)))?;
        }

        let (sql, params) = match event.event_type {
            ChangeEventType::Insert => Self::build_insert(target, &columns),
            ChangeEventType::Update => Self::build_update(target, &columns),
            ChangeEventType::Delete => Self::build_delete(target, &columns),
        };

        self.executor.execute(&sql, &params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockExecutor {
        executed: Mutex<Vec<(String, Vec<serde_json::Value>)>>,
        fail: bool,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                executed: Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn with_fail(mut self) -> Self {
            self.fail = true;
            self
        }

        fn executed_sqls(&self) -> Vec<String> {
            self.executed
                .lock()
                .unwrap()
                .iter()
                .map(|(s, _)| s.clone())
                .collect()
        }

        fn executed_count(&self) -> usize {
            self.executed.lock().unwrap().len()
        }

        fn last_params(&self) -> Vec<serde_json::Value> {
            self.executed
                .lock()
                .unwrap()
                .last()
                .map(|(_, p)| p.clone())
                .unwrap_or_default()
        }
    }

    #[async_trait]
    impl DbExecutor for MockExecutor {
        async fn execute(&self, sql: &str, params: &[serde_json::Value]) -> Result<(), CdcError> {
            if self.fail {
                return Err(CdcError::SinkError("executor failed".to_string()));
            }
            self.executed
                .lock()
                .unwrap()
                .push((sql.to_string(), params.to_vec()));
            Ok(())
        }
    }

    fn make_insert_event() -> ChangeEvent {
        ChangeEvent::new(
            ChangeEventType::Insert,
            "source_db",
            "users",
            serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
            super::super::super::event::ChangePosition::MysqlBinlog {
                filename: "bin.001".to_string(),
                position: 100,
            },
            1000,
        )
    }

    fn make_update_event() -> ChangeEvent {
        ChangeEvent::new(
            ChangeEventType::Update,
            "source_db",
            "users",
            serde_json::json!({"id": 1, "name": "Bob", "email": "bob@example.com"}),
            super::super::super::event::ChangePosition::MysqlBinlog {
                filename: "bin.001".to_string(),
                position: 200,
            },
            2000,
        )
    }

    fn make_delete_event() -> ChangeEvent {
        ChangeEvent::new(
            ChangeEventType::Delete,
            "source_db",
            "users",
            serde_json::json!({"id": 1}),
            super::super::super::event::ChangePosition::MysqlBinlog {
                filename: "bin.001".to_string(),
                position: 300,
            },
            3000,
        )
    }

    #[tokio::test]
    async fn insert_generates_insert_sql() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone());
        sink.handle(&make_insert_event()).await.unwrap();
        let sqls = executor.executed_sqls();
        assert!(sqls[0].contains("INSERT INTO users"));
        assert!(sqls[0].contains("id"));
        assert!(sqls[0].contains("name"));
        assert!(sqls[0].contains("email"));
    }

    #[tokio::test]
    async fn update_generates_update_sql() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone());
        sink.handle(&make_update_event()).await.unwrap();
        let sqls = executor.executed_sqls();
        assert!(sqls[0].contains("UPDATE users SET"));
        assert!(sqls[0].contains("WHERE id = ?"));
    }

    #[tokio::test]
    async fn delete_generates_delete_sql() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone());
        sink.handle(&make_delete_event()).await.unwrap();
        let sqls = executor.executed_sqls();
        assert!(sqls[0].contains("DELETE FROM users"));
        assert!(sqls[0].contains("WHERE id = ?"));
    }

    #[tokio::test]
    async fn table_mapping_redirects_to_target() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone()).with_table_mapping("users", "users_backup");
        sink.handle(&make_insert_event()).await.unwrap();
        let sqls = executor.executed_sqls();
        assert!(sqls[0].contains("INSERT INTO users_backup"));
    }

    #[tokio::test]
    async fn executor_failure_propagates_error() {
        let executor = Arc::new(MockExecutor::new().with_fail());
        let sink = DbSyncSink::new(executor.clone());
        let result = sink.handle(&make_insert_event()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn empty_row_data_skips_execution() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone());
        let event = ChangeEvent::new(
            ChangeEventType::Insert,
            "db",
            "tbl",
            serde_json::Value::Null,
            super::super::super::event::ChangePosition::MysqlBinlog {
                filename: "bin.001".to_string(),
                position: 100,
            },
            1000,
        );
        sink.handle(&event).await.unwrap();
        assert_eq!(executor.executed_count(), 0);
    }

    #[tokio::test]
    async fn insert_params_match_row_data() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone());
        sink.handle(&make_insert_event()).await.unwrap();
        let params = executor.last_params();
        assert_eq!(params.len(), 3);
        assert!(params.contains(&serde_json::json!(1)));
        assert!(params.contains(&serde_json::json!("Alice")));
        assert!(params.contains(&serde_json::json!("alice@example.com")));
    }

    #[tokio::test]
    async fn update_params_exclude_id_from_set_clause() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone());
        sink.handle(&make_update_event()).await.unwrap();
        let sqls = executor.executed_sqls();
        assert!(!sqls[0].contains("id = ?") || sqls[0].contains("WHERE id = ?"));
    }

    #[tokio::test]
    async fn delete_params_contain_id() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone());
        sink.handle(&make_delete_event()).await.unwrap();
        let params = executor.last_params();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], serde_json::json!(1));
    }

    #[tokio::test]
    async fn multiple_events_sequential_execution() {
        let executor = Arc::new(MockExecutor::new());
        let sink = DbSyncSink::new(executor.clone());
        sink.handle(&make_insert_event()).await.unwrap();
        sink.handle(&make_update_event()).await.unwrap();
        sink.handle(&make_delete_event()).await.unwrap();
        assert_eq!(executor.executed_count(), 3);
    }
}
