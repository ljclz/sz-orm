//! 行级安全策略（v7.1.0）
//!
//! 基于属性的行级过滤策略，生成 WHERE 子句条件。

use std::collections::HashMap;

/// 行级策略
#[derive(Debug, Clone)]
pub struct RowLevelPolicy {
    /// 策略名称
    pub name: String,
    /// 表名
    pub table: String,
    /// 过滤条件（参数化 WHERE 子句，不含 WHERE 关键字）
    pub filter_sql: String,
    /// 参数
    pub params: Vec<String>,
}

impl RowLevelPolicy {
    /// 创建策略
    pub fn new(name: &str, table: &str, filter_sql: &str) -> Self {
        Self {
            name: name.to_string(),
            table: table.to_string(),
            filter_sql: filter_sql.to_string(),
            params: Vec::new(),
        }
    }

    /// 添加参数
    pub fn with_param(mut self, param: &str) -> Self {
        self.params.push(param.to_string());
        self
    }

    /// 生成 WHERE 子句
    pub fn where_clause(&self) -> String {
        if self.filter_sql.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.filter_sql)
        }
    }
}

/// 行级策略管理器
pub struct RowLevelPolicyManager {
    policies: HashMap<String, Vec<RowLevelPolicy>>,
}

impl RowLevelPolicyManager {
    /// 创建管理器
    pub fn new() -> Self {
        Self {
            policies: HashMap::new(),
        }
    }

    /// 添加策略
    pub fn add_policy(&mut self, policy: RowLevelPolicy) {
        self.policies
            .entry(policy.table.clone())
            .or_default()
            .push(policy);
    }

    /// 获取表的所有策略
    pub fn get_policies(&self, table: &str) -> &[RowLevelPolicy] {
        match self.policies.get(table) {
            Some(policies) => policies,
            None => &[],
        }
    }

    /// 生成组合 WHERE 子句（AND 连接所有策略）
    pub fn build_where_clause(&self, table: &str) -> (String, Vec<String>) {
        let policies = self.get_policies(table);
        if policies.is_empty() {
            return (String::new(), Vec::new());
        }
        let conditions: Vec<&str> = policies.iter().map(|p| p.filter_sql.as_str()).collect();
        let mut params = Vec::new();
        for p in policies {
            params.extend(p.params.iter().cloned());
        }
        (format!("WHERE {}", conditions.join(" AND ")), params)
    }

    /// 策略数
    pub fn policy_count(&self) -> usize {
        self.policies.values().map(|v| v.len()).sum()
    }
}

impl Default for RowLevelPolicyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_where_clause() {
        let policy = RowLevelPolicy::new("dept_isolation", "orders", "department_id = ?");
        assert_eq!(policy.where_clause(), "WHERE department_id = ?");
    }

    #[test]
    fn test_manager_build_where() {
        let mut mgr = RowLevelPolicyManager::new();
        mgr.add_policy(RowLevelPolicy::new("p1", "orders", "department_id = ?").with_param("eng"));
        mgr.add_policy(RowLevelPolicy::new("p2", "orders", "is_deleted = ?").with_param("false"));
        let (sql, params) = mgr.build_where_clause("orders");
        assert!(sql.contains("department_id = ?"));
        assert!(sql.contains("is_deleted = ?"));
        assert!(sql.contains("AND"));
        assert_eq!(params, vec!["eng", "false"]);
    }

    #[test]
    fn test_manager_no_policies() {
        let mgr = RowLevelPolicyManager::new();
        let (sql, params) = mgr.build_where_clause("orders");
        assert!(sql.is_empty());
        assert!(params.is_empty());
    }

    #[test]
    fn test_manager_multiple_tables() {
        let mut mgr = RowLevelPolicyManager::new();
        mgr.add_policy(RowLevelPolicy::new("p1", "orders", "dept = ?"));
        mgr.add_policy(RowLevelPolicy::new("p2", "users", "tenant = ?"));
        assert_eq!(mgr.get_policies("orders").len(), 1);
        assert_eq!(mgr.get_policies("users").len(), 1);
        assert_eq!(mgr.policy_count(), 2);
    }

    #[test]
    fn test_empty_filter() {
        let policy = RowLevelPolicy::new("empty", "orders", "");
        assert!(policy.where_clause().is_empty());
    }
}
