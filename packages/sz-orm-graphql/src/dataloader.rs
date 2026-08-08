//! DataLoader — N+1 自动消除批量加载器
//!
//! 在单个事件循环 tick 内收集多个 `load` 请求，合并为一次批量调用，
//! 结果按键映射回填各请求点，保持原始请求顺序。
//!
//! # 设计
//!
//! - `load(key)` 收集请求到 pending 并返回 oneshot Receiver
//! - 当前 tick 结束（`yield_now`）时触发 `batch_load`
//! - 键去重避免重复查询
//! - 结果按键映射回各请求点保持顺序
//!
//! # 使用示例
//!
//! ```ignore
//! use sz_orm_graphql::dataloader::{BatchLoader, DataLoader, BatchLoadError};
//! use std::collections::HashMap;
//! use std::sync::Arc;
//!
//! struct OrderLoader;
//!
//! impl BatchLoader<i64, String> for OrderLoader {
//!     fn batch_load(
//!         &self,
//!         keys: Vec<i64>,
//!     ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<HashMap<i64, String>, BatchLoadError>> + Send + '_>> {
//!         Box::pin(async move {
//!             // SELECT * FROM orders WHERE user_id IN ($1, $2, $3)
//!             let mut map = HashMap::new();
//!             for k in keys {
//!                 map.insert(k, format!("order_for_{}", k));
//!             }
//!             Ok(map)
//!         })
//!     }
//! }
//!
//! let loader = DataLoader::new(Arc::new(OrderLoader));
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::oneshot;

/// 批量加载错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchLoadError {
    LoadFailed(String),
    ChannelClosed,
    KeyNotFound,
}

impl std::fmt::Display for BatchLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadFailed(msg) => write!(f, "batch load failed: {msg}"),
            Self::ChannelClosed => write!(f, "channel closed"),
            Self::KeyNotFound => write!(f, "key not found in batch result"),
        }
    }
}

impl std::error::Error for BatchLoadError {}

/// 批量加载器 trait
///
/// 调用方实现批量加载逻辑（如 `SELECT * FROM orders WHERE user_id IN (?, ?, ?)`）。
pub trait BatchLoader<K, V>: Send + Sync {
    /// 批量加载多个键对应的值
    ///
    /// 返回 `HashMap<K, V>`，按键映射结果。键集合已去重。
    fn batch_load(
        &self,
        keys: Vec<K>,
    ) -> Pin<Box<dyn Future<Output = Result<HashMap<K, V>, BatchLoadError>> + Send + '_>>;
}

type PendingMap<K, V> = HashMap<K, Vec<oneshot::Sender<Result<V, BatchLoadError>>>>;

/// DataLoader — 批量加载器
///
/// 在单个事件循环 tick 内收集多个 `load` 请求，合并为一次批量调用。
pub struct DataLoader<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    batch_loader: Arc<dyn BatchLoader<K, V>>,
    pending: Arc<Mutex<PendingMap<K, V>>>,
    tick_scheduled: Arc<AtomicBool>,
}

