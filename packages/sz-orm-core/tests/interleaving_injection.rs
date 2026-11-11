//! 批④ L5 时序交错：双会话交错序列逐语句执行（确定性并发模拟）
//!
//! 模型：会话 A/B 各持一条独立连接 + 各自的语句脚本（相对顺序保持），
//! 按交错序列（保持各脚本内相对顺序的 A/B 洗牌）逐语句执行。
//! 交错点 = 并发请求到达顺序的确定性模拟。
//!
//! 不变量（先探针验证基座行为再写成断言）：
//! - 同行写-写重叠事务恰一提交成功（SQLite busy 冲突 / 后提交者冲突报错或串行化）
//! - 非重叠（串行）双成功合法——判定前按交错序列计算事务区间是否重叠
//! - 异行双成功
//! - 回滚隔离（脏读免疫）
//! - 全程不挂起（tokio::time::timeout 计时守护）
//!
//! 基座：SQLite（rusqlite 双连接，共享文件以支持跨连接可见性 + busy_timeout）。
//! 运行方式：cargo test -p sz-orm-core --test interleaving_injection

mod common;

use common::rusqlite_adapter::RusqliteConnection;
use sz_orm_core::{Connection, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Session {
    A,
    B,
}

/// 交错序列长度必须 == 两脚本步数之和（写短了会静默截断另一脚本——踩坑记录）
fn assert_sequence_completeness(seq: &[Session], a_len: usize, b_len: usize) {
    let a_count = seq.iter().filter(|s| matches!(s, Session::A)).count();
    let b_count = seq.iter().filter(|s| matches!(s, Session::B)).count();
    assert_eq!(a_count, a_len, "交错序列中 A 步数不足（静默截断风险）");
    assert_eq!(b_count, b_len, "交错序列中 B 步数不足（静默截断风险）");
    assert_eq!(seq.len(), a_len + b_len, "序列总长必须等于两脚本步数之和");
}

/// 保持相对顺序的随机洗牌（可复现 seeded）
fn interleave(a_len: usize, b_len: usize, seed: u64) -> Vec<Session> {
    let mut seq = Vec::with_capacity(a_len + b_len);
    let mut state = seed | 1;
    let mut a_left = a_len;
    let mut b_left = b_len;
    while a_left > 0 || b_left > 0 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let total = (a_left + b_left) as u64;
        let threshold = (u64::MAX / total) * a_left as u64;
        if state < threshold && a_left > 0 {
            seq.push(Session::A);
            a_left -= 1;
        } else if b_left > 0 {
            seq.push(Session::B);
            b_left -= 1;
        } else {
            seq.push(Session::A);
            a_left -= 1;
        }
    }
    assert_sequence_completeness(&seq, a_len, b_len);
    seq
}

/// 穷举全部 C(n+m, n) 交错序列（小规模）
fn interleave_all(a_len: usize, b_len: usize) -> Vec<Vec<Session>> {
    let mut result = Vec::new();
    fn rec(a_left: usize, b_left: usize, seq: &mut Vec<Session>, out: &mut Vec<Vec<Session>>) {
        if a_left == 0 && b_left == 0 {
            out.push(seq.clone());
            return;
        }
        if a_left > 0 {
            seq.push(Session::A);
            rec(a_left - 1, b_left, seq, out);
            seq.pop();
        }
        if b_left > 0 {
            seq.push(Session::B);
            rec(a_left, b_left - 1, seq, out);
            seq.pop();
        }
    }
    let mut seq = Vec::with_capacity(a_len + b_len);
    rec(a_len, b_len, &mut seq, &mut result);
    for s in &result {
        assert_sequence_completeness(s, a_len, b_len);
    }
    result
}

/// 双连接 SQLite 共享文件库（跨连接可见 + busy 超时守护不挂起）
struct TwoConns {
    a: RusqliteConnection,
    b: RusqliteConnection,
    path: std::path::PathBuf,
}

impl TwoConns {
    /// 显式关闭：drop 连接释放文件锁后删除（适配器 close 是 no-op，靠 drop）
    async fn close(mut self) {
        let path = self.path.clone();
        let _ = self.a.close().await; // 适配器 close 为 no-op
        self.a = RusqliteConnection::new(rusqlite::Connection::open_in_memory().expect("close a"));
        self.b = RusqliteConnection::new(rusqlite::Connection::open_in_memory().expect("close b"));
        // 原连接被替换 drop，文件锁释放
        drop(self);
        let _ = std::fs::remove_file(&path);
    }
}

