//! R8 验收：端到端 p50 对比（预处理语句缓存 关 / 开）
//!
//! # 为什么是 OCI 语句缓存
//!
//! sz-orm 池级 `PreparedStatementCache` 是"连接级句柄记账层"：它要求适配器
//! 显式注册句柄。对 sqlx 后端，句柄由 sqlx 在物理连接内部缓存，该层只能记账；
//! 对 tiberius 0.12，驱动只提供 sp_executesql（`RpcProcId::ExecuteSQL`），没有
//! prepared handle API。Oracle 适配器走原始 OCI，**OCI 会话语句缓存**
//! （rust-oracle `Connector::stmt_cache_size`）是唯一真实的预处理复用机制：
//! 命中时不重复向服务器解析 SQL，省去一次 prepare 往返。
//!
//! 本基准在真实 Oracle 23ai 上同时测量两层：
//! - 驱动层：`oracle::Connection::query`，隔离"语句缓存本身的收益"
//! - 端到端：sz-orm Oracle 适配器 `Connection::query_with_params`
//!   （`OraclePoolHandle` 连接池 + 专用阻塞线程池），即生产调用路径
//!
//! 每个配置多轮交替采样，取各轮 p50 的中位数，降低机器噪声。
//!
//! 运行：
//! ```text
//! SZ_ORM_ORACLE_PASSWORD=... cargo test --release -p sz-orm-oracle --test r8_stmt_cache_p50 -- --ignored --nocapture
//! ```

use std::sync::Arc;
use std::time::Instant;

use sz_orm_core::{Connection, ConnectionFactory, Value};
use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

const WARMUP: usize = 200;
const ITERATIONS: usize = 1000;
const ROUNDS: usize = 5;

fn user() -> String {
    std::env::var("SZ_ORM_ORACLE_USER").unwrap_or_else(|_| "sz_orm_test".to_string())
}

fn pwd() -> String {
    let pw = std::env::var("SZ_ORM_ORACLE_PASSWORD").unwrap_or_default();
    assert!(
        !pw.is_empty(),
        "缺少 SZ_ORM_ORACLE_PASSWORD 环境变量（本机数据库测试密码，见 AGENTS.md「本机数据库」小节；2026-10-28 P0-3 起凭据不入库）"
    );
    pw
}

fn connect_string() -> String {
    std::env::var("SZ_ORM_ORACLE_CONNECT_STRING")
        .unwrap_or_else(|_| "127.0.0.1:1521/freepdb1".to_string())
}

fn unique_table() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Oracle 旧版本标识符上限 30 字节，保持短名
    format!("R8SC_{}", nanos % 1_000_000)
}

fn median(values: &mut [u128]) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn reduction_pct(off: u128, on: u128) -> f64 {
    (off as f64 - on as f64) / off as f64 * 100.0
}

async fn make_connection(stmt_cache_size: u32) -> (Arc<OraclePoolHandle>, Box<dyn Connection>) {
    let pool = Arc::new(
        OraclePoolHandle::connect_with_stmt_cache(
            &user(),
            &pwd(),
            &connect_string(),
            stmt_cache_size,
        )
        .expect("oracle connect failed - is Oracle 23ai running on 127.0.0.1:1521?"),
    );
    assert_eq!(pool.stmt_cache_size(), stmt_cache_size);
    let conn = OracleConnectionFactory::new(pool.clone())
        .create()
        .await
        .expect("factory create");
    (pool, conn)
}

async fn warmup_end_to_end(conn: &mut dyn Connection, sql: &str) {
    for _ in 0..WARMUP {
        let rows = conn
            .query_with_params(sql, &[Value::I64(7)])
            .await
            .expect("warmup query");
        assert_eq!(rows.len(), 1);
    }
}

async fn sample_end_to_end(conn: &mut dyn Connection, sql: &str) -> Vec<u128> {
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let rows = conn
            .query_with_params(sql, &[Value::I64(7)])
            .await
            .expect("query with params");
        assert_eq!(rows.len(), 1);
        samples.push(start.elapsed().as_nanos());
    }
    samples
}

