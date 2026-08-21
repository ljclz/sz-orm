//! Cypher 子集递归下降解析器
//!
//! 支持语法子集：
//! 1. `MATCH (n:Label) RETURN n`
//! 2. `MATCH (n:Label {prop: $param}) RETURN n`
//! 3. `MATCH (a:L1)-[r:RelType]->(b:L2) RETURN a, r, b`
//! 4. `MATCH (n) WHERE n.prop = $param RETURN n`
//! 5. `MATCH (n:Label) RETURN count(n)`

use crate::error::GraphError;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ParsedQuery {
    MatchNode {
        alias: String,
        label: Option<String>,
        where_clause: Option<WhereClause>,
        return_items: Vec<ReturnItem>,
    },
    MatchRelationship {
        from: NodePattern,
        rel: RelPattern,
        to: NodePattern,
        return_items: Vec<ReturnItem>,
    },
}

#[derive(Debug, Clone)]
pub struct NodePattern {
    pub alias: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RelPattern {
    pub alias: String,
    pub rel_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WhereClause {
    pub alias: String,
    pub prop: String,
    pub param_name: String,
}

#[derive(Debug, Clone)]
pub enum ReturnItem {
    Node(String),
    Relationship(String),
    Count(String),
}

pub struct CypherSubsetParser;

impl CypherSubsetParser {
    pub fn parse(
        cypher: &str,
        _params: &HashMap<String, serde_json::Value>,
    ) -> Result<ParsedQuery, GraphError> {
        let cypher = cypher.trim();

        if cypher.is_empty() {
            return Err(GraphError::QueryError("empty query".into()));
        }

        let upper = cypher.to_uppercase();
        for kw in &["CREATE", "MERGE", "DELETE", "SET "] {
            if upper.contains(kw) {
                return Err(GraphError::QueryError(format!(
                    "unsupported syntax: {} is not supported in Cypher subset",
                    kw.trim()
                )));
            }
        }

        if upper.contains("SELECT") {
            return Err(GraphError::SqlNotSupported(
                "SQL passthrough is not supported in graph query".into(),
            ));
        }

        if !upper.starts_with("MATCH") {
            return Err(GraphError::QueryError(format!(
                "unsupported syntax: query must start with MATCH, got: {}",
                &cypher[..cypher.len().min(20)]
            )));
        }

        let after_match = cypher[5..].trim_start();

        if let Some(arrow_pos) = after_match.find("]->") {
            return Self::parse_relationship(after_match, arrow_pos);
        }

        Self::parse_node(after_match)
    }

    fn parse_node(rest: &str) -> Result<ParsedQuery, GraphError> {
        let (node_pattern, after_node) = Self::parse_node_pattern(rest)?;
        let mut where_clause = None;
        let mut after_where = after_node;

        let after_trimmed = after_node.trim_start();
        if after_trimmed.to_uppercase().starts_with("WHERE") {
            let where_rest = after_trimmed[5..].trim_start();
            let (wc, rest) = Self::parse_where(where_rest)?;
            where_clause = Some(wc);
            after_where = rest;
        }

        let return_rest = after_where.trim_start();
        let return_items = Self::parse_return(return_rest)?;

        Ok(ParsedQuery::MatchNode {
            alias: node_pattern.alias,
            label: node_pattern.label,
            where_clause,
            return_items,
        })
    }

    fn parse_relationship(rest: &str, _arrow_pos: usize) -> Result<ParsedQuery, GraphError> {
        let (from, after_from) = Self::parse_node_pattern(rest)?;

        let after_from_trimmed = after_from.trim_start();
        if !after_from_trimmed.starts_with('-') {
            return Err(GraphError::QueryError(
                "expected '-' after node pattern in relationship".into(),
            ));
        }

        let bracket_start = after_from_trimmed.find('[').ok_or_else(|| {
            GraphError::QueryError("expected '[' for relationship pattern".into())
        })?;
        let bracket_end = after_from_trimmed.find(']').ok_or_else(|| {
            GraphError::QueryError("expected ']' for relationship pattern".into())
        })?;

        let rel_content = &after_from_trimmed[bracket_start + 1..bracket_end];
        let rel_pattern = Self::parse_rel_pattern(rel_content)?;

        let after_bracket = &after_from_trimmed[bracket_end + 1..];
        let after_arrow = after_bracket.trim_start_matches(['-', '>']).trim_start();

        let (to, after_to) = Self::parse_node_pattern(after_arrow)?;

        let return_rest = after_to.trim_start();
        let return_items = Self::parse_return(return_rest)?;

        Ok(ParsedQuery::MatchRelationship {
            from,
            rel: rel_pattern,
            to,
            return_items,
        })
    }

    fn parse_node_pattern(rest: &str) -> Result<(NodePattern, &str), GraphError> {
        let rest = rest.trim_start();
        if !rest.starts_with('(') {
            return Err(GraphError::QueryError(
                "expected '(' for node pattern".into(),
            ));
        }

        let close_pos = rest
            .find(')')
            .ok_or_else(|| GraphError::QueryError("expected ')' for node pattern".into()))?;

        let content = &rest[1..close_pos];
        let after = &rest[close_pos + 1..];

        let (alias, label) = if let Some(colon_pos) = content.find(':') {
            let alias = content[..colon_pos].trim().to_string();
            let label_part = &content[colon_pos + 1..];
            let label = if let Some(brace_pos) = label_part.find('{') {
                label_part[..brace_pos].trim().to_string()
            } else {
                label_part.trim().to_string()
            };
            (alias, Some(label))
        } else {
            (content.trim().to_string(), None)
        };

        if alias.is_empty() {
            return Err(GraphError::QueryError(
                "node pattern alias cannot be empty".into(),
            ));
        }

        Ok((NodePattern { alias, label }, after))
    }

    fn parse_rel_pattern(content: &str) -> Result<RelPattern, GraphError> {
        let content = content.trim();
        let (alias, rel_type) = if let Some(colon_pos) = content.find(':') {
            let alias = content[..colon_pos].trim().to_string();
            let rel_type = content[colon_pos + 1..].trim().to_string();
            (alias, Some(rel_type))
        } else {
            (content.to_string(), None)
        };

        Ok(RelPattern { alias, rel_type })
    }

    fn parse_where(rest: &str) -> Result<(WhereClause, &str), GraphError> {
        let eq_pos = rest
            .find('=')
            .ok_or_else(|| GraphError::QueryError("WHERE clause must contain '='".into()))?;

        let left = rest[..eq_pos].trim();
        let right_start = rest[eq_pos + 1..].trim_start();

        let param_match = right_start.find('$').ok_or_else(|| {
            GraphError::QueryError("WHERE clause must use $param parameter".into())
        })?;

        let param_rest = &right_start[param_match + 1..];
        let param_end = param_rest
            .find(|c: char| c.is_whitespace())
            .unwrap_or(param_rest.len());
        let param_name = param_rest[..param_end].to_string();

        let after_param = &param_rest[param_end..];

        let dot_pos = left.find('.').ok_or_else(|| {
            GraphError::QueryError("WHERE clause must use alias.prop format".into())
        })?;

        let alias = left[..dot_pos].trim().to_string();
        let prop = left[dot_pos + 1..].trim().to_string();

        Ok((
            WhereClause {
                alias,
                prop,
                param_name,
            },
            after_param,
        ))
    }

    fn parse_return(rest: &str) -> Result<Vec<ReturnItem>, GraphError> {
        let rest = rest.trim_start();
        let upper = rest.to_uppercase();
        if !upper.starts_with("RETURN") {
            return Err(GraphError::QueryError("expected RETURN clause".into()));
        }

        let return_rest = rest[6..].trim();
        let mut items = Vec::new();

        for part in return_rest.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if part.to_uppercase().starts_with("COUNT(") {
                let inner_start = part.find('(').unwrap();
                let inner_end = part.find(')').unwrap();
                let alias = part[inner_start + 1..inner_end].trim().to_string();
                items.push(ReturnItem::Count(alias));
            } else {
                items.push(ReturnItem::Node(part.to_string()));
            }
        }

        if items.is_empty() {
            return Err(GraphError::QueryError(
                "RETURN clause must have at least one item".into(),
            ));
        }

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_params() -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }

    #[test]
    fn test_parse_match_node_label() {
        let q = CypherSubsetParser::parse("MATCH (n:Person) RETURN n", &empty_params()).unwrap();
        match q {
            ParsedQuery::MatchNode { alias, label, .. } => {
                assert_eq!(alias, "n");
                assert_eq!(label.as_deref(), Some("Person"));
            }
            _ => panic!("expected MatchNode"),
        }
    }

    #[test]
    fn test_parse_match_node_where_param() {
        let q = CypherSubsetParser::parse(
            "MATCH (n:Person) WHERE n.name = $name RETURN n",
            &empty_params(),
        )
        .unwrap();
        match q {
            ParsedQuery::MatchNode { where_clause, .. } => {
                let wc = where_clause.unwrap();
                assert_eq!(wc.alias, "n");
                assert_eq!(wc.prop, "name");
                assert_eq!(wc.param_name, "name");
            }
            _ => panic!("expected MatchNode"),
        }
    }

    #[test]
    fn test_parse_match_relationship() {
        let q = CypherSubsetParser::parse(
            "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a, r, b",
            &empty_params(),
        )
        .unwrap();
        match q {
            ParsedQuery::MatchRelationship { from, rel, to, .. } => {
                assert_eq!(from.alias, "a");
                assert_eq!(from.label.as_deref(), Some("Person"));
                assert_eq!(rel.rel_type.as_deref(), Some("KNOWS"));
                assert_eq!(to.alias, "b");
            }
            _ => panic!("expected MatchRelationship"),
        }
    }

    #[test]
    fn test_parse_count_aggregation() {
        let q =
            CypherSubsetParser::parse("MATCH (n:Person) RETURN count(n)", &empty_params()).unwrap();
        match q {
            ParsedQuery::MatchNode { return_items, .. } => {
                assert!(matches!(return_items[0], ReturnItem::Count(_)));
            }
            _ => panic!("expected MatchNode"),
        }
    }

    #[test]
    fn test_parse_reject_create() {
        let result = CypherSubsetParser::parse("CREATE (n:Person {name: $name})", &empty_params());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_reject_delete() {
        let result = CypherSubsetParser::parse("MATCH (n) DELETE n", &empty_params());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_reject_sql() {
        let result = CypherSubsetParser::parse("SELECT * FROM users", &empty_params());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GraphError::SqlNotSupported(_)
        ));
    }

    #[test]
    fn test_parse_reject_no_match() {
        let result = CypherSubsetParser::parse("RETURN n", &empty_params());
        assert!(result.is_err());
    }
}
