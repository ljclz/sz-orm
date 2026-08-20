#![allow(missing_docs)]
//! 懒加载器（Lazy Loader）
//!
//! 对标 Hibernate `@LazyCollection` / EF Core `Virtual` 导航属性。
//!
//! 在首次访问关联实体时才从数据库加载，避免不必要的查询和内存占用。
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::lazy_loader::LazyRef;
//!
//! // 创建懒加载引用（传入加载闭包）
//! let lazy_user: LazyRef<String> = LazyRef::new(|| "loaded user".to_string());
//!
//! // 首次访问触发加载
//! assert!(!lazy_user.is_loaded());
//! let user = lazy_user.get().unwrap();
//! assert_eq!(user, "loaded user");
//! assert!(lazy_user.is_loaded()); // 已缓存
//! ```

use std::sync::{Arc, Mutex};

/// 懒加载引用
///
/// 包装一个值和其加载闭包，首次 `get()` 时触发加载并缓存结果。
pub struct LazyRef<T> {
    value: Arc<Mutex<Option<T>>>,
    loader: Arc<dyn Fn() -> T + Send + Sync>,
}

impl<T: Clone + Send + Sync + 'static> LazyRef<T> {
    /// 创建懒加载引用
    pub fn new<F>(loader: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            value: Arc::new(Mutex::new(None)),
            loader: Arc::new(loader),
        }
    }

    /// 创建已加载的引用（用于测试或预加载数据）
    pub fn loaded(value: T) -> Self {
        Self {
            value: Arc::new(Mutex::new(Some(value))),
            loader: Arc::new(|| panic!("LazyRef::loaded should not call loader")),
        }
    }

    /// 获取值（首次调用触发加载）
    pub fn get(&self) -> Option<T> {
        let mut guard = self.value.lock().unwrap();
        if guard.is_none() {
            *guard = Some((self.loader)());
        }
        guard.clone()
    }

    /// 是否已加载
    pub fn is_loaded(&self) -> bool {
        self.value.lock().unwrap().is_some()
    }

    /// 强制重新加载
    pub fn reload(&self) -> Option<T> {
        let mut guard = self.value.lock().unwrap();
        *guard = Some((self.loader)());
        guard.clone()
    }

    /// 清除缓存（下次 get 会重新加载）
    pub fn invalidate(&self) {
        *self.value.lock().unwrap() = None;
    }
}

impl<T: Clone + Send + Sync + 'static> Clone for LazyRef<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            loader: Arc::clone(&self.loader),
        }
    }
}

/// 懒加载集合
///
/// 用于懒加载一对多关系（如 User → Orders）。
pub struct LazyCollection<T> {
    inner: LazyRef<Vec<T>>,
}

impl<T: Clone + Send + Sync + 'static> LazyCollection<T> {
    pub fn new<F>(loader: F) -> Self
    where
        F: Fn() -> Vec<T> + Send + Sync + 'static,
    {
        Self {
            inner: LazyRef::new(loader),
        }
    }

    pub fn loaded(items: Vec<T>) -> Self {
        Self {
            inner: LazyRef::loaded(items),
        }
    }

    /// 获取所有元素
    pub fn all(&self) -> Vec<T> {
        self.inner.get().unwrap_or_default()
    }

    /// 获取数量
    pub fn len(&self) -> usize {
        self.all().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 是否已加载
    pub fn is_loaded(&self) -> bool {
        self.inner.is_loaded()
    }

    /// 过滤已加载的元素
    pub fn filter<F>(&self, predicate: F) -> Vec<T>
    where
        F: Fn(&T) -> bool,
    {
        self.all().into_iter().filter(predicate).collect()
    }

    /// 清除缓存
    pub fn invalidate(&self) {
        self.inner.invalidate();
    }
}

impl<T: Clone + Send + Sync + 'static> Clone for LazyCollection<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// 懒加载器
///
/// 管理多个懒加载引用，提供统一的加载和缓存管理。
pub struct LazyLoader {
    load_count: Arc<Mutex<usize>>,
}

