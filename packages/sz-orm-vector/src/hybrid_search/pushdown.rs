//! 结构化过滤下推（将结构化过滤下推到向量/全文源）

use super::{FulltextQuery, StructuredQuery, VectorQuery};

/// 过滤下推器
pub struct FilterPushdown;

impl FilterPushdown {
    /// 将结构化过滤下推到向量查询（pgvector WHERE 子句）
    pub fn pushdown_to_vector(filter: &StructuredQuery, vector_query: &mut VectorQuery) {
        if filter.where_clauses.is_empty() {
            return;
        }
        let combined = filter.where_clauses.join(" AND ");
        match &mut vector_query.filter {
            Some(existing) => {
                *existing = format!("{existing} AND ({combined})");
            }
            None => {
                vector_query.filter = Some(combined);
            }
        }
    }

    /// 将结构化过滤下推到全文查询（ES filter）
    pub fn pushdown_to_fulltext(filter: &StructuredQuery, fulltext_query: &mut FulltextQuery) {
        if filter.where_clauses.is_empty() {
            return;
        }
        let combined = filter.where_clauses.join(" AND ");
        fulltext_query.fields.push(format!("__filter__:{combined}"));
    }
}

#[cfg(test)]
mod tests {
    use super::super::VectorMetric;
    use super::*;

    #[test]
    fn test_pushdown_to_vector_new_filter() {
        let filter = StructuredQuery {
            table: "products".to_string(),
            where_clauses: vec!["price < 1000".to_string()],
            order_by: None,
        };
        let mut vector_query = VectorQuery {
            collection: "docs".to_string(),
            query_vector: vec![1.0, 0.0],
            metric: VectorMetric::Cosine,
            filter: None,
        };
        FilterPushdown::pushdown_to_vector(&filter, &mut vector_query);
        assert_eq!(vector_query.filter.as_deref(), Some("price < 1000"));
    }

    #[test]
    fn test_pushdown_to_vector_existing_filter() {
        let filter = StructuredQuery {
            table: "products".to_string(),
            where_clauses: vec!["price < 1000".to_string()],
            order_by: None,
        };
        let mut vector_query = VectorQuery {
            collection: "docs".to_string(),
            query_vector: vec![1.0, 0.0],
            metric: VectorMetric::Cosine,
            filter: Some("category = 'electronics'".to_string()),
        };
        FilterPushdown::pushdown_to_vector(&filter, &mut vector_query);
        assert!(vector_query.filter.as_ref().unwrap().contains("category"));
        assert!(vector_query.filter.as_ref().unwrap().contains("price"));
    }

    #[test]
    fn test_pushdown_to_fulltext() {
        let filter = StructuredQuery {
            table: "products".to_string(),
            where_clauses: vec!["price < 1000".to_string()],
            order_by: None,
        };
        let mut fulltext_query = FulltextQuery {
            index: "docs_idx".to_string(),
            query_text: "laptop".to_string(),
            fields: vec!["title".to_string()],
        };
        FilterPushdown::pushdown_to_fulltext(&filter, &mut fulltext_query);
        assert!(fulltext_query
            .fields
            .iter()
            .any(|f| f.contains("__filter__")));
    }

    #[test]
    fn test_pushdown_empty_clauses() {
        let filter = StructuredQuery {
            table: "products".to_string(),
            where_clauses: vec![],
            order_by: None,
        };
        let mut vector_query = VectorQuery {
            collection: "docs".to_string(),
            query_vector: vec![1.0],
            metric: VectorMetric::Cosine,
            filter: None,
        };
        FilterPushdown::pushdown_to_vector(&filter, &mut vector_query);
        assert!(vector_query.filter.is_none());
    }
}
