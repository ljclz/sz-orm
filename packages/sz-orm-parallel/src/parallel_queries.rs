//! v6.5.0 多表并行查询高层 API
//!
//! 接收一组独立的 async 查询闭包，并行执行，返回按输入顺序对齐的结果。
//! 委托既有 `ParallelQueryScheduler::parallel` 执行（并发度控制 + 超时 + 失败降级）。
//!
//! 特性：
//! - 空输入拒绝（`ParallelQueryError::NoQueries`）
//! - 单查询失败隔离（`result[i] = Err`，其余照常）
//! - 结果按输入索引对齐（`result[i]` 对应 `queries[i]`）
//! - 并发度控制（`config.concurrency`）
//! - 超时降级（per_query_timeout / overall_timeout）

use futures::future::BoxFuture;

use crate::config::ParallelQueryConfig;
use crate::error::ParallelQueryError;
use crate::outcome::QueryOutcome;
use crate::scheduler::{DefaultLike, ParallelQueryScheduler};

/// 多表并行查询
///
/// 接收一组独立的 async 查询闭包，并行执行，返回按输入顺序对齐的结果。
///
/// # 参数
///
/// - `queries`：查询闭包列表（每个 `FnOnce() -> BoxFuture<Result<QueryOutcome<T>, String>>`）
/// - `config`：并行配置（并发度/超时/降级/合并）
///
/// # 返回
///
/// `Vec<Result<T, String>>`，按输入索引对齐：
/// - `Ok(value)`：查询成功，`value` 为 `QueryOutcome.value`
/// - `Err(msg)`：查询失败或超时
///
/// # 错误
///
/// - `ParallelQueryError::NoQueries`：`queries` 为空
///
/// # 示例
///
/// ```ignore
/// use sz_orm_parallel::parallel_queries;
/// use sz_orm_parallel::config::ParallelQueryConfig;
/// use sz_orm_parallel::outcome::QueryOutcome;
///
/// let queries = vec![
///     Box::pin(async move { Ok(QueryOutcome::new(users, users.len(), 10)) }),
///     Box::pin(async move { Ok(QueryOutcome::new(orders, orders.len(), 15)) }),
/// ];
/// let results = parallel_queries(queries, ParallelQueryConfig::default()).await?;
/// ```
pub async fn parallel_queries<T>(
    queries: Vec<Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<T>, String>> + Send>>,
    config: ParallelQueryConfig,
) -> Result<Vec<Result<T, String>>, ParallelQueryError>
where
    T: DefaultLike + Send + 'static,
{
    if queries.is_empty() {
        return Err(ParallelQueryError::NoQueries);
    }

    let n = queries.len();
    let scheduler = ParallelQueryScheduler::new();
    let outcome = scheduler.parallel(queries, config).await?;

    let mut results: Vec<Result<T, String>> = vec![Err("unknown failure".to_string()); n];

    for idx in &outcome.timed_out {
        if *idx < n {
            results[*idx] = Err("__timeout__".to_string());
        }
    }

    for failure in &outcome.failures {
        if failure.query_index < n {
            results[failure.query_index] = Err(failure.error.clone());
        }
    }

    for (i, opt) in outcome.results.iter().enumerate() {
        if let Some(qo) = opt {
            results[i] = Ok(qo.value.clone());
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::QueryOutcome;

    type QF<T> = Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<T>, String>> + Send>;

    fn ok_query<T: DefaultLike + Send + 'static>(value: T) -> QF<T> {
        Box::new(move || Box::pin(async move { Ok(QueryOutcome::new(value, 1, 0)) }))
    }

    fn err_query<T: DefaultLike + Send + 'static>(err: String) -> QF<T> {
        Box::new(move || Box::pin(async move { Err(err) }))
    }

    #[tokio::test]
    async fn test_empty_input_rejected() {
        let result = parallel_queries::<i64>(vec![], ParallelQueryConfig::default()).await;
        assert!(matches!(result, Err(ParallelQueryError::NoQueries)));
    }

    #[tokio::test]
    async fn test_single_query_success() {
        let queries = vec![ok_query(42i64)];
        let results = parallel_queries(queries, ParallelQueryConfig::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().unwrap(), &42);
    }

    #[tokio::test]
    async fn test_failure_isolation() {
        let queries: Vec<QF<i64>> =
            vec![ok_query(1i64), err_query("db error".into()), ok_query(3i64)];
        let results = parallel_queries(queries, ParallelQueryConfig::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap(), &1);
        assert!(results[1].is_err());
        assert_eq!(results[2].as_ref().unwrap(), &3);
    }

    #[tokio::test]
    async fn test_result_order_alignment() {
        let queries = vec![
            ok_query("first".to_string()),
            ok_query("second".to_string()),
            ok_query("third".to_string()),
        ];
        let results = parallel_queries(queries, ParallelQueryConfig::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap(), "first");
        assert_eq!(results[1].as_ref().unwrap(), "second");
        assert_eq!(results[2].as_ref().unwrap(), "third");
    }

    #[tokio::test]
    async fn test_n_2_parallel() {
        let queries = vec![ok_query(1i64), ok_query(2i64)];
        let results = parallel_queries(queries, ParallelQueryConfig::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_n_10_parallel() {
        let queries: Vec<QF<i64>> = (0..10).map(|i| ok_query(i as i64)).collect();
        let results = parallel_queries(queries, ParallelQueryConfig::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 10);
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.as_ref().unwrap(), &(i as i64));
        }
    }

    #[tokio::test]
    async fn test_all_failures() {
        let queries: Vec<QF<i64>> = vec![err_query("e1".into()), err_query("e2".into())];
        let results = parallel_queries(queries, ParallelQueryConfig::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].is_err());
        assert!(results[1].is_err());
    }

    #[tokio::test]
    async fn test_mixed_ok_err() {
        let queries: Vec<QF<i64>> = vec![
            ok_query(10i64),
            err_query("fail".into()),
            ok_query(30i64),
            err_query("fail2".into()),
        ];
        let results = parallel_queries(queries, ParallelQueryConfig::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 4);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert!(results[2].is_ok());
        assert!(results[3].is_err());
    }

    #[tokio::test]
    async fn test_concurrency_limit() {
        let config = ParallelQueryConfig {
            concurrency: 1,
            ..Default::default()
        };
        let queries = vec![ok_query(1i64), ok_query(2i64), ok_query(3i64)];
        let results = parallel_queries(queries, config).await.unwrap();
        assert_eq!(results.len(), 3);
    }
}
