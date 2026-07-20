//! Soak Test 测试入口
//!
//! 长时间中等压力持续运行，检测内存泄漏、连接池耗尽、句柄泄漏、慢退化等问题。
//!
//! # 运行方式
//!
//! ## CI 快速验证（默认 60s）
//! ```bash
//! cargo test --test soak -- --ignored --nocapture
//! ```
//!
//! ## 周末长时验证（24h）
//! ```bash
//! SOAK_DURATION=24h cargo test --test soak -- --ignored --nocapture
//! ```
//!
//! ## 自定义时长
//! ```bash
//! SOAK_DURATION=5m cargo test --test soak -- --ignored --nocapture
//! SOAK_DURATION=2h cargo test --test soak -- --ignored --nocapture
//! ```
//!
//! 说明：Rust test harness 会拦截自定义参数，因此通过 `SOAK_DURATION` 环境变量传递时长。
//!
//! # 监控指标
//!
//! - RSS / fd_count / pool_active / pool_idle
//! - ops_per_sec / p99_latency_us / error_count
//!
//! # 退化检测
//!
//! - RSS 增长 > 50MB → 内存泄漏
//! - fd_count 增长 > 10 → 句柄泄漏
//! - 终态 pool_active != pool_idle → 连接泄漏
//! - ops_per_sec 衰减 > 10% → 性能退化
//! - p99_latency 增长 > 2x → 慢退化
//! - error_count > 0 → 偶发错误

#![cfg(test)]

mod common;

use common::soak::{parse_duration_from_args, record_latency, SoakMonitor, SoakRegression};
use common::MockConnectionFactory;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sz_orm_core::{Pool, PoolConfigBuilder};

