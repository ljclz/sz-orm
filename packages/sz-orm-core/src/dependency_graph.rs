//! 表-查询依赖关系图
//!
//! 维护 table → Set<CacheKey> 映射，支持写操作时仅失效依赖该表的缓存项。
//! 解析失败时降级为表级失效（调用方处理）。

use crate::l2_cache::CacheKey;
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

/// 表-查询依赖关系图
///
/// 维护 table → Set<CacheKey> 映射，支持精确失效。
/// 解析失败时降级为表级失效（调用方处理）。
pub struct DependencyGraph {
    table_to_keys: RwLock<HashMap<String, HashSet<CacheKey>>>,
    capacity_limit: usize,
}

impl DependencyGraph {
    /// 创建指定容量上限的依赖图
    pub fn new(capacity_limit: usize) -> Self {
        Self {
            table_to_keys: RwLock::new(HashMap::new()),
            capacity_limit,
        }
    }

    /// 从 SQL 解析依赖表名（FROM/JOIN/UPDATE/INSERT INTO/DELETE FROM）
    ///
    /// 轻量词法扫描，不引入新 SQL parser 依赖。
    /// 返回解析到的表名集合；解析失败返回空集（调用方降级）。
    pub fn parse_dependencies(sql: &str) -> HashSet<String> {
        let mut result = HashSet::new();
        let keywords = ["FROM", "JOIN", "UPDATE", "INTO", "TABLE"];
        let upper = sql.to_uppercase();
        let tokens: Vec<&str> = upper.split_whitespace().collect();
        let original_tokens: Vec<&str> = sql.split_whitespace().collect();

        let mut i = 0;
        while i < tokens.len() {
            let tok = tokens[i];

            if tok == "FROM" || tok == "JOIN" || tok == "UPDATE" || tok == "INTO" || tok == "TABLE"
            {
                extract_table(&tokens, &original_tokens, i + 1, &mut result, &keywords);
            }

            if tok == "DELETE" && i + 1 < tokens.len() && tokens[i + 1] == "FROM" {
                extract_table(&tokens, &original_tokens, i + 2, &mut result, &keywords);
            }

            if tok == "INSERT" && i + 1 < tokens.len() && tokens[i + 1] == "INTO" {
                extract_table(&tokens, &original_tokens, i + 2, &mut result, &keywords);
            }

            i += 1;
        }

        result
    }

    /// 记录缓存项依赖的表
    ///
    /// 容量超 `capacity_limit` 时拒绝新增（防 OOM）。
    pub fn add_dependency(&self, table: &str, cache_key: &CacheKey) {
        let mut map = self.table_to_keys.write().unwrap();
        let entry = map.entry(table.to_string()).or_default();
        if entry.len() < self.capacity_limit {
            entry.insert(cache_key.clone());
        }
    }

    /// 查询依赖该表的所有缓存 key（用于精确失效）
    ///
    /// 返回空集表示无依赖（调用方降级为表级失效）。
    pub fn dependent_keys(&self, table: &str) -> HashSet<CacheKey> {
        let map = self.table_to_keys.read().unwrap();
        map.get(table).cloned().unwrap_or_default()
    }

    /// 当前依赖图中的表数量
    pub fn table_count(&self) -> usize {
        self.table_to_keys.read().unwrap().len()
    }

    /// 指定表的依赖 key 数量
    pub fn key_count(&self, table: &str) -> usize {
        self.table_to_keys
            .read()
            .unwrap()
            .get(table)
            .map(|s| s.len())
            .unwrap_or(0)
    }
}

fn extract_table(
    upper_tokens: &[&str],
    original_tokens: &[&str],
    idx: usize,
    result: &mut HashSet<String>,
    keywords: &[&str],
) {
    if idx >= upper_tokens.len() {
        return;
    }
    let next = upper_tokens[idx];
    if keywords.contains(&next) || is_sql_keyword(next) || next.is_empty() {
        return;
    }
    let table = original_tokens
        .get(idx)
        .unwrap_or(&next)
        .trim_matches(|c| c == ',' || c == ';' || c == '(' || c == ')');
    if !table.is_empty() {
        result.insert(table.to_string());
    }
}

