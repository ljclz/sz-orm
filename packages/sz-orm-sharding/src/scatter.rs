//! # 跨分片聚合（Scatter-Gather）
//!
//! 提供 [`ScatterGather`] 用于跨多个 shard 并行执行查询并合并结果。
//!
//! ## 设计
//!
//! - 不引入 async 依赖，使用 `std::thread::scope`（Rust 1.63+ 稳定）实现并行
//! - `broadcast`：对所有 shard 并行调用同一闭包
//! - `scatter_by_keys`：先按 router 分组 key，每个 shard 调用一次（传入该 shard 的所有 key）
//! - `merge`：合并多个分片结果，遇错立即返回

use crate::{ShardingError, ShardingRouter};
use std::collections::HashMap;
use std::sync::Arc;

/// 跨分片聚合器
///
/// 持有 `Arc<ShardingRouter>`，可被多线程共享。
pub struct ScatterGather {
    router: Arc<ShardingRouter>,
}

impl ScatterGather {
    /// 创建 ScatterGather
    pub fn new(router: Arc<ShardingRouter>) -> Self {
        Self { router }
    }

    /// 广播到所有 shard，并行执行查询
    ///
    /// 对 `router.query_all()` 返回的每个 shard 调用一次 `f(shard)`，
    /// 使用 `std::thread::scope` 并行执行。
    ///
    /// # 返回
    ///
    /// 返回 `Vec<Result<T, ShardingError>>`，每个元素对应一个 shard 的执行结果，
    /// 顺序与 `query_all()` 一致。若某线程 panic，对应位置为 `Err(ThreadPanic)`。
    pub fn broadcast<F, T>(&self, f: F) -> Vec<Result<T, ShardingError>>
    where
        F: Fn(&str) -> Result<T, ShardingError> + Send + Sync,
        T: Send,
    {
        let shards = self.router.query_all();
        if shards.is_empty() {
            return Vec::new();
        }
        std::thread::scope(|s| {
            let handles: Vec<_> = shards
                .iter()
                .map(|shard| s.spawn(|| f(shard.as_str())))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err(ShardingError::ThreadPanic)))
                .collect()
        })
    }

    /// 按 keys 路由后分组执行（每个 shard 一次调用，传入该 shard 的所有 keys）
    ///
    /// 1. 用 `router.route(key)` 将每个 key 分配到对应 shard
    /// 2. 对每个命中的 shard 调用一次 `f(shard, &[keys])`
    /// 3. 并行执行（`std::thread::scope`）
    ///
    /// 无法路由的 key 会被静默跳过。
    ///
    /// # 返回
    ///
    /// 返回 `Vec<Result<T, ShardingError>>`，每个元素对应一个命中 shard 的执行结果。
    pub fn scatter_by_keys<F, T>(&self, keys: &[String], f: F) -> Vec<Result<T, ShardingError>>
    where
        F: Fn(&str, &[String]) -> Result<T, ShardingError> + Send + Sync,
        T: Send,
    {
        // 按 shard 分组
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for key in keys {
            if let Ok(shard) = self.router.route(key) {
                groups
                    .entry(shard.to_string())
                    .or_default()
                    .push(key.clone());
            }
            // 不可路由的 key 被跳过
        }
        if groups.is_empty() {
            return Vec::new();
        }
        // 转为 Vec 以便在 scope 中按索引稳定引用
        let groups_vec: Vec<(String, Vec<String>)> = groups.into_iter().collect();
        std::thread::scope(|s| {
            let handles: Vec<_> = groups_vec
                .iter()
                .map(|(shard, shard_keys)| s.spawn(|| f(shard.as_str(), shard_keys)))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err(ShardingError::ThreadPanic)))
                .collect()
        })
    }

    /// 合并多个分片的结果（用户提供的合并函数）
    ///
    /// 遍历 `results`：遇到第一个 `Err` 立即返回该错误；全部 `Ok` 则调用 `merger` 合并。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let results: Vec<Result<i32, _>> = vec![Ok(1), Ok(2), Ok(3)];
    /// let sum = ScatterGather::merge(results, |vs| Ok(vs.iter().sum())).unwrap();
    /// assert_eq!(sum, 6);
    /// ```
    pub fn merge<T, F>(results: Vec<Result<T, ShardingError>>, merger: F) -> Result<T, ShardingError>
    where
        F: FnOnce(Vec<T>) -> Result<T, ShardingError>,
    {
        let mut oks: Vec<T> = Vec::with_capacity(results.len());
        for r in results {
            let t = r?;
            oks.push(t);
        }
        merger(oks)
    }
}

