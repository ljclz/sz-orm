//! SQLite update-hook 变更捕获（v6.8.0 CDC-SQLITE-01）
//!
//! 通过 SQLite 触发器将行变更写入 `_sz_cdc_events` 表，
//! `SqliteHookCapturer` 轮询该表并转化为 `ChangeEvent` 分发到下游。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sqlx::sqlite::SqlitePool;
use tokio::sync::mpsc;

use super::dispatcher::CdcEventDispatcher;
use super::event::{ChangeEvent, ChangeEventType, ChangePosition};

/// SQLite CDC 配置
#[derive(Debug, Clone)]
pub struct SqliteCdcConfig {
    /// 轮询间隔
    pub poll_interval: Duration,
    /// 监听的表列表
    pub watched_tables: Vec<String>,
    /// 源数据库名
    pub source_db: String,
}

impl Default for SqliteCdcConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(100),
            watched_tables: vec![],
            source_db: "sqlite".to_string(),
        }
    }
}

/// SQLite 变更捕获器
pub struct SqliteHookCapturer {
    config: SqliteCdcConfig,
    seq: AtomicU64,
}

impl SqliteHookCapturer {
    /// 创建捕获器
    pub fn new(config: SqliteCdcConfig) -> Self {
        Self {
            config,
            seq: AtomicU64::new(0),
        }
    }

    /// 安装 CDC 触发器到指定表
    pub async fn install_hooks(&self, pool: &SqlitePool) -> Result<(), String> {
        sqlx::raw_sql(sqlx::AssertSqlSafe(
            "CREATE TABLE IF NOT EXISTS _sz_cdc_events (\
             seq INTEGER PRIMARY KEY AUTOINCREMENT,\
             op TEXT NOT NULL,\
             table_name TEXT NOT NULL,\
             row_id TEXT NOT NULL,\
             row_data TEXT NOT NULL,\
             consumed INTEGER DEFAULT 0)"
                .to_string(),
        ))
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        for table in &self.config.watched_tables {
            super::validate_identifier(table)
                .map_err(|e| format!("表名 '{}' 非法: {}", table, e))?;

            let drop_insert = format!("DROP TRIGGER IF EXISTS _sz_cdc_{}_insert", table);
            let drop_update = format!("DROP TRIGGER IF EXISTS _sz_cdc_{}_update", table);
            let drop_delete = format!("DROP TRIGGER IF EXISTS _sz_cdc_{}_delete", table);

            sqlx::raw_sql(sqlx::AssertSqlSafe(drop_insert))
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::raw_sql(sqlx::AssertSqlSafe(drop_update))
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::raw_sql(sqlx::AssertSqlSafe(drop_delete))
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;

            let insert_trigger = format!(
                "CREATE TRIGGER _sz_cdc_{t}_insert AFTER INSERT ON {t} \
                 BEGIN \
                   INSERT INTO _sz_cdc_events(op, table_name, row_id, row_data) \
                   VALUES('INSERT', '{t}', CAST(NEW.rowid AS TEXT), ''); \
                 END",
                t = table
            );
            let update_trigger = format!(
                "CREATE TRIGGER _sz_cdc_{t}_update AFTER UPDATE ON {t} \
                 BEGIN \
                   INSERT INTO _sz_cdc_events(op, table_name, row_id, row_data) \
                   VALUES('UPDATE', '{t}', CAST(NEW.rowid AS TEXT), ''); \
                 END",
                t = table
            );
            let delete_trigger = format!(
                "CREATE TRIGGER _sz_cdc_{t}_delete AFTER DELETE ON {t} \
                 BEGIN \
                   INSERT INTO _sz_cdc_events(op, table_name, row_id, row_data) \
                   VALUES('DELETE', '{t}', CAST(OLD.rowid AS TEXT), ''); \
                 END",
                t = table
            );

            sqlx::raw_sql(sqlx::AssertSqlSafe(insert_trigger))
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::raw_sql(sqlx::AssertSqlSafe(update_trigger))
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::raw_sql(sqlx::AssertSqlSafe(delete_trigger))
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 轮询未消费的 CDC 事件并分发
    pub async fn poll_and_dispatch(
        &self,
        pool: &SqlitePool,
        dispatcher: &CdcEventDispatcher,
    ) -> Result<usize, String> {
        let rows = sqlx::query_as::<_, (i64, String, String, String)>(
            "SELECT seq, op, table_name, row_id FROM _sz_cdc_events WHERE consumed = 0 ORDER BY seq",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        let mut count = 0;
        for (seq, op, table_name, row_id) in rows {
            let event_type = match op.as_str() {
                "INSERT" => ChangeEventType::Insert,
                "UPDATE" => ChangeEventType::Update,
                "DELETE" => ChangeEventType::Delete,
                _ => continue,
            };

            let row_data = serde_json::json!({ "rowid": row_id });
            let position = ChangePosition::SqliteHook { seq: seq as u64 };
            let event = ChangeEvent::new(
                event_type,
                &self.config.source_db,
                &table_name,
                row_data,
                position,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            );

            dispatcher
                .dispatch(event)
                .await
                .map_err(|e| e.to_string())?;

            sqlx::query("UPDATE _sz_cdc_events SET consumed = 1 WHERE seq = ?")
                .bind(seq)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;

            self.seq.store(seq as u64, Ordering::Relaxed);
            count += 1;
        }
        Ok(count)
    }

    /// 启动捕获器（持续轮询直到收到停止信号）
    pub async fn start(
        self: Arc<Self>,
        pool: SqlitePool,
        dispatcher: Arc<CdcEventDispatcher>,
        mut stop_rx: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<usize, String> {
        let mut total = 0;
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                _ = tokio::time::sleep(self.config.poll_interval) => {
                    let n = self.poll_and_dispatch(&pool, &dispatcher).await?;
                    total += n;
                }
            }
        }
        Ok(total)
    }

    /// 返回最后处理的序号
    pub fn last_seq(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }

    /// 将变更事件发送到 channel（用于测试验证）
    pub async fn poll_and_send(
        &self,
        pool: &SqlitePool,
        sender: &mpsc::Sender<ChangeEvent>,
    ) -> Result<usize, String> {
        let rows = sqlx::query_as::<_, (i64, String, String, String)>(
            "SELECT seq, op, table_name, row_id FROM _sz_cdc_events WHERE consumed = 0 ORDER BY seq",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        let mut count = 0;
        for (seq, op, table_name, row_id) in rows {
            let event_type = match op.as_str() {
                "INSERT" => ChangeEventType::Insert,
                "UPDATE" => ChangeEventType::Update,
                "DELETE" => ChangeEventType::Delete,
                _ => continue,
            };

            let row_data = serde_json::json!({ "rowid": row_id });
            let position = ChangePosition::SqliteHook { seq: seq as u64 };
            let event = ChangeEvent::new(
                event_type,
                &self.config.source_db,
                &table_name,
                row_data,
                position,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            );

            sender.send(event).await.map_err(|e| e.to_string())?;

            sqlx::query("UPDATE _sz_cdc_events SET consumed = 1 WHERE seq = ?")
                .bind(seq)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;

            self.seq.store(seq as u64, Ordering::Relaxed);
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(
            "CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT)".to_string(),
        ))
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn capturer_creation() {
        let config = SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string()],
            ..Default::default()
        };
        let capturer = SqliteHookCapturer::new(config);
        assert_eq!(capturer.last_seq(), 0);
    }

    #[tokio::test]
    async fn install_hooks_creates_cdc_table() {
        let pool = setup_pool().await;
        let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string()],
            ..Default::default()
        });
        capturer.install_hooks(&pool).await.unwrap();

