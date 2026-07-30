//! P1-3：Transaction 端到端回滚验证
//!
//! 与 `transaction_contract.rs` 的区别：
//! - 契约测试验证状态机迁移（Active→Committed/RolledBack）
//! - 端到端测试验证**真实数据持久化/回滚行为**
//!
//! 使用 `TransactionalConnection`（支持事务快照 + 简单 SQL 解析）作为底层连接，
//! 验证事务 commit/rollback/savepoint 对 `InMemoryDb` 数据的实际影响。
//!
//! ## L3 验证维度
//!
//! 1. **SQL 下推**：所有 DML 通过 SQL 解析实际操作 `InMemoryDb`，非内存标记
//! 2. **真实执行**：事务 commit/rollback 真实应用/恢复快照，非状态标记
//! 3. **行为验证**：从用户视角验证"数据是否持久化/回滚"，非内部方法调用计数

#![cfg(test)]

#[path = "common/mod.rs"]
mod common;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use sz_orm_core::{Connection, TransactOptions, Transaction, TransactionState, TxError};

use common::{InMemoryDb, TransactionalConnection};

// ===== 辅助函数 =====

/// 创建测试用的事务连接 + db 句柄
///
/// 返回 (conn, db_handle)，db_handle 可用于外部直接查询验证数据状态
fn make_conn() -> (TransactionalConnection, Arc<Mutex<InMemoryDb>>) {
    let db = Arc::new(Mutex::new(InMemoryDb::new()));
    let conn = TransactionalConnection::new(db.clone());
    (conn, db)
}

/// 初始化测试表并预置一行数据
async fn seed_users(db: &Arc<Mutex<InMemoryDb>>) {
    let mut d = db.lock().await;
    d.create_table("users");
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), sz_orm_core::Value::I64(1));
    row.insert("name".to_string(), sz_orm_core::Value::String("Alice".to_string()));
    row.insert("age".to_string(), sz_orm_core::Value::I64(30));
    d.insert("users", row);
}

// ==================== L3-T1：基本提交持久化 ====================

#[tokio::test]
async fn test_l3_t1_basic_commit_persists_data() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    // 开始事务
    conn.begin_transaction().await.unwrap();

    // 事务中插入新行
    conn.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();

    // 提交前验证：数据可见（事务内）
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 2, "事务内应可见新插入的行");
    }

    // 提交事务
    conn.commit().await.unwrap();

    // 提交后验证：数据持久化
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 2, "commit 后数据应持久化");
        let bob = d.find_where("users", "id", &sz_orm_core::Value::I64(2));
        assert!(bob.is_some(), "Bob 的行应存在");
        assert_eq!(
            bob.unwrap().get("name"),
            Some(&sz_orm_core::Value::String("Bob".to_string()))
        );
    }
}

// ==================== L3-T2：基本回滚丢弃 ====================

#[tokio::test]
async fn test_l3_t2_basic_rollback_discards_data() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    // 开始事务
    conn.begin_transaction().await.unwrap();

    // 事务中插入新行
    conn.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();

    // 回滚前验证：数据可见（事务内）
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 2, "事务内应可见新插入的行");
    }

    // 回滚事务
    conn.rollback().await.unwrap();

    // 回滚后验证：数据被丢弃
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 1, "rollback 后新插入的行应被丢弃");
        assert!(
            d.find_where("users", "id", &sz_orm_core::Value::I64(2)).is_none(),
            "Bob 的行不应存在"
        );
    }
}

// ==================== L3-T3：多语句提交 ====================

#[tokio::test]
async fn test_l3_t3_multi_statement_commit() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    conn.begin_transaction().await.unwrap();

    // INSERT
    conn.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();
    // UPDATE 已有行
    conn.execute("UPDATE users SET age = 31 WHERE id = 1")
        .await
        .unwrap();
    // DELETE（不影响已有行，验证 DELETE 也参与事务）
    conn.execute("DELETE FROM users WHERE id = 999")
        .await
        .unwrap();

    conn.commit().await.unwrap();

    let d = db.lock().await;
    assert_eq!(d.count("users"), 2, "应有 2 行（Alice + Bob）");
    // 验证 UPDATE 已应用
    let alice = d.find_where("users", "id", &sz_orm_core::Value::I64(1));
    assert_eq!(
        alice.unwrap().get("age"),
        Some(&sz_orm_core::Value::I64(31)),
        "Alice 的 age 应被更新为 31"
    );
}

