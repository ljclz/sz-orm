//! 并行查询结果与单查询定义

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 单个并行查询
pub struct ParallelQuery<T> {
    /// SQL 语句（参数化）
    pub sql: String,
    /// 参数绑定
    pub params: Vec<Value>,
    /// 查询标识（用于统计/缓存）
    pub query_key: Option<String>,
    /// 降级值（FailureStrategy::Fallback 时返回）
    pub fallback_value: Option<T>,
    pub _marker: PhantomData<T>,
}

impl<T> ParallelQuery<T> {
    /// 创建并行查询
    pub fn new(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            query_key: None,
            fallback_value: None,
            _marker: PhantomData,
        }
    }

    /// 设置查询标识
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.query_key = Some(key.into());
        self
    }

    /// 设置降级值
    pub fn with_fallback(mut self, value: T) -> Self {
        self.fallback_value = Some(value);
        self
    }
}

/// 单查询执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryOutcome<T> {
    /// 结果值
    pub value: T,
    /// 影响行数
    pub rows: usize,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
    /// 是否来自缓存
    pub from_cache: bool,
}

impl<T> QueryOutcome<T> {
    /// 创建查询结果
    pub fn new(value: T, rows: usize, elapsed_ms: u64) -> Self {
        Self {
            value,
            rows,
            elapsed_ms,
            from_cache: false,
        }
    }

    /// 标记来自缓存
    pub fn from_cache(mut self) -> Self {
        self.from_cache = true;
        self
    }

    pub fn is_from_cache(&self) -> bool {
        self.from_cache
    }
}

/// 查询失败信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFailure {
    /// 查询索引
    pub query_index: usize,
    /// 错误信息
    pub error: String,
}

impl QueryFailure {
    pub fn new(query_index: usize, error: impl Into<String>) -> Self {
        Self {
            query_index,
            error: error.into(),
        }
    }
}

/// 并行查询整体结果
#[derive(Debug, Clone)]
pub struct ParallelQueryOutcome<T> {
    /// 各查询结果（失败/超时为 None）
    pub results: Vec<Option<QueryOutcome<T>>>,
    /// 失败信息
    pub failures: Vec<QueryFailure>,
    /// 超时查询索引
    pub timed_out: Vec<usize>,
    /// 整体耗时（毫秒）
    pub total_elapsed_ms: u64,
    /// 合并结果
    pub merged_result: Option<T>,
}

impl<T> ParallelQueryOutcome<T> {
    /// 成功查询数
    pub fn success_count(&self) -> usize {
        self.results.iter().filter(|r| r.is_some()).count()
    }

    /// 失败查询数
    pub fn failure_count(&self) -> usize {
        self.failures.len()
    }

    /// 超时查询数
    pub fn timeout_count(&self) -> usize {
        self.timed_out.len()
    }

    /// 是否全部成功
    pub fn all_succeeded(&self) -> bool {
        self.failures.is_empty() && self.timed_out.is_empty()
    }

    pub fn total_count(&self) -> usize {
        self.results.len()
    }

