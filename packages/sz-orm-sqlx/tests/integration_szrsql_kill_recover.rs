//! M5c：kill-recover 池恢复实验（矩阵差分评测最后遗留项）
//!
//! 流程：拉起独立 szrsql 实例（端口 6544，临时数据目录，不干扰主 6543 实例）
//! → 池正常租用查询 → kill 服务进程 → 租用失败/超时 → 重启服务 → 池恢复。
//!
//! 运行：cargo test -p sz-orm-sqlx --test integration_szrsql_kill_recover -- --ignored --nocapture
//! 前置：SZRSQL_BIN（默认 D:/rust-target/debug/szrsql.exe，指向 szrsql 仓库构建产物）。
//! szrsql 仓库只读（ADR-0001）：本测试仅执行其二进制，不改其文件。

use std::sync::Arc;
use std::time::Duration;

use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{AnyPool, PgPoolHandle, SqlxPgConnectionFactory};

const PORT: u16 = 6544;

fn szrsql_bin() -> Option<std::path::PathBuf> {
    std::env::var_os("SZRSQL_BIN")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let p = std::path::PathBuf::from("D:/rust-target/debug/szrsql.exe");
            p.is_file().then_some(p)
        })
}

fn start_server(data_dir: &std::path::Path) -> std::io::Result<std::process::Child> {
    std::process::Command::new(szrsql_bin().expect("szrsql binary"))
        .arg("--port")
        .arg(PORT.to_string())
        .arg("--auth-mode")
        .arg("trust")
        .arg("--connection-idle-timeout")
        .arg("0")
        .arg("--data-dir")
        .arg(data_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}

async fn wait_listening(timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", PORT)).is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

#[tokio::test]
#[ignore = "需要 SZRSQL_BIN（szrsql 二进制）且端口 6544 可用"]
async fn kill_recover_pool_survives_backend_restart() {
    let Some(_bin) = szrsql_bin() else {
        eprintln!("skip: 未找到 szrsql 二进制（设 SZRSQL_BIN 指向 szrsql.exe）");
        return;
    };
    let data_dir = std::env::temp_dir().join(format!("szrsql_kill_recover_{}", std::process::id()));
    std::fs::create_dir_all(&data_dir).unwrap();

    // 1) 启动实例
    let mut child = start_server(&data_dir).expect("启动 szrsql 实例");
    assert!(
        wait_listening(Duration::from_secs(20)).await,
        "szrsql 实例应在 20s 内监听 6544"
    );

    // 2) 池建立 + 基线查询
    let url = format!("postgres://postgres:trust@127.0.0.1:{PORT}/postgres");
    let handle = Arc::new(PgPoolHandle::connect(&url).await.expect("基线连接"));
    let factory = Arc::new(SqlxPgConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new()
        .max_size(2)
        .min_idle(0)
        .acquire_timeout(5)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();
    {
        let mut conn = pool.acquire().await.unwrap();
        let rows = conn.query("SELECT 1 AS n").await.unwrap();
        assert_eq!(rows.len(), 1, "基线查询应成功");
        pool.release(conn).await;
    }

    // 3) kill 服务进程
    let _ = child.kill();
    let _ = child.wait();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 4) 后端不可用：租用应失败或超时（不得挂死）
    let acquire = tokio::time::timeout(Duration::from_secs(15), pool.acquire()).await;
    match acquire {
        Err(_) => eprintln!("kill 后租用超时（预期行为之一）"),
        Ok(Err(e)) => eprintln!("kill 后租用失败（预期行为之一）: {e}"),
        Ok(Ok(mut conn)) => {
            // 拿到僵尸连接：查询应报错而非静默错误结果
            let r = conn.query("SELECT 1 AS n").await;
            assert!(
                r.is_err(),
                "kill 后旧连接查询必须失败，不得返回静默错误结果"
            );
            eprintln!("kill 后旧连接查询失败（预期）");
            pool.release(conn).await;
        }
    }

    // 5) 重启服务（同一数据目录）
    let mut child2 = start_server(&data_dir).expect("重启 szrsql 实例");
    assert!(
        wait_listening(Duration::from_secs(20)).await,
        "szrsql 重启应在 20s 内监听 6544"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 6) 池恢复：新租用 + 查询成功；循环 3 次验证稳定
    for i in 0..3u32 {
        let mut conn = pool.acquire().await.unwrap();
        let sql = format!("SELECT {i} + 1 AS n");
        let rows = conn.query(&sql).await.unwrap();
        assert_eq!(rows.len(), 1, "重启后第 {i} 次租用查询应成功");
        pool.release(conn).await;
    }
    let status = pool.status().await;
    assert!(
        status.active <= 2,
        "恢复后池容量应收敛，active={}",
        status.active
    );

    let _ = child2.kill();
    let _ = child2.wait();
    let _ = std::fs::remove_dir_all(&data_dir);
    let _: Option<AnyPool> = None;
}