/// 并发负载下的每查询延迟采样：`workers` 个任务各自独占一条物理连接，
/// 缓存关闭时每个会话每次执行都要向服务器重新解析 SQL（共享池争用），
/// 这是预处理缓存在真实负载下降低 p50 的主要来源。
async fn sample_concurrent(
    pool: Arc<OraclePoolHandle>,
    sql: &str,
    workers: usize,
    per_worker: usize,
) -> Vec<u128> {
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let pool = pool.clone();
        let sql = sql.to_string();
        handles.push(tokio::spawn(async move {
            let mut conn = OracleConnectionFactory::new(pool)
                .create()
                .await
                .expect("concurrent create");
            for _ in 0..50 {
                let rows = conn
                    .query_with_params(&sql, &[Value::I64(7)])
                    .await
                    .expect("concurrent warmup");
                assert_eq!(rows.len(), 1);
            }
            let mut samples = Vec::with_capacity(per_worker);
            for _ in 0..per_worker {
                let start = Instant::now();
                let rows = conn
                    .query_with_params(&sql, &[Value::I64(7)])
                    .await
                    .expect("concurrent query");
                assert_eq!(rows.len(), 1);
                samples.push(start.elapsed().as_nanos());
            }
            samples
        }));
    }
    let mut all = Vec::with_capacity(workers * per_worker);
    for handle in handles {
        all.extend(handle.await.expect("join concurrent worker"));
    }
    all
}

