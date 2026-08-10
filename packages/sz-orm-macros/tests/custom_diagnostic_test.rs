//! M3-T5: 自定义诊断集成测试
//!
//! 验证 diagnostic_error! 宏和 type_check 属性宏在 custom-diagnostic feature 下正常工作。

#![cfg(feature = "custom-diagnostic")]

#[cfg(test)]
mod tests {
    use trybuild::TestCases;

    #[test]
    fn diagnostic_macro_tests_custom_feature() {
        let t = TestCases::new();
        t.compile_fail("tests/ui/diagnostic_error_fail.rs");
        t.pass("tests/ui/type_check_pass.rs");
    }
}
