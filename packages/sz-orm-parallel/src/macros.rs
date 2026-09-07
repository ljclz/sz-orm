//! v6.5.0 parallel_join! 辅助宏
//!
//! 展开为 `tokio::join!` + 结果收集为 `Vec` 按顺序对齐。
//! 支持 2/3/4 路展开，覆盖常见场景。
//!
//! 语义等价 `tokio::join!`，但返回 `Vec` 便于动态处理结果。

/// 并行 join 宏
///
/// 展开为 `tokio::join!`，结果收集为 `Vec` 按输入顺序对齐。
///
/// # 示例
///
/// ```ignore
/// let results = parallel_join!(
///     async { Ok(1) },
///     async { Ok(2) },
/// ).await;
/// // results: Vec<Result<i32, _>> = vec![Ok(1), Ok(2)]
/// ```
#[macro_export]
macro_rules! parallel_join {
    ($f1:expr, $f2:expr $(,)?) => {{
        async move {
            let (r1, r2) = tokio::join!($f1, $f2);
            vec![r1, r2]
        }
    }};
    ($f1:expr, $f2:expr, $f3:expr $(,)?) => {{
        async move {
            let (r1, r2, r3) = tokio::join!($f1, $f2, $f3);
            vec![r1, r2, r3]
        }
    }};
    ($f1:expr, $f2:expr, $f3:expr, $f4:expr $(,)?) => {{
        async move {
            let (r1, r2, r3, r4) = tokio::join!($f1, $f2, $f3, $f4);
            vec![r1, r2, r3, r4]
        }
    }};
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_2_way_order() {
        let results: Vec<Result<i32, &str>> =
            parallel_join!(async { Ok(1) }, async { Ok(2) },).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], Ok(1));
        assert_eq!(results[1], Ok(2));
    }

    #[tokio::test]
    async fn test_3_way_order() {
        let results: Vec<Result<i32, &str>> =
            parallel_join!(async { Ok(10) }, async { Ok(20) }, async { Ok(30) },).await;
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], Ok(10));
        assert_eq!(results[1], Ok(20));
        assert_eq!(results[2], Ok(30));
    }

    #[tokio::test]
    async fn test_4_way_order() {
        let results: Vec<Result<i32, &str>> =
            parallel_join!(async { Ok(1) }, async { Ok(2) }, async { Ok(3) }, async {
                Ok(4)
            },)
            .await;
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], Ok(1));
        assert_eq!(results[1], Ok(2));
        assert_eq!(results[2], Ok(3));
        assert_eq!(results[3], Ok(4));
    }

    #[tokio::test]
    async fn test_mixed_ok_err() {
        let results: Vec<Result<i32, &str>> =
            parallel_join!(async { Ok(1) }, async { Err("fail") }, async { Ok(3) },).await;
        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert!(results[2].is_ok());
    }
}
