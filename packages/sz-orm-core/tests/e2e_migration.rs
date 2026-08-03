//! P1-4：Migration `__migrations` 持久化表 L3 行为测试
//!
//! 验证目标：
//! - `migrate`/`up` 自动创建 `__migrations` 表并写入执行记录
//! - `rollback`/`down` 删除对应执行记录
//! - 新 `Migrator` 实例能从 `__migrations` 表恢复已执行状态（重启场景）
//! - 跨方言 DDL 生成正确（MySQL/PG/SQLite/Oracle/SQL Server）
//! - 安全校验：非法版本号/名称拒绝写入

#![cfg(test)]

#[path = "common/mod.rs"]
mod common;

use common::{InMemoryDb, TransactionalConnection};
use std::sync::Arc;
use sz_orm_core::migration::Migration;
use sz_orm_core::{DbType, MigrationContext, Migrator, Value};
use tokio::sync::Mutex;

// ===== 辅助函数 =====

/// 创建一个带连接的 Migrator，返回 (migrator, 共享 db 句柄)
fn make_migrator(db_type: DbType) -> (Migrator, Arc<Mutex<InMemoryDb>>) {
    let db = Arc::new(Mutex::new(InMemoryDb::new()));
    let conn = TransactionalConnection::new(db.clone());
    let context = MigrationContext {
        table_name: "__migrations".to_string(),
        connection: Some(Box::new(conn)),
        db_type: Some(db_type),
    };
    (Migrator::new(context), db)
}

/// 构造一组测试迁移
fn test_migrations() -> Vec<Migration> {
    vec![
        Migration::new(
            "001",
            "create_users",
            "INSERT INTO users (id, name) VALUES (1, 'Alice')",
            "DELETE FROM users WHERE id = 1",
        ),
        Migration::new(
            "002",
            "add_age_col",
            "INSERT INTO users (id, name) VALUES (2, 'Bob')",
            "DELETE FROM users WHERE id = 2",
        ),
        Migration::new(
            "003",
            "add_index",
            "INSERT INTO users (id, name) VALUES (3, 'Carol')",
            "DELETE FROM users WHERE id = 3",
        ),
    ]
}

/// 从共享 db 句柄查询 __migrations 表的所有记录
async fn query_migrations_table(
    db: &Arc<Mutex<InMemoryDb>>,
) -> Vec<std::collections::HashMap<String, Value>> {
    let db = db.lock().await;
    db.select_all("__migrations").to_vec()
}

/// 从 __migrations 表记录中提取 version 字段列表
fn extract_versions(rows: &[std::collections::HashMap<String, Value>]) -> Vec<String> {
    rows.iter()
        .filter_map(|r| match r.get("version") {
            Some(Value::String(v)) => Some(v.clone()),
            _ => None,
        })
        .collect()
}

// ===== L3-1：migrate 自动创建 __migrations 表 =====

#[tokio::test]
async fn test_l3_1_migrate_creates_migrations_table() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    let applied = migrator.migrate().await.unwrap();
    assert_eq!(applied.len(), 3, "应执行 3 个迁移");

    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 3, "__migrations 表应有 3 条记录");
}

// ===== L3-2：migrate 后 __migrations 表包含所有已执行迁移 =====

#[tokio::test]
async fn test_l3_2_migrate_records_all_applied_migrations() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    migrator.migrate().await.unwrap();

    let rows = query_migrations_table(&db).await;
    let versions = extract_versions(&rows);
    assert!(versions.contains(&"001".to_string()), "应包含 001");
    assert!(versions.contains(&"002".to_string()), "应包含 002");
    assert!(versions.contains(&"003".to_string()), "应包含 003");
}

// ===== L3-3：rollback 删除 __migrations 表对应记录 =====

#[tokio::test]
async fn test_l3_3_rollback_removes_migration_record() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    migrator.migrate().await.unwrap();
    migrator.rollback("003").await.unwrap();

    let rows = query_migrations_table(&db).await;
    let versions = extract_versions(&rows);
    assert!(
        !versions.contains(&"003".to_string()),
        "003 应已从 __migrations 删除"
    );
    assert!(versions.contains(&"001".to_string()), "001 应保留");
    assert!(versions.contains(&"002".to_string()), "002 应保留");
}