/// 主 Soak Test：长时间运行 Pool acquire/release + Mock 查询
///
/// 默认 60 秒（CI 验证），可通过 `--soak-duration` 参数延长至 24h（周末任务）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "soak test 需显式 --ignored 启动；默认 60s，周末任务 --soak-duration=24h"]
async fn soak_pool_long_running_steady_state() {
    let duration = parse_duration_from_args();
    let sample_interval = Duration::from_secs(if duration.as_secs() >= 3600 {
        60 // ≥1h 时每分钟采样
    } else {
        5 // 短时测试每 5s 采样
    });

    eprintln!(
        "[soak] 启动：duration={:?}, sample_interval={:?}",
        duration, sample_interval
    );

    // 构造 Pool：max_size=20, min_idle=5
    let db = Arc::new(tokio::sync::Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(20)
        .min_idle(5)
        .acquire_timeout(5)
        .idle_timeout(60)
        .max_lifetime(300)
        .build()
        .expect("PoolConfig invalid");
    let pool = Arc::new(Pool::new(config, factory).expect("Pool::new failed"));

    // 创建 SoakMonitor
    let mut monitor = SoakMonitor::new(duration, sample_interval);
    let ops_counter = monitor.ops_counter();
    let errors_counter = monitor.errors_counter();
    let latency_window = monitor.latency_window();

    // 工作线程数：8 个并发 worker
    const WORKER_COUNT: usize = 8;
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut workers = Vec::new();

    for worker_id in 0..WORKER_COUNT {
        let pool_clone = pool.clone();
        let ops_clone = ops_counter.clone();
        let errors_clone = errors_counter.clone();
        let latency_clone = latency_window.clone();
        let stop_clone = stop_flag.clone();

        workers.push(tokio::spawn(async move {
            while !stop_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let t0 = Instant::now();
                // 模拟一次完整的 acquire -> execute -> release 周期
                match pool_clone.acquire().await {
                    Ok(conn) => {
                        // 模拟工作（让出调度器，模拟真实查询耗时）
                        tokio::task::yield_now().await;
                        // 显式 release（PooledConnection 无 Drop 自动归还）
                        pool_clone.release(conn).await;
                        let elapsed_us = t0.elapsed().as_micros() as u64;
                        record_latency(&latency_clone, elapsed_us);
                        ops_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        errors_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        eprintln!("[soak worker {}] acquire error: {}", worker_id, e);
                        // 短暂退避，避免错误循环
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        }));
    }

    // 主线程：周期性采样
    while !monitor.is_finished() {
        tokio::time::sleep(sample_interval).await;
        let status = pool.status().await;
        let snap = monitor.snapshot((status.idle, status.active, status.max));
        eprintln!(
            "[soak] t={}s ops={} ops/s={:.1} pool(idle={},active={},max={}) rss={}MB fd={} p99={}us errors={}",
            snap.elapsed_secs,
            snap.ops_completed,
            snap.ops_per_sec,
            snap.pool_idle,
            snap.pool_active,
            snap.pool_max,
            snap.rss_bytes / 1024 / 1024,
            snap.fd_count,
            snap.p99_latency_us,
            snap.error_count,
        );
    }

    // 停止工作线程
    stop_flag.store(true, std::sync::atomic::Ordering::Release);
    for w in workers {
        let _ = w.await;
    }

    // 等待所有连接 release 完成
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 采集最终快照
    let final_status = pool.status().await;
    let final_snap = monitor.snapshot((final_status.idle, final_status.active, final_status.max));
    eprintln!(
        "[soak] 完成：总操作 {} 次，错误 {} 次，最终 pool(idle={},active={})",
        final_snap.ops_completed,
        final_snap.error_count,
        final_snap.pool_idle,
        final_snap.pool_active,
    );

    // 导出 CSV 报告
    // cargo test 工作目录是包目录（packages/sz-orm-core），target 在 workspace 根
    let csv_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("soak-report.csv");
    let csv_path_str = csv_path.to_str().expect("CSV path not UTF-8");
    if let Err(e) = monitor.export_csv(csv_path_str) {
        eprintln!("[soak] CSV 导出失败: {}", e);
    } else {
        eprintln!("[soak] CSV 报告已导出: {}", csv_path_str);
    }

    // 退化检测
    let regressions = monitor.detect_regressions();
    if regressions.is_empty() {
        eprintln!("[soak] ✅ 未检测到退化");
    } else {
        eprintln!("[soak] ⚠ 检测到 {} 项退化：", regressions.len());
        for r in &regressions {
            eprintln!("  - {}", r);
        }
    }

    // 关闭连接池
    pool.close_all().await;

    // 断言：无任何退化（CI 验证标准）
    // 周末任务若发现退化，应在此失败并打印详细信息
    assert!(
        regressions.is_empty(),
        "Soak test 检测到退化：{}",
        regressions
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    );
}

/// 短时 Soak 冒烟测试（10s）：验证 Soak 框架自身正确
///
/// 不需要 --ignored，每次 commit 都运行。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn soak_smoke_10s() {
    let duration = Duration::from_secs(10);
    let sample_interval = Duration::from_secs(2);

    let db = Arc::new(tokio::sync::Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(2)
        .build()
        .expect("PoolConfig invalid");
    let pool = Arc::new(Pool::new(config, factory).expect("Pool::new failed"));

    let mut monitor = SoakMonitor::new(duration, sample_interval);
    let ops = monitor.ops_counter();
    let errors = monitor.errors_counter();
    let latency = monitor.latency_window();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut workers = Vec::new();
    for _ in 0..4 {
        let pool_c = pool.clone();
        let ops_c = ops.clone();
        let err_c = errors.clone();
        let lat_c = latency.clone();
        let stop_c = stop.clone();
        workers.push(tokio::spawn(async move {
            while !stop_c.load(std::sync::atomic::Ordering::Relaxed) {
                let t0 = Instant::now();
                match pool_c.acquire().await {
                    Ok(conn) => {
                        // 模拟短暂查询耗时（让出调度器）
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        // 显式 release（PooledConnection 无 Drop 自动归还）
                        pool_c.release(conn).await;
                        record_latency(&lat_c, t0.elapsed().as_micros() as u64);
                        ops_c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(_) => {
                        err_c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    while !monitor.is_finished() {
        tokio::time::sleep(sample_interval).await;
        let s = pool.status().await;
        let snap = monitor.snapshot((s.idle, s.active, s.max));
        eprintln!(
            "[soak-smoke] t={}s ops={} pool(idle={},active={})",
            snap.elapsed_secs, snap.ops_completed, snap.pool_idle, snap.pool_active
        );
    }

    stop.store(true, std::sync::atomic::Ordering::Release);
    for w in workers {
        let _ = w.await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    let final_s = pool.status().await;
    monitor.snapshot((final_s.idle, final_s.active, final_s.max));
    pool.close_all().await;

    // 冒烟测试要求：至少完成 50 次操作，且无 PoolLeak
    // 10s mock 模式下 4 worker × 1ms/ops ≈ 4000 次理论上限，
    // 但 worker_threads=2 + acquire/release 锁竞争，实际可能远低于理论值，
    // 设 50 为下限以容忍调度抖动。
    assert!(
        monitor.snapshots().last().unwrap().ops_completed >= 50,
        "Soak smoke 应至少完成 50 次操作"
    );

    let regressions = monitor.detect_regressions();
    // 短时测试允许 RSS 微小波动，但 pool_active 必须等于 pool_idle
    let critical: Vec<&SoakRegression> = regressions
        .iter()
        .filter(|r| matches!(r, SoakRegression::PoolLeak { .. }))
        .collect();
    assert!(
        critical.is_empty(),
        "Soak smoke 不应有 PoolLeak：{:?}",
        critical
    );
}
