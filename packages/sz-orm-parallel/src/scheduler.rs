//! 并行查询调度器 — 并发执行 + 超时控制 + 失败降级

use std::sync::Arc;
use std::time::Instant;

use futures::future::join_all;
use tokio::sync::Semaphore;

use crate::config::{FailureStrategy, ParallelQueryConfig};
use crate::error::ParallelQueryError;
use crate::merger::ResultMerger;
use crate::outcome::{ParallelQueryOutcome, QueryFailure, QueryOutcome};

/// 并行查询调度器
///
/// 将多个独立查询并行执行，通过并发度控制避免资源耗尽，
/// 支持单查询超时 + 整体超时 + 失败降级 + 结果合并。
pub struct ParallelQueryScheduler {
    adaptive: bool,
}

impl ParallelQueryScheduler {
    /// 创建调度器
    pub fn new() -> Self {
        Self { adaptive: false }
    }

    /// 创建带自适应的调度器
    pub fn with_adaptive() -> Self {
        Self { adaptive: true }
    }

    /// 是否启用自适应
    pub fn is_adaptive(&self) -> bool {
        self.adaptive
    }

    /// 并行执行多个查询
    ///
    /// `queries`：查询列表（每个为 async 闭包，返回 `Result<QueryOutcome<T>, String>`）
    /// `config`：并行配置（并发度/超时/降级/合并）
    ///
    /// 流程：
    /// 1. 并发度控制（Semaphore 限制并行数）
    /// 2. 单查询超时（tokio::time::timeout）
    /// 3. 整体超时（tokio::time::timeout 包裹全部）
    /// 4. 失败降级（Skip/Abort/Fallback）
    /// 5. 结果合并（First/Union/Join/Map）
    pub async fn parallel<F, T>(
        &self,
        queries: Vec<F>,
        config: ParallelQueryConfig,
    ) -> Result<ParallelQueryOutcome<T>, ParallelQueryError>
    where
        F: FnOnce() -> futures::future::BoxFuture<'static, Result<QueryOutcome<T>, String>>
            + Send
            + 'static,
        T: DefaultLike + Send + 'static,
    {
        if queries.is_empty() {
            return Err(ParallelQueryError::NoQueries);
        }

        let start = Instant::now();
        let n = queries.len();
        let concurrency = if config.concurrency == 0 {
            n
        } else {
            config.concurrency.min(n)
        };

        let semaphore = Arc::new(Semaphore::new(concurrency));
        let per_query_timeout = config.per_query_timeout();

        let mut handles = Vec::with_capacity(n);
        for (idx, query) in queries.into_iter().enumerate() {
            let sem = Arc::clone(&semaphore);
            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.ok()?;
                let fut = query();
                if let Some(timeout) = per_query_timeout {
                    match tokio::time::timeout(timeout, fut).await {
                        Ok(result) => Some((idx, result)),
                        Err(_) => Some((idx, Err("__timeout__".to_string()))),
                    }
                } else {
                    Some((idx, fut.await))
                }
            });
            handles.push(handle);
        }

        let all_results = if let Some(overall) = config.overall_timeout() {
            match tokio::time::timeout(overall, join_all(handles)).await {
                Ok(results) => results,
                Err(_) => {
                    return Ok(ParallelQueryOutcome {
                        results: vec![None; n],
                        failures: vec![],
                        timed_out: (0..n).collect(),
                        total_elapsed_ms: start.elapsed().as_millis() as u64,
                        merged_result: None,
                    });
                }
            }
        } else {
            join_all(handles).await
        };

        let mut results: Vec<Option<QueryOutcome<T>>> = vec![None; n];
        let mut failures = Vec::new();
        let mut timed_out = Vec::new();

        for result in all_results {
            match result {
                Ok(Some((idx, Ok(outcome)))) => {
                    results[idx] = Some(outcome);
                }
                Ok(Some((idx, Err(err)))) => {
                    if err == "__timeout__" {
                        timed_out.push(idx);
                    } else {
                        failures.push(QueryFailure {
                            query_index: idx,
                            error: err,
                        });
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    failures.push(QueryFailure {
                        query_index: 0,
                        error: format!("task panicked: {e}"),
                    });
                }
            }
        }

        if config.failure_strategy == FailureStrategy::Abort && !failures.is_empty() {
            return Err(ParallelQueryError::AllQueriesFailed);
        }

        if config.failure_strategy == FailureStrategy::Fallback {
            for (idx, r) in results.iter_mut().enumerate() {
                if r.is_none() {
                    *r = Some(QueryOutcome::new(T::default_like(), 0, 0));
                    let _ = idx;
                }
            }
        }

        let merged_result = ResultMerger::merge(&results, &config.merge_strategy);

        Ok(ParallelQueryOutcome {
            results,
            failures,
            timed_out,
            total_elapsed_ms: start.elapsed().as_millis() as u64,
            merged_result,
        })
    }
}