fn is_sql_keyword(tok: &str) -> bool {
    matches!(
        tok,
        "SELECT"
            | "WHERE"
            | "ON"
            | "AS"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "INNER"
            | "LEFT"
            | "RIGHT"
            | "FULL"
            | "OUTER"
            | "CROSS"
            | "NATURAL"
            | "USING"
            | "GROUP"
            | "ORDER"
            | "HAVING"
            | "LIMIT"
            | "OFFSET"
            | "UNION"
            | "EXCEPT"
            | "INTERSECT"
            | "VALUES"
            | "SET"
            | "JOIN"
            | "FROM"
            | "INTO"
            | "UPDATE"
            | "DELETE"
            | "INSERT"
            | "TABLE"
            | "IF"
            | "EXISTS"
            | "CREATE"
            | "DROP"
            | "ALTER"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l2_cache::CacheKeyKind;

    fn make_key(table: &str, id: &str) -> CacheKey {
        CacheKey {
            table: table.to_string(),
            kind: CacheKeyKind::ByQuery,
            identifier: id.to_string(),
        }
    }

    #[test]
    fn test_dependency_graph_parse() {
        let tables = DependencyGraph::parse_dependencies(
            "SELECT * FROM orders o JOIN items i ON o.id = i.order_id",
        );
        assert!(tables.contains("orders"), "应包含 orders");
        assert!(tables.contains("items"), "应包含 items");
        assert_eq!(tables.len(), 2, "应恰好解析出 2 个表");
    }

    #[test]
    fn test_dependency_graph_parse_update() {
        let tables = DependencyGraph::parse_dependencies("UPDATE users SET name = 'x'");
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_dependency_graph_parse_insert() {
        let tables =
            DependencyGraph::parse_dependencies("INSERT INTO products (id, name) VALUES (1, 'x')");
        assert!(tables.contains("products"));
    }

    #[test]
    fn test_dependency_graph_parse_delete() {
        let tables = DependencyGraph::parse_dependencies("DELETE FROM orders WHERE id = 1");
        assert!(tables.contains("orders"));
    }

    #[test]
    fn test_dependency_graph_parse_empty() {
        let tables = DependencyGraph::parse_dependencies("SELECT 1");
        assert!(tables.is_empty());
    }

    #[test]
    fn test_dependency_graph_precise_invalidation() {
        let dg = DependencyGraph::new(1000);
        let q1 = make_key("table_a", "q1");
        let q2 = make_key("table_b", "q2");

        dg.add_dependency("table_a", &q1);
        dg.add_dependency("table_b", &q2);

        let keys_a = dg.dependent_keys("table_a");
        assert_eq!(keys_a.len(), 1);
        assert!(keys_a.contains(&q1));
        assert!(!keys_a.contains(&q2), "table_a 的依赖不应包含 Q2");
    }

    #[test]
    fn test_dependency_graph_no_dependency() {
        let dg = DependencyGraph::new(1000);
        let keys = dg.dependent_keys("nonexistent");
        assert!(keys.is_empty(), "不存在的表应返回空集");
    }

    #[test]
    fn test_dependency_graph_capacity_limit() {
        let dg = DependencyGraph::new(2);
        let k1 = make_key("t", "k1");
        let k2 = make_key("t", "k2");
        let k3 = make_key("t", "k3");

        dg.add_dependency("t", &k1);
        dg.add_dependency("t", &k2);
        dg.add_dependency("t", &k3);

        let keys = dg.dependent_keys("t");
        assert_eq!(keys.len(), 2, "容量上限 2 时不应超过 2 个 key");
    }

    #[test]
    fn test_dependency_graph_degrade_on_parse_fail() {
        let tables = DependencyGraph::parse_dependencies("");
        assert!(tables.is_empty(), "空 SQL 应返回空集，不 panic");

        let tables = DependencyGraph::parse_dependencies("not a sql at all !!!");
        assert!(tables.is_empty(), "无效 SQL 应返回空集，不 panic");
    }

    #[test]
    fn test_dependency_graph_multiple_keys_same_table() {
        let dg = DependencyGraph::new(1000);
        let k1 = make_key("orders", "q1");
        let k2 = make_key("orders", "q2");
        let k3 = make_key("orders", "q3");

        dg.add_dependency("orders", &k1);
        dg.add_dependency("orders", &k2);
        dg.add_dependency("orders", &k3);

        let keys = dg.dependent_keys("orders");
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn test_dependency_graph_table_count() {
        let dg = DependencyGraph::new(1000);
        dg.add_dependency("t1", &make_key("t1", "k1"));
        dg.add_dependency("t2", &make_key("t2", "k2"));
        assert_eq!(dg.table_count(), 2);
    }
}