// ==================== L3-T4：多语句回滚 ====================

#[tokio::test]
async fn test_l3_t4_multi_statement_rollback() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    let original_snapshot = db.lock().await.snapshot("users");

    conn.begin_transaction().await.unwrap();

    conn.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();
    conn.execute("UPDATE users SET age = 99 WHERE id = 1")
        .await
        .unwrap();
    conn.execute("DELETE FROM users WHERE id = 1")
        .await
        .unwrap();

    conn.rollback().await.unwrap();

    let d = db.lock().await;
    let current = d.snapshot("users");
    assert_eq!(
        current.len(),
        original_snapshot.len(),
        "rollback 后行数应恢复原始值"
    );
    // 验证 Alice 的 age 未被修改
    let alice = d.find_where("users", "id", &sz_orm_core::Value::I64(1));
    assert_eq!(
        alice.unwrap().get("age"),
        Some(&sz_orm_core::Value::I64(30)),
        "Alice 的 age 应恢复为原始值 30"
    );
}

// ==================== L3-T5：保存点回滚 ====================

#[tokio::test]
async fn test_l3_t5_savepoint_rollback() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    conn.begin_transaction().await.unwrap();

    // 外层 INSERT（应在 commit 后保留）
    conn.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();

    // 通过 Transaction API 创建保存点
    let mut tx = take_conn_into_tx(conn);
    let sp = tx.savepoint().await.unwrap();

    // 保存点后 INSERT（应在回滚到保存点后丢弃）
    tx.execute("INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 40)")
        .await
        .unwrap();

    // 验证保存点后有 3 行
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 3, "保存点后应有 3 行");
    }

    // 回滚到保存点
    tx.rollback_to_savepoint(&sp).await.unwrap();
    tx.release_savepoint(&sp).await.unwrap();

    // 验证回滚到保存点后只有 2 行
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 2, "回滚到保存点后应有 2 行");
        assert!(
            d.find_where("users", "id", &sz_orm_core::Value::I64(3)).is_none(),
            "Charlie 的行应被回滚"
        );
    }

    tx.commit().await.unwrap();

    // commit 后验证
    let d = db.lock().await;
    assert_eq!(d.count("users"), 2, "commit 后应有 2 行（Alice + Bob）");
    assert!(
        d.find_where("users", "id", &sz_orm_core::Value::I64(2)).is_some(),
        "Bob 的行应保留"
    );
}

// ==================== L3-T6：嵌套保存点 ====================

#[tokio::test]
async fn test_l3_t6_nested_savepoints() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    conn.begin_transaction().await.unwrap();
    let mut tx = take_conn_into_tx(conn);

    // 外层 INSERT（id=2）
    tx.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();

    // 保存点 sp1
    let sp1 = tx.savepoint().await.unwrap();

    // sp1 后 INSERT（id=3）
    tx.execute("INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 40)")
        .await
        .unwrap();

    // 保存点 sp2（嵌套）
    let sp2 = tx.savepoint().await.unwrap();

    // sp2 后 INSERT（id=4）
    tx.execute("INSERT INTO users (id, name, age) VALUES (4, 'Dave', 50)")
        .await
        .unwrap();

    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 4, "sp2 后应有 4 行");
    }

    // 回滚到 sp2：丢弃 id=4
    tx.rollback_to_savepoint(&sp2).await.unwrap();
    tx.release_savepoint(&sp2).await.unwrap();
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 3, "回滚到 sp2 后应有 3 行");
        assert!(
            d.find_where("users", "id", &sz_orm_core::Value::I64(4)).is_none(),
            "Dave 应被回滚"
        );
    }

    // 回滚到 sp1：丢弃 id=3 和 id=4
    tx.rollback_to_savepoint(&sp1).await.unwrap();
    tx.release_savepoint(&sp1).await.unwrap();
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 2, "回滚到 sp1 后应有 2 行");
        assert!(
            d.find_where("users", "id", &sz_orm_core::Value::I64(3)).is_none(),
            "Charlie 应被回滚"
        );
    }

    tx.commit().await.unwrap();
    let d = db.lock().await;
    assert_eq!(d.count("users"), 2, "commit 后应有 2 行（Alice + Bob）");
}

// ==================== L3-T7：保存点释放后整体回滚 ====================