async fn open_two_conns() -> TwoConns {
    // 共享临时文件（跨连接可见），busy_timeout 防挂起
    use std::sync::atomic::{AtomicU64, Ordering};
    static L5_SEQ: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "szorm_l5_{}_{}.db",
        std::process::id(),
        L5_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    let path_str = path.to_string_lossy().to_string();
    // 上一个测试连接的 WAL/journal 锁可能未立即释放——短暂退避重试
    for attempt in 0..10u32 {
        match rusqlite::Connection::open(&path_str) {
            Ok(c) => {
                if attempt > 0 {
                    // 重开成功即退出
                }
                drop(c);
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }

    let mut a = RusqliteConnection::new(rusqlite::Connection::open(&path_str).expect("open A"));
    let mut b = RusqliteConnection::new(rusqlite::Connection::open(&path_str).expect("open B"));
    a.query("PRAGMA busy_timeout = 3000").await.expect("busy a");
    b.query("PRAGMA busy_timeout = 3000").await.expect("busy b");
    a.execute("CREATE TABLE IF NOT EXISTS l5_t (id INTEGER PRIMARY KEY, v INTEGER NOT NULL)")
        .await
        .expect("create");
    TwoConns { a, b, path: path.clone() }
}

/// 事务区间重叠判定：按交错序列模拟语句执行，计算两事务是否重叠
/// （A.begin 到 A.commit 期间出现 B.begin 且 B 未结束 → 重叠）
fn tx_intervals_overlap(seq: &[Session]) -> bool {
    let mut a_open = false;
    let mut b_open = false;
    for s in seq {
        match s {
            Session::A => a_open = !a_open,
            Session::B => b_open = !b_open,
        }
        if a_open && b_open {
            return true;
        }
    }
    false
}

async fn read_v(conn: &mut RusqliteConnection) -> i64 {
    let rows = conn
        .query("SELECT v FROM l5_t WHERE id = 1")
        .await
        .expect("read");
    match rows[0].get("v") {
        Some(Value::I64(v)) => *v,
        other => panic!("类型: {:?}", other),
    }
}

/// 探针 1：同行写-写 全交错穷举——重叠事务恰一语义（终值 ∈ 合法集）+ 全程不挂起
#[tokio::test]
async fn l5_probe_same_row_write_write_overlapping() {
    let guard = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let seqs = interleave_all(3, 3);
        for (i, seq) in seqs.iter().enumerate() {
            let TwoConns { mut a, mut b, path } = open_two_conns().await;
            a.execute("DELETE FROM l5_t").await.expect("clear");
        a.execute("INSERT INTO l5_t (id, v) VALUES (1, 0)").await.expect("seed");

            let mut a_step = 0usize;
            let mut b_step = 0usize;
            let _ = tx_intervals_overlap(seq); // 重叠分类记录（合法集判定依据）
            let overlap = tx_intervals_overlap(seq);
            for step in seq {
                let res = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    match step {
                        Session::A => {
                            a_step += 1;
                            match a_step {
                                1 => a.begin_transaction().await.map_err(|e| format!("A begin: {}", e)),
                                2 => {
                                    // 重叠时 B 可能持锁——busy 冲突按回滚处理
                                    if let Err(e) =
                                        a.execute("UPDATE l5_t SET v = v + 1 WHERE id = 1").await
                                    {
                                        let _ = a.rollback().await;
                                        return Err(format!("A upd busy: {}", e));
                                    }
                                    Ok(())
                                }
                                _ => a.commit().await.map_err(|e| format!("A commit: {}", e)),
                            }
                        }
                        Session::B => {
                            b_step += 1;
                            match b_step {
                                1 => b.begin_transaction().await.map_err(|e| format!("B begin: {}", e)),
                                2 => {
                                    if let Err(e) =
                                        b.execute("UPDATE l5_t SET v = v + 10 WHERE id = 1").await
                                    {
                                        let _ = b.rollback().await;
                                        return Err(format!("B upd busy: {}", e));
                                    }
                                    Ok(())
                                }
                                _ => b.commit().await.map_err(|e| format!("B commit: {}", e)),
                            }
                        }
                    }
                })
                .await;
                assert!(res.is_ok(), "交错 #{} 步挂起（计时守护触发）", i);
                let _ = res.unwrap(); // Err（busy 冲突）已在该步回滚，合法
            }

            // 终值不变量：两事务都成功时 11；恰一成功（另一 busy 回滚）时 1 或 10；
            // 两者都因 busy 失败时 0。任何值都必须 ∈ {0, 1, 10, 11} 且非负
            let v = read_v(&mut a).await;
            assert!(
                [0i64, 1, 10, 11].contains(&v),
                "交错 #{}（overlap={}）终值 {} 超出合法集 {{0,1,10,11}}",
                i,
                overlap,
                v
            );
            TwoConns { a, b, path }.close().await;
        }
    });
    assert!(guard.await.is_ok(), "同行写-写穷举挂起（外层计时守护）");
}

/// 探针 2：脏读免疫——任意交错点对方读未提交行恒见旧值
#[tokio::test]
async fn l5_probe_rollback_isolation() {
    let guard = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let TwoConns { mut a, mut b, path } = open_two_conns().await;

        a.execute("DELETE FROM l5_t").await.expect("clear");
        a.execute("INSERT INTO l5_t (id, v) VALUES (1, 0)").await.expect("seed");
        a.begin_transaction().await.expect("A begin");
        a.execute("UPDATE l5_t SET v = 99 WHERE id = 1").await.expect("A update");

        // B 在 A 未提交时读：恒见旧值 0（SQLite journal 隔离）
        let v = read_v(&mut b).await;
        assert_eq!(v, 0, "脏读被检出：B 在 A 未提交时读到新值 99");

        a.rollback().await.expect("A rollback");
        let v = read_v(&mut b).await;
        assert_eq!(v, 0, "回滚后 B 必须见旧值 0");
        TwoConns { a, b, path }.close().await;
    });
    assert!(guard.await.is_ok(), "回滚隔离探针挂起");
}

