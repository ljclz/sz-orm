//! 测试超时工具
//!
//! 使用 tokio::time::timeout 包装测试，超时标记失败并输出卡点。

use std::future::Future;
use std::time::Duration;

/// 单方言默认超时：60 秒
pub const SINGLE_DIALECT_TIMEOUT: Duration = Duration::from_secs(60);

/// 全方言默认超时：300 秒
pub const ALL_DIALECT_TIMEOUT: Duration = Duration::from_secs(300);

/// 包装异步测试函数，添加超时保护
pub async fn run_with_timeout<F, T>(test_name: &str, timeout: Duration, test_fn: F) -> T
where
    F: Future<Output = T>,
{
    match tokio::time::timeout(timeout, test_fn).await {
        Ok(result) => result,
        Err(_) => {
            panic!(
                "测试超时: {} 超过 {:?}，可能卡在数据库连接或死锁",
                test_name, timeout
            );
        }
    }
}