// ===== L3-4：down 批量回滚删除多条记录 =====

#[tokio::test]
async fn test_l3_4_down_removes_multiple_records() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    migrator.migrate().await.unwrap();
    let rolled = migrator.down(Some("001")).await.unwrap();
    assert_eq!(rolled.len(), 2, "应回滚 002 和 003");

    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 1, "应只剩 001 的记录");
}

// ===== L3-5：重启场景 — 新 Migrator 从 __migrations 表恢复状态 =====

#[tokio::test]
async fn test_l3_5_restart_recovers_state_from_migrations_table() {
    let db = Arc::new(Mutex::new(InMemoryDb::new()));
    let conn = TransactionalConnection::new(db.clone());
    let context = MigrationContext {
        table_name: "__migrations".to_string(),
        connection: Some(Box::new(conn)),
        db_type: Some(DbType::Sqlite),
    };
    let mut migrator1 = Migrator::new(context).add_migrations(test_migrations());
    migrator1.migrate().await.unwrap();

    // 模拟重启：复用同一个 db（包含 __migrations 表记录），新建 Migrator
    let conn2 = TransactionalConnection::new(db.clone());
    let context2 = MigrationContext {
        table_name: "__migrations".to_string(),
        connection: Some(Box::new(conn2)),
        db_type: Some(DbType::Sqlite),
    };
    let mut migrator2 = Migrator::new(context2).add_migrations(test_migrations());

    // 触发状态同步（通过 migrate 内部自动调用）
    let re_applied = migrator2.migrate().await.unwrap();
    assert_eq!(re_applied.len(), 0, "已执行的迁移不应重复执行");

    // 验证：所有迁移应已标记为已执行（batch > 0）
    let applied = migrator2.get_applied_migrations();
    assert_eq!(applied.len(), 3, "重启后应识别 3 个已执行迁移");

    // __migrations 表记录数应仍为 3（未重复写入）
    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 3, "记录数不应翻倍");
}

// ===== L3-6：up 到指定版本只记录到该版本 =====

#[tokio::test]
async fn test_l3_6_up_to_target_records_correctly() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    migrator.up(Some("002")).await.unwrap();

    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 2, "应只有 001 和 002 的记录");
}

// ===== L3-7：跨方言 DDL 生成（MySQL/PG/SQLite/Oracle/SQL Server） =====

#[tokio::test]
async fn test_l3_7_create_migrations_table_sql_per_dialect() {
    let check = |db_type: DbType, expected: &str| {
        let context = MigrationContext {
            table_name: "__migrations".to_string(),
            connection: None,
            db_type: Some(db_type),
        };
        let migrator = Migrator::new(context);
        let sql = migrator.build_create_migrations_table_sql();
        assert!(
            sql.contains(expected),
            "方言 {:?} 应包含 {}: {}",
            db_type,
            expected,
            sql
        );
    };

    check(
        DbType::MySQL,
        "TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP",
    );
    check(
        DbType::PostgreSQL,
        "TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP",
    );
    check(
        DbType::Sqlite,
        "TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP",
    );
    check(DbType::Oracle, "TIMESTAMP DEFAULT CURRENT_TIMESTAMP");
    check(DbType::SqlServer, "DATETIME DEFAULT GETDATE()");
}

// ===== L3-8：CREATE TABLE IF NOT EXISTS 跨方言一致 =====

#[tokio::test]
async fn test_l3_8_create_table_uses_if_not_exists() {
    let context = MigrationContext {
        table_name: "__migrations".to_string(),
        connection: None,
        db_type: Some(DbType::MySQL),
    };
    let migrator = Migrator::new(context);
    let sql = migrator.build_create_migrations_table_sql();
    assert!(
        sql.to_uppercase().contains("CREATE TABLE IF NOT EXISTS"),
        "应使用 IF NOT EXISTS: {}",
        sql
    );
}

// ===== L3-9：__migrations 表结构包含必需字段 =====

