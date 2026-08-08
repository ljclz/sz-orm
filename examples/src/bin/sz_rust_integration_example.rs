//! sz-rust 协同集成示例 — 多后端透明访问
//!
//! 演示 sz-rust 透明适配层仅依赖 sz-orm 公开 API（AnyBackend/AnyPool/UnifiedPool）
//! 完成统一访问，不触碰 sz-orm 内部实现。
//!
//! 本示例使用 SQLite 内存数据库，无需外部数据库连接。
//!
//! 运行：`cargo run -p sz-orm-examples --bin sz_rust_integration_example`

use std::sync::Arc;

use sz_orm_sqlx::any_driver::AnyBackend;
use sz_orm_sqlx::UnifiedPool;

/// 模拟 sz-rust 的 AppState — 持有单一类型 `Arc<UnifiedPool>`
///
/// 这是 sz-rust 集成 sz-orm 的推荐方式：
/// - 业务代码无需感知后端类型
/// - 通过 DSN 自动识别后端（mysql/postgres/sqlite/oracle/mssql）
/// - 所有方法委托内部 Pool，零能力丢失
struct AppState {
    pool: Arc<UnifiedPool>,
}

impl AppState {
    /// 从 DSN 创建 AppState
    ///
    /// sz-rust 只需传入配置中的 DSN 字符串，sz-orm 自动识别后端类型。
    async fn from_dsn(dsn: &str) -> Result<Self, sz_orm_core::DbError> {
        let pool = UnifiedPool::connect(dsn).await?;
        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    /// 获取后端类型（用于日志/调试）
    fn backend(&self) -> AnyBackend {
        self.pool.backend()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== sz-rust 协同集成示例 ===\n");

    // ===== 1. DSN 自动识别后端 =====
    println!("1. DSN 自动识别后端");

    let test_dsns = [
        ("mysql://root:pass@127.0.0.1/db", AnyBackend::MySql),
        ("postgres://user:pass@127.0.0.1/db", AnyBackend::Postgres),
        ("sqlite::memory:", AnyBackend::Sqlite),
        ("oracle://user:pass@127.0.0.1/service", AnyBackend::Oracle),
        ("mssql://user:pass@127.0.0.1/db", AnyBackend::Mssql),
    ];

    for (dsn, expected) in &test_dsns {
        let detected = AnyBackend::from_dsn(dsn)?;
        assert_eq!(detected, *expected);
        println!("   {} → {:?} ✓", dsn, detected);
    }
    println!();

    // ===== 2. 创建 AppState（使用 SQLite 内存数据库）=====
    println!("2. 创建 AppState（SQLite 内存数据库）");

    let state = AppState::from_dsn("sqlite::memory:").await?;
    println!("   后端类型: {:?}", state.backend());
    println!();

    // ===== 3. CRUD 操作（透明访问）=====
    println!("3. CRUD 操作（业务代码无需感知后端）");

    let mut conn = state.pool.acquire().await?;

    // CREATE TABLE
    conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT)")
        .await?;
    println!("   CREATE TABLE users ✓");

    // INSERT
    conn.execute("INSERT INTO users (id, name, email) VALUES (1, 'Alice', 'alice@example.com')")
        .await?;
    conn.execute("INSERT INTO users (id, name, email) VALUES (2, 'Bob', 'bob@example.com')")
        .await?;
    println!("   INSERT 2 rows ✓");

    // SELECT
    let rows = conn
        .query("SELECT id, name, email FROM users ORDER BY id")
        .await?;
    println!("   SELECT: {} rows", rows.len());
    for row in rows.iter() {
        println!("     {:?}", row);
    }

    // UPDATE
    conn.execute("UPDATE users SET email = 'alice@updated.com' WHERE id = 1")
        .await?;
    println!("   UPDATE ✓");

    // SELECT 验证更新
    let rows = conn.query("SELECT email FROM users WHERE id = 1").await?;
    println!("   验证更新: {:?}", rows.first());
    println!();

    // ===== 4. 方言特性查询 =====
    println!("4. 方言特性查询（运行时感知后端能力）");

    let dialect = state.pool.dialect();
    println!("   supports_returning: {}", dialect.supports_returning());
    println!("   supports_if_exists: {}", dialect.supports_if_exists());
    println!(
        "   supports_if_not_exists: {}",
        dialect.supports_if_not_exists()
    );
    println!(
        "   supports_lock_for_update: {}",
        dialect.supports_lock_for_update()
    );
    println!(
        "   auto_increment_keyword: {}",
        dialect.auto_increment_keyword()
    );
    println!("   last_insert_id_sql: {:?}", dialect.last_insert_id_sql());
    println!();

    // ===== 5. 连接池状态 =====
    println!("5. 连接池状态");

    let status = state.pool.status().await;
    println!("   max: {}", status.max);
    println!("   idle: {}", status.idle);
    println!();

    // ===== 6. 事务操作 =====
    println!("6. 事务操作");

    conn.begin_transaction().await?;
    conn.execute(
        "INSERT INTO users (id, name, email) VALUES (3, 'Charlie', 'charlie@example.com')",
    )
    .await?;
    println!("   BEGIN + INSERT ✓");

    conn.commit().await?;
    println!("   COMMIT ✓");

    let rows = conn.query("SELECT COUNT(*) as count FROM users").await?;
    println!("   事务后行数: {:?}", rows.first());
    println!();

    // ===== 7. 事务回滚验证 =====
    println!("7. 事务回滚验证");

    conn.begin_transaction().await?;
    conn.execute("INSERT INTO users (id, name, email) VALUES (4, 'Dave', 'dave@example.com')")
        .await?;
    conn.rollback().await?;
    println!("   BEGIN + INSERT + ROLLBACK ✓");

    let rows = conn.query("SELECT COUNT(*) as count FROM users").await?;
    println!("   回滚后行数: {:?}", rows.first());
    println!();

    // ===== 8. 零成本迁移路径 =====
    println!("8. 零成本迁移路径（from_pool）");
    println!("   sz-rust 可从 Arc<Pool> 迁移到 Arc<UnifiedPool>，行为完全一致");
    println!("   UnifiedPool::from_pool(existing_pool, AnyBackend::Sqlite)");
    println!();

    // ===== 9. 运行时切换后端 =====
    println!("9. 运行时切换后端（DSN 驱动）");
    println!("   同一份业务代码，只需改变 DSN 即可切换数据库后端：");
    println!("   - 开发环境: sqlite::memory:");
    println!("   - 测试环境: postgres://user:pass@test-db/db");
    println!("   - 生产环境: mysql://user:pass@prod-db/db");
    println!();

    // ===== 10. 清理 =====
    println!("10. 清理");

    state.pool.close_all().await;
    println!("   连接池已关闭 ✓");

    println!("\n=== 示例完成 ===");
    println!(
        "\n结论：sz-orm 公开 API（AnyBackend + UnifiedPool）已满足 sz-rust P2-1 多后端 ORM 需求。"
    );
    println!("sz-rust 透明适配层无需触碰 sz-orm 内部实现，仅依赖公开 API 即可完成统一访问。");

    Ok(())
}