#[tokio::test]
async fn test_l3_t7_release_savepoint_then_rollback_all() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    conn.begin_transaction().await.unwrap();
    let mut tx = take_conn_into_tx(conn);

    let sp = tx.savepoint().await.unwrap();
    tx.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();
    tx.release_savepoint(&sp).await.unwrap();

    // 释放保存点后，整体回滚应丢弃所有事务内修改
    tx.rollback().await.unwrap();

    let d = db.lock().await;
    assert_eq!(d.count("users"), 1, "rollback 后应只有原始的 Alice");
    assert!(
        d.find_where("users", "id", &sz_orm_core::Value::I64(2)).is_none(),
        "Bob 应被回滚"
    );
}

// ==================== L3-T8：事务超时回滚 ====================

#[tokio::test]
async fn test_l3_t8_transaction_timeout_rollback() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    conn.begin_transaction().await.unwrap();

    // 设置 50ms 超时
    let opts = TransactOptions::default().with_timeout(Duration::from_millis(50));
    let mut tx = Transaction::new(Box::new(conn), opts);

    tx.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();

    // 等待超时
    tokio::time::sleep(Duration::from_millis(100)).await;

    // commit 应因超时失败并触发 rollback
    let result = tx.commit().await;
    assert!(result.is_err(), "超时后 commit 应失败");
    match result.unwrap_err() {
        TxError::CommitFailed(msg) => {
            assert!(msg.contains("timeout"), "错误信息应包含 timeout");
        }
        other => panic!("期望 CommitFailed(timeout)，实际: {:?}", other),
    }

    // 验证事务状态为 RolledBack
    assert_eq!(tx.state(), TransactionState::RolledBack);

    // 验证数据被回滚（注意：超时触发的 rollback 调用的是底层 conn.rollback）
    let d = db.lock().await;
    assert_eq!(
        d.count("users"),
        1,
        "超时回滚后应只有原始的 Alice"
    );
}

// ==================== L3-T9：嵌套深度限制 ====================

#[tokio::test]
async fn test_l3_t9_max_nesting_depth_enforced() {
    let (mut conn, _db) = make_conn();
    conn.begin_transaction().await.unwrap();

    let opts = TransactOptions::default().with_max_nesting_depth(3);
    let mut tx = Transaction::new(Box::new(conn), opts);

    // 创建 3 个保存点应成功
    for i in 1..=3 {
        let sp = tx.savepoint().await.unwrap();
        assert_eq!(sp, format!("sp_{}", i));
    }

    // 第 4 个保存点应失败
    let result = tx.savepoint().await;
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::MaxNestingDepthExceeded {
            current_depth,
            max_depth,
        } => {
            assert_eq!(current_depth, 4);
            assert_eq!(max_depth, 3);
        }
        other => panic!("期望 MaxNestingDepthExceeded，实际: {:?}", other),
    }
}

// ==================== L3-T10：DROP 时自动回滚 ====================

#[tokio::test]
async fn test_l3_t10_drop_auto_rollback() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    conn.begin_transaction().await.unwrap();
    let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());

    tx.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();

    // 不提交也不回滚，直接 drop
    drop(tx);

    // 给 Drop 中 spawn 的后台 rollback 任务一点时间执行
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 验证数据被回滚
    let d = db.lock().await;
    assert_eq!(
        d.count("users"),
        1,
        "Drop 自动回滚后应只有原始的 Alice"
    );
    assert!(
        d.find_where("users", "id", &sz_orm_core::Value::I64(2)).is_none(),
        "Bob 应被 Drop 自动回滚"
    );
}

// ==================== L3-T11：commit 失败验证 ====================

#[tokio::test]
async fn test_l3_t11_commit_failure_rolls_back() {
    use std::future::Future;
    use std::pin::Pin;
    use sz_orm_core::DbError;

    /// commit 时失败的连接
    struct FailingCommitConnection {
        inner: TransactionalConnection,
    }

    impl Connection for FailingCommitConnection {
        fn execute<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
            self.inner.execute(sql)
        }
        fn query<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                            DbError,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            self.inner.query(sql)
        }
        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            self.inner.begin_transaction()
        }
        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async move {
                // 模拟 commit 失败（如网络断开）
                Err(DbError::Internal("injected commit failure".to_string()))
            })
        }
        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            self.inner.rollback()
        }
        fn is_connected(&self) -> bool {
            self.inner.is_connected()
        }
        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.ping()
        }
        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            self.inner.close()
        }
    }

    let db = Arc::new(Mutex::new(InMemoryDb::new()));
    seed_users(&db).await;

    let inner = TransactionalConnection::new(db.clone());
    let mut conn = FailingCommitConnection { inner };
    conn.begin_transaction().await.unwrap();

    conn.execute("INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25)")
        .await
        .unwrap();

    let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());

    // commit 应失败
    let result = tx.commit().await;
    assert!(result.is_err(), "commit 应失败");

    // 验证事务状态：commit 失败时状态保持 Active（未迁移到 Committed）
    // 注：commit 失败后用户应主动 rollback
    assert_eq!(tx.state(), TransactionState::Active);

    // 主动 rollback 清理
    tx.rollback().await.unwrap();

    // 验证数据被回滚
    let d = db.lock().await;
    assert_eq!(d.count("users"), 1, "commit 失败 + rollback 后应只有原始 Alice");
}