    pub fn has_merged_result(&self) -> bool {
        self.merged_result.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_query_builder() {
        let q = ParallelQuery::new("SELECT * FROM users WHERE id = $1", vec![Value::from(42)])
            .with_key("user_by_id")
            .with_fallback(Vec::<String>::new());
        assert_eq!(q.sql, "SELECT * FROM users WHERE id = $1");
        assert_eq!(q.query_key.as_deref(), Some("user_by_id"));
        assert!(q.fallback_value.is_some());
    }

    #[test]
    fn query_outcome_creation() {
        let outcome = QueryOutcome::new(vec!["a".to_string()], 1, 50);
        assert_eq!(outcome.rows, 1);
        assert_eq!(outcome.elapsed_ms, 50);
        assert!(!outcome.from_cache);

        let cached = QueryOutcome::new(vec!["b".to_string()], 1, 5).from_cache();
        assert!(cached.from_cache);
    }

    #[test]
    fn parallel_outcome_counts() {
        let outcome = ParallelQueryOutcome {
            results: vec![
                Some(QueryOutcome::new(1, 1, 10)),
                None,
                Some(QueryOutcome::new(3, 1, 20)),
            ],
            failures: vec![QueryFailure {
                query_index: 1,
                error: "db down".into(),
            }],
            timed_out: vec![],
            total_elapsed_ms: 25,
            merged_result: Some(1),
        };
        assert_eq!(outcome.success_count(), 2);
        assert_eq!(outcome.failure_count(), 1);
        assert_eq!(outcome.timeout_count(), 0);
        assert!(!outcome.all_succeeded());
    }

    #[test]
    fn parallel_outcome_all_succeeded() {
        let outcome = ParallelQueryOutcome {
            results: vec![
                Some(QueryOutcome::new(1, 1, 10)),
                Some(QueryOutcome::new(2, 1, 20)),
            ],
            failures: vec![],
            timed_out: vec![],
            total_elapsed_ms: 25,
            merged_result: Some(1),
        };
        assert!(outcome.all_succeeded());
    }

    #[test]
    fn test_query_outcome_is_from_cache() {
        let fresh = QueryOutcome::new(1, 1, 10);
        assert!(!fresh.is_from_cache());
        let cached = QueryOutcome::new(2, 1, 5).from_cache();
        assert!(cached.is_from_cache());
    }

    #[test]
    fn test_query_failure_new() {
        let f = QueryFailure::new(3, "connection refused");
        assert_eq!(f.query_index, 3);
        assert_eq!(f.error, "connection refused");
    }

    #[test]
    fn test_parallel_outcome_total_count() {
        let outcome = ParallelQueryOutcome {
            results: vec![
                Some(QueryOutcome::new(1, 1, 10)),
                None,
                Some(QueryOutcome::new(3, 1, 20)),
            ],
            failures: vec![],
            timed_out: vec![],
            total_elapsed_ms: 30,
            merged_result: None,
        };
        assert_eq!(outcome.total_count(), 3);
    }

    #[test]
    fn test_parallel_outcome_has_merged_result() {
        let with_merge = ParallelQueryOutcome {
            results: vec![Some(QueryOutcome::new(1, 1, 10))],
            failures: vec![],
            timed_out: vec![],
            total_elapsed_ms: 10,
            merged_result: Some(1),
        };
        let no_merge = ParallelQueryOutcome {
            results: vec![Some(QueryOutcome::new(1, 1, 10))],
            failures: vec![],
            timed_out: vec![],
            total_elapsed_ms: 10,
            merged_result: None,
        };
        assert!(with_merge.has_merged_result());
        assert!(!no_merge.has_merged_result());
    }

    #[test]
    fn test_parallel_outcome_is_empty() {
        let empty: ParallelQueryOutcome<i32> = ParallelQueryOutcome {
            results: vec![],
            failures: vec![],
            timed_out: vec![],
            total_elapsed_ms: 0,
            merged_result: None,
        };
        let non_empty = ParallelQueryOutcome {
            results: vec![Some(QueryOutcome::new(1, 1, 10))],
            failures: vec![],
            timed_out: vec![],
            total_elapsed_ms: 10,
            merged_result: None,
        };
        assert!(empty.is_empty());
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_parallel_query_with_fallback() {
        let q = ParallelQuery::new("SELECT 1", vec![])
            .with_key("test")
            .with_fallback(42);
        assert_eq!(q.query_key.as_deref(), Some("test"));
        assert_eq!(q.fallback_value, Some(42));
    }

    #[test]
    fn test_parallel_outcome_mixed_results() {
        let outcome = ParallelQueryOutcome {
            results: vec![
                Some(QueryOutcome::new(vec!["a".to_string()], 1, 10).from_cache()),
                None,
                Some(QueryOutcome::new(vec!["b".to_string()], 1, 20)),
                None,
            ],
            failures: vec![
                QueryFailure::new(1, "timeout"),
                QueryFailure::new(3, "db error"),
            ],
            timed_out: vec![1],
            total_elapsed_ms: 30,
            merged_result: Some(vec!["a".to_string(), "b".to_string()]),
        };
        assert_eq!(outcome.total_count(), 4);
        assert_eq!(outcome.success_count(), 2);
        assert_eq!(outcome.failure_count(), 2);
        assert_eq!(outcome.timeout_count(), 1);
        assert!(!outcome.all_succeeded());
        assert!(outcome.has_merged_result());
    }
}
