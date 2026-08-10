//! QueryBuilder 迁移 fix 模块测试
//!
//! 验证 `qb_migration_fix` 模块的 API 转换、dry-run 模式和复杂场景标注。

use sz_orm_core::qb_migration_fix::{fix_source, MigrationFix};

/// 测试 Query::select() 转换
#[test]
fn test_fix_select() {
    let source = r#"let q = Query::select().from("users");"#;
    let result = fix_source(source, true);
    assert!(!result.changes.is_empty(), "应有变更");
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.original.contains("Query::select()")),
        "应替换 Query::select()"
    );
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.replacement.contains("QueryBuilder")),
        "应替换为 QueryBuilder"
    );
}

/// 测试 Query::insert() 转换
#[test]
fn test_fix_insert() {
    let source = r#"let q = Query::insert().into_table("users");"#;
    let result = fix_source(source, true);
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.original == "Query::insert()"),
        "应替换 Query::insert()"
    );
}

/// 测试 Query::update() 转换
#[test]
fn test_fix_update() {
    let source = r#"let q = Query::update().table("users");"#;
    let result = fix_source(source, true);
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.original == "Query::update()"),
        "应替换 Query::update()"
    );
}

/// 测试 Query::delete() 转换
#[test]
fn test_fix_delete() {
    let source = r#"let q = Query::delete().from_table("users");"#;
    let result = fix_source(source, true);
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.original == "Query::delete()"),
        "应替换 Query::delete()"
    );
    assert!(
        result.changes.iter().any(|c| c.original == ".from_table("),
        "应替换 .from_table()"
    );
}

/// 测试完整路径 sz_orm_query_builder::Query::select() 转换
#[test]
fn test_fix_full_path_select() {
    let source = r#"let q = sz_orm_query_builder::Query::select();"#;
    let result = fix_source(source, false);
    assert!(
        result.fixed.contains("QueryBuilder"),
        "修复后应包含 QueryBuilder"
    );
    assert!(
        !result
            .fixed
            .contains("sz_orm_query_builder::Query::select()"),
        "修复后不应包含旧 API"
    );
}

/// 测试表名方法转换
#[test]
fn test_fix_table_methods() {
    let source = r#".from("users")"#;
    let result = fix_source(source, true);
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.original == ".from(" && c.replacement == ".table("),
        "应将 .from() 替换为 .table()"
    );
}

/// 测试 .into_table() 转换
#[test]
fn test_fix_into_table() {
    let source = r#".into_table("users")"#;
    let result = fix_source(source, true);
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.original == ".into_table(" && c.replacement == ".table("),
        "应将 .into_table() 替换为 .table()"
    );
}

/// 测试 .order_by("col", true) 转换
#[test]
fn test_fix_order_by_asc() {
    let source = r#".order_by("id", true)"#;
    let result = fix_source(source, true);
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.replacement.contains(".order_by(") && !c.replacement.contains("true")),
        "应将 .order_by(\"id\", true) 替换为 .order_by(\"id\")"
    );
}

/// 测试 .order_by("col", false) 转换
#[test]
fn test_fix_order_by_desc() {
    let source = r#".order_by("id", false)"#;
    let result = fix_source(source, true);
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.replacement.contains(".order_desc(")),
        "应将 .order_by(\"id\", false) 替换为 .order_desc(\"id\")"
    );
}

/// 测试 dry-run 模式不修改源码
#[test]
fn test_fix_dry_run_preserves_original() {
    let source = r#"let q = Query::select().from("users");"#;
    let result = fix_source(source, true);
    assert_eq!(
        result.original, result.fixed,
        "dry-run 模式下 fixed 应等于 original"
    );
    assert!(!result.changes.is_empty(), "dry-run 仍应生成变更列表");
}

/// 测试非 dry-run 模式修改源码
#[test]
fn test_fix_apply_modifies_source() {
    let source = r#"let q = Query::select().from("users");"#;
    let result = fix_source(source, false);
    assert_ne!(result.original, result.fixed, "非 dry-run 应修改源码");
    assert!(
        result.fixed.contains("QueryBuilder"),
        "修复后应包含 QueryBuilder"
    );
    assert!(
        !result.fixed.contains("Query::select()"),
        "修复后不应包含 Query::select()"
    );
}

