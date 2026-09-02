//! SQL 逐子句可解释性（TASK-026）

use crate::types::NlQueryError;
use serde::{Deserialize, Serialize};

/// SQL 子句解释
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClauseExplanation {
    pub clause_type: String,
    pub original_text: String,
    pub explanation: String,
}

/// SQL 解释结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlExplanation {
    pub clauses: Vec<ClauseExplanation>,
    pub summary: String,
}

/// SQL 解释器
pub struct SqlExplainer;

impl SqlExplainer {
    pub fn new() -> Self {
        Self
    }

    /// 逐子句解释 SQL
    pub fn explain(&self, sql: &str) -> Result<SqlExplanation, NlQueryError> {
        let upper = sql.to_uppercase();
        if !upper.contains("SELECT") {
            return Err(NlQueryError::Nl2SqlFailed(
                "SQL 缺少 SELECT 子句".to_string(),
            ));
        }

        let mut clauses = Vec::new();

        if let Some(c) = self.explain_select(sql) {
            clauses.push(c);
        }
        if let Some(c) = self.explain_from(sql) {
            clauses.push(c);
        }
        if let Some(c) = self.explain_where(sql) {
            clauses.push(c);
        }
        if let Some(c) = self.explain_group_by(sql) {
            clauses.push(c);
        }
        if let Some(c) = self.explain_order_by(sql) {
            clauses.push(c);
        }
        if let Some(c) = self.explain_limit(sql) {
            clauses.push(c);
        }

        let summary = self.generate_summary(&clauses);

        Ok(SqlExplanation { clauses, summary })
    }

    fn explain_select(&self, sql: &str) -> Option<ClauseExplanation> {
        let upper = sql.to_uppercase();
        let start = upper.find("SELECT")?;
        let end = upper.find(" FROM ").unwrap_or(upper.len());
        let text = sql[start + 6..end].trim();

        let explanation = if text == "*" {
            "查询所有列".to_string()
        } else {
            let cols: Vec<_> = text.split(',').map(|c| c.trim()).collect();
            format!("查询 {} 个列: {}", cols.len(), cols.join(", "))
        };

        Some(ClauseExplanation {
            clause_type: "SELECT".to_string(),
            original_text: text.to_string(),
            explanation,
        })
    }

    fn explain_from(&self, sql: &str) -> Option<ClauseExplanation> {
        let upper = sql.to_uppercase();
        let start = upper.find(" FROM ")?;
        let after = sql[start + 6..].trim();
        let table: String = after
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != ';')
            .collect();

