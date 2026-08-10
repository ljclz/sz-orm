//! M1-T4.3: 自定义编译期诊断信息测试
//!
//! 使用 trybuild 验证：
//! 1. diagnostic_error! 宏生成带 help 建议的编译期错误
//! 2. type_check 属性宏正确处理有效代码

#[cfg(test)]
mod tests {
    use trybuild::TestCases;

    #[test]
    fn diagnostic_macro_tests() {
        let t = TestCases::new();
        // 验证 diagnostic_error! 宏生成正确的编译期错误
        t.compile_fail("tests/ui/diagnostic_error_fail.rs");
        // 验证 type_check 属性宏正确处理有效代码
        t.pass("tests/ui/type_check_pass.rs");
    }
}