// ==================== L3-T12：事务隔离级别设置 ====================

#[tokio::test]
async fn test_l3_t12_isolation_level_options() {
    use sz_orm_core::IsolationLevel;

    // 注：内存模式无法验证隔离级别的实际生效（需真实数据库 + 并发会话），
    // 此处验证所有变体的 round-trip（设置 → 读取 → 值匹配），
    // 确保 with_isolation 正确存储每个变体，不发生默认值回落或丢失。

    let variants = vec![
        IsolationLevel::ReadUncommitted,
        IsolationLevel::ReadCommitted,
        IsolationLevel::RepeatableRead,
        IsolationLevel::Serializable,
        IsolationLevel::Snapshot,
    ];

    for level in variants {
        let (conn, _db) = make_conn();
        let opts = TransactOptions::default()
            .with_isolation(level.clone())
            .read_only()
            .with_timeout(Duration::from_secs(30));
        let tx = Transaction::new(Box::new(conn), opts);

        assert_eq!(
            tx.options().isolation_level,
            Some(level),
            "隔离级别 round-trip 应保持值不变"
        );
        assert!(tx.options().read_only, "read_only 应为 true");
        assert_eq!(
            tx.options().timeout,
            Some(Duration::from_secs(30)),
            "timeout 应为 30s"
        );
    }

    // default 不应设置隔离级别（None），证明上方的 Some 是 with_isolation 真实设置的结果
    let (conn, _db) = make_conn();
    let tx = Transaction::new(Box::new(conn), TransactOptions::default());
    assert_eq!(
        tx.options().isolation_level,
        None,
        "default opts 不应设置隔离级别"
    );
}

// ==================== L3-T13：事务传播行为设置 ====================

#[tokio::test]
async fn test_l3_t13_propagation_behavior_options() {
    use sz_orm_core::PropagationBehavior;

    // 注：内存模式无法验证传播行为的实际生效（需真实事务管理器 + 嵌套事务场景），
    // 此处验证所有变体的 round-trip（设置 → 读取 → 值匹配），
    // 确保 with_propagation 正确存储每个变体，不发生默认值回落或丢失。

    let variants = vec![
        PropagationBehavior::Required,
        PropagationBehavior::Mandatory,
        PropagationBehavior::Never,
        PropagationBehavior::Supports,
        PropagationBehavior::RequiresNew,
        PropagationBehavior::Nested,
    ];

    for behavior in variants {
        let (conn, _db) = make_conn();
        let opts = TransactOptions::default().with_propagation(behavior);
        let tx = Transaction::new(Box::new(conn), opts);

        assert_eq!(
            tx.options().propagation,
            behavior,
            "传播行为 round-trip 应保持值不变"
        );
    }

    // default 应为 Required（默认值），证明上方的值是 with_propagation 真实设置的结果
    let (conn, _db) = make_conn();
    let tx = Transaction::new(Box::new(conn), TransactOptions::default());
    assert_eq!(
        tx.options().propagation,
        PropagationBehavior::Required,
        "default 传播行为应为 Required"
    );
}

// ==================== L3-T14：多表事务回滚 ====================