impl std::fmt::Debug for ScatterGather {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScatterGather")
            .field("shard_count", &self.router.shard_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShardingStrategy;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[test]
    fn test_broadcast_all_shards() {
        let router = Arc::new(ShardingRouter::new(
            ShardingStrategy::Hash,
            vec!["s0", "s1", "s2"],
        ));
        let sg = ScatterGather::new(router);
        let results = sg.broadcast(|shard| Ok(shard.to_string()));
        assert_eq!(results.len(), 3);
        let mut shards: HashSet<String> = results.into_iter().map(|r| r.unwrap()).collect();
        assert!(shards.remove("s0"));
        assert!(shards.remove("s1"));
        assert!(shards.remove("s2"));
    }

    #[test]
    fn test_broadcast_empty_shards() {
        let router = Arc::new(ShardingRouter::new(ShardingStrategy::Hash, vec![]));
        let sg = ScatterGather::new(router);
        let results: Vec<Result<String, ShardingError>> = sg.broadcast(|_| Ok("x".to_string()));
        assert!(results.is_empty());
    }

    #[test]
    fn test_broadcast_single_shard() {
        let router = Arc::new(ShardingRouter::new(ShardingStrategy::Hash, vec!["only"]));
        let sg = ScatterGather::new(router);
        let results = sg.broadcast(|s| Ok(s.to_string()));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().unwrap(), &"only");
    }