#[tokio::test]
async fn test_l3_9_migrations_table_has_required_columns() {
    let context = MigrationContext {
        table_name: "__migrations".to_string(),
        connection: None,
        db_type: Some(DbType::MySQL),
    };
    let migrator = Migrator::new(context);
    let sql = migrator.build_create_migrations_table_sql().to_uppercase();
    assert!(sql.contains("VERSION"), "应有 version 字段");
    assert!(sql.contains("NAME"), "应有 name 字段");
    assert!(sql.contains("BATCH"), "应有 batch 字段");
    assert!(sql.contains("EXECUTED_AT"), "应有 executed_at 字段");
    assert!(sql.contains("PRIMARY KEY"), "version 应为主键");
}

// ===== L3-10：版本冲突检测在 migrate 前执行 =====

#[tokio::test]
async fn test_l3_10_version_conflict_rejected_before_migrate() {
    let (mut migrator, _db) = make_migrator(DbType::Sqlite);
    let dup_migrations = vec![
        Migration::new("001", "first", "INSERT INTO t VALUES (1)", "DELETE FROM t"),
        Migration::new(
            "001",
            "duplicate",
            "INSERT INTO t VALUES (2)",
            "DELETE FROM t",
        ),
    ];
    migrator = migrator.add_migrations(dup_migrations);

    let result = migrator.migrate().await;
    assert!(result.is_err(), "版本冲突应导致 migrate 失败");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("001"), "错误信息应包含冲突版本号 001");
}

// ===== L3-11：安全校验 — 非法版本号拒绝写入 =====

#[tokio::test]
async fn test_l3_11_illegal_version_rejected() {
    let (mut migrator, _db) = make_migrator(DbType::Sqlite);
    let bad_migrations = vec![Migration::new(
        "001'; DROP TABLE __migrations; --",
        "evil",
        "INSERT INTO t VALUES (1)",
        "DELETE FROM t",
    )];
    migrator = migrator.add_migrations(bad_migrations);

    let result = migrator.migrate().await;
    assert!(result.is_err(), "非法版本号应导致 migrate 失败");
}

// ===== L3-12：安全校验 — 非法迁移名称拒绝写入 =====

#[tokio::test]
async fn test_l3_12_illegal_name_rejected() {
    let (mut migrator, _db) = make_migrator(DbType::Sqlite);
    let bad_migrations = vec![Migration::new(
        "001",
        "evil'; DROP TABLE users; --",
        "INSERT INTO t VALUES (1)",
        "DELETE FROM t",
    )];
    migrator = migrator.add_migrations(bad_migrations);

    let result = migrator.migrate().await;
    assert!(result.is_err(), "非法名称应导致 migrate 失败");
}

// ===== L3-13：migrate 幂等性 — 重复调用不重复执行 =====

#[tokio::test]
async fn test_l3_13_migrate_idempotent() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    let first = migrator.migrate().await.unwrap();
    assert_eq!(first.len(), 3);

    let second = migrator.migrate().await.unwrap();
    assert_eq!(second.len(), 0, "第二次 migrate 不应重复执行");

    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 3, "记录数不应翻倍");
}

// ===== L3-14：reset = down + migrate，__migrations 表状态正确 =====

