//! # Validator — Cypher 参数化校验 + SQL 透传拒绝

use crate::error::GraphError;
use crate::query::CypherQuery;

/// SQL 关键字列表（图接口拒绝 SQL 透传）
const SQL_KEYWORDS: &[&str] = &[
    "SELECT ",
    "INSERT ",
    "UPDATE ",
    "DELETE ",
    "CREATE TABLE ",
    "DROP TABLE ",
    "ALTER TABLE ",
    "select ",
    "insert ",
    "update ",
    "delete ",
    "create table ",
    "drop table ",
    "alter table ",
];

/// Cypher 查询校验器
pub struct CypherValidator;

impl CypherValidator {
    /// 校验查询：拒绝 SQL 透传 + 强制参数化
    pub fn validate(query: &CypherQuery) -> Result<(), GraphError> {
        Self::check_sql_keywords(&query.cypher)?;
        Self::check_parameterization(&query.cypher)?;
        Ok(())
    }

    /// 检测 SQL 关键字 → 返回 GraphError::SqlNotSupported
    fn check_sql_keywords(cypher: &str) -> Result<(), GraphError> {
        for keyword in SQL_KEYWORDS {
            if cypher.contains(keyword) {
                return Err(GraphError::SqlNotSupported(format!(
                    "SQL keyword '{}' detected in Cypher query",
                    keyword.trim()
                )));
            }
        }
        Ok(())
    }

    /// 检测字面量拼接 → 返回 GraphError::ParameterizationError
    ///
    /// 检测规则：Cypher 查询中字符串字面量（单引号或双引号包裹）且非参数占位符（$param）
    fn check_parameterization(cypher: &str) -> Result<(), GraphError> {
        let mut in_string = false;
        let mut quote_char = '\0';
        let mut string_start = 0usize;
        let chars: Vec<char> = cypher.chars().collect();

        for (i, &ch) in chars.iter().enumerate() {
            if matches!(ch, '\'' | '"') {
                if !in_string {
                    in_string = true;
                    quote_char = ch;
                    string_start = i;
                } else if ch == quote_char {
                    let literal = &cypher[string_start + 1..i];
                    if literal.len() > 2 && !literal.starts_with('$') {
                        return Err(GraphError::ParameterizationError(format!(
                            "string literal '{}' should use parameterized form ($param)",
                            literal
                        )));
                    }
                    in_string = false;
                    quote_char = '\0';
                }
            }
        }

        if in_string {
            return Err(GraphError::ParameterizationError(
                "unterminated string literal".into(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_reject_sql_select() {
        let q = CypherQuery::new("SELECT * FROM nodes");
        let result = CypherValidator::validate(&q);
        assert!(matches!(result, Err(GraphError::SqlNotSupported(_))));
    }

    #[test]
    fn test_reject_sql_insert() {
        let q = CypherQuery::new("INSERT INTO nodes VALUES (1)");
        let result = CypherValidator::validate(&q);
        assert!(matches!(result, Err(GraphError::SqlNotSupported(_))));
    }

    #[test]
    fn test_reject_sql_create_table() {
        let q = CypherQuery::new("CREATE TABLE foo (id INT)");
        let result = CypherValidator::validate(&q);
        assert!(matches!(result, Err(GraphError::SqlNotSupported(_))));
    }

    #[test]
    fn test_accept_parameterized_cypher() {
        let q = CypherQuery::new("MATCH (n:Person {name: $name}) RETURN n");
        let result = CypherValidator::validate(&q);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_string_literal() {
        let q = CypherQuery::new("MATCH (n:Person {name: 'Alice'}) RETURN n");
        let result = CypherValidator::validate(&q);
        assert!(matches!(result, Err(GraphError::ParameterizationError(_))));
    }

    #[test]
    fn test_injection_as_parameter() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), serde_json::json!("' OR 1=1 --"));
        let q = CypherQuery::with_params("MATCH (n:Person {name: $name}) RETURN n", params);
        let result = CypherValidator::validate(&q);
        assert!(result.is_ok());
    }
}