        let exists: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE name = '_sz_cdc_events'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(exists.0, 1);
    }

    #[tokio::test]
    async fn insert_trigger_captures_event() {
        let pool = setup_pool().await;
        let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string()],
            ..Default::default()
        });
        capturer.install_hooks(&pool).await.unwrap();

        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'alice')")
            .execute(&pool)
            .await
            .unwrap();

        let (op, table): (String, String) =
            sqlx::query_as("SELECT op, table_name FROM _sz_cdc_events WHERE consumed = 0")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(op, "INSERT");
        assert_eq!(table, "test_table");
    }

    #[tokio::test]
    async fn update_trigger_captures_event() {
        let pool = setup_pool().await;
        let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string()],
            ..Default::default()
        });
        capturer.install_hooks(&pool).await.unwrap();

        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'alice')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE test_table SET name = 'bob' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();

        let ops: Vec<String> = sqlx::query_as("SELECT op FROM _sz_cdc_events ORDER BY seq")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|(op,)| op)
            .collect();
        assert!(ops.contains(&"INSERT".to_string()));
        assert!(ops.contains(&"UPDATE".to_string()));
    }

    #[tokio::test]
    async fn delete_trigger_captures_event() {
        let pool = setup_pool().await;
        let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string()],
            ..Default::default()
        });
        capturer.install_hooks(&pool).await.unwrap();

        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'alice')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM test_table WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();

        let ops: Vec<String> = sqlx::query_as("SELECT op FROM _sz_cdc_events ORDER BY seq")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|(op,)| op)
            .collect();
        assert!(ops.contains(&"DELETE".to_string()));
    }

    #[tokio::test]
    async fn poll_and_send_delivers_events() {
        let pool = setup_pool().await;
        let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string()],
            ..Default::default()
        });
        capturer.install_hooks(&pool).await.unwrap();

        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'alice')")
            .execute(&pool)
            .await
            .unwrap();

        let (tx, mut rx) = mpsc::channel(100);
        let count = capturer.poll_and_send(&pool, &tx).await.unwrap();
        assert_eq!(count, 1);

        let event = rx.recv().await.unwrap();
        assert_eq!(event.event_type, ChangeEventType::Insert);
        assert_eq!(event.source_table, "test_table");
        assert!(capturer.last_seq() > 0);
    }

    #[tokio::test]
    async fn poll_marks_events_consumed() {
        let pool = setup_pool().await;
        let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string()],
            ..Default::default()
        });
        capturer.install_hooks(&pool).await.unwrap();

        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'a')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO test_table (id, name) VALUES (2, 'b')")
            .execute(&pool)
            .await
            .unwrap();

        let (tx, _rx) = mpsc::channel(100);
        capturer.poll_and_send(&pool, &tx).await.unwrap();

        let remaining: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM _sz_cdc_events WHERE consumed = 0")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining.0, 0);
    }

    #[tokio::test]
    async fn multiple_tables_captured() {
        let pool = setup_pool().await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(
            "CREATE TABLE other_table (id INTEGER PRIMARY KEY, value TEXT)".to_string(),
        ))
        .execute(&pool)
        .await
        .unwrap();

        let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string(), "other_table".to_string()],
            ..Default::default()
        });
        capturer.install_hooks(&pool).await.unwrap();

        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'a')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO other_table (id, value) VALUES (1, 'x')")
            .execute(&pool)
            .await
            .unwrap();

        let tables: Vec<String> =
            sqlx::query_as("SELECT table_name FROM _sz_cdc_events ORDER BY seq")
                .fetch_all(&pool)
                .await
                .unwrap()
                .into_iter()
                .map(|(t,)| t)
                .collect();
        assert!(tables.contains(&"test_table".to_string()));
        assert!(tables.contains(&"other_table".to_string()));
    }

    fn t23_make_capturer(tables: &[&str]) -> SqliteHookCapturer {
        SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: tables.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
    }

    #[test]
    fn t23_config_default_values() {
        let config = SqliteCdcConfig::default();
        assert_eq!(config.poll_interval, Duration::from_millis(100));
        assert!(config.watched_tables.is_empty());
        assert_eq!(config.source_db, "sqlite");
    }

    #[tokio::test]
    async fn t23_capturer_new_initializes_seq_zero() {
        let capturer = t23_make_capturer(&["test_table"]);
        assert_eq!(capturer.last_seq(), 0);
    }

    #[tokio::test]
    async fn t23_install_hooks_empty_watched_tables_creates_events_table() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&[]);
        capturer.install_hooks(&pool).await.unwrap();
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE name = '_sz_cdc_events'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn t23_install_hooks_rejects_invalid_table_name() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["bad table!"]);
        let result = capturer.install_hooks(&pool).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("非法"));
    }

    #[tokio::test]
    async fn t23_install_hooks_rejects_injection_table_name() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["users; DROP TABLE test_table; --"]);
        let result = capturer.install_hooks(&pool).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn t23_install_hooks_idempotent() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["test_table"]);
        capturer.install_hooks(&pool).await.unwrap();
        capturer.install_hooks(&pool).await.unwrap();
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND tbl_name = 'test_table'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 3);
    }

    #[tokio::test]
    async fn t23_poll_and_dispatch_no_events_returns_zero() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["test_table"]);
        capturer.install_hooks(&pool).await.unwrap();
        let dispatcher = CdcEventDispatcher::new(vec![], 100);
        let count = capturer
            .poll_and_dispatch(&pool, &dispatcher)
            .await
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(capturer.last_seq(), 0);
    }

    #[tokio::test]
    async fn t23_poll_and_dispatch_delivers_insert_event() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["test_table"]);
        capturer.install_hooks(&pool).await.unwrap();
        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'alice')")
            .execute(&pool)
            .await
            .unwrap();
        let sink = Arc::new(super::super::dispatcher::MemorySink::new());
        let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);
        let count = capturer
            .poll_and_dispatch(&pool, &dispatcher)
            .await
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(sink.count(), 1);
        let events = sink.events();
        assert_eq!(events[0].event_type, ChangeEventType::Insert);
        assert_eq!(events[0].source_table, "test_table");
        assert_eq!(events[0].source_db, "sqlite");
        assert!(capturer.last_seq() > 0);
    }

    #[tokio::test]
    async fn t23_poll_and_dispatch_delivers_update_and_delete() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["test_table"]);
        capturer.install_hooks(&pool).await.unwrap();
        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'a')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE test_table SET name = 'b' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM test_table WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let sink = Arc::new(super::super::dispatcher::MemorySink::new());
        let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);
        let count = capturer
            .poll_and_dispatch(&pool, &dispatcher)
            .await
            .unwrap();
        assert_eq!(count, 3);
        let events = sink.events();
        assert_eq!(events[0].event_type, ChangeEventType::Insert);
        assert_eq!(events[1].event_type, ChangeEventType::Update);
        assert_eq!(events[2].event_type, ChangeEventType::Delete);
    }

    #[tokio::test]
    async fn t23_poll_and_dispatch_skips_unknown_op() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["test_table"]);
        capturer.install_hooks(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO _sz_cdc_events(op, table_name, row_id, row_data) \
             VALUES ('UNKNOWN', 'test_table', '1', '')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let dispatcher = CdcEventDispatcher::new(vec![], 100);
        let count = capturer
            .poll_and_dispatch(&pool, &dispatcher)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn t23_poll_and_dispatch_custom_source_db() {
        let pool = setup_pool().await;
        let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
            watched_tables: vec!["test_table".to_string()],
            source_db: "my_custom_db".to_string(),
            ..Default::default()
        });
        capturer.install_hooks(&pool).await.unwrap();
        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'x')")
            .execute(&pool)
            .await
            .unwrap();
        let sink = Arc::new(super::super::dispatcher::MemorySink::new());
        let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);
        capturer
            .poll_and_dispatch(&pool, &dispatcher)
            .await
            .unwrap();
        let events = sink.events();
        assert_eq!(events[0].source_db, "my_custom_db");
    }

    #[tokio::test]
    async fn t23_poll_and_send_no_events_returns_zero() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["test_table"]);
        capturer.install_hooks(&pool).await.unwrap();
        let (tx, _rx) = mpsc::channel(100);
        let count = capturer.poll_and_send(&pool, &tx).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn t23_poll_and_send_multiple_events_in_order() {
        let pool = setup_pool().await;
        let capturer = t23_make_capturer(&["test_table"]);
        capturer.install_hooks(&pool).await.unwrap();
        for idx in 1i64..=3 {
            sqlx::query("INSERT INTO test_table (id, name) VALUES (?, ?)")
                .bind(idx)
                .bind(format!("n{}", idx))
                .execute(&pool)
                .await
                .unwrap();
        }
        let (tx, mut rx) = mpsc::channel(100);
        let count = capturer.poll_and_send(&pool, &tx).await.unwrap();
        assert_eq!(count, 3);
        drop(tx);
        let mut seqs = Vec::new();
        while let Some(event) = rx.recv().await {
            if let ChangePosition::SqliteHook { seq } = event.position {
                seqs.push(seq);
            }
        }
        assert_eq!(seqs.len(), 3);
        assert!(seqs.windows(2).all(|w| w[0] < w[1]));
        assert!(capturer.last_seq() > 0);
    }

    #[tokio::test]
    async fn t23_start_stops_immediately_on_signal() {
        let pool = setup_pool().await;
        let capturer = Arc::new(t23_make_capturer(&["test_table"]));
        capturer.install_hooks(&pool).await.unwrap();
        let dispatcher = Arc::new(CdcEventDispatcher::new(vec![], 100));
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        stop_tx.send(()).unwrap();
        let total = capturer.start(pool, dispatcher, stop_rx).await.unwrap();
        assert_eq!(total, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn t23_start_processes_events_then_stops() {
        let pool = setup_pool().await;
        let capturer = Arc::new(SqliteHookCapturer::new(SqliteCdcConfig {
            poll_interval: Duration::from_millis(10),
            watched_tables: vec!["test_table".to_string()],
            ..Default::default()
        }));
        capturer.install_hooks(&pool).await.unwrap();
        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'a')")
            .execute(&pool)
            .await
            .unwrap();
        let sink = Arc::new(super::super::dispatcher::MemorySink::new());
        let dispatcher = Arc::new(CdcEventDispatcher::new(vec![sink.clone()], 100));
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let capturer_clone = capturer.clone();
        let pool_clone = pool.clone();
        let handle =
            tokio::spawn(
                async move { capturer_clone.start(pool_clone, dispatcher, stop_rx).await },
            );
        tokio::time::sleep(Duration::from_millis(200)).await;
        stop_tx.send(()).unwrap();
        let total = handle.await.unwrap().unwrap();
        assert!(total >= 1);
        assert_eq!(sink.count(), 1);
    }
}