#[tokio::test]
async fn test_l3_14_reset_preserves_table_state() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    migrator.migrate().await.unwrap();

    // 在 reset 前篡改 users 表数据：把 id=1 的 name 改为 "Modified"。
    // 若 reset 真的执行了 down + migrate，down 会 DELETE id=1（删除篡改行），
    // migrate 会重新 INSERT id=1 name='Alice'（恢复初始状态）。
    {
        let mut d = db.lock().await;
        d.update_where(
            "users",
            "id",
            &Value::I64(1),
            "name",
            Value::String("Modified".to_string()),
        );
    }

    migrator.reset().await.unwrap();

    // 1) __migrations 表记录数：down 清空 + migrate 重写 → 3 条
    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 3, "reset 后 __migrations 应有 3 条记录");

    // 2) users 表行数：down 删除 1/2/3，migrate 重新插入 1/2/3 → 3 行
    let users_count = {
        let d = db.lock().await;
        d.count("users")
    };
    assert_eq!(users_count, 3, "reset 后 users 表应恢复为 3 行");

    // 3) id=1 的 name 必须恢复为 'Alice'，证明 down 真正执行了删除（篡改值消失），
    //    且 migrate 重新执行了 up SQL（原始值回归）。若 reset 未执行，此处仍为 "Modified"。
    let row = {
        let d = db.lock().await;
        d.find_where("users", "id", &Value::I64(1))
    };
    let name = row
        .as_ref()
        .and_then(|r| r.get("name"))
        .and_then(|v| match v {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("");
    assert_eq!(
        name, "Alice",
        "reset 应恢复 id=1 的 name 为 Alice，而非保留篡改值"
    );
}

// ===== L3-15：自定义表名生效 =====

#[tokio::test]
async fn test_l3_15_custom_table_name_works() {
    let db = Arc::new(Mutex::new(InMemoryDb::new()));
    let conn = TransactionalConnection::new(db.clone());
    let context = MigrationContext {
        table_name: "schema_migrations".to_string(),
        connection: Some(Box::new(conn)),
        db_type: Some(DbType::PostgreSQL),
    };
    let mut migrator = Migrator::new(context).add_migrations(test_migrations());

    migrator.migrate().await.unwrap();

    let db_ref = db.lock().await;
    let rows = db_ref.select_all("schema_migrations");
    assert_eq!(rows.len(), 3, "自定义表名应生效");
}

// ===== L3-16：无连接时仍可工作（纯内存模式） =====

#[tokio::test]
async fn test_l3_16_no_connection_works_in_memory() {
    let context = MigrationContext {
        table_name: "__migrations".to_string(),
        connection: None,
        db_type: Some(DbType::Sqlite),
    };
    let mut migrator = Migrator::new(context).add_migrations(test_migrations());

    let applied = migrator.migrate().await.unwrap();
    assert_eq!(applied.len(), 3, "无连接也应执行所有迁移");

    let applied_migrations = migrator.get_applied_migrations();
    assert_eq!(applied_migrations.len(), 3);
}

// ===== L3-17：progress 反映正确进度 =====

#[tokio::test]
async fn test_l3_17_progress_tracks_correctly() {
    let (mut migrator, _db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    let initial = migrator.progress();
    assert_eq!(initial.total, 3);
    assert_eq!(initial.applied, 0);
    assert_eq!(initial.pending, 3);

    migrator.migrate().await.unwrap();

    let after = migrator.progress();
    assert_eq!(after.total, 3);
    assert_eq!(after.applied, 3);
    assert_eq!(after.pending, 0);
    assert_eq!(after.percent_complete(), 100.0);
}

// ===== L3-18：DDL 事务包裹（SQLite 方言）数据持久化 =====

#[tokio::test]
async fn test_l3_18_ddl_transaction_commit_persists_data() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    // SQLite 支持 DDL 事务，migrate 应使用事务包裹
    migrator.migrate().await.unwrap();

    // 数据应已持久化（事务已提交）
    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 3, "事务提交后数据应可见");
}

// ===== L3-19：空 SQL 迁移跳过而非失败 =====

#[tokio::test]
async fn test_l3_19_empty_sql_migration_skipped() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    let migrations = vec![
        Migration::new(
            "001",
            "ok",
            "INSERT INTO users (id) VALUES (1)",
            "DELETE FROM users WHERE id = 1",
        ),
        Migration::new("002", "empty", "", "DELETE FROM users WHERE id = 2"),
        Migration::new(
            "003",
            "after_empty",
            "INSERT INTO users (id) VALUES (3)",
            "DELETE FROM users WHERE id = 3",
        ),
    ];
    migrator = migrator.add_migrations(migrations);

    let result = migrator.migrate().await;
    assert!(result.is_ok(), "空 SQL 应跳过而非失败");

    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 3, "空 SQL 迁移也应记录到 __migrations 表");
}

// ===== L3-20：批次号正确递增 =====

#[tokio::test]
async fn test_l3_20_batch_number_increments() {
    let (mut migrator, db) = make_migrator(DbType::Sqlite);
    migrator = migrator.add_migrations(test_migrations());

    migrator.migrate().await.unwrap();

    let rows = query_migrations_table(&db).await;
    assert_eq!(rows.len(), 3);
    for row in &rows {
        let batch = match row.get("batch") {
            Some(Value::I32(b)) => *b,
            Some(Value::I64(b)) => *b as i32,
            _ => 0,
        };
        assert_eq!(batch, 1, "所有迁移的批次号应为 1, row: {:?}", row);
    }
}
