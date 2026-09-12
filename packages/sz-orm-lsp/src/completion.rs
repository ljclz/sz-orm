//! # 上下文感知补全引擎（v6.9.0 REQ-DEV-001）
//!
//! 根据光标位置上下文返回合法补全项：
//! - `where_` 后仅返回 where_eq / where_gt / where_ne / where_like
//! - `select_` 后仅返回 select_columns
//! - Model 名后仅返回字段名

use crate::server::CompletionItem;

/// 补全上下文类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionContext {
    /// 顶层上下文（无前驱 token）
    TopLevel,
    /// `where_` 后的 WHERE 条件方法
    WhereClause,
    /// `select_` 后的 SELECT 方法
    SelectClause,
    /// Model 名后的字段访问
    ModelField,
    /// 查询构建器方法
    QueryBuilder,
}

/// 分析光标位置上下文
pub fn analyze_context(line: &str, character: u32) -> CompletionContext {
    let prefix = &line[..(character as usize).min(line.len())];

    let last_token: String = prefix
        .rsplit(|c: char| c.is_whitespace() || c == '.')
        .next()
        .unwrap_or("")
        .to_string();

    let trimmed = prefix.trim_end();

    if trimmed.ends_with("where_") || trimmed.ends_with("where.") {
        CompletionContext::WhereClause
    } else if trimmed.ends_with("select_") || trimmed.ends_with("select.") {
        CompletionContext::SelectClause
    } else if last_token.ends_with("_model") || last_token.ends_with("Model") {
        CompletionContext::ModelField
    } else if prefix.contains("query") || prefix.contains("Query") {
        CompletionContext::QueryBuilder
    } else {
        CompletionContext::TopLevel
    }
}

/// WHERE 条件方法
pub fn where_clause_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "where_eq".to_string(),
            kind: 2,
            detail: Some("where_eq(col, value) — 参数化等值查询".to_string()),
            documentation: Some("参数化等值查询，防止 SQL 注入".to_string()),
            insert_text: Some("where_eq(\"${1:column}\", ${2:value})".to_string()),
        },
        CompletionItem {
            label: "where_gt".to_string(),
            kind: 2,
            detail: Some("where_gt(col, value) — 大于条件".to_string()),
            documentation: Some("参数化大于查询".to_string()),
            insert_text: Some("where_gt(\"${1:column}\", ${2:value})".to_string()),
        },
        CompletionItem {
            label: "where_ne".to_string(),
            kind: 2,
            detail: Some("where_ne(col, value) — 不等条件".to_string()),
            documentation: Some("参数化不等查询".to_string()),
            insert_text: Some("where_ne(\"${1:column}\", ${2:value})".to_string()),
        },
        CompletionItem {
            label: "where_like".to_string(),
            kind: 2,
            detail: Some("where_like(col, pattern) — LIKE 模式匹配".to_string()),
            documentation: Some("参数化 LIKE 查询".to_string()),
            insert_text: Some("where_like(\"${1:column}\", \"${2:pattern}\")".to_string()),
        },
        CompletionItem {
            label: "where_in".to_string(),
            kind: 2,
            detail: Some("where_in(col, values) — IN 范围查询".to_string()),
            documentation: Some("参数化 IN 查询".to_string()),
            insert_text: Some("where_in(\"${1:column}\", ${2:values})".to_string()),
        },
    ]
}

/// SELECT 方法
pub fn select_clause_completions() -> Vec<CompletionItem> {
    vec![CompletionItem {
        label: "select_columns".to_string(),
        kind: 2,
        detail: Some("select_columns(cols) — 指定查询列".to_string()),
        documentation: Some("指定查询列，避免 SELECT *".to_string()),
        insert_text: Some("select_columns(&[${1:columns}])".to_string()),
    }]
}