/// 测试 .where_clause() 标注需人工审查
#[test]
fn test_fix_where_clause_needs_review() {
    let source = r#".where_clause("id = 1")"#;
    let result = fix_source(source, true);
    assert!(result.needs_review, "where_clause 应标注需审查");
    assert!(result.changes.iter().any(|c| !c.auto), "应有非自动变更");
}

/// 测试 .column() 标注需人工审查
#[test]
fn test_fix_column_needs_review() {
    let source = r#".column("id")"#;
    let result = fix_source(source, true);
    assert!(result.needs_review, "column 应标注需审查");
}

/// 测试 UNION 标注需人工审查
#[test]
fn test_fix_union_needs_review() {
    let source = r#"Query::select().union(other)"#;
    let result = fix_source(source, true);
    assert!(result.needs_review, "UNION 应标注需审查");
}

/// 测试 CTE 标注需人工审查
#[test]
fn test_fix_cte_needs_review() {
    let source = r#"Query::select().with_cte("t", "SELECT 1")"#;
    let result = fix_source(source, true);
    assert!(result.needs_review, "CTE 应标注需审查");
}

/// 测试 UNION ALL 标注需人工审查
#[test]
fn test_fix_union_all_needs_review() {
    let source = r#"Query::select().union_all(other)"#;
    let result = fix_source(source, true);
    assert!(result.needs_review, "UNION ALL 应标注需审查");
}

/// 测试递归 CTE 标注需人工审查
#[test]
fn test_fix_recursive_cte_needs_review() {
    let source = r#"Query::select().with_recursive_cte("t", "SELECT 1")"#;
    let result = fix_source(source, true);
    assert!(result.needs_review, "递归 CTE 应标注需审查");
}

/// 测试 FixResult::diff() 输出
#[test]
fn test_fix_result_diff() {
    let source = r#"Query::select()"#;
    let result = fix_source(source, true);
    let diff = result.diff();
    assert!(!diff.is_empty(), "diff 应非空");
    assert!(diff.contains("L1"), "diff 应包含行号");
}

/// 测试 MigrationFix 结构体用法
#[test]
fn test_migration_fix_struct() {
    let fixer = MigrationFix::new(true);
    let source = r#"Query::select()"#;
    let result = fixer.fix(source);
    assert!(!result.changes.is_empty(), "应生成变更");
    assert!(fixer.dry_run, "dry_run 应为 true");
}

/// 测试无旧版 API 的源码
#[test]
fn test_fix_clean_source() {
    let source = r#"let q = QueryBuilder::<Model>::new(dialect).table("users");"#;
    let result = fix_source(source, false);
    assert!(result.changes.is_empty(), "新版 API 应无变更");
    assert!(!result.needs_review, "新版 API 不需审查");
}

/// 测试空源码
#[test]
fn test_fix_empty_source() {
    let result = fix_source("", false);
    assert!(result.changes.is_empty(), "空源码应无变更");
    assert!(!result.needs_review, "空源码不需审查");
}

/// 测试多行源码综合转换
#[test]
fn test_fix_multiline_source() {
    let source = r#"let q = Query::select()
    .from("users")
    .order_by("id", true)
    .limit(10);"#;
    let result = fix_source(source, false);
    assert!(
        result.fixed.contains("QueryBuilder"),
        "应替换 Query::select()"
    );
    assert!(result.fixed.contains(".table(\"users\")"), "应替换 .from()");
    assert!(
        result.fixed.contains(".order_by(\"id\")"),
        "应替换 .order_by(\"id\", true)"
    );
    assert!(result.fixed.contains(".limit(10)"), "limit 应保持不变");
}

/// 测试 dry-run 保留行尾换行
#[test]
fn test_fix_preserves_trailing_newline() {
    let source = "Query::select()\n";
    let result = fix_source(source, false);
    assert!(result.fixed.ends_with('\n'), "应保留行尾换行");
}
