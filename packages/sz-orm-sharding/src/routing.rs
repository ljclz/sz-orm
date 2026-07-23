//! # 分片键提取器
//!
//! 提供 `ShardKeyExtractor` trait 与若干实现，用于从业务数据对象中提取分片键，
//! 配合 [`ShardingRouter::route_by_data`](crate::ShardingRouter::route_by_data) 使用。
//!
//! ## 提供的提取器
//!
//! - [`FieldExtractor`]：通过闭包从数据中提取字段（闭包捕获字段访问逻辑）
//! - [`CompositeKeyExtractor`]：组合多个 extractor，结果以 `:` 拼接

use crate::ShardingError;
use std::any::Any;

/// 分片键提取器 trait
///
/// 实现者负责从 `data: &dyn Any` 中提取分片键字符串。
/// 必须是 `Send + Sync` 以便在跨线程场景（如 `ScatterGather`）中使用。
pub trait ShardKeyExtractor: Send + Sync {
    /// 从数据中提取分片键
    ///
    /// # Errors
    ///
    /// 提取失败时返回 [`ShardingError`]。
    fn extract(&self, data: &dyn Any) -> Result<String, ShardingError>;
}

/// 字段提取器：通过闭包从数据中提取字段
///
/// 闭包签名 `Fn() -> String`，由调用方在闭包内捕获对数据源的访问方式。
/// `extract` 收到的 `data: &dyn Any` 参数被忽略（由闭包自行决定如何取值），
/// 这种设计让用户可以灵活地把外部状态（如 `Arc<T>`、数据库行等）捕获进闭包。
pub struct FieldExtractor {
    extractor: Box<dyn Fn() -> String + Send + Sync>,
}

impl FieldExtractor {
    /// 创建字段提取器
    ///
    /// # 参数
    ///
    /// - `f`: 闭包，捕获字段访问逻辑，返回字段值字符串
    pub fn new<F>(f: F) -> Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        Self {
            extractor: Box::new(f),
        }
    }
}

impl ShardKeyExtractor for FieldExtractor {
    fn extract(&self, _data: &dyn Any) -> Result<String, ShardingError> {
        Ok((self.extractor)())
    }
}

impl std::fmt::Debug for FieldExtractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldExtractor")
            .field("extractor", &"<closure>")
            .finish()
    }
}

/// 复合键提取器：组合多个 extractor，结果以 `:` 拼接
///
/// # 示例
///
/// ```rust,ignore
/// use sz_orm_sharding::routing::{CompositeKeyExtractor, FieldExtractor};
///
/// let ext = CompositeKeyExtractor::new()
///     .with(FieldExtractor::new(|| "cn".to_string()))
///     .with(FieldExtractor::new(|| "user:123".to_string()));
/// // extract 后得到 "cn:user:123"
/// ```
pub struct CompositeKeyExtractor {
    extractors: Vec<Box<dyn ShardKeyExtractor>>,
}

impl CompositeKeyExtractor {
    /// 创建空的复合提取器
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }

    /// 添加一个子提取器（链式 API）
    ///
    /// 命名为 `with` 而非 `add`，避免与 `std::ops::Add::add` trait 方法混淆。
    pub fn with<E>(mut self, extractor: E) -> Self
    where
        E: ShardKeyExtractor + 'static,
    {
        self.extractors.push(Box::new(extractor));
        self
    }

    /// 返回子提取器数量
    pub fn len(&self) -> usize {
        self.extractors.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.extractors.is_empty()
    }
}

impl Default for CompositeKeyExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardKeyExtractor for CompositeKeyExtractor {
    fn extract(&self, data: &dyn Any) -> Result<String, ShardingError> {
        let mut parts: Vec<String> = Vec::with_capacity(self.extractors.len());
        for ext in &self.extractors {
            parts.push(ext.extract(data)?);
        }
        Ok(parts.join(":"))
    }
}

