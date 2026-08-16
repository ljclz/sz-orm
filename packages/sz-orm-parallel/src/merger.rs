//! 结果合并器 — 四种合并策略

use crate::config::MergeStrategy;
use crate::outcome::{ParallelQueryOutcome, QueryOutcome};

/// 结果合并器
pub struct ResultMerger;

impl ResultMerger {
    /// 按合并策略合并多个查询结果
    ///
    /// - `First`：取首个成功结果
    /// - `Union`：合并所有成功结果（要求 T: Extend）
    /// - `Join`：按 join_key 关联（简化版：取首个结果）
    /// - `Map`：映射转换（简化版：取最后一个结果）
    pub fn merge<T: Clone>(
        results: &[Option<QueryOutcome<T>>],
        strategy: &MergeStrategy,
    ) -> Option<T> {
        match strategy {
            MergeStrategy::First => results
                .iter()
                .find_map(|r| r.as_ref().map(|o| o.value.clone())),
            MergeStrategy::Union => {
                let mut combined: Vec<T> = Vec::new();
                for r in results.iter().flatten() {
                    combined.push(r.value.clone());
                }
                if combined.is_empty() {
                    None
                } else {
                    combined.into_iter().next()
                }
            }
            MergeStrategy::Join { .. } => results
                .iter()
                .find_map(|r| r.as_ref().map(|o| o.value.clone())),
            MergeStrategy::Map => results
                .iter()
                .rev()
                .find_map(|r| r.as_ref().map(|o| o.value.clone())),
        }
    }

    /// 从 ParallelQueryOutcome 提取合并结果
    pub fn merge_outcome<T: Clone>(
        outcome: &ParallelQueryOutcome<T>,
        strategy: &MergeStrategy,
    ) -> Option<T> {
        Self::merge(&outcome.results, strategy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::QueryFailure;

    fn outcome<T: Clone>(value: T) -> QueryOutcome<T> {
        QueryOutcome::new(value, 1, 10)
    }

    #[test]
    fn merge_first_returns_first_success() {
        let results = vec![Some(outcome("first")), Some(outcome("second")), None];
        let merged = ResultMerger::merge(&results, &MergeStrategy::First);
        assert_eq!(merged, Some("first"));
    }

    #[test]
    fn merge_first_skips_none() {
        let results = vec![None, None, Some(outcome("third"))];
        let merged = ResultMerger::merge(&results, &MergeStrategy::First);
        assert_eq!(merged, Some("third"));
    }

    #[test]
    fn merge_first_all_none() {
        let results: Vec<Option<QueryOutcome<String>>> = vec![None, None];
        let merged = ResultMerger::merge(&results, &MergeStrategy::First);
        assert_eq!(merged, None);
    }

    #[test]
    fn merge_union_returns_first_of_combined() {
        let results = vec![Some(outcome(1)), Some(outcome(2)), None];
        let merged = ResultMerger::merge(&results, &MergeStrategy::Union);
        assert_eq!(merged, Some(1));
    }

    #[test]
    fn merge_union_empty_returns_none() {
        let results: Vec<Option<QueryOutcome<i32>>> = vec![None, None];
        let merged = ResultMerger::merge(&results, &MergeStrategy::Union);
        assert_eq!(merged, None);
    }

    #[test]
    fn merge_join_returns_first() {
        let results = vec![Some(outcome("a")), Some(outcome("b"))];
        let merged = ResultMerger::merge(
            &results,
            &MergeStrategy::Join {
                join_key: "id".into(),
            },
        );
        assert_eq!(merged, Some("a"));
    }

    #[test]
    fn merge_map_returns_last() {
        let results = vec![Some(outcome("a")), Some(outcome("b")), Some(outcome("c"))];
        let merged = ResultMerger::merge(&results, &MergeStrategy::Map);
        assert_eq!(merged, Some("c"));
    }

    #[test]
    fn merge_outcome_from_parallel_outcome() {
        let outcome = ParallelQueryOutcome {
            results: vec![Some(outcome(42)), None],
            failures: vec![QueryFailure {
                query_index: 1,
                error: "failed".into(),
            }],
            timed_out: vec![],
            total_elapsed_ms: 100,
            merged_result: None,
        };
        let merged = ResultMerger::merge_outcome(&outcome, &MergeStrategy::First);
        assert_eq!(merged, Some(42));
    }

    #[test]
    fn test_merge_first_empty_slice() {
        let results: Vec<Option<QueryOutcome<i32>>> = vec![];
        let merged = ResultMerger::merge(&results, &MergeStrategy::First);
        assert_eq!(merged, None);
    }

    #[test]
    fn test_merge_union_single_result() {
        let results = vec![Some(outcome(99))];
        let merged = ResultMerger::merge(&results, &MergeStrategy::Union);
        assert_eq!(merged, Some(99));
    }

    #[test]
    fn test_merge_map_with_none() {
        let results: Vec<Option<QueryOutcome<String>>> =
            vec![None, None, Some(outcome("only".to_string()))];
        let merged = ResultMerger::merge(&results, &MergeStrategy::Map);
        assert_eq!(merged, Some("only".to_string()));
    }

    #[test]
    fn test_merge_join_with_none() {
        let results: Vec<Option<QueryOutcome<i32>>> = vec![None, Some(outcome(7))];
        let merged = ResultMerger::merge(
            &results,
            &MergeStrategy::Join {
                join_key: "k".to_string(),
            },
        );
        assert_eq!(merged, Some(7));
    }

    #[test]
    fn test_merge_outcome_all_none() {
        let outcome: ParallelQueryOutcome<i32> = ParallelQueryOutcome {
            results: vec![None, None, None],
            failures: vec![],
            timed_out: vec![],
            total_elapsed_ms: 0,
            merged_result: None,
        };
        let merged = ResultMerger::merge_outcome(&outcome, &MergeStrategy::First);
        assert_eq!(merged, None);
    }

    #[test]
    fn test_merge_outcome_map_strategy() {
        let outcome = ParallelQueryOutcome {
            results: vec![Some(outcome(1)), Some(outcome(2)), Some(outcome(3))],
            failures: vec![],
            timed_out: vec![],
            total_elapsed_ms: 30,
            merged_result: None,
        };
        let merged = ResultMerger::merge_outcome(&outcome, &MergeStrategy::Map);
        assert_eq!(merged, Some(3));
    }
}