#[tokio::test]
async fn test_l3_t14_multi_table_transaction_rollback() {
    let (mut conn, db) = make_conn();
    {
        let mut d = db.lock().await;
        d.create_table("users");
        d.create_table("orders");
        let mut user = std::collections::HashMap::new();
        user.insert("id".to_string(), sz_orm_core::Value::I64(1));
        user.insert("name".to_string(), sz_orm_core::Value::String("Alice".to_string()));
        d.insert("users", user);
        let mut order = std::collections::HashMap::new();
        order.insert("id".to_string(), sz_orm_core::Value::I64(100));
        order.insert("user_id".to_string(), sz_orm_core::Value::I64(1));
        order.insert("amount".to_string(), sz_orm_core::Value::I64(99));
        d.insert("orders", order);
    }

    conn.begin_transaction().await.unwrap();
    let mut tx = take_conn_into_tx(conn);

    // 在 users 表插入
    tx.execute("INSERT INTO users (id, name) VALUES (2, 'Bob')")
        .await
        .unwrap();
    // 在 orders 表插入
    tx.execute("INSERT INTO orders (id, user_id, amount) VALUES (101, 2, 50)")
        .await
        .unwrap();
    // 更新 orders 表
    tx.execute("UPDATE orders SET amount = 199 WHERE id = 100")
        .await
        .unwrap();

    // 验证事务内多表修改
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 2);
        assert_eq!(d.count("orders"), 2);
    }

    // 回滚：所有表的修改都应被撤销
    tx.rollback().await.unwrap();

    let d = db.lock().await;
    assert_eq!(d.count("users"), 1, "users 表应恢复原始 1 行");
    assert_eq!(d.count("orders"), 1, "orders 表应恢复原始 1 行");
    let order = d.find_where("orders", "id", &sz_orm_core::Value::I64(100));
    assert_eq!(
        order.unwrap().get("amount"),
        Some(&sz_orm_core::Value::I64(99)),
        "orders 表的 amount 应恢复为原始值 99"
    );
}

// ==================== L3-T15：UPDATE 回滚恢复原值 ====================

#[tokio::test]
async fn test_l3_t15_update_rollback_restores_original() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    conn.begin_transaction().await.unwrap();
    let mut tx = take_conn_into_tx(conn);

    // 事务中多次 UPDATE
    tx.execute("UPDATE users SET age = 40 WHERE id = 1")
        .await
        .unwrap();
    tx.execute("UPDATE users SET name = 'Alice2' WHERE id = 1")
        .await
        .unwrap();

    // 验证事务内修改
    {
        let d = db.lock().await;
        let alice = d.find_where("users", "id", &sz_orm_core::Value::I64(1));
        let alice = alice.unwrap();
        assert_eq!(alice.get("age"), Some(&sz_orm_core::Value::I64(40)));
        assert_eq!(
            alice.get("name"),
            Some(&sz_orm_core::Value::String("Alice2".to_string()))
        );
    }

    // 回滚
    tx.rollback().await.unwrap();

    // 验证恢复原值
    let d = db.lock().await;
    let alice = d.find_where("users", "id", &sz_orm_core::Value::I64(1));
    let alice = alice.unwrap();
    assert_eq!(alice.get("age"), Some(&sz_orm_core::Value::I64(30)), "age 应恢复为 30");
    assert_eq!(
        alice.get("name"),
        Some(&sz_orm_core::Value::String("Alice".to_string())),
        "name 应恢复为 Alice"
    );
}

// ==================== L3-T16：DELETE 回滚恢复数据 ====================

#[tokio::test]
async fn test_l3_t16_delete_rollback_restores_data() {
    let (mut conn, db) = make_conn();
    seed_users(&db).await;

    conn.begin_transaction().await.unwrap();
    let mut tx = take_conn_into_tx(conn);

    // 事务中 DELETE
    let affected = tx.execute("DELETE FROM users WHERE id = 1").await.unwrap();
    assert_eq!(affected, 1, "应删除 1 行");

    // 验证事务内已删除
    {
        let d = db.lock().await;
        assert_eq!(d.count("users"), 0, "事务内 users 应为空");
    }

    // 回滚
    tx.rollback().await.unwrap();

    // 验证数据恢复
    let d = db.lock().await;
    assert_eq!(d.count("users"), 1, "rollback 后 users 应恢复 1 行");
    let alice = d.find_where("users", "id", &sz_orm_core::Value::I64(1));
    assert!(alice.is_some(), "Alice 应恢复");
}

// ==================== 辅助：将 conn 转为 Transaction ====================

/// 将 TransactionalConnection 包装为 Transaction
///
/// 注意：调用方需先 `conn.begin_transaction().await.unwrap()` 启动事务，
/// 然后调用此函数。Transaction::new 不会自动调用 begin_transaction。
fn take_conn_into_tx(conn: TransactionalConnection) -> Transaction {
    Transaction::new(Box::new(conn), TransactOptions::default())
}
