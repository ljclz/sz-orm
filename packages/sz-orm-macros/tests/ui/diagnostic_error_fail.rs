//! 验证 diagnostic_error! 宏生成编译期错误
//!
//! 此文件应编译失败，trybuild 会验证错误信息。

use sz_orm_macros::diagnostic_error;

fn main() {
    diagnostic_error!("类型不匹配：列 `id` 期望 `i64`，但发现 `String`", "请使用 Cast 显式转换");
}