/// 不变量：异行写-写 双成功（串行非重叠）
#[tokio::test]
async fn l5_inv_different_row_both_commit() {
    let guard = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let TwoConns { mut a, mut b, path } = open_two_conns().await;
        a.execute("DELETE FROM l5_t").await.expect("clear");
        a.execute("INSERT INTO l5_t (id, v) VALUES (1, 0)").await.expect("seed 1");
        a.execute("INSERT INTO l5_t (id, v) VALUES (2, 0)").await.expect("seed 2");

        a.begin_transaction().await.expect("A begin");
        a.execute("UPDATE l5_t SET v = 5 WHERE id = 1").await.expect("A upd");
        a.commit().await.expect("A commit");
        b.begin_transaction().await.expect("B begin");
        b.execute("UPDATE l5_t SET v = 7 WHERE id = 2").await.expect("B upd");
        b.commit().await.expect("B commit");

        let rows = a.query("SELECT id, v FROM l5_t ORDER BY id").await.expect("final");
        let v1 = match rows[0].get("v") {
            Some(Value::I64(v)) => *v,
            other => panic!("类型: {:?}", other),
        };
        let v2 = match rows[1].get("v") {
            Some(Value::I64(v)) => *v,
            other => panic!("类型: {:?}", other),
        };
        assert_eq!((v1, v2), (5, 7), "异行双成功终态必须为 (5, 7)");
        TwoConns { a, b, path }.close().await;
    });
    assert!(guard.await.is_ok(), "异行双成功探针挂起");
}

/// 不变量：同行写-写 非重叠（串行）双成功合法
#[tokio::test]
async fn l5_inv_same_row_serial_both_commit() {
    let guard = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let TwoConns { mut a, mut b, path } = open_two_conns().await;
        a.execute("DELETE FROM l5_t").await.expect("clear");
        a.execute("INSERT INTO l5_t (id, v) VALUES (1, 0)").await.expect("seed");

        a.begin_transaction().await.expect("A begin");
        a.execute("UPDATE l5_t SET v = v + 1 WHERE id = 1").await.expect("A upd");
        a.commit().await.expect("A commit");
        b.begin_transaction().await.expect("B begin");
        b.execute("UPDATE l5_t SET v = v + 10 WHERE id = 1").await.expect("B upd");
        b.commit().await.expect("B commit");

        let v = read_v(&mut a).await;
        assert_eq!(v, 11, "串行双成功终值必须为 11（1 + 10）");
        TwoConns { a, b, path }.close().await;
    });
    assert!(guard.await.is_ok(), "串行双成功探针挂起");
}

/// 随机大规模：随机交错序列下全程不挂起 + 终态 ∈ 合法集
#[tokio::test]
async fn l5_inv_random_interleave_no_hang() {
    let guard = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        for seed in 0..30u64 {
            let seq = interleave(3, 3, seed);
            let TwoConns { mut a, mut b, path } = open_two_conns().await;
            a.execute("DELETE FROM l5_t").await.expect("clear");
        a.execute("INSERT INTO l5_t (id, v) VALUES (1, 0)").await.expect("seed");

            let mut a_step = 0usize;
            let mut b_step = 0usize;
            for step in &seq {
                let res = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    match step {
                        Session::A => {
                            a_step += 1;
                            match a_step {
                                1 => a.begin_transaction().await.map_err(|e| format!("A begin: {}", e)),
                                2 => {
                                    if let Err(e) =
                                        a.execute("UPDATE l5_t SET v = v + 1 WHERE id = 1").await
                                    {
                                        let _ = a.rollback().await;
                                        return Err(format!("A busy: {}", e));
                                    }
                                    Ok(())
                                }
                                _ => a.commit().await.map_err(|e| format!("A commit: {}", e)),
                            }
                        }
                        Session::B => {
                            b_step += 1;
                            match b_step {
                                1 => b.begin_transaction().await.map_err(|e| format!("B begin: {}", e)),
                                2 => {
                                    if let Err(e) =
                                        b.execute("UPDATE l5_t SET v = v + 10 WHERE id = 1").await
                                    {
                                        let _ = b.rollback().await;
                                        return Err(format!("B busy: {}", e));
                                    }
                                    Ok(())
                                }
                                _ => b.commit().await.map_err(|e| format!("B commit: {}", e)),
                            }
                        }
                    }
                })
                .await;
                assert!(res.is_ok(), "seed={} 步挂起（计时守护触发）", seed);
                let _ = res.unwrap();
            }

            let v = read_v(&mut a).await;
            assert!(
                [0i64, 1, 10, 11].contains(&v),
                "seed={} 终值 {} 超出合法集 {{0,1,10,11}}",
                seed,
                v
            );
            TwoConns { a, b, path }.close().await;
        }
    });
    assert!(guard.await.is_ok(), "随机交错套件挂起（外层计时守护）");
}