impl<K, V> DataLoader<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// 创建 DataLoader
    pub fn new(batch_loader: Arc<dyn BatchLoader<K, V>>) -> Self {
        Self {
            batch_loader,
            pending: Arc::new(Mutex::new(HashMap::new())),
            tick_scheduled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 加载单个键，收集到 pending，返回异步结果
    ///
    /// 当前 tick 结束时触发 `batch_load` 并回填所有 pending。
    pub fn load(
        &self,
        key: K,
    ) -> Pin<Box<dyn Future<Output = Result<V, BatchLoadError>> + Send + '_>>
    where
        K: 'static,
        V: 'static,
    {
        let (tx, rx) = oneshot::channel::<Result<V, BatchLoadError>>();
        {
            let mut pending = self.pending.lock();
            pending.entry(key).or_default().push(tx);
        }
        self.schedule_tick();
        Box::pin(async move { rx.await.map_err(|_| BatchLoadError::ChannelClosed)? })
    }

    /// 立即触发批量加载并回填所有 pending 请求
    ///
    /// 可手动调用以立即刷新，不等待 tick 结束。
    pub async fn dispatch(&self) -> Result<(), BatchLoadError> {
        let requests: PendingMap<K, V> = {
            let mut pending = self.pending.lock();
            std::mem::take(&mut *pending)
        };
        if requests.is_empty() {
            return Ok(());
        }
        let keys: Vec<K> = requests.keys().cloned().collect();
        let results = self.batch_loader.batch_load(keys).await?;
        for (key, senders) in requests {
            match results.get(&key) {
                Some(value) => {
                    for sender in senders {
                        let _ = sender.send(Ok(value.clone()));
                    }
                }
                None => {
                    for sender in senders {
                        let _ = sender.send(Err(BatchLoadError::KeyNotFound));
                    }
                }
            }
        }
        Ok(())
    }

    fn schedule_tick(&self) {
        if self
            .tick_scheduled
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let pending = self.pending.clone();
        let batch_loader = self.batch_loader.clone();
        let tick_scheduled = self.tick_scheduled.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            tick_scheduled.store(false, Ordering::SeqCst);
            let requests: PendingMap<K, V> = {
                let mut p = pending.lock();
                std::mem::take(&mut *p)
            };
            if requests.is_empty() {
                return;
            }
            let keys: Vec<K> = requests.keys().cloned().collect();
            match batch_loader.batch_load(keys).await {
                Ok(results) => {
                    for (key, senders) in requests {
                        match results.get(&key) {
                            Some(value) => {
                                for sender in senders {
                                    let _ = sender.send(Ok(value.clone()));
                                }
                            }
                            None => {
                                for sender in senders {
                                    let _ = sender.send(Err(BatchLoadError::KeyNotFound));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    for (_, senders) in requests {
                        for sender in senders {
                            let _ = sender.send(Err(e.clone()));
                        }
                    }
                }
            }
        });
    }
}

impl<K, V> Clone for DataLoader<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            batch_loader: self.batch_loader.clone(),
            pending: self.pending.clone(),
            tick_scheduled: self.tick_scheduled.clone(),
        }
    }
}

/// DataLoader resolver 集成辅助
///
/// 在 `DbResolver` 执行路径外批量收集关联字段访问，
/// 不修改 `DbResolver` trait。N 个关联字段查询次数 ≤ 2
/// （主查询 1 次 + 批量关联查询 1 次）。
///
/// # 使用示例
///
/// ```ignore
/// use sz_orm_graphql::dataloader::{DataLoaderResolver, BatchLoader, BatchLoadError};
/// use std::sync::Arc;
///
/// let resolver = DataLoaderResolver::new(Arc::new(OrderLoader));
/// // 在 resolver 执行路径外批量加载关联字段
/// let order = resolver.resolve_relation(user_id).await?;
/// ```
pub struct DataLoaderResolver<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    loader: DataLoader<K, V>,
}

