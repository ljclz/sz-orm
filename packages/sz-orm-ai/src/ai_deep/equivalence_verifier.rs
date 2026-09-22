//! 重写等价性验证器
//!
//! 在真实 DB 上执行原 SQL 与改写 SQL，对比结果集。
//! 一致返回 `EquivalenceResult::Equivalent`；否则 `NotEquivalent`。
//! 使用 trait `DbExecutor` 解耦具体 DB 驱动，调用方提供具体实现（如 sqlx）。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{AiDeepError, EquivalenceResult};

/// DB 执行器 trait（供 RewriteEquivalenceVerifier 使用）
///
/// 调用方提供具体实现（如 sqlx 执行器），源码不耦合具体 DB 驱动。
pub trait DbExecutor: Send + Sync {
    /// 执行 SQL 并返回结果集（每行为 Vec<String> 列值）
    fn execute(
        &self,
        sql: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<String>>, String>> + Send + '_>>;

    /// 检查连接是否可用
    fn is_connected(&self) -> bool {
        true
    }
}

/// 等价性验证配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquivalenceConfig {
    /// 最大对比行数（防止大结果集 OOM）
    pub max_compare_rows: usize,
    /// 是否严格模式（严格模式要求列顺序一致）
    pub strict_mode: bool,
}

impl Default for EquivalenceConfig {
    fn default() -> Self {
        Self {
            max_compare_rows: 10000,
            strict_mode: false,
        }
    }
}

/// 重写等价性验证器
///
/// 在真实 DB 上执行原 SQL 与改写 SQL，对比结果集。
/// 一致返回 `EquivalenceResult::Equivalent`；否则 `NotEquivalent`。
pub struct RewriteEquivalenceVerifier {
    executor: Arc<dyn DbExecutor>,
    config: EquivalenceConfig,
}

impl RewriteEquivalenceVerifier {
    /// 创建等价性验证器
    pub fn new(executor: Arc<dyn DbExecutor>, config: EquivalenceConfig) -> Self {
        Self { executor, config }
    }

    /// 使用默认配置创建
    pub fn with_default_config(executor: Arc<dyn DbExecutor>) -> Self {
        Self::new(executor, EquivalenceConfig::default())
    }

    /// 验证两 SQL 是否等价
    ///
    /// 执行原 SQL 与改写 SQL，对比结果集。
    /// - 结果集完全相同 → `EquivalenceResult::Equivalent`
    /// - 结果集不同 → `EquivalenceResult::NotEquivalent`
    /// - DB 连接失败 → `AiDeepError::DbConnectionFailed`
    pub async fn verify(
        &self,
        original_sql: &str,
        rewritten_sql: &str,
    ) -> Result<EquivalenceResult, AiDeepError> {
        if !self.executor.is_connected() {
            return Err(AiDeepError::DbConnectionFailed(
                "DB 执行器未连接".to_string(),
            ));
        }

        let original_rows = self
            .executor
            .execute(original_sql)
            .await
            .map_err(|e| AiDeepError::DbConnectionFailed(format!("原 SQL 执行失败：{}", e)))?;

        let rewritten_rows =
            self.executor.execute(rewritten_sql).await.map_err(|e| {
                AiDeepError::DbConnectionFailed(format!("改写 SQL 执行失败：{}", e))
            })?;

        let orig_count = original_rows.len();
        let rewrite_count = rewritten_rows.len();

        if orig_count != rewrite_count {
            return Ok(EquivalenceResult::not_equivalent(
                orig_count,
                rewrite_count,
                format!(
                    "行数不匹配：原 SQL {} 行，改写 SQL {} 行",
                    orig_count, rewrite_count
                ),
            ));
        }

        if orig_count > self.config.max_compare_rows {
            return Ok(EquivalenceResult::not_equivalent(
                orig_count,
                rewrite_count,
                format!(
                    "结果集过大（{} 行 > 上限 {}），跳过逐行对比",
                    orig_count, self.config.max_compare_rows
                ),
            ));
        }

        for (i, (orig_row, rewrite_row)) in
            original_rows.iter().zip(rewritten_rows.iter()).enumerate()
        {
            if self.config.strict_mode {
                if orig_row != rewrite_row {
                    return Ok(EquivalenceResult::not_equivalent(
                        orig_count,
                        rewrite_count,
                        format!(
                            "第 {} 行内容不匹配（严格模式）：原 {:?} vs 改写 {:?}",
                            i + 1,
                            orig_row,
                            rewrite_row
                        ),
                    ));
                }
            } else {
                let mut orig_sorted = orig_row.clone();
                let mut rewrite_sorted = rewrite_row.clone();
                orig_sorted.sort();
                rewrite_sorted.sort();
                if orig_sorted != rewrite_sorted {
                    return Ok(EquivalenceResult::not_equivalent(
                        orig_count,
                        rewrite_count,
                        format!(
                            "第 {} 行内容不匹配（非严格模式，列顺序无关）：原 {:?} vs 改写 {:?}",
                            i + 1,
                            orig_row,
                            rewrite_row
                        ),
                    ));
                }
            }
        }

        Ok(EquivalenceResult::equivalent(orig_count))
    }

