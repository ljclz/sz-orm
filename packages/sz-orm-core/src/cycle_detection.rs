//! 循环检测 — Eager Loading 多级关联递归安全保护（v2.2.0 B-1）
//!
//! 当 Eager Loading 多级关联存在循环引用（如 User→Order→User）时，
//! [`CycleDetector`] 提供三种策略避免无限递归：
//!
//! - [`CyclePolicy::Error`]：检测到循环时返回错误
//! - [`CyclePolicy::Truncate`]：检测到循环时终止递归，返回已加载部分
//! - [`CyclePolicy::AllowWithDepthLimit`]：允许循环但限制最大深度

use std::collections::HashSet;

use crate::DbError;

/// 循环检测策略
///
/// 控制 [`CycleDetector`] 在检测到循环引用时的行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CyclePolicy {
    /// 检测到循环时返回 `Err`，含循环路径描述
    Error,
    /// 检测到循环时终止递归，返回已加载部分（默认策略）
    #[default]
    Truncate,
    /// 允许循环但限制最大递归深度，超限时终止
    AllowWithDepthLimit(usize),
}

/// 循环检测器
///
/// 按 entity 类型 + 关联名联合去重，避免同类型不同关联误判。
/// 例如 `User::manager` ≠ `User::orders`，不会误判为循环。
pub struct CycleDetector {
    policy: CyclePolicy,
    visited: HashSet<String>,
    current_depth: usize,
    path: Vec<String>,
}

impl CycleDetector {
    /// 创建新的循环检测器
    pub fn new(policy: CyclePolicy) -> Self {
        Self {
            policy,
            visited: HashSet::new(),
            current_depth: 0,
            path: Vec::new(),
        }
    }

    /// 检查是否可以继续递归
    ///
    /// 返回 `Ok(true)` 表示可以继续递归，`Ok(false)` 表示应终止递归，
    /// `Err(_)` 表示检测到循环且策略为 `Error`。
    pub fn check(&mut self, entity_type: &str, relation_name: &str) -> Result<bool, DbError> {
        let key = format!("{}::{}", entity_type, relation_name);

        if self.visited.contains(&key) {
            return match self.policy {
                CyclePolicy::Error => {
                    self.path.push(key.clone());
                    Err(DbError::InvalidInput(format!(
                        "检测到循环引用: {}",
                        self.path.join(" → ")
                    )))
                }
                CyclePolicy::Truncate => Ok(false),
                CyclePolicy::AllowWithDepthLimit(max_depth) => {
                    if self.current_depth >= max_depth {
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                }
            };
        }

        if let CyclePolicy::AllowWithDepthLimit(max_depth) = self.policy {
            if self.current_depth >= max_depth {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// 进入一个关联层级
    pub fn enter(&mut self, entity_type: &str, relation_name: &str) {
        let key = format!("{}::{}", entity_type, relation_name);
        self.visited.insert(key.clone());
        self.path.push(key);
        self.current_depth += 1;
    }

    /// 离开一个关联层级
    pub fn leave(&mut self) {
        self.path.pop();
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    /// 当前递归深度
    pub fn depth(&self) -> usize {
        self.current_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_policy_default() {
        assert_eq!(CyclePolicy::default(), CyclePolicy::Truncate);
    }

    #[test]
    fn test_cycle_detector_no_cycle() {
        let mut detector = CycleDetector::new(CyclePolicy::Error);
        assert!(detector.check("User", "orders").unwrap());
        detector.enter("User", "orders");
        assert!(detector.check("Order", "items").unwrap());
        detector.enter("Order", "items");
        detector.leave();
        detector.leave();
    }

    #[test]
    fn test_cycle_detector_error_policy() {
        let mut detector = CycleDetector::new(CyclePolicy::Error);
        detector.enter("User", "orders");
        assert!(detector.check("Order", "user").unwrap());
        detector.enter("Order", "user");
        let result = detector.check("User", "orders");
        assert!(result.is_err());
    }

    #[test]
    fn test_cycle_detector_truncate_policy() {
        let mut detector = CycleDetector::new(CyclePolicy::Truncate);
        detector.enter("User", "orders");
        detector.enter("Order", "user");
        let result = detector.check("User", "orders").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_cycle_detector_depth_limit() {
        let mut detector = CycleDetector::new(CyclePolicy::AllowWithDepthLimit(3));
        detector.enter("User", "orders");
        assert_eq!(detector.depth(), 1);
        detector.enter("Order", "items");
        assert_eq!(detector.depth(), 2);
        detector.enter("OrderItem", "product");
        assert_eq!(detector.depth(), 3);
        let result = detector.check("Product", "category").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_cycle_detector_different_relation_no_false_positive() {
        let mut detector = CycleDetector::new(CyclePolicy::Error);
        detector.enter("User", "orders");
        let result = detector.check("User", "manager");
        assert!(result.unwrap());
    }
}
