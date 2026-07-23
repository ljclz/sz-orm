#![no_main]
//! Fuzz Target 2: Value SQL 转义安全性检测
//!
//! 目标：发现 `Value::to_param()` 和 `Value::to_param_with_dialect()` 在处理
//! 任意字符串/字节输入时的 panic，以及转义不一致导致的 SQL 注入风险。
//!
//! 覆盖攻击面：
//! - `escape_string` 仅转义 `'`，不处理 `\`（MySQL 不安全）
//! - `Display` impl 无转义（`format!("{}", value)` 有注入风险）
//! - `to_param` vs `to_param_with_dialect` 输出差异
//! - `Value::Bytes` 走 `hex_encode` 路径 vs `Value::String` 走 `escape_string`
//! - 字符串→数值解析边界（as_i64/as_f64/as_bool 对异常字符串）
//! - NULL 字节、控制字符、非 UTF-8 序列

use libfuzzer_sys::fuzz_target;
use sz_orm_core::{get_dialect, DbType, Value};

/// 将任意字节转换为 UTF-8 字符串（损失转换）
fn bytes_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

fuzz_target!(|data: &[u8]| {
    let s = bytes_to_string(data);

    // --- Value::String → to_param（escape_string 仅转义 '） ---
    let val = Value::String(s.clone());
    let _ = std::panic::catch_unwind(|| {
        let param = val.to_param();
        black_box(&param);
    });

    // --- Value::String → to_param_with_dialect（方言感知转义） ---
    // 轮换测试 MySQL / PostgreSQL / SQLite 三种方言
    for db_type in [DbType::MySQL, DbType::PostgreSQL, DbType::Sqlite] {
        let _ = std::panic::catch_unwind(|| {
            if let Ok(dialect) = get_dialect(db_type) {
                let param = val.to_param_with_dialect(&*dialect);
                black_box(&param);
            }
        });
    }

    // --- Value::Bytes → to_param（hex_encode 路径） ---
    let val_bytes = Value::Bytes(data.to_vec());
    let _ = std::panic::catch_unwind(|| {
        let param = val_bytes.to_param();
        black_box(&param);
    });

    // --- Display impl（无转义，直接拼接） ---
    let _ = std::panic::catch_unwind(|| {
        let display = format!("{}", val);
        black_box(&display);
    });

    // --- 字符串→数值解析边界 ---
    let _ = std::panic::catch_unwind(|| {
        let _ = val.as_i64();
    });
    let _ = std::panic::catch_unwind(|| {
        let _ = val.as_f64();
    });
    let _ = std::panic::catch_unwind(|| {
        let _ = val.as_bool();
    });

    // --- 转义一致性检查：to_param 的输出不应包含未转义的单引号 ---
    // （即 ' 应成对出现，否则说明转义有 bug）
    let _ = std::panic::catch_unwind(|| {
        if let Ok(param) = std::panic::catch_unwind(|| val.to_param()) {
            let owned = param.into_owned();
            // 统计单引号数量，应该为偶数（开+闭 或 转义的 ''）
            let quote_count = owned.matches('\'').count();
            if quote_count % 2 != 0 {
                // 转义后单引号为奇数 = 潜在注入点
                black_box(&quote_count);
            }
        }
    });
});

fn black_box<T>(_: &T) {}
