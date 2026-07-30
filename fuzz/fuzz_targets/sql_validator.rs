#![no_main]
//! Fuzz Target 4: SQL Validator 注入检测/语法校验
//!
//! 目标：发现 `sz_orm_sql_validator` 在处理任意 SQL 字符串时的 panic/crash，
//! 以及注入检测绕过漏洞。
//!
//! 覆盖攻击面：
//! - `validate_sql` 对畸形 SQL 的处理（空字符串、超长字符串、非 UTF-8）
//! - `validate_select/insert/update/delete` 分支覆盖
//! - `detect_statement_type` 对非标准 SQL 的识别
//! - `validate_parameter_count` 对 `?` 占位符计数的边界
//! - SQL 注入模式检测绕过（编码绕过、注释绕过、大小写绕过）
//! - 字符串字面量内的关键字误判

use libfuzzer_sys::fuzz_target;
use sz_orm_sql_validator::{
    detect_statement_type, validate_delete, validate_insert, validate_parameter_count,
    validate_select, validate_sql, validate_update,
};
use std::hint::black_box;

/// 将任意字节转换为 UTF-8 字符串（损失转换）
fn bytes_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

fuzz_target!(|data: &[u8]| {
    let sql = bytes_to_string(data);

    // --- validate_sql：综合校验（不应 panic） ---
    let result = validate_sql(&sql);
    black_box(&result);

    // --- 按语句类型分别校验 ---
    let result = validate_select(&sql);
    black_box(&result);

    let result = validate_insert(&sql);
    black_box(&result);

    let result = validate_update(&sql);
    black_box(&result);

    let result = validate_delete(&sql);
    black_box(&result);

    // --- detect_statement_type：识别 SQL 类型 ---
    let stmt_type = detect_statement_type(&sql);
    black_box(&stmt_type);

    // --- validate_parameter_count：? 占位符计数 ---
    // 从 fuzz 输入中提取期望参数数（前 4 字节）
    let expected = data.chunks(4).next().map(|c| {
        let mut buf = [0u8; 4];
        for (i, &b) in c.iter().enumerate().take(4) {
            buf[i] = b;
        }
        u32::from_le_bytes(buf) as usize
    }).unwrap_or(0);
    let result = validate_parameter_count(&sql, expected);
    black_box(&result);

    // --- 注入检测绕过测试 ---
    // 常见绕过手法：大小写混合、URL 编码、注释、嵌套
    let injection_patterns = [
        "1' OR '1'='1",
        "1; DROP TABLE users",
        "1' UNION SELECT * FROM users--",
        "1'/**/UNION/**/SELECT/**/NULL--",
        "admin'--",
        "admin'/*",
        "1' OR 1=1#",
        "0x73656c656374", // hex 编码的 "select"
    ];
    for pattern in &injection_patterns {
        let combined = format!("{} {}", sql, pattern);
        let result = validate_sql(&combined);
        black_box(&result);
    }
});