    /// 获取配置
    pub fn config(&self) -> &EquivalenceConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 内存执行器（模拟 DB）
    struct MemoryExecutor {
        rows: Vec<Vec<String>>,
        connected: bool,
    }

    impl MemoryExecutor {
        fn new(rows: Vec<Vec<String>>) -> Self {
            Self {
                rows,
                connected: true,
            }
        }

        fn disconnected() -> Self {
            Self {
                rows: vec![],
                connected: false,
            }
        }
    }

    impl DbExecutor for MemoryExecutor {
        fn execute(
            &self,
            _sql: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<String>>, String>> + Send + '_>> {
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }

        fn is_connected(&self) -> bool {
            self.connected
        }
    }

    /// 固定结果执行器（不同 SQL 返回不同结果）
    struct FixedExecutor {
        results: std::collections::HashMap<String, Vec<Vec<String>>>,
    }

    impl FixedExecutor {
        fn new(results: Vec<(&str, Vec<Vec<String>>)>) -> Self {
            Self {
                results: results
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            }
        }
    }

    impl DbExecutor for FixedExecutor {
        fn execute(
            &self,
            sql: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<String>>, String>> + Send + '_>> {
            let result = self
                .results
                .get(sql)
                .cloned()
                .unwrap_or_else(|| Err(format!("SQL 未注册: {}", sql)).unwrap_or_default());
            Box::pin(async move { Ok(result) })
        }
    }

    #[tokio::test]
    async fn test_verify_equivalent_sql() {
        let executor = Arc::new(MemoryExecutor::new(vec![
            vec!["1".to_string(), "Alice".to_string()],
            vec!["2".to_string(), "Bob".to_string()],
        ]));
        let verifier = RewriteEquivalenceVerifier::with_default_config(executor);
        let result = verifier
            .verify("SELECT * FROM users", "SELECT * FROM users ORDER BY id")
            .await
            .unwrap();
        assert!(result.is_equivalent);
        assert_eq!(result.original_row_count, 2);
        assert_eq!(result.rewritten_row_count, 2);
    }

    #[tokio::test]
    async fn test_verify_not_equivalent_different_row_count() {
        let executor = Arc::new(FixedExecutor::new(vec![
            (
                "SELECT * FROM users",
                vec![vec!["1".to_string()], vec!["2".to_string()]],
            ),
            (
                "SELECT * FROM users WHERE id = 1",
                vec![vec!["1".to_string()]],
            ),
        ]));
        let verifier = RewriteEquivalenceVerifier::with_default_config(executor);
        let result = verifier
            .verify("SELECT * FROM users", "SELECT * FROM users WHERE id = 1")
            .await
            .unwrap();
        assert!(!result.is_equivalent);
        assert_eq!(result.original_row_count, 2);
        assert_eq!(result.rewritten_row_count, 1);
        assert!(result
            .diff_details
            .as_deref()
            .unwrap()
            .contains("行数不匹配"));
    }

