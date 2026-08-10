//! QueryBuilder 迁移 lint 模块测试
//!
//! 验证 `qb_migration_lint` 模块的精确匹配能力和告警格式。

use sz_orm_core::qb_migration_lint::{lint_source, LintWarning, MigrationLint};

/// 测试 use 语句精确匹配 `sz_orm_query_builder::Query`
#[test]
fn test_lint_use_statement() {
    let source = "use sz_orm_query_builder::Query;";
    let warnings = lint_source(source);
    assert_eq!(warnings.len(), 1, "应检测到 1 个 use 语句告警");
    assert!(
        warnings[0].message.contains("sz_orm_query_builder::Query"),
        "告警消息应包含旧版路径"
    );
    assert!(
        warnings[0].message.contains("sz_orm_core::QueryBuilder"),
        "告警消息应包含新版路径"
    );
}

/// 测试 use group 形式 `use sz_orm_query_builder::{Query, SelectQuery};`
#[test]
fn test_lint_use_group() {
    let source = "use sz_orm_query_builder::{Query, SelectQuery};";
    let warnings = lint_source(source);
    assert_eq!(warnings.len(), 1, "应只检测到 Query，不检测 SelectQuery");
    assert!(
        warnings[0].message.contains("sz_orm_query_builder::Query"),
        "告警应针对 Query 类型"
    );
}

/// 测试 use rename 形式 `use sz_orm_query_builder::Query as OldQuery;`
#[test]
fn test_lint_use_rename() {
    let source = "use sz_orm_query_builder::Query as OldQuery;";
    let warnings = lint_source(source);
    assert_eq!(warnings.len(), 1, "应检测到 rename 导入");
    assert!(
        warnings[0].suggestion.contains("sz_orm_core::QueryBuilder"),
        "建议应指向新版 QueryBuilder"
    );
}

/// 测试完整路径方法调用 `sz_orm_query_builder::Query::select()`
#[test]
fn test_lint_full_path_method_call() {
    let source = r#"fn build() { let q = sz_orm_query_builder::Query::select().from("users"); }"#;
    let warnings = lint_source(source);
    assert!(!warnings.is_empty(), "应检测到完整路径调用");
    assert!(
        warnings[0].message.contains("sz_orm_query_builder::Query"),
        "告警应包含完整路径"
    );
}

/// 测试完整路径类型引用
#[test]
fn test_lint_full_path_type_reference() {
    let source = "fn foo() -> sz_orm_query_builder::Query { todo!() }";
    let warnings = lint_source(source);
    assert!(!warnings.is_empty(), "应检测到类型引用");
}

/// 测试不误报其他库的 Query
#[test]
fn test_lint_no_false_positive_other_lib() {
    let source = r#"use other_lib::Query;
fn build() { let q = Query::select(); }"#;
    let warnings = lint_source(source);
    assert!(warnings.is_empty(), "不应误报 other_lib::Query");
}

/// 测试不误报 std 路径
#[test]
fn test_lint_no_false_positive_std() {
    let source = "use std::Query;";
    let warnings = lint_source(source);
    assert!(warnings.is_empty(), "不应误报 std::Query");
}

/// 测试不误报同名但不同 crate 的路径
#[test]
fn test_lint_no_false_positive_similar_crate() {
    let source = "use sz_orm_query_builderx::Query;";
    let warnings = lint_source(source);
    assert!(warnings.is_empty(), "不应误报 sz_orm_query_builderx");
}

/// 测试告警格式
#[test]
fn test_warning_format() {
    let warning = LintWarning {
        file: "test.rs".to_string(),
        line: 10,
        col: 5,
        message: "sz_orm_query_builder::Query 已废弃".to_string(),
        suggestion: "使用 sz_orm_core::QueryBuilder".to_string(),
    };
    let formatted = warning.format();
    assert!(formatted.contains("warning:"), "格式应包含 warning:");
    assert!(formatted.contains("[test.rs:10:5]"), "格式应包含位置");
    assert!(formatted.contains("suggestion:"), "格式应包含 suggestion");
}

/// 测试 MigrationLint 结构体用法
#[test]
fn test_migration_lint_struct() {
    let mut lint = MigrationLint::new();
    let source = "use sz_orm_query_builder::Query;";
    let warnings = lint.lint(source);
    assert_eq!(warnings.len(), 1);
    assert_eq!(lint.warnings().len(), 1);
}

/// 测试空源码
#[test]
fn test_lint_empty_source() {
    let warnings = lint_source("");
    assert!(warnings.is_empty(), "空源码应无告警");
}

/// 测试无旧版 API 的源码
#[test]
fn test_lint_clean_source() {
    let source = r#"use sz_orm_core::QueryBuilder;
fn build() { let q = QueryBuilder::<Model>::new(dialect); }"#;
    let warnings = lint_source(source);
    assert!(warnings.is_empty(), "新版 API 应无告警");
}

/// 测试语法错误的源码返回空列表
#[test]
fn test_lint_invalid_syntax() {
    let source = "this is not valid rust code !!!";
    let warnings = lint_source(source);
    assert!(warnings.is_empty(), "语法错误应返回空列表");
}

/// 测试多行源码中多个告警
#[test]
fn test_lint_multiple_warnings() {
    let source = r#"
use sz_orm_query_builder::Query;

fn build() -> sz_orm_query_builder::Query {
    Query::select()
}
"#;
    let warnings = lint_source(source);
    assert!(
        warnings.len() >= 2,
        "应检测到至少 2 个告警（use 语句 + 类型引用），实际: {}",
        warnings.len()
    );
}

/// 测试告警的行号和列号有效
#[test]
fn test_lint_warning_position_valid() {
    let source = "use sz_orm_query_builder::Query;";
    let warnings = lint_source(source);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].line >= 1, "行号应 >= 1");
    assert!(warnings[0].col >= 1, "列号应 >= 1");
}