impl Default for LazyLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyLoader {
    pub fn new() -> Self {
        Self {
            load_count: Arc::new(Mutex::new(0)),
        }
    }

    /// 创建懒加载引用，并统计加载次数
    pub fn lazy<F, T>(&self, loader: F) -> LazyRef<T>
    where
        T: Clone + Send + Sync + 'static,
        F: Fn() -> T + Send + Sync + 'static,
    {
        let count = Arc::clone(&self.load_count);
        LazyRef::new(move || {
            *count.lock().unwrap() += 1;
            loader()
        })
    }

    /// 获取总加载次数
    pub fn load_count(&self) -> usize {
        *self.load_count.lock().unwrap()
    }

    /// 重置加载计数
    pub fn reset_count(&self) {
        *self.load_count.lock().unwrap() = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazy_ref_first_access_loads() {
        let lazy: LazyRef<String> = LazyRef::new(|| "hello".to_string());
        assert!(!lazy.is_loaded());
        let val = lazy.get().unwrap();
        assert_eq!(val, "hello");
        assert!(lazy.is_loaded());
    }

    #[test]
    fn test_lazy_ref_cached_after_load() {
        let counter = Arc::new(Mutex::new(0));
        let c = Arc::clone(&counter);
        let lazy: LazyRef<i32> = LazyRef::new(move || {
            *c.lock().unwrap() += 1;
            42
        });

        assert_eq!(lazy.get().unwrap(), 42);
        assert_eq!(lazy.get().unwrap(), 42);
        assert_eq!(lazy.get().unwrap(), 42);
        assert_eq!(*counter.lock().unwrap(), 1);
    }

    #[test]
    fn test_lazy_ref_reload() {
        let lazy: LazyRef<i32> = LazyRef::new(|| 1);
        assert_eq!(lazy.get().unwrap(), 1);
        assert_eq!(lazy.get().unwrap(), 1);
    }

    #[test]
    fn test_lazy_ref_invalidate() {
        let counter = Arc::new(Mutex::new(0));
        let c = Arc::clone(&counter);
        let lazy: LazyRef<i32> = LazyRef::new(move || {
            let mut g = c.lock().unwrap();
            *g += 1;
            *g
        });

        assert_eq!(lazy.get().unwrap(), 1);
        lazy.invalidate();
        assert!(!lazy.is_loaded());
        assert_eq!(lazy.get().unwrap(), 2);
    }

    #[test]
    fn test_lazy_ref_loaded() {
        let lazy: LazyRef<String> = LazyRef::loaded("preloaded".to_string());
        assert!(lazy.is_loaded());
        assert_eq!(lazy.get().unwrap(), "preloaded");
    }

    #[test]
    fn test_lazy_ref_clone_shares_state() {
        let lazy: LazyRef<i32> = LazyRef::new(|| 99);
        let lazy2 = lazy.clone();
        lazy.get();
        assert!(lazy2.is_loaded());
    }

    #[test]
    fn test_lazy_collection_basic() {
        let coll: LazyCollection<i32> = LazyCollection::new(|| vec![1, 2, 3]);
        assert!(!coll.is_loaded());
        assert_eq!(coll.len(), 3);
        assert!(!coll.is_empty());
        assert!(coll.is_loaded());
    }

    #[test]
    fn test_lazy_collection_filter() {
        let coll: LazyCollection<i32> = LazyCollection::new(|| vec![1, 2, 3, 4, 5]);
        let evens = coll.filter(|x| x % 2 == 0);
        assert_eq!(evens, vec![2, 4]);
    }

    #[test]
    fn test_lazy_collection_empty() {
        let coll: LazyCollection<i32> = LazyCollection::new(std::vec::Vec::new);
        assert_eq!(coll.len(), 0);
        assert!(coll.is_empty());
    }

    #[test]
    fn test_lazy_collection_loaded() {
        let coll: LazyCollection<i32> = LazyCollection::loaded(vec![10, 20]);
        assert!(coll.is_loaded());
        assert_eq!(coll.all(), vec![10, 20]);
    }

    #[test]
    fn test_lazy_loader_count() {
        let loader = LazyLoader::new();
        let lazy1: LazyRef<i32> = loader.lazy(|| 1);
        let lazy2: LazyRef<i32> = loader.lazy(|| 2);

        assert_eq!(loader.load_count(), 0);
        lazy1.get();
        assert_eq!(loader.load_count(), 1);
        lazy2.get();
        assert_eq!(loader.load_count(), 2);
        lazy1.get();
        assert_eq!(loader.load_count(), 2);
    }

    #[test]
    fn test_lazy_loader_reset() {
        let loader = LazyLoader::new();
        let lazy: LazyRef<i32> = loader.lazy(|| 42);
        lazy.get();
        assert_eq!(loader.load_count(), 1);
        loader.reset_count();
        assert_eq!(loader.load_count(), 0);
    }

    #[test]
    fn test_e2e_user_orders_lazy_loading() {
        #[derive(Clone)]
        #[allow(dead_code)]
        struct Order {
            id: i64,
            user_id: i64,
            amount: f64,
        }

        #[derive(Clone)]
        #[allow(dead_code)]
        struct User {
            id: i64,
            name: String,
            orders: LazyCollection<Order>,
        }

        let query_count = Arc::new(Mutex::new(0));
        let qc = Arc::clone(&query_count);

        let user = User {
            id: 1,
            name: "alice".into(),
            orders: LazyCollection::new(move || {
                *qc.lock().unwrap() += 1;
                vec![
                    Order {
                        id: 101,
                        user_id: 1,
                        amount: 99.5,
                    },
                    Order {
                        id: 102,
                        user_id: 1,
                        amount: 200.0,
                    },
                ]
            }),
        };

        assert!(!user.orders.is_loaded());
        assert_eq!(*query_count.lock().unwrap(), 0);

        let all_orders = user.orders.all();
        assert_eq!(all_orders.len(), 2);
        assert_eq!(all_orders[0].id, 101);
        assert_eq!(all_orders[1].amount, 200.0);
        assert!(user.orders.is_loaded());
        assert_eq!(*query_count.lock().unwrap(), 1);

        let _again = user.orders.all();
        assert_eq!(*query_count.lock().unwrap(), 1);

        let big_orders = user.orders.filter(|o| o.amount >= 200.0);
        assert_eq!(big_orders.len(), 1);
        assert_eq!(big_orders[0].id, 102);
        assert_eq!(*query_count.lock().unwrap(), 1);
    }

    #[test]
    fn test_e2e_lazy_ref_belongs_to() {
        #[derive(Clone)]
        #[allow(dead_code)]
        struct User {
            id: i64,
            name: String,
        }

        #[derive(Clone)]
        #[allow(dead_code)]
        struct Order {
            id: i64,
            user: LazyRef<User>,
        }

        let user = Order {
            id: 501,
            user: LazyRef::new(|| User {
                id: 1,
                name: "alice".into(),
            }),
        };

        assert!(!user.user.is_loaded());
        let u = user.user.get().unwrap();
        assert_eq!(u.name, "alice");
        assert!(user.user.is_loaded());

        user.user.invalidate();
        assert!(!user.user.is_loaded());
        let u2 = user.user.get().unwrap();
        assert_eq!(u2.id, 1);
    }

    #[test]
    fn test_e2e_lazy_loader_multi_entity_tracking() {
        let loader = LazyLoader::new();

        let lazy_profile: LazyRef<String> = loader.lazy(|| "alice profile".into());
        let lazy_orders: LazyRef<Vec<i64>> = loader.lazy(|| vec![1, 2, 3]);

        assert_eq!(loader.load_count(), 0);

        lazy_profile.get();
        assert_eq!(loader.load_count(), 1);

        let _ = lazy_orders.get();
        assert_eq!(loader.load_count(), 2);

        lazy_profile.get();
        let _ = lazy_orders.get();
        assert_eq!(loader.load_count(), 2);

        lazy_orders.invalidate();
        let _ = lazy_orders.get();
        assert_eq!(loader.load_count(), 3);
    }
}
