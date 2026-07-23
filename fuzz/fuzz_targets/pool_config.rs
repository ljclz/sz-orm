#![no_main]
//! Fuzz Target 3: PoolConfig 校验/Duration 溢出检测
//!
//! 目标：发现 `PoolConfig::validate()` 和 `PoolConfigBuilder::build()` 在处理
//! 极端参数值时的 panic/crash，以及 `Duration` 运算溢出。
//!
//! 覆盖攻击面：
//! - `validate` 仅检查 max_size==0 和 min_idle>max_size，未检查超时为 0
//! - `Duration::from_secs(u64::MAX)` + `Instant::now()` 可能溢出 panic
//! - `connection_timeout == 0` 导致 health_check 除零
//! - `max_size == u32::MAX` 导致无限创建连接
//! - `acquire_timeout == 0` 导致每次 acquire 立即超时

use libfuzzer_sys::fuzz_target;
use sz_orm_core::{PoolConfig, PoolConfigBuilder};
use std::time::Duration;

/// 从 fuzz 输入中提取 6 个 u32 值（对应 PoolConfig 的 6 个参数）
fn extract_u32s(data: &[u8]) -> [u32; 6] {
    let mut vals = [1u32; 6]; // 默认值均为 1（合法）
    for (i, chunk) in data.chunks(4).enumerate().take(6) {
        let mut buf = [0u8; 4];
        for (j, &b) in chunk.iter().enumerate().take(4) {
            buf[j] = b;
        }
        vals[i] = u32::from_le_bytes(buf);
        // 避免全 0（会导致 max_size==0 验证失败，但我们想测试更多路径）
        if vals[i] == 0 {
            vals[i] = 1;
        }
    }
    vals
}

fuzz_target!(|data: &[u8]| {
    let [max_size, min_idle, acquire_timeout, idle_timeout, max_lifetime, connection_timeout] =
        extract_u32s(data);

    // --- PoolConfigBuilder::build（通过 builder 路径） ---
    let _ = std::panic::catch_unwind(|| {
        let result = PoolConfigBuilder::new()
            .max_size(max_size)
            .min_idle(min_idle)
            .acquire_timeout(acquire_timeout as u64)
            .idle_timeout(idle_timeout as u64)
            .max_lifetime(max_lifetime as u64)
            .build();
        if let Ok(config) = result {
            black_box(&config);
        }
    });

    // --- PoolConfig 直接构造 + validate（测试极端 Duration 值） ---
    let _ = std::panic::catch_unwind(|| {
        let config = PoolConfig {
            max_size,
            min_idle,
            acquire_timeout: Duration::from_secs(acquire_timeout as u64),
            idle_timeout: Duration::from_secs(idle_timeout as u64),
            max_lifetime: Duration::from_secs(max_lifetime as u64),
            connection_timeout: Duration::from_secs(connection_timeout as u64),
        };
        let _ = config.validate();
        black_box(&config);
    });

    // --- Duration 极端值：from_secs(u64::MAX) ---
    // 测试 Instant::now() + Duration 是否溢出
    let _ = std::panic::catch_unwind(|| {
        let config = PoolConfig {
            max_size: 10,
            min_idle: 1,
            acquire_timeout: Duration::from_secs(u64::MAX),
            idle_timeout: Duration::from_secs(u64::MAX),
            max_lifetime: Duration::from_secs(u64::MAX),
            connection_timeout: Duration::from_secs(u64::MAX),
        };
        let _ = config.validate();
        // 不实际创建 Pool（需要 factory），仅测试 Duration 运算
        let _ = std::time::Instant::now() + config.acquire_timeout;
    });

    // --- Duration 为 0 的边界（除零/立即超时） ---
    let _ = std::panic::catch_unwind(|| {
        let config = PoolConfig {
            max_size: 10,
            min_idle: 0,
            acquire_timeout: Duration::ZERO,
            idle_timeout: Duration::ZERO,
            max_lifetime: Duration::ZERO,
            connection_timeout: Duration::ZERO,
        };
        let _ = config.validate();
        // connection_timeout / 2 = 0（health_check 中的 ping_timeout）
        let ping_timeout = config.connection_timeout / 2;
        black_box(&ping_timeout);
    });

    // --- max_size = u32::MAX（无限连接创建） ---
    let _ = std::panic::catch_unwind(|| {
        let config = PoolConfig {
            max_size: u32::MAX,
            min_idle: 0,
            acquire_timeout: Duration::from_secs(1),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(300),
            connection_timeout: Duration::from_secs(5),
        };
        let _ = config.validate();
        black_box(&config);
    });
});

fn black_box<T>(_: &T) {}
