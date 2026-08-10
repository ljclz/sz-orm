use serde_json::{json, Value};
use sz_orm_batch::*;

#[test]
fn test_batch_result_new() {
    let result = BatchResult::new();
    assert_eq!(result.inserted, 0);
    assert_eq!(result.updated, 0);
    assert_eq!(result.failed, 0);
    assert!(result.generated_sqls.is_empty());
}

#[test]
fn test_batch_result_default() {
    let result = BatchResult::default();
    assert_eq!(result.inserted, 0);
}

#[test]
fn test_default_batch_ops_new() {
    let ops = DefaultBatchOps::new();
    assert_eq!(ops.primary_key, "id");
    assert_eq!(ops.upsert_mode, UpsertMode::MysqlOnDuplicate);
    assert_eq!(ops.chunk_size, DEFAULT_CHUNK_SIZE);
}

#[test]
fn test_default_batch_ops_with_primary_key() {
    let ops = DefaultBatchOps::with_primary_key("user_id");
    assert_eq!(ops.primary_key, "user_id");
}

#[test]
fn test_default_batch_ops_with_upsert_mode() {
    let ops = DefaultBatchOps::new().with_upsert_mode(UpsertMode::PostgresOnConflict);
    assert_eq!(ops.upsert_mode, UpsertMode::PostgresOnConflict);
}

#[test]
fn test_default_batch_ops_with_chunk_size() {
    let ops = DefaultBatchOps::new().with_chunk_size(500);
    assert_eq!(ops.chunk_size, 500);
}

#[test]
fn test_default_batch_ops_with_chunk_size_zero() {
    let ops = DefaultBatchOps::new().with_chunk_size(0);
    assert_eq!(ops.chunk_size, 1);
}

#[test]
fn test_batch_insert_basic() {
    let ops = DefaultBatchOps::new();
    let rows = vec![
        json!({"id": 1, "name": "Alice"}),
        json!({"id": 2, "name": "Bob"}),
    ];
    let result = ops.batch_insert("users", rows);
    assert_eq!(result.inserted, 2);
    assert_eq!(result.failed, 0);
    assert_eq!(result.generated_sqls.len(), 1);
    assert!(result.generated_sqls[0].contains("INSERT INTO"));
    assert!(result.generated_sqls[0].contains("`users`"));
}

#[test]
fn test_batch_insert_empty() {
    let ops = DefaultBatchOps::new();
    let result = ops.batch_insert("users", vec![]);
    assert_eq!(result.inserted, 0);
    assert_eq!(result.failed, 0);
}

#[test]
fn test_batch_insert_non_object_rows() {
    let ops = DefaultBatchOps::new();
    let rows = vec![json!(42), json!("string")];
    let result = ops.batch_insert("users", rows);
    assert_eq!(result.failed, 2);
    assert_eq!(result.inserted, 0);
}

#[test]
fn test_batch_insert_with_chunking() {
    let ops = DefaultBatchOps::new().with_chunk_size(2);
    let rows: Vec<Value> = (0..5)
        .map(|i| json!({"id": i, "name": format!("user{}", i)}))
        .collect();
    let result = ops.batch_insert("users", rows);
    assert_eq!(result.inserted, 5);
    assert_eq!(result.generated_sqls.len(), 3);
}

#[test]
fn test_batch_update_basic() {
    let ops = DefaultBatchOps::new();
    let rows = vec![
        json!({"id": 1, "name": "Alice"}),
        json!({"id": 2, "name": "Bob"}),
    ];
    let result = ops.batch_update("users", rows);
    assert_eq!(result.updated, 2);
    assert!(result.generated_sqls[0].contains("UPDATE"));
    assert!(result.generated_sqls[0].contains("CASE WHEN"));
}

#[test]
fn test_batch_update_empty() {
    let ops = DefaultBatchOps::new();
    let result = ops.batch_update("users", vec![]);
    assert_eq!(result.updated, 0);
}

#[test]
fn test_batch_update_missing_pk() {
    let ops = DefaultBatchOps::new();
    let rows = vec![json!({"name": "Alice"})];
    let result = ops.batch_update("users", rows);
    assert_eq!(result.failed, 1);
    assert_eq!(result.updated, 0);
}