    #[tokio::test]
    async fn test_verify_not_equivalent_different_content() {
        let executor = Arc::new(FixedExecutor::new(vec![
            (
                "SELECT * FROM users",
                vec![vec!["1".to_string(), "Alice".to_string()]],
            ),
            (
                "SELECT * FROM users_rewritten",
                vec![vec!["1".to_string(), "Bob".to_string()]],
            ),
        ]));
        let verifier = RewriteEquivalenceVerifier::with_default_config(executor);
        let result = verifier
            .verify("SELECT * FROM users", "SELECT * FROM users_rewritten")
            .await
            .unwrap();
        assert!(!result.is_equivalent);
        assert!(result
            .diff_details
            .as_deref()
            .unwrap()
            .contains("内容不匹配"));
    }

    #[tokio::test]
    async fn test_verify_db_connection_failed() {
        let executor = Arc::new(MemoryExecutor::disconnected());
        let verifier = RewriteEquivalenceVerifier::with_default_config(executor);
        let result = verifier
            .verify("SELECT * FROM users", "SELECT * FROM users")
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AiDeepError::DbConnectionFailed(_)));
        assert_eq!(err.error_code(), "DB_CONNECTION_FAILED");
    }

    #[tokio::test]
    async fn test_verify_empty_result_equivalent() {
        let executor = Arc::new(MemoryExecutor::new(vec![]));
        let verifier = RewriteEquivalenceVerifier::with_default_config(executor);
        let result = verifier
            .verify(
                "SELECT * FROM users WHERE 1=0",
                "SELECT * FROM users WHERE 1=0",
            )
            .await
            .unwrap();
        assert!(result.is_equivalent);
        assert_eq!(result.original_row_count, 0);
    }

    #[tokio::test]
    async fn test_verify_strict_mode_column_order() {
        let executor = Arc::new(FixedExecutor::new(vec![
            (
                "SELECT a, b FROM t",
                vec![vec!["1".to_string(), "2".to_string()]],
            ),
            (
                "SELECT b, a FROM t",
                vec![vec!["2".to_string(), "1".to_string()]],
            ),
        ]));
        let strict_verifier = RewriteEquivalenceVerifier::new(
            executor.clone(),
            EquivalenceConfig {
                max_compare_rows: 10000,
                strict_mode: true,
            },
        );
        let result = strict_verifier
            .verify("SELECT a, b FROM t", "SELECT b, a FROM t")
            .await
            .unwrap();
        assert!(!result.is_equivalent);

        let non_strict_verifier = RewriteEquivalenceVerifier::new(
            executor,
            EquivalenceConfig {
                max_compare_rows: 10000,
                strict_mode: false,
            },
        );
        let result = non_strict_verifier
            .verify("SELECT a, b FROM t", "SELECT b, a FROM t")
            .await
            .unwrap();
        assert!(result.is_equivalent);
    }

    #[tokio::test]
    async fn test_verify_max_compare_rows_exceeded() {
        let large_result: Vec<Vec<String>> = (0..100).map(|i| vec![i.to_string()]).collect();
        let executor = Arc::new(MemoryExecutor::new(large_result));
        let verifier = RewriteEquivalenceVerifier::new(
            executor,
            EquivalenceConfig {
                max_compare_rows: 50,
                strict_mode: false,
            },
        );
        let result = verifier
            .verify("SELECT * FROM big_table", "SELECT * FROM big_table")
            .await
            .unwrap();
        assert!(!result.is_equivalent);
        assert!(result
            .diff_details
            .as_deref()
            .unwrap()
            .contains("结果集过大"));
    }
}
