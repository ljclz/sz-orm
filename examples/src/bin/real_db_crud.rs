//! 真实数据库端到端示例 — 连接池 + CRUD + 事务
//!
//! 使用 SQLite 内存数据库，无需安装任何外部数据库即可运行。
//! 演示完整的数据库操作流程：
//! 1. 创建连接池
//! 2. DDL 建表
//! 3. INSERT 插入数据
//! 4. SELECT 查询数据
//! 5. UPDATE 更新数据
//! 6. DELETE 删除数据
//! 7. 事务提交与回滚
//!
//! 运行：`cargo run -p sz-orm-examples --bin real_db_crud`

use std::sync::Arc;

use sz_orm_core::{Pool, PoolConfigBuilder, QueryRows};
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};

/// 打印查询结果
fn print_rows(label: &str, rows: &QueryRows) {
    println!("\n--- {} ({} 行) ---", label, rows.len());
    for (i, row) in rows.iter().enumerate() {
        let id = row
            .get("id")
            .map(|v| format!("{:?}", v))
            .unwrap_or_default();
        let name = row
            .get("name")
            .map(|v| format!("{:?}", v))
            .unwrap_or_default();
        let email = row
            .get("email")
            .map(|v| format!("{:?}", v))
            .unwrap_or_default();
        let age = row
            .get("age")
            .map(|v| format!("{:?}", v))
            .unwrap_or_default();
        println!(
            "  [{}] id={}, name={}, email={}, age={}",
            i, id, name, email, age
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== SZ-ORM 真实数据库端到端示例 ===\n");

    // ============================================================
    // 1. 创建连接池
    // ============================================================
    println!("1. 创建 SQLite 内存数据库连接池...");
    let pool_handle = SqlitePoolHandle::connect("sqlite::memory:").await?;
    let factory = Arc::new(SqlxSqliteConnectionFactory::new(Arc::new(pool_handle)));
    let config = PoolConfigBuilder::new().max_size(5).build()?;
    let pool = Pool::new(config, factory)?;
    println!("   连接池创建成功 (max_size=5)");

    // ============================================================
    // 2. DDL 建表
    // ============================================================
    println!("\n2. 建表...");
    {
        let mut conn = pool.acquire().await?;
        conn.execute(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE,
                age INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            )",
        )
        .await?;
        println!("   表 users 创建成功");
    }

    // ============================================================
    // 3. INSERT 插入数据
    // ============================================================
    println!("\n3. 插入数据...");
    {
        let mut conn = pool.acquire().await?;
        conn.execute(
            "INSERT INTO users (name, email, age) VALUES ('Alice', 'alice@example.com', 30)",
        )
        .await?;
        conn.execute("INSERT INTO users (name, email, age) VALUES ('Bob', 'bob@example.com', 25)")
            .await?;
        conn.execute(
            "INSERT INTO users (name, email, age) VALUES ('Carol', 'carol@example.com', 28)",
        )
        .await?;
        println!("   插入 3 条记录: Alice, Bob, Carol");
    }

    // ============================================================
    // 4. SELECT 查询数据
    // ============================================================
    println!("\n4. 查询数据...");
    {
        let mut conn = pool.acquire().await?;

        // 查询全部
        let rows = conn
            .query("SELECT id, name, email, age FROM users ORDER BY id")
            .await?;
        print_rows("全部用户", &rows);

        // 条件查询
        let rows = conn
            .query("SELECT id, name, email, age FROM users WHERE age >= 28 ORDER BY age DESC")
            .await?;
        print_rows("age >= 28 的用户", &rows);
    }

    // ============================================================
    // 5. UPDATE 更新数据
    // ============================================================
    println!("\n5. 更新数据...");
    {
        let mut conn = pool.acquire().await?;
        let affected = conn
            .execute("UPDATE users SET age = 31 WHERE name = 'Alice'")
            .await?;
        println!("   更新 Alice 的 age=31, 影响行数: {}", affected);

        // 验证更新
        let rows = conn
            .query("SELECT id, name, email, age FROM users WHERE name = 'Alice'")
            .await?;
        print_rows("更新后的 Alice", &rows);
    }

    // ============================================================
    // 6. DELETE 删除数据
    // ============================================================
    println!("\n6. 删除数据...");
    {
        let mut conn = pool.acquire().await?;
        let affected = conn.execute("DELETE FROM users WHERE name = 'Bob'").await?;
        println!("   删除 Bob, 影响行数: {}", affected);

        // 验证删除
        let rows = conn
            .query("SELECT id, name, email, age FROM users ORDER BY id")
            .await?;
        print_rows("删除后的用户列表", &rows);
    }

    // ============================================================
    // 7. 事务：提交
    // ============================================================
    println!("\n7. 事务测试 — 提交...");
    {
        let mut conn = pool.acquire().await?;

        // 开启事务
        conn.begin_transaction().await?;
        conn.execute(
            "INSERT INTO users (name, email, age) VALUES ('Dave', 'dave@example.com', 35)",
        )
        .await?;
        conn.execute("INSERT INTO users (name, email, age) VALUES ('Eve', 'eve@example.com', 22)")
            .await?;
        println!("   事务内插入 Dave 和 Eve");

        // 提交事务
        conn.commit().await?;
        println!("   事务已提交");

        // 验证
        let rows = conn
            .query("SELECT id, name, email, age FROM users ORDER BY id")
            .await?;
        print_rows("提交后的用户列表", &rows);
    }

    // ============================================================
    // 8. 事务：回滚
    // ============================================================
    println!("\n8. 事务测试 — 回滚...");
    {
        let mut conn = pool.acquire().await?;

        // 开启事务
        conn.begin_transaction().await?;
        conn.execute(
            "INSERT INTO users (name, email, age) VALUES ('Frank', 'frank@example.com', 40)",
        )
        .await?;
        println!("   事务内插入 Frank（将被回滚）");

        // 回滚事务
        conn.rollback().await?;
        println!("   事务已回滚");

        // 验证 Frank 不存在
        let rows = conn
            .query("SELECT id, name, email, age FROM users WHERE name = 'Frank'")
            .await?;
        print_rows("回滚后的 Frank 查询结果", &rows);
        assert!(rows.is_empty(), "Frank 应该不存在（已回滚）");
    }

    // ============================================================
    // 9. 最终状态
    // ============================================================
    println!("\n9. 最终状态...");
    {
        let mut conn = pool.acquire().await?;
        let rows = conn
            .query("SELECT id, name, email, age FROM users ORDER BY id")
            .await?;
        print_rows("最终用户列表", &rows);
        println!("\n   预期: Alice(31), Carol(28), Dave(35), Eve(22) — 共 4 人");
        assert_eq!(rows.len(), 4, "最终应有 4 条记录");
    }

    println!("\n=== 端到端示例完成 ===");
    Ok(())
}