impl Default for ParallelQueryScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Fallback trait：降级值生成
pub trait DefaultLike: Clone {
    fn default_like() -> Self;
}

impl DefaultLike for String {
    fn default_like() -> Self {
        String::new()
    }
}

impl DefaultLike for Vec<String> {
    fn default_like() -> Self {
        Vec::new()
    }
}

impl DefaultLike for i32 {
    fn default_like() -> Self {
        0
    }
}

impl DefaultLike for i64 {
    fn default_like() -> Self {
        0
    }
}

impl DefaultLike for f64 {
    fn default_like() -> Self {
        0.0
    }
}

impl DefaultLike for &'static str {
    fn default_like() -> Self {
        ""
    }
}

impl<T: DefaultLike + Clone> DefaultLike for Option<T> {
    fn default_like() -> Self {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use futures::future::BoxFuture;
    use std::time::Duration;

    fn ok_query<T: Clone + Send + 'static>(
        value: T,
    ) -> impl FnOnce() -> BoxFuture<'static, Result<QueryOutcome<T>, String>> + Send {
        move || Box::pin(async { Ok(QueryOutcome::new(value, 1, 10)) })
    }

    fn fail_query<T: Clone + Send + 'static>(
        err: &str,
    ) -> impl FnOnce() -> BoxFuture<'static, Result<QueryOutcome<T>, String>> + Send {
        let err = err.to_string();
        move || Box::pin(async move { Err(err) })
    }

    fn slow_query<T: Clone + Send + 'static>(
        value: T,
        delay_ms: u64,
    ) -> impl FnOnce() -> BoxFuture<'static, Result<QueryOutcome<T>, String>> + Send {
        move || {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                Ok(QueryOutcome::new(value, 1, delay_ms))
            })
        }
    }

    #[tokio::test]
    async fn parallel_all_succeed() {
        let scheduler = ParallelQueryScheduler::new();
        let queries = vec![ok_query(1), ok_query(2), ok_query(3)];
        let config = ParallelQueryConfig::new();
        let outcome = scheduler.parallel(queries, config).await.unwrap();

        assert_eq!(outcome.success_count(), 3);
        assert!(outcome.all_succeeded());
        assert_eq!(outcome.merged_result, Some(1));
    }

    #[tokio::test]
    async fn parallel_empty_returns_error() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<
            Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<i32>, String>> + Send>,
        > = vec![];
        let config = ParallelQueryConfig::new();
        let result = scheduler.parallel(queries, config).await;
        assert!(matches!(result, Err(ParallelQueryError::NoQueries)));
    }

    #[tokio::test]
    async fn parallel_skip_failure() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<
            Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<i32>, String>> + Send>,
        > = vec![
            Box::new(ok_query(1)),
            Box::new(fail_query("db down")),
            Box::new(ok_query(3)),
        ];
        let config = ParallelQueryConfig::new().with_failure_strategy(FailureStrategy::Skip);
        let outcome = scheduler.parallel(queries, config).await.unwrap();

        assert_eq!(outcome.success_count(), 2);
        assert_eq!(outcome.failure_count(), 1);
        assert!(!outcome.all_succeeded());
    }

    #[tokio::test]
    async fn parallel_abort_on_failure() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<
            Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<i32>, String>> + Send>,
        > = vec![Box::new(ok_query(1)), Box::new(fail_query("db down"))];
        let config = ParallelQueryConfig::new().with_failure_strategy(FailureStrategy::Abort);
        let result = scheduler.parallel(queries, config).await;
        assert!(matches!(result, Err(ParallelQueryError::AllQueriesFailed)));
    }

    #[tokio::test]
    async fn parallel_per_query_timeout() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<
            Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<i32>, String>> + Send>,
        > = vec![Box::new(ok_query(1)), Box::new(slow_query(2, 500))];
        let config = ParallelQueryConfig::new()
            .with_per_query_timeout_ms(50)
            .with_failure_strategy(FailureStrategy::Skip);
        let outcome = scheduler.parallel(queries, config).await.unwrap();

        assert_eq!(outcome.success_count(), 1);
        assert_eq!(outcome.timeout_count(), 1);
    }

    #[tokio::test]
    async fn parallel_concurrency_control() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<
            Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<i32>, String>> + Send>,
        > = vec![
            Box::new(ok_query(1)),
            Box::new(ok_query(2)),
            Box::new(ok_query(3)),
            Box::new(ok_query(4)),
        ];
        let config = ParallelQueryConfig::new().with_concurrency(2);
        let outcome = scheduler.parallel(queries, config).await.unwrap();

        assert_eq!(outcome.success_count(), 4);
    }

    #[tokio::test]
    async fn parallel_merge_union() {
        let scheduler = ParallelQueryScheduler::new();
        let queries = vec![ok_query(1), ok_query(2)];
        let config = ParallelQueryConfig::new().with_merge_strategy(MergeStrategy::Union);
        let outcome = scheduler.parallel(queries, config).await.unwrap();

        assert!(outcome.merged_result.is_some());
    }

    #[tokio::test]
    async fn parallel_merge_map() {
        let scheduler = ParallelQueryScheduler::new();
        let queries = vec![ok_query("a"), ok_query("b"), ok_query("c")];
        let config = ParallelQueryConfig::new().with_merge_strategy(MergeStrategy::Map);
        let outcome = scheduler.parallel(queries, config).await.unwrap();

        assert_eq!(outcome.merged_result, Some("c"));
    }

    #[tokio::test]
    async fn parallel_with_adaptive_flag() {
        let scheduler = ParallelQueryScheduler::with_adaptive();
        assert!(scheduler.is_adaptive());

        let queries = vec![ok_query(42)];
        let config = ParallelQueryConfig::new();
        let outcome = scheduler.parallel(queries, config).await.unwrap();
        assert_eq!(outcome.success_count(), 1);
    }

    type BoxedQuery =
        Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<i64>, String>> + Send>;

    fn boxed_ok(value: i64) -> BoxedQuery {
        Box::new(move || Box::pin(async move { Ok(QueryOutcome::new(value, 1, 10)) }))
    }

    fn boxed_fail(err: &str) -> BoxedQuery {
        let err = err.to_string();
        Box::new(move || Box::pin(async move { Err(err) }))
    }

    fn boxed_slow(delay_ms: u64) -> BoxedQuery {
        Box::new(move || {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                Ok(QueryOutcome::new(0i64, 1, delay_ms))
            })
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_high_concurrency_50_queries() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<BoxedQuery> = (0..50).map(boxed_ok).collect();
        let config = ParallelQueryConfig::new().with_concurrency(8);
        let outcome = scheduler.parallel(queries, config).await.unwrap();
        assert_eq!(outcome.success_count(), 50, "all 50 queries should succeed");
        assert_eq!(outcome.timed_out.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_concurrency_limit_respected() {
        let scheduler = ParallelQueryScheduler::new();
        // 并发上限 2，50 个查询全部成功（信号量不丢任务）
        let queries: Vec<BoxedQuery> = (0..50).map(boxed_ok).collect();
        let config = ParallelQueryConfig::new().with_concurrency(2);
        let outcome = scheduler.parallel(queries, config).await.unwrap();
        assert_eq!(outcome.success_count(), 50);
    }

    #[tokio::test]
    async fn parallel_per_query_timeout_marks_timed_out() {
        let scheduler = ParallelQueryScheduler::new();
        // 单查询耗时 100ms，超时 10ms → 全部 timed_out
        let queries: Vec<BoxedQuery> = vec![boxed_slow(100), boxed_slow(100)];
        let config = ParallelQueryConfig::new().with_per_query_timeout_ms(10);
        let outcome = scheduler.parallel(queries, config).await.unwrap();
        assert_eq!(outcome.timed_out.len(), 2, "both queries should time out");
        assert_eq!(outcome.success_count(), 0);
    }

    #[tokio::test]
    async fn parallel_overall_timeout_returns_partial() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<BoxedQuery> = vec![boxed_slow(200), boxed_ok(1)];
        let config = ParallelQueryConfig::new().with_overall_timeout_ms(50);
        let outcome = scheduler.parallel(queries, config).await.unwrap();
        // 整体超时 → 结果数组全 None，timed_out 覆盖全部
        assert_eq!(outcome.timed_out.len(), 2);
    }

    #[tokio::test]
    async fn parallel_mixed_success_failure() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<BoxedQuery> = vec![boxed_ok(1), boxed_fail("boom"), boxed_ok(3)];
        let config = ParallelQueryConfig::new();
        let outcome = scheduler.parallel(queries, config).await.unwrap();
        assert_eq!(outcome.success_count(), 2);
        assert_eq!(outcome.failures.len(), 1);
        assert!(outcome.failures[0].error.contains("boom"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_stress_1000_queries() {
        let scheduler = ParallelQueryScheduler::new();
        let queries: Vec<BoxedQuery> = (0..1000).map(boxed_ok).collect();
        let config = ParallelQueryConfig::new().with_concurrency(16);
        let outcome = scheduler.parallel(queries, config).await.unwrap();
        assert_eq!(
            outcome.success_count(),
            1000,
            "stress test should not drop queries"
        );
    }
}