        Some(ClauseExplanation {
            clause_type: "FROM".to_string(),
            original_text: table.clone(),
            explanation: format!("从 {} 表读取数据", table),
        })
    }

    fn explain_where(&self, sql: &str) -> Option<ClauseExplanation> {
        let upper = sql.to_uppercase();
        let start = upper.find(" WHERE ")?;
        let after = &sql[start + 7..];
        let end_keywords = [" GROUP BY", " ORDER BY", " LIMIT"];
        let mut end = after.len();
        for kw in end_keywords {
            if let Some(pos) = after.to_uppercase().find(kw) {
                end = end.min(pos);
            }
        }
        let text = after[..end].trim();

        let condition_count = text.matches("AND").count() + text.matches("OR").count() + 1;
        Some(ClauseExplanation {
            clause_type: "WHERE".to_string(),
            original_text: text.to_string(),
            explanation: format!("过滤条件：包含 {} 个条件", condition_count),
        })
    }

    fn explain_group_by(&self, sql: &str) -> Option<ClauseExplanation> {
        let upper = sql.to_uppercase();
        let start = upper.find(" GROUP BY ")?;
        let after = &sql[start + 10..];
        let end_keywords = [" ORDER BY", " LIMIT", " HAVING"];
        let mut end = after.len();
        for kw in end_keywords {
            if let Some(pos) = after.to_uppercase().find(kw) {
                end = end.min(pos);
            }
        }
        let text = after[..end].trim();

        let cols: Vec<_> = text.split(',').map(|c| c.trim()).collect();
        Some(ClauseExplanation {
            clause_type: "GROUP BY".to_string(),
            original_text: text.to_string(),
            explanation: format!("按 {} 分组: {}", cols.len(), cols.join(", ")),
        })
    }

    fn explain_order_by(&self, sql: &str) -> Option<ClauseExplanation> {
        let upper = sql.to_uppercase();
        let start = upper.find(" ORDER BY ")?;
        let after = &sql[start + 10..];
        let end = after.to_uppercase().find(" LIMIT").unwrap_or(after.len());
        let text = after[..end].trim();

        let direction = if text.to_uppercase().contains("DESC") {
            "降序"
        } else {
            "升序"
        };
        Some(ClauseExplanation {
            clause_type: "ORDER BY".to_string(),
            original_text: text.to_string(),
            explanation: format!("按 {} 排序", direction),
        })
    }

    fn explain_limit(&self, sql: &str) -> Option<ClauseExplanation> {
        let upper = sql.to_uppercase();
        let start = upper.find(" LIMIT ")?;
        let after = sql[start + 7..].trim();
        let num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();

        Some(ClauseExplanation {
            clause_type: "LIMIT".to_string(),
            original_text: num.clone(),
            explanation: format!("最多返回 {} 行", num),
        })
    }

    fn generate_summary(&self, clauses: &[ClauseExplanation]) -> String {
        let select = clauses.iter().find(|c| c.clause_type == "SELECT");
        let from = clauses.iter().find(|c| c.clause_type == "FROM");
        let where_c = clauses.iter().find(|c| c.clause_type == "WHERE");

        let mut summary = String::new();
        if let (Some(s), Some(f)) = (select, from) {
            summary.push_str(&format!("该查询从{}中{}", f.original_text, s.explanation));
        }
        if let Some(w) = where_c {
            summary.push_str(&format!("，{}", w.explanation));
        }
        summary.push_str("。");
        summary
    }
}

impl Default for SqlExplainer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_simple_select() {
        let explainer = SqlExplainer::new();
        let result = explainer.explain("SELECT id, name FROM users").unwrap();
        assert!(result.clauses.len() >= 2);
        assert!(result.summary.contains("users"));
    }

    #[test]
    fn test_explain_with_where() {
        let explainer = SqlExplainer::new();
        let result = explainer
            .explain("SELECT id, name FROM users WHERE age > 18 AND status = 'active'")
            .unwrap();
        let where_clause = result
            .clauses
            .iter()
            .find(|c| c.clause_type == "WHERE")
            .unwrap();
        assert!(where_clause.explanation.contains("2 个条件"));
    }

    #[test]
    fn test_explain_with_group_order_limit() {
        let explainer = SqlExplainer::new();
        let result = explainer
            .explain("SELECT dept, COUNT(*) FROM employees GROUP BY dept ORDER BY COUNT(*) DESC LIMIT 10")
            .unwrap();
        assert!(result.clauses.iter().any(|c| c.clause_type == "GROUP BY"));
        assert!(result.clauses.iter().any(|c| c.clause_type == "ORDER BY"));
        assert!(result.clauses.iter().any(|c| c.clause_type == "LIMIT"));
    }

    #[test]
    fn test_explain_select_star() {
        let explainer = SqlExplainer::new();
        let result = explainer.explain("SELECT * FROM orders").unwrap();
        let select = result
            .clauses
            .iter()
            .find(|c| c.clause_type == "SELECT")
            .unwrap();
        assert!(select.explanation.contains("所有列"));
    }

    #[test]
    fn test_explain_invalid_sql() {
        let explainer = SqlExplainer::new();
        let result = explainer.explain("NOT A SQL");
        assert!(result.is_err());
    }
}