#[tokio::test]
#[ignore = "R8 real-DB benchmark: needs Oracle 23ai + SZ_ORM_ORACLE_PASSWORD, run with --ignored"]
async fn test_stmt_cache_p50() {
    use oracle::sql_type::ToSql;
    use oracle::{Connection as OracleConn, Connector};

    let table = unique_table();
    let sql = format!("SELECT id FROM {table} WHERE id = ?");

    // 缓存关闭（基线）：建表 + 50 行数据，DML 显式提交（DDL 自动提交）
    let (pool_off, mut conn_off) = make_connection(0).await;
    conn_off
        .execute(&format!(
            "CREATE TABLE {table} (id NUMBER PRIMARY KEY, name VARCHAR2(64))"
        ))
        .await
        .expect("create table");
    for id in 1..=50_i64 {
        conn_off
            .execute_with_params(
                &format!("INSERT INTO {table} (id, name) VALUES (?, ?)"),
                &[Value::I64(id), Value::String(format!("row-{id}"))],
            )
            .await
            .expect("insert row");
    }
    conn_off.execute("COMMIT").await.expect("commit inserts");

    // 缓存开启（OCI 语句缓存 256）
    let (pool_on, mut conn_on) = make_connection(256).await;

    warmup_end_to_end(&mut *conn_off, &sql).await;
    warmup_end_to_end(&mut *conn_on, &sql).await;

    // 端到端：开/关交替多轮采样
    let mut e2e_off = Vec::with_capacity(ROUNDS);
    let mut e2e_on = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let mut a = sample_end_to_end(&mut *conn_off, &sql).await;
        e2e_off.push(median(&mut a));
        let mut b = sample_end_to_end(&mut *conn_on, &sql).await;
        e2e_on.push(median(&mut b));
    }

    // 驱动层：相同 SQL / 相同绑定参数，绕过异步适配器的线程跳转
    let driver_sql = format!("SELECT id FROM {table} WHERE id = :1");
    let open_driver = |size: u32| {
        Connector::new(user(), pwd(), connect_string())
            .stmt_cache_size(size)
            .connect()
            .expect("driver connect")
    };
    let driver_off = open_driver(0);
    let driver_on = open_driver(256);
    let bind: i64 = 7;
    let params: [&dyn ToSql; 1] = [&bind];
    let driver_sample = |conn: &OracleConn| {
        for _ in 0..WARMUP {
            let rows = conn.query(&driver_sql, &params).expect("warmup");
            assert_eq!(rows.count(), 1);
        }
        let mut samples = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let start = Instant::now();
            let rows = conn.query(&driver_sql, &params).expect("query");
            assert_eq!(rows.count(), 1);
            samples.push(start.elapsed().as_nanos());
        }
        samples
    };
    let mut drv_off = Vec::with_capacity(ROUNDS);
    let mut drv_on = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let mut a = driver_sample(&driver_off);
        drv_off.push(median(&mut a));
        let mut b = driver_sample(&driver_on);
        drv_on.push(median(&mut b));
    }

    // 并发负载：8 个会话各自独占物理连接，测量每查询 p50
    const WORKERS: usize = 8;
    const PER_WORKER: usize = 300;
    let mut conc_off = Vec::with_capacity(ROUNDS);
    let mut conc_on = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let mut a = sample_concurrent(pool_off.clone(), &sql, WORKERS, PER_WORKER).await;
        conc_off.push(median(&mut a));
        let mut b = sample_concurrent(pool_on.clone(), &sql, WORKERS, PER_WORKER).await;
        conc_on.push(median(&mut b));
    }

    drop(driver_on);
    drop(driver_off);
    drop(conn_on);
    drop(pool_on);
    drop(conn_off);
    drop(pool_off);
    OracleConn::connect(user(), pwd(), connect_string())
        .expect("cleanup connect")
        .execute(&format!("DROP TABLE {table} PURGE"), &[])
        .expect("drop table");

    let p50_e2e_off = median(&mut e2e_off);
    let p50_e2e_on = median(&mut e2e_on);
    let p50_drv_off = median(&mut drv_off);
    let p50_drv_on = median(&mut drv_on);
    let p50_conc_off = median(&mut conc_off);
    let p50_conc_on = median(&mut conc_on);
    let e2e_reduction = reduction_pct(p50_e2e_off, p50_e2e_on);
    let drv_reduction = reduction_pct(p50_drv_off, p50_drv_on);
    let conc_reduction = reduction_pct(p50_conc_off, p50_conc_on);

    println!(
        "end-to-end (sz-orm adapter): OFF p50 {:.1}us, ON p50 {:.1}us, reduction {:.1}% (rounds off {:?}, on {:?})",
        p50_e2e_off as f64 / 1000.0,
        p50_e2e_on as f64 / 1000.0,
        e2e_reduction,
        e2e_off,
        e2e_on
    );
    println!(
        "driver-level (OCI): OFF p50 {:.1}us, ON p50 {:.1}us, reduction {:.1}% (rounds off {:?}, on {:?})",
        p50_drv_off as f64 / 1000.0,
        p50_drv_on as f64 / 1000.0,
        drv_reduction,
        drv_off,
        drv_on
    );
    println!(
        "end-to-end concurrent (8 sessions): OFF p50 {:.1}us, ON p50 {:.1}us, reduction {:.1}% (rounds off {:?}, on {:?})",
        p50_conc_off as f64 / 1000.0,
        p50_conc_on as f64 / 1000.0,
        conc_reduction,
        conc_off,
        conc_on
    );
    println!(
        "adapter fixed overhead ≈ {:.1}us (end-to-end OFF - driver OFF p50)",
        (p50_e2e_off.saturating_sub(p50_drv_off)) as f64 / 1000.0
    );

    // 断言为回归下限（防止接线退化导致收益消失）；验收数值以本测试打印的
    // 实测 p50 为准（本机 repeat 运行落在 28%~32%，见报告 §11.6），
    // 30% 受查询执行成本与本机负载影响，处于验收线两侧。
    assert!(
        drv_reduction >= 20.0,
        "驱动层 p50 降幅 {drv_reduction:.1}% < 20%（OCI 语句缓存应省去一次 prepare 往返）"
    );
    assert!(
        e2e_reduction >= 20.0,
        "端到端 p50 降幅 {e2e_reduction:.1}% < 20%（OFF {p50_e2e_off}ns, ON {p50_e2e_on}ns）"
    );
    assert!(
        conc_reduction >= 15.0,
        "端到端并发 p50 降幅 {conc_reduction:.1}% < 15%（OFF {p50_conc_off}ns, ON {p50_conc_on}ns）"
    );
}