#[test]
fn test_batch_update_only_pk_no_columns() {
    let ops = DefaultBatchOps::new();
    let rows = vec![json!({"id": 1})];
    let result = ops.batch_update("users", rows);
    assert_eq!(result.failed, 1);
}

#[test]
fn test_batch_upsert_mysql() {
    let ops = DefaultBatchOps::new().with_upsert_mode(UpsertMode::MysqlOnDuplicate);
    let rows = vec![
        json!({"id": 1, "name": "Alice"}),
        json!({"id": 2, "name": "Bob"}),
    ];
    let result = ops.batch_upsert("users", rows);
    assert_eq!(result.inserted, 2);
    assert!(result.generated_sqls[0].contains("ON DUPLICATE KEY UPDATE"));
}

#[test]
fn test_batch_upsert_postgres() {
    let ops = DefaultBatchOps::new().with_upsert_mode(UpsertMode::PostgresOnConflict);
    let rows = vec![
        json!({"id": 1, "name": "Alice"}),
        json!({"id": 2, "name": "Bob"}),
    ];
    let result = ops.batch_upsert("users", rows);
    assert_eq!(result.inserted, 2);
    assert!(result.generated_sqls[0].contains("ON CONFLICT"));
    assert!(result.generated_sqls[0].contains("DO UPDATE SET"));
}

#[test]
fn test_batch_upsert_empty() {
    let ops = DefaultBatchOps::new();
    let result = ops.batch_upsert("users", vec![]);
    assert_eq!(result.inserted, 0);
}

#[test]
fn test_batch_upsert_missing_pk() {
    let ops = DefaultBatchOps::new();
    let rows = vec![json!({"name": "Alice"})];
    let result = ops.batch_upsert("users", rows);
    assert_eq!(result.failed, 1);
}

#[test]
fn test_batch_upsert_with_chunking() {
    let ops = DefaultBatchOps::new()
        .with_chunk_size(2)
        .with_upsert_mode(UpsertMode::PostgresOnConflict);
    let rows: Vec<Value> = (0..5)
        .map(|i| json!({"id": i, "name": format!("u{}", i)}))
        .collect();
    let result = ops.batch_upsert("users", rows);
    assert_eq!(result.inserted, 5);
    assert_eq!(result.generated_sqls.len(), 3);
}

#[test]
fn test_batch_progress_percent() {
    let progress = BatchProgress {
        chunk_index: 0,
        total_chunks: 4,
        chunk_rows: 100,
        processed_rows: 100,
        total_rows: 400,
        stage: BatchStage::ChunkCompleted,
    };
    assert_eq!(progress.percent(), 25.0);
}

#[test]
fn test_batch_progress_percent_zero_total() {
    let progress = BatchProgress {
        chunk_index: 0,
        total_chunks: 0,
        chunk_rows: 0,
        processed_rows: 0,
        total_rows: 0,
        stage: BatchStage::Finished,
    };
    assert_eq!(progress.percent(), 100.0);
}

#[test]
fn test_batch_progress_is_finished() {
    let progress = BatchProgress {
        chunk_index: 0,
        total_chunks: 1,
        chunk_rows: 10,
        processed_rows: 10,
        total_rows: 10,
        stage: BatchStage::Finished,
    };
    assert!(progress.is_finished());
}

#[test]
fn test_batch_progress_not_finished() {
    let progress = BatchProgress {
        chunk_index: 0,
        total_chunks: 2,
        chunk_rows: 10,
        processed_rows: 10,
        total_rows: 20,
        stage: BatchStage::ProcessingChunk,
    };
    assert!(!progress.is_finished());
}

#[test]
fn test_rollback_strategy_default() {
    let strategy = RollbackStrategy::default();
    assert_eq!(strategy, RollbackStrategy::None);
}

#[test]
fn test_upsert_mode_equality() {
    assert_eq!(UpsertMode::MysqlOnDuplicate, UpsertMode::MysqlOnDuplicate);
    assert_ne!(UpsertMode::MysqlOnDuplicate, UpsertMode::PostgresOnConflict);
}
