//! trybuild 编译测试：验证 #[derive(Relation)] 宏生成的代码能通过编译

#[test]
fn relation_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/relation_pass/*.rs");
}

#[test]
fn relation_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/relation_fail/*.rs");
}
