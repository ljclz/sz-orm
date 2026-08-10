//! 验证 type_check 属性宏正确处理有效代码
//!
//! 此文件应编译通过，trybuild 会验证无错误。

use sz_orm_macros::type_check;

#[type_check]
fn valid_typed_query() -> i64 {
    42
}

fn main() {
    let _ = valid_typed_query();
}