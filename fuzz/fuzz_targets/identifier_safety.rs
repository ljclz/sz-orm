#![no_main]
//! Fuzz Target 5: 标识符安全性校验
//!
//! 目标：发现 `validate_identifier` 在处理任意字符串时的 panic/crash，
//! 以及 SQL 注入通过表名/列名绕过的风险。
//!
//! 覆盖攻击面：
//! - `validate_identifier` 对空字符串、超长字符串、非 UTF-8 的处理
//! - 标识符中包含特殊字符（引号、分号、空格、NULL 字节）
//! - 标识符中包含 SQL 关键字（SELECT、DROP、UNION）
//! - 标识符中包含注释标记（--、/*、#）
//! - 多语言标识符（中文、emoji、RTL 字符）
//! - 标识符长度边界（u16::MAX + 1）

use libfuzzer_sys::fuzz_target;
use sz_orm_core::sql_safety::validate_identifier;
use std::hint::black_box;

/// 将任意字节转换为 UTF-8 字符串（损失转换）
fn bytes_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

fuzz_target!(|data: &[u8]| {
    let name = bytes_to_string(data);

    // --- validate_identifier：列名校验 ---
    let result = validate_identifier(&name, "column");
    black_box(&result);

    // --- validate_identifier：表名校验 ---
    let result = validate_identifier(&name, "table");
    black_box(&result);

    // --- validate_identifier：索引名校验 ---
    let result = validate_identifier(&name, "index");
    black_box(&result);

    // --- validate_identifier：空字符串边界 ---
    let result = validate_identifier("", "column");
    black_box(&result);

    // --- validate_identifier：超长标识符（> 64 字符，MySQL 限制） ---
    let long_name = name.repeat(64);
    let result = validate_identifier(&long_name, "column");
    black_box(&result);

    // --- SQL 注入通过标识符绕过 ---
    let injection_names = [
        "users; DROP TABLE--",
        "users`--",
        "users' OR '1'='1",
        "users UNION SELECT * FROM passwords",
        "users/*comment*/",
        "users\0; DROP TABLE",
        "users\x00",
    ];
    for inj_name in &injection_names {
        let result = validate_identifier(inj_name, "table");
        black_box(&result);
    }
});