    #[test]
    fn test_broadcast_parallel_execution() {
        // 通过 sleep 验证并行：3 个 shard 各 sleep 60ms，总耗时应 < 180ms（串行）
        let router = Arc::new(ShardingRouter::new(
            ShardingStrategy::Hash,
            vec!["s0", "s1", "s2"],
        ));
        let sg = ScatterGather::new(router);
        let start = Instant::now();
        let results = sg.broadcast(|_| {
            std::thread::sleep(Duration::from_millis(60));
            Ok(1u32)
        });
        let elapsed = start.elapsed();
        assert_eq!(results.len(), 3);
        assert!(
            elapsed < Duration::from_millis(170),
            "并行执行总耗时应远小于串行 180ms，实际: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_broadcast_collects_values() {
        let router = Arc::new(ShardingRouter::new(
            ShardingStrategy::Hash,
            vec!["a", "b", "c", "d"],
        ));
        let sg = ScatterGather::new(router);
        let results = sg.broadcast(|shard| Ok(shard.len() as i32));
        let sum: i32 = results.into_iter().map(|r| r.unwrap()).sum();
        // 每个 shard 名长度都是 1，共 4 个 → 4
        assert_eq!(sum, 4);
    }

    #[test]
    fn test_scatter_by_keys_groups() {
        let router = Arc::new(ShardingRouter::new(
            ShardingStrategy::Hash,
            vec!["s0", "s1", "s2", "s3"],
        ));
        let sg = ScatterGather::new(router);
        let keys: Vec<String> = (0..100).map(|i| format!("key_{}", i)).collect();
        let results = sg.scatter_by_keys(&keys, |shard, ks| Ok((shard.to_string(), ks.len())));
        let mut total = 0usize;
        for r in results {
            let (shard, count) = r.unwrap();
            assert!(shard.starts_with('s'));
            total += count;
        }
        assert_eq!(total, 100);
    }

    #[test]
    fn test_scatter_by_keys_one_call_per_shard() {
        let router = Arc::new(ShardingRouter::new(
            ShardingStrategy::Hash,
            vec!["s0", "s1"],
        ));
        let sg = ScatterGather::new(router);
        let call_count = Arc::new(AtomicUsize::new(0));
        let keys: Vec<String> = (0..50).map(|i| format!("k{}", i)).collect();
        let cc = call_count.clone();
        let results = sg.scatter_by_keys(&keys, move |_shard, ks| {
            cc.fetch_add(1, Ordering::SeqCst);
            Ok(ks.len())
        });
        let calls = call_count.load(Ordering::SeqCst);
        // 50 个 key 哈希到 2 个 shard，命中的 shard 数应为 1 或 2
        assert!(calls == 1 || calls == 2, "calls should be 1 or 2, got {}", calls);
        assert_eq!(results.len(), calls);
        let total: usize = results.into_iter().map(|r| r.unwrap()).sum();
        assert_eq!(total, 50);
    }

    #[test]
    fn test_scatter_by_keys_empty() {
        let router = Arc::new(ShardingRouter::new(
            ShardingStrategy::Hash,
            vec!["s0", "s1"],
        ));
        let sg = ScatterGather::new(router);
        let results: Vec<Result<u32, ShardingError>> = sg.scatter_by_keys(&[], |_, _| Ok(1));
        assert!(results.is_empty());
    }

    #[test]
    fn test_scatter_by_keys_returns_keys_per_shard() {
        // 用 List 策略让特定 key 路由到特定 shard，验证分组正确性
        let mut keys = HashSet::new();
        keys.insert("h1".to_string());
        keys.insert("h2".to_string());
        let router = Arc::new(ShardingRouter::new_list(
            keys,
            "hit_shard".to_string(),
            Some("default_shard".to_string()),
        ));
        let sg = ScatterGather::new(router);
        let input = vec![
            "h1".to_string(),
            "h2".to_string(),
            "miss1".to_string(),
            "miss2".to_string(),
        ];
        let results = sg.scatter_by_keys(&input, |shard, ks| Ok((shard.to_string(), ks.to_vec())));
        // 应有 2 个分组：hit_shard（2 个 key）和 default_shard（2 个 key）
        let mut hit_count = 0;
        let mut default_count = 0;
        for r in results {
            let (shard, ks) = r.unwrap();
            match shard.as_str() {
                "hit_shard" => {
                    assert_eq!(ks.len(), 2);
                    assert!(ks.contains(&"h1".to_string()));
                    assert!(ks.contains(&"h2".to_string()));
                    hit_count += 1;
                }
                "default_shard" => {
                    assert_eq!(ks.len(), 2);
                    assert!(ks.contains(&"miss1".to_string()));
                    assert!(ks.contains(&"miss2".to_string()));
                    default_count += 1;
                }
                _ => panic!("unexpected shard: {}", shard),
            }
        }
        assert_eq!(hit_count, 1);
        assert_eq!(default_count, 1);
    }

    #[test]
    fn test_merge_all_ok() {
        let results: Vec<Result<i32, ShardingError>> = vec![Ok(1), Ok(2), Ok(3)];
        let merged = ScatterGather::merge(results, |vs| Ok(vs.iter().sum())).unwrap();
        assert_eq!(merged, 6);
    }

    #[test]
    fn test_merge_propagates_first_error() {
        let results: Vec<Result<i32, ShardingError>> = vec![
            Ok(1),
            Err(ShardingError::ThreadPanic),
            Ok(3),
        ];
        let result = ScatterGather::merge(results, |vs| Ok(vs.iter().sum()));
        assert!(matches!(result, Err(ShardingError::ThreadPanic)));
    }

    #[test]
    fn test_merge_empty() {
        let results: Vec<Result<i32, ShardingError>> = vec![];
        let merged = ScatterGather::merge(results, |vs| Ok(vs.len() as i32)).unwrap();
        assert_eq!(merged, 0);
    }

    #[test]
    fn test_merge_single_value() {
        let results: Vec<Result<String, ShardingError>> = vec![Ok("only".to_string())];
        let merged = ScatterGather::merge(results, |mut vs| Ok(vs.remove(0))).unwrap();
        assert_eq!(merged, "only");
    }

    #[test]
    fn test_debug_format() {
        let router = Arc::new(ShardingRouter::new(ShardingStrategy::Hash, vec!["s0"]));
        let sg = ScatterGather::new(router);
        let s = format!("{:?}", sg);
        assert!(s.contains("ScatterGather"));
        assert!(s.contains("shard_count"));
    }
}
