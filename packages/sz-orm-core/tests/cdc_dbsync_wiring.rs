use std::sync::{Arc, Mutex};
use sz_orm_core::cdc::dispatcher::CdcSink;
use sz_orm_core::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};
use sz_orm_core::cdc::sinks::db::{DbExecutor, DbSyncSink};

struct WiringExecutor {
    executed: Mutex<Vec<(String, Vec<serde_json::Value>)>>,
    fail: bool,
}

impl WiringExecutor {
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

    fn count(&self) -> usize {
        self.executed.lock().unwrap().len()
    }

    fn sqls(&self) -> Vec<String> {
        self.executed
            .lock()
            .unwrap()
            .iter()
            .map(|(s, _)| s.clone())
            .collect()
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

#[async_trait::async_trait]
impl DbExecutor for WiringExecutor {
    async fn execute(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<(), sz_orm_core::cdc::checkpoint::CdcError> {
        if self.fail {
            return Err(sz_orm_core::cdc::checkpoint::CdcError::SinkError(
                "executor failed".to_string(),
            ));
        }
        self.executed
            .lock()
            .unwrap()
            .push((sql.to_string(), params.to_vec()));
        Ok(())
    }
}

fn make_event(event_type: ChangeEventType, row_data: serde_json::Value) -> ChangeEvent {
    ChangeEvent::new(
        event_type,
        "source_db",
        "users",
        row_data,
        ChangePosition::MysqlBinlog {
            filename: "bin.001".to_string(),
            position: 100,
        },
        1000,
    )
}

#[tokio::test]
async fn dbsync_insert_executes_insert_sql() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone());
    sink.handle(&make_event(
        ChangeEventType::Insert,
        serde_json::json!({"id": 1, "name": "Alice"}),
    ))
    .await
    .unwrap();
    let sqls = executor.sqls();
    assert!(sqls[0].starts_with("INSERT INTO users"));
    assert!(sqls[0].contains("?"));
}

#[tokio::test]
async fn dbsync_update_executes_update_sql() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone());
    sink.handle(&make_event(
        ChangeEventType::Update,
        serde_json::json!({"id": 1, "name": "Bob"}),
    ))
    .await
    .unwrap();
    let sqls = executor.sqls();
    assert!(sqls[0].starts_with("UPDATE users SET"));
    assert!(sqls[0].contains("WHERE id = ?"));
}

#[tokio::test]
async fn dbsync_delete_executes_delete_sql() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone());
    sink.handle(&make_event(
        ChangeEventType::Delete,
        serde_json::json!({"id": 1}),
    ))
    .await
    .unwrap();
    let sqls = executor.sqls();
    assert!(sqls[0].starts_with("DELETE FROM users"));
    assert!(sqls[0].contains("WHERE id = ?"));
}

#[tokio::test]
async fn dbsync_table_mapping_redirects() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone()).with_table_mapping("users", "users_mirror");
    sink.handle(&make_event(
        ChangeEventType::Insert,
        serde_json::json!({"id": 1, "name": "Alice"}),
    ))
    .await
    .unwrap();
    let sqls = executor.sqls();
    assert!(sqls[0].contains("INSERT INTO users_mirror"));
}

#[tokio::test]
async fn dbsync_executor_error_propagates() {
    let executor = Arc::new(WiringExecutor::new().with_fail());
    let sink = DbSyncSink::new(executor.clone());
    let result = sink
        .handle(&make_event(
            ChangeEventType::Insert,
            serde_json::json!({"id": 1}),
        ))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn dbsync_insert_params_contain_values() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone());
    sink.handle(&make_event(
        ChangeEventType::Insert,
        serde_json::json!({"id": 42, "name": "Test"}),
    ))
    .await
    .unwrap();
    let params = executor.last_params();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], serde_json::json!(42));
    assert_eq!(params[1], serde_json::json!("Test"));
}

#[tokio::test]
async fn dbsync_multiple_events_all_executed() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone());
    sink.handle(&make_event(
        ChangeEventType::Insert,
        serde_json::json!({"id": 1, "name": "A"}),
    ))
    .await
    .unwrap();
    sink.handle(&make_event(
        ChangeEventType::Update,
        serde_json::json!({"id": 1, "name": "B"}),
    ))
    .await
    .unwrap();
    sink.handle(&make_event(
        ChangeEventType::Delete,
        serde_json::json!({"id": 1}),
    ))
    .await
    .unwrap();
    assert_eq!(executor.count(), 3);
}

#[tokio::test]
async fn dbsync_empty_row_data_skips() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone());
    sink.handle(&make_event(
        ChangeEventType::Insert,
        serde_json::Value::Null,
    ))
    .await
    .unwrap();
    assert_eq!(executor.count(), 0);
}

#[tokio::test]
async fn dbsync_delete_params_contain_only_id() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone());
    sink.handle(&make_event(
        ChangeEventType::Delete,
        serde_json::json!({"id": 99}),
    ))
    .await
    .unwrap();
    let params = executor.last_params();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], serde_json::json!(99));
}

#[tokio::test]
async fn dbsync_update_set_clause_excludes_id() {
    let executor = Arc::new(WiringExecutor::new());
    let sink = DbSyncSink::new(executor.clone());
    sink.handle(&make_event(
        ChangeEventType::Update,
        serde_json::json!({"id": 1, "name": "New", "email": "new@test.com"}),
    ))
    .await
    .unwrap();
    let sqls = executor.sqls();
    assert!(sqls[0].contains("name = ?"));
    assert!(sqls[0].contains("email = ?"));
    assert!(!sqls[0].contains("SET id = ?"));
}