impl std::fmt::Debug for CompositeKeyExtractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeKeyExtractor")
            .field("count", &self.extractors.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShardingRouter, ShardingStrategy};
    use std::collections::HashMap;

    #[test]
    fn test_field_extractor_basic() {
        let ext = FieldExtractor::new(|| "key:123".to_string());
        let key = ext.extract(&"ignored").unwrap();
        assert_eq!(key, "key:123");
    }

    #[test]
    fn test_field_extractor_deterministic() {
        let ext = FieldExtractor::new(|| "stable_key".to_string());
        let k1 = ext.extract(&42i32).unwrap();
        let k2 = ext.extract(&"other").unwrap();
        assert_eq!(k1, k2);
        assert_eq!(k1, "stable_key");
    }

    #[test]
    fn test_composite_key_extractor_combines() {
        let ext = CompositeKeyExtractor::new()
            .with(FieldExtractor::new(|| "cn".to_string()))
            .with(FieldExtractor::new(|| "user:123".to_string()));
        let key = ext.extract(&"ignored").unwrap();
        assert_eq!(key, "cn:user:123");
    }

    #[test]
    fn test_composite_key_extractor_three_parts() {
        let ext = CompositeKeyExtractor::new()
            .with(FieldExtractor::new(|| "2026-07-18".to_string()))
            .with(FieldExtractor::new(|| "cn".to_string()))
            .with(FieldExtractor::new(|| "user:42".to_string()));
        let key = ext.extract(&"x").unwrap();
        assert_eq!(key, "2026-07-18:cn:user:42");
    }

    #[test]
    fn test_composite_key_extractor_empty() {
        let ext = CompositeKeyExtractor::new();
        assert!(ext.is_empty());
        assert_eq!(ext.len(), 0);
        let key = ext.extract(&"ignored").unwrap();
        assert_eq!(key, "");
    }

    #[test]
    fn test_route_by_data_with_field_extractor_enum() {
        let mut mapping = HashMap::new();
        mapping.insert("key:123".to_string(), "shard_a".to_string());
        let router = ShardingRouter::new_enum(mapping, None);
        let ext = FieldExtractor::new(|| "key:123".to_string());
        let result = router.route_by_data(&"data", &ext).unwrap();
        assert_eq!(result, "shard_a");
    }

    #[test]
    fn test_route_by_data_no_match_errors() {
        let router = ShardingRouter::new_enum(HashMap::new(), None);
        let ext = FieldExtractor::new(|| "missing".to_string());
        let result = router.route_by_data(&"data", &ext);
        assert!(matches!(result, Err(ShardingError::NoMappingForKey(_))));
    }

    #[test]
    fn test_route_by_data_with_hash_strategy() {
        let router = ShardingRouter::new(ShardingStrategy::Hash, vec!["s0", "s1"]);
        let ext = FieldExtractor::new(|| "user:42".to_string());
        let result = router.route_by_data(&"data", &ext).unwrap();
        assert!(result == "s0" || result == "s1");
    }

    #[test]
    fn test_route_by_data_with_composite_extractor() {
        // 二级 key 形如 "cn:user:123"
        let mut mapping = HashMap::new();
        mapping.insert("cn:user:123".to_string(), "shard_x".to_string());
        let router = ShardingRouter::new_enum(mapping, None);
        let ext = CompositeKeyExtractor::new()
            .with(FieldExtractor::new(|| "cn".to_string()))
            .with(FieldExtractor::new(|| "user:123".to_string()));
        let result = router.route_by_data(&"data", &ext).unwrap();
        assert_eq!(result, "shard_x");
    }

    #[test]
    fn test_field_extractor_debug() {
        let ext = FieldExtractor::new(|| "k".to_string());
        let s = format!("{:?}", ext);
        assert!(s.contains("FieldExtractor"));
    }

    #[test]
    fn test_composite_key_extractor_debug() {
        let ext = CompositeKeyExtractor::new()
            .with(FieldExtractor::new(|| "a".to_string()));
        let s = format!("{:?}", ext);
        assert!(s.contains("CompositeKeyExtractor"));
        assert!(s.contains("count"));
    }
}
