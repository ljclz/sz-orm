//! 事务 — ACID 事务与保存点用法
//!
//! 演示 TransactOptions 配置、Transaction 的 commit/rollback/savepoint。
//! 由于无数据库连接，仅展示 API 用法（不会真正执行 SQL）。
//!
//! 运行：`cargo run -p sz-orm-examples --bin transaction`

use sz_orm_core::{IsolationLevel, TransactOptions};

fn main() {
    println!("=== 事务隔离级别 ===");
    let levels = [
        IsolationLevel::ReadUncommitted,
        IsolationLevel::ReadCommitted,
        IsolationLevel::RepeatableRead,
        IsolationLevel::Serializable,
        IsolationLevel::Snapshot,
    ];
    for level in &levels {
        println!("- {} ({:?})", level, level);
    }

    println!("\n=== 默认事务选项 ===");
    let default_opts = TransactOptions::default();
    println!(
        "isolation_level={:?}, read_only={}, timeout={:?}",
        default_opts.isolation_level, default_opts.read_only, default_opts.timeout
    );

    println!("\n=== 自定义事务选项 ===");
    let opts = TransactOptions::default()
        .with_isolation(IsolationLevel::Serializable)
        .read_only()
        .with_timeout(std::time::Duration::from_secs(30));
    println!("隔离级别:     {:?}", opts.isolation_level);
    println!("只读:         {}", opts.read_only);
    println!("超时:         {:?}", opts.timeout);

    println!("\n=== 事务使用模式（伪代码）===");
    println!(
        r#"// 1. 基本事务
let mut tx = Transaction::new(conn, opts);
tx.execute("INSERT INTO users (name) VALUES ('Alice')").await?;
tx.commit().await?;

// 2. 回滚
let mut tx = Transaction::new(conn, opts);
tx.execute("INSERT INTO users (name) VALUES ('Bob')").await?;
// 出错时回滚
tx.rollback().await?;

// 3. 保存点（嵌套事务）
let mut tx = Transaction::new(conn, opts);
tx.execute("INSERT INTO users (name) VALUES ('Carol')").await?;
let sp = tx.savepoint().await?;           // SAVEPOINT sp_1
tx.execute("INSERT INTO users (name) VALUES ('Dave')").await?;
tx.rollback_to_savepoint(&sp).await?;     // 回滚 Dave，保留 Carol
tx.release_savepoint(&sp).await?;
tx.commit().await?;                       // 提交 Carol

// 4. TransactionManager 多事务管理
let mgr = TransactionManager::new();
mgr.begin("tx1", conn1, opts).await?;
mgr.begin("tx2", conn2, opts).await?;
mgr.commit("tx1").await?;
mgr.rollback("tx2").await?;
println!("活跃事务: {{:?}}", mgr.list().await);"#
    );
}
