//! TASK-026 集成测试：SQL 逐子句解释端到端验证

use sz_orm_nl_query::sql_explainer::{SqlExplainer, SqlExplanation};

#[test]
fn test_explain_select_from() {
    let explainer = SqlExplainer::new();
    let result: SqlExplanation = explainer
        .explain("SELECT id, name, email FROM users")
        .unwrap();
    assert!(result.clauses.iter().any(|c| c.clause_type == "SELECT"));
    assert!(result.clauses.iter().any(|c| c.clause_type == "FROM"));
    assert!(result.summary.contains("users"));
}

#[test]
fn test_explain_with_where_clause() {
    let explainer = SqlExplainer::new();
    let result = explainer
        .explain("SELECT id FROM orders WHERE status = 'paid' AND amount > 100")
        .unwrap();
    let where_c = result
        .clauses
        .iter()
        .find(|c| c.clause_type == "WHERE")
        .unwrap();
    assert!(where_c.explanation.contains("2 个条件"));
}

#[test]
fn test_explain_full_query() {
    let explainer = SqlExplainer::new();
    let result = explainer
        .explain("SELECT dept, COUNT(*) AS cnt FROM employees WHERE salary > 50000 GROUP BY dept ORDER BY cnt DESC LIMIT 10")
        .unwrap();

    let clause_types: Vec<_> = result
        .clauses
        .iter()
        .map(|c| c.clause_type.as_str())
        .collect();
    assert!(clause_types.contains(&"SELECT"));
    assert!(clause_types.contains(&"FROM"));
    assert!(clause_types.contains(&"WHERE"));
    assert!(clause_types.contains(&"GROUP BY"));
    assert!(clause_types.contains(&"ORDER BY"));
    assert!(clause_types.contains(&"LIMIT"));
}

#[test]
fn test_explain_select_star() {
    let explainer = SqlExplainer::new();
    let result = explainer.explain("SELECT * FROM products").unwrap();
    let select = result
        .clauses
        .iter()
        .find(|c| c.clause_type == "SELECT")
        .unwrap();
    assert!(select.explanation.contains("所有列"));
}

#[test]
fn test_explain_invalid_sql_returns_error() {
    let explainer = SqlExplainer::new();
    assert!(explainer.explain("DELETE FROM users").is_err());
    assert!(explainer.explain("").is_err());
}

#[test]
fn test_explain_summary_generated() {
    let explainer = SqlExplainer::new();
    let result = explainer
        .explain("SELECT name FROM users WHERE age > 18")
        .unwrap();
    assert!(!result.summary.is_empty());
    assert!(result.summary.contains("users"));
}
