#![no_main]
#![allow(deprecated)]
//! Fuzz Target 1: SQL Query Builder 注入/溢出检测
//!
//! 目标：发现 `sz_orm_query_builder` 在处理任意字符串输入时的 panic/crash。
//!
//! 覆盖攻击面：
//! - `check_where_injection` 黑名单绕过（UNION SELECT、' OR '1'='1、# 注释等）
//! - `having`/`join on`/`value` 无校验直接拼接
//! - `paginate` 整数溢出（page * size）
//! - `quote_ident` 对空字符串/NULL 字节/超长标识符的处理
//! - `build()` 在不同 DbType 下的行为
//!
//! # 重要
//!
//! 不使用 `catch_unwind` 包装：libfuzzer 自身有 panic 信号处理机制，
//! 任何 panic 都会被正确报告为 crash。手动 catch_unwind 会让 libfuzzer 永远发现不了问题。

use libfuzzer_sys::fuzz_target;
use std::hint::black_box;
use sz_orm_core::DbType;
use sz_orm_query_builder::Query;

/// 将任意字节转换为 UTF-8 字符串（损失转换，保留非 UTF-8 字节为替换符）
fn bytes_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

/// 从 fuzz 输入中提取 3 个字符串段（用 0xFF 分隔）
fn split_input(data: &[u8]) -> (String, String, String) {
    let parts: Vec<&[u8]> = data.splitn(3, |&b| b == 0xFF).collect();
    let s1 = bytes_to_string(parts.first().copied().unwrap_or(&[]));
    let s2 = bytes_to_string(parts.get(1).copied().unwrap_or(&[]));
    let s3 = bytes_to_string(parts.get(2).copied().unwrap_or(&[]));
    (s1, s2, s3)
}

fuzz_target!(|data: &[u8]| {
    let (table, col, condition) = split_input(data);

    // DbType 轮换（根据首字节选择）
    let db_type = match data.first().copied().unwrap_or(0) % 4 {
        0 => DbType::MySQL,
        1 => DbType::PostgreSQL,
        2 => DbType::Sqlite,
        _ => DbType::Oracle,
    };

    // --- SELECT 查询构造（触发 check_where_injection / quote_ident） ---
    // 不使用 catch_unwind：panic 自然传播给 libfuzzer，由其报告为 crash
    let sql = Query::select()
        .column(&col)
        .from(&table)
        .where_clause(&condition)
        .having(&condition)
        .group_by(&col)
        .order_by(&col, true)
        .build(db_type);
    black_box(&sql);

    // --- SELECT with JOIN（join on 无校验） ---
    let sql = Query::select()
        .column(&col)
        .from(&table)
        .inner_join(&table, &condition)
        .build(db_type);
    black_box(&sql);

    // --- SELECT with paginate（整数溢出检测） ---
    let page = data
        .iter()
        .fold(1u64, |acc, &b| acc.wrapping_mul(b.max(1) as u64));
    let size = data
        .iter()
        .fold(10u64, |acc, &b| acc.wrapping_add(b as u64));
    let sql = Query::select()
        .column(&col)
        .from(&table)
        .paginate(page, size)
        .build(db_type);
    black_box(&sql);

    // --- INSERT 查询（value 无校验） ---
    let sql = Query::insert()
        .into_table(&table)
        .value(&col, &condition)
        .build();
    black_box(&sql);

    // --- UPDATE 查询 ---
    let sql = Query::update()
        .table(&table)
        .set(&col, &condition)
        .where_clause(&condition)
        .build();
    black_box(&sql);

    // --- DELETE 查询 ---
    let sql = Query::delete()
        .from_table(&table)
        .where_clause(&condition)
        .build();
    black_box(&sql);
});