/// 查询构建器方法
pub fn query_builder_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "table".to_string(),
            kind: 2,
            detail: Some("table(name) — 设置表名".to_string()),
            documentation: Some("设置查询目标表".to_string()),
            insert_text: Some("table(\"${1:name}\")".to_string()),
        },
        CompletionItem {
            label: "where_eq".to_string(),
            kind: 2,
            detail: Some("where_eq(col, value) — 等值条件".to_string()),
            documentation: Some("参数化等值查询".to_string()),
            insert_text: Some("where_eq(\"${1:column}\", ${2:value})".to_string()),
        },
        CompletionItem {
            label: "order_by".to_string(),
            kind: 2,
            detail: Some("order_by(col, desc) — 排序".to_string()),
            documentation: Some("设置排序".to_string()),
            insert_text: Some("order_by(\"${1:column}\", ${2:false})".to_string()),
        },
        CompletionItem {
            label: "limit".to_string(),
            kind: 2,
            detail: Some("limit(n) — 限制行数".to_string()),
            documentation: Some("设置返回行数上限".to_string()),
            insert_text: Some("limit(${1:n})".to_string()),
        },
        CompletionItem {
            label: "build_select".to_string(),
            kind: 2,
            detail: Some("build_select() — 构建 SELECT SQL".to_string()),
            documentation: Some("构建最终 SELECT 语句".to_string()),
            insert_text: Some("build_select()".to_string()),
        },
    ]
}

/// 根据上下文生成补全列表
pub fn context_aware_completion(line: &str, character: u32) -> Vec<CompletionItem> {
    match analyze_context(line, character) {
        CompletionContext::WhereClause => where_clause_completions(),
        CompletionContext::SelectClause => select_clause_completions(),
        CompletionContext::QueryBuilder => query_builder_completions(),
        CompletionContext::ModelField => vec![],
        CompletionContext::TopLevel => query_builder_completions(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_context_where() {
        let ctx = analyze_context("query.where_", 12);
        assert_eq!(ctx, CompletionContext::WhereClause);
    }

    #[test]
    fn test_analyze_context_select() {
        let ctx = analyze_context("query.select_", 13);
        assert_eq!(ctx, CompletionContext::SelectClause);
    }

    #[test]
    fn test_analyze_context_query_builder() {
        let ctx = analyze_context("query.", 6);
        assert_eq!(ctx, CompletionContext::QueryBuilder);
    }

    #[test]
    fn test_analyze_context_top_level() {
        let ctx = analyze_context("", 0);
        assert_eq!(ctx, CompletionContext::TopLevel);
    }

    #[test]
    fn test_where_clause_completions() {
        let items = where_clause_completions();
        assert!(items.iter().any(|i| i.label == "where_eq"));
        assert!(items.iter().any(|i| i.label == "where_gt"));
        assert!(items.iter().any(|i| i.label == "where_ne"));
        assert!(items.iter().any(|i| i.label == "where_like"));
        assert!(!items.iter().any(|i| i.label == "select_columns"));
    }

    #[test]
    fn test_select_clause_completions() {
        let items = select_clause_completions();
        assert!(items.iter().any(|i| i.label == "select_columns"));
        assert!(!items.iter().any(|i| i.label == "where_eq"));
    }

    #[test]
    fn test_context_aware_completion_where() {
        let items = context_aware_completion("query.where_", 12);
        assert!(items.iter().any(|i| i.label == "where_eq"));
        assert!(!items.iter().any(|i| i.label == "select_columns"));
    }

    #[test]
    fn test_context_aware_completion_select() {
        let items = context_aware_completion("query.select_", 13);
        assert!(items.iter().any(|i| i.label == "select_columns"));
        assert!(!items.iter().any(|i| i.label == "where_eq"));
    }

    #[test]
    fn test_context_aware_completion_query_builder() {
        let items = context_aware_completion("query.", 6);
        assert!(items.iter().any(|i| i.label == "table"));
        assert!(items.iter().any(|i| i.label == "build_select"));
    }
}