impl<K, V> DataLoaderResolver<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// 创建 resolver 辅助
    pub fn new(batch_loader: Arc<dyn BatchLoader<K, V>>) -> Self {
        Self {
            loader: DataLoader::new(batch_loader),
        }
    }

    /// 批量加载关联字段
    ///
    /// 在单个事件循环 tick 内收集多个调用合并为一次批量查询。
    pub fn resolve_relation(
        &self,
        key: K,
    ) -> Pin<Box<dyn Future<Output = Result<V, BatchLoadError>> + Send + '_>>
    where
        K: 'static,
        V: 'static,
    {
        self.loader.load(key)
    }

    /// 获取内部 DataLoader 引用
    pub fn loader(&self) -> &DataLoader<K, V> {
        &self.loader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct CountingLoader {
        load_count: Arc<AtomicUsize>,
    }

    impl BatchLoader<i64, String> for CountingLoader {
        fn batch_load(
            &self,
            keys: Vec<i64>,
        ) -> Pin<Box<dyn Future<Output = Result<HashMap<i64, String>, BatchLoadError>> + Send + '_>>
        {
            let count = self.load_count.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                let mut map = HashMap::new();
                for k in keys {
                    map.insert(k, format!("value_{}", k));
                }
                Ok(map)
            })
        }
    }

    #[tokio::test]
    async fn test_single_load() {
        let load_count = Arc::new(AtomicUsize::new(0));
        let loader = DataLoader::new(Arc::new(CountingLoader {
            load_count: load_count.clone(),
        }));
        let result = loader.load(1).await.unwrap();
        assert_eq!(result, "value_1");
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_batch_concurrent_loads() {
        let load_count = Arc::new(AtomicUsize::new(0));
        let loader = DataLoader::new(Arc::new(CountingLoader {
            load_count: load_count.clone(),
        }));

        let f1 = loader.load(1);
        let f2 = loader.load(2);
        let f3 = loader.load(3);

        let (r1, r2, r3) = tokio::join!(f1, f2, f3);
        assert_eq!(r1.unwrap(), "value_1");
        assert_eq!(r2.unwrap(), "value_2");
        assert_eq!(r3.unwrap(), "value_3");
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_duplicate_keys_deduplicated() {
        let load_count = Arc::new(AtomicUsize::new(0));
        let loader = DataLoader::new(Arc::new(CountingLoader {
            load_count: load_count.clone(),
        }));

        let f1 = loader.load(1);
        let f2 = loader.load(1);
        let f3 = loader.load(1);

        let (r1, r2, r3) = tokio::join!(f1, f2, f3);
        assert_eq!(r1.unwrap(), "value_1");
        assert_eq!(r2.unwrap(), "value_1");
        assert_eq!(r3.unwrap(), "value_1");
    }

    #[tokio::test]
    async fn test_dispatch_manual() {
        let load_count = Arc::new(AtomicUsize::new(0));
        let loader = DataLoader::new(Arc::new(CountingLoader {
            load_count: load_count.clone(),
        }));

        let f1 = loader.load(10);
        loader.dispatch().await.unwrap();
        let r1 = f1.await.unwrap();
        assert_eq!(r1, "value_10");
    }

    #[tokio::test]
    async fn test_key_not_found() {
        struct EmptyLoader;
        impl BatchLoader<i64, String> for EmptyLoader {
            fn batch_load(
                &self,
                _keys: Vec<i64>,
            ) -> Pin<
                Box<dyn Future<Output = Result<HashMap<i64, String>, BatchLoadError>> + Send + '_>,
            > {
                Box::pin(async move { Ok(HashMap::new()) })
            }
        }

        let loader = DataLoader::new(Arc::new(EmptyLoader));
        let result = loader.load(99).await;
        assert_eq!(result, Err(BatchLoadError::KeyNotFound));
    }

    #[tokio::test]
    async fn test_batch_load_error_propagation() {
        struct FailingLoader;
        impl BatchLoader<i64, String> for FailingLoader {
            fn batch_load(
                &self,
                _keys: Vec<i64>,
            ) -> Pin<
                Box<dyn Future<Output = Result<HashMap<i64, String>, BatchLoadError>> + Send + '_>,
            > {
                Box::pin(async move {
                    Err(BatchLoadError::LoadFailed("DB connection lost".to_string()))
                })
            }
        }

        let loader = DataLoader::new(Arc::new(FailingLoader));
        let result = loader.load(1).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(BatchLoadError::LoadFailed(_))));
    }

    #[tokio::test]
    async fn test_order_preserved() {
        struct OrderedLoader;
        impl BatchLoader<i64, i64> for OrderedLoader {
            fn batch_load(
                &self,
                keys: Vec<i64>,
            ) -> Pin<Box<dyn Future<Output = Result<HashMap<i64, i64>, BatchLoadError>> + Send + '_>>
            {
                Box::pin(async move {
                    let map: HashMap<i64, i64> = keys.into_iter().map(|k| (k, k * 10)).collect();
                    Ok(map)
                })
            }
        }

        let loader = DataLoader::new(Arc::new(OrderedLoader));
        let f0 = loader.load(5);
        let f1 = loader.load(3);
        let f2 = loader.load(8);
        let f3 = loader.load(1);
        let f4 = loader.load(9);
        let (r0, r1, r2, r3, r4) = tokio::join!(f0, f1, f2, f3, f4);
        let results = [r0, r1, r2, r3, r4];
        let keys = [5, 3, 8, 1, 9];
        for i in 0..5 {
            assert_eq!(results[i].as_ref().unwrap(), &(keys[i] * 10));
        }
    }
}
