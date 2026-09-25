//! 临时复现测试（验证后删除）—— 复现 sz-rust sz300 db_integration_test
//! `test_mysql_transaction_commit_rollback` 场景 3 第二次 pool.acquire() 挂死。
//!
//! 1:1 镜像 sz-rust `db::init_pool` 两层池配置：
//! - 内层 sqlx MySqlPool: max_connections=20, acquire_timeout=30s（无 idle_timeout/max_lifetime）
//! - 外层 sz-orm Pool: max_size=20, min_idle=10, connection_timeout=10s, Pool::new（无预热）
//! 测试序列完全对齐 sz-rust：ensure 阶段（建池#1→acquire→SELECT 1→close_all）→
//! 建池#2 → 场景1 commit → 场景2 rollback → 场景3 第二次 acquire。

use std::sync::Arc;
use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{MySqlPoolHandle, SqlxMySqlConnectionFactory};

fn mysql_url() -> String {
    std::env::var("SZ_ORM_MYSQL_URL")
        .unwrap_or_else(|_| "mysql://root:test123@127.0.0.1:3306/sz_orm_test".to_string())
}

async fn init_pool_two_layer() -> Pool {
    let url = mysql_url();
    let sqlx_pool = sqlx::pool::PoolOptions::<sqlx::MySql>::new()
        .max_connections(20)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&url)
        .await
        .expect("sqlx 内层池连接失败");
    let factory = SqlxMySqlConnectionFactory::new(Arc::new(MySqlPoolHandle::from_pool(sqlx_pool)));
    let mut pool_cfg = PoolConfigBuilder::new()
        .max_size(20)
        .min_idle(10)
        .build()
        .expect("池配置构建失败");
    pool_cfg.connection_timeout = std::time::Duration::from_secs(10);
    Pool::new(pool_cfg, Arc::new(factory)).expect("外层池创建失败")
}

#[tokio::test]
async fn repro_two_layer_second_acquire() {
    // 看门狗：若它在卡住期间仍在打印 → 执行线程活着（future 卡死）；
    // 若它停止打印 → 执行线程被同步阻塞（timeout 永不触发的根因）。
    let watchdog = tokio::spawn(async {
        for i in 1..=12 {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            eprintln!("[watchdog] {}s 仍存活（执行线程未冻结）", i * 10);
        }
    });

    // ── Phase 0：ensure_mysql_available 等价流程 ──
    eprintln!("[P0] init pool #1");
    let pool0 = init_pool_two_layer().await;
    eprintln!("[P0] acquire + SELECT 1");
    let mut c0 = pool0.acquire().await.expect("P0 acquire 失败");
    c0.query("SELECT 1").await.expect("P0 SELECT 1 失败");
    eprintln!("[P0] close_all");
    pool0.close_all().await;
    drop(c0);
    drop(pool0);
    eprintln!("[P0] done");

    // ── Phase 1：主测试池（对齐 sz-rust 测试体）──
    eprintln!("[P1] init pool #2");
    let pool = init_pool_two_layer().await;
    eprintln!("[P1] first acquire");
    let mut conn = pool.acquire().await.expect("P1 首次 acquire 失败");
    conn.execute("DROP TABLE IF EXISTS sz300_repro_tx").await.ok();
    conn.execute("CREATE TABLE sz300_repro_tx (id INT AUTO_INCREMENT PRIMARY KEY, val VARCHAR(50) NOT NULL)")
        .await
        .expect("建表失败");
    eprintln!("[P1] table ready, status={:?}", pool.status().await);

    // 场景 1：commit
    eprintln!("[S1] begin/insert/select/commit");
    conn.begin_transaction().await.expect("S1 begin 失败");
    conn.execute("INSERT INTO sz300_repro_tx (val) VALUES ('committed')")
        .await
        .expect("S1 insert 失败");
    let rows = conn
        .query("SELECT val FROM sz300_repro_tx WHERE val = 'committed'")
        .await
        .expect("S1 select 失败");
    assert_eq!(rows.len(), 1, "S1 会话内可见性");
    conn.commit().await.expect("S1 commit 失败");
    let rows = conn
        .query("SELECT val FROM sz300_repro_tx WHERE val = 'committed'")
        .await
        .expect("S1 post-commit select 失败");
    assert_eq!(rows.len(), 1, "S1 commit 持久可见");
    eprintln!("[S1] ok");

    // 场景 2：rollback
    eprintln!("[S2] begin/insert/rollback");
    conn.begin_transaction().await.expect("S2 begin 失败");
    conn.execute("INSERT INTO sz300_repro_tx (val) VALUES ('rolled_back')")
        .await
        .expect("S2 insert 失败");
    conn.rollback().await.expect("S2 rollback 失败");
    let rows = conn
        .query("SELECT COUNT(*) AS cnt FROM sz300_repro_tx")
        .await
        .expect("S2 count 失败");
    eprintln!("[S2] ok, cnt={:?}", rows[0].get("cnt"));

    // 场景 3：第二次 acquire（sz-rust 报告的挂点）
    eprintln!(
        "[S3] second acquire ... status={:?}",
        pool.status().await
    );
    let mut c2 = pool.acquire().await.expect("S3 第二次 acquire 失败");
    eprintln!("[S3] second acquire OK");
    c2.begin_transaction().await.expect("S3 begin 失败");
    c2.execute("INSERT INTO sz300_repro_tx (val) VALUES ('dropped')")
        .await
        .expect("S3 insert 失败");
    drop(c2); // 不 commit 直接 drop → spawn 异步 release
    eprintln!("[S3] c2 dropped（release 已 spawn，事务未回滚）");

    let rows = conn
        .query("SELECT COUNT(*) AS cnt FROM sz300_repro_tx")
        .await
        .expect("S3 count 失败");
    eprintln!("[S3] post-drop count={:?}", rows[0].get("cnt"));

    eprintln!("[END] final DROP TABLE（MDL 阻塞观察点）");
    conn.execute("DROP TABLE IF EXISTS sz300_repro_tx").await.ok();
    pool.close_all().await;
    watchdog.abort();
    eprintln!("✅ 复现流程全程无挂死");
}
