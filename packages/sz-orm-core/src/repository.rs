//! Repository Pattern 仓储模式
//!
//! 对应文档 6.8 节改进项 37（Repository Pattern 仓储模式）。
//!
//! # 核心概念
//!
//! - **Repository trait**：统一的仓储接口，定义 CRUD + 分页 + 条件查询
//! - **InMemoryRepository**：内存仓储实现（用于测试、原型、无数据库场景）
//! - **WhereCondition / WhereOp**：查询条件
//! - **PageResult**：分页结果
//! - **RepositoryError**：仓储错误
//!
//! # 设计灵感
//!
//! - Doctrine `EntityRepository`
//! - Spring Data JPA `@Repository` / `JpaRepository`
//! - MyBatis-Plus `IService` / `BaseMapper`
//! - Laravel Eloquent `Repository`
//! - DDD（领域驱动设计）的 Repository 模式
//!
//! # 优势
//!
//! 1. **分层解耦**：业务层依赖 Repository 接口，不直接依赖 Model 静态方法
//! 2. **可替换性**：同一接口可有 InMemory / SQL / NoSQL 等多种实现
//! 3. **可测试**：单元测试用 InMemoryRepository，集成测试用 SqlRepository
//! 4. **统一 API**：CRUD + 分页 + 条件查询接口统一
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::repository::{
//!     InMemoryRepository, Repository, WhereCondition, WhereOp, PageResult,
//! };
//! use sz_orm_core::Value;
//! use std::collections::HashMap;
//!
//! // 假设 User 是 Model 的实现
//! // let repo = InMemoryRepository::<User>::new();
//! // let user = repo.find_by_id(1)?;
//! // let adults = repo.find_by(&[WhereCondition::new("age", WhereOp::Ge, Value::I64(18))])?;
//! ```

use crate::value::Value;
use std::fmt::Debug;
use std::sync::RwLock;

// ============================================================================
// WhereCondition — 查询条件
// ============================================================================

/// 查询操作符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhereOp {
    /// `=`
    Eq,
    /// `!=`
    Ne,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `LIKE`
    Like,
    /// `IN`
    In,
    /// `NOT IN`
    NotIn,
    /// `IS NULL`
    IsNull,
    /// `IS NOT NULL`
    IsNotNull,
    /// `BETWEEN`
    Between,
}

impl WhereOp {
    /// 操作符名称
    pub fn name(&self) -> &'static str {
        match self {
            WhereOp::Eq => "eq",
            WhereOp::Ne => "ne",
            WhereOp::Gt => "gt",
            WhereOp::Ge => "ge",
            WhereOp::Lt => "lt",
            WhereOp::Le => "le",
            WhereOp::Like => "like",
            WhereOp::In => "in",
            WhereOp::NotIn => "not_in",
            WhereOp::IsNull => "is_null",
            WhereOp::IsNotNull => "is_not_null",
            WhereOp::Between => "between",
        }
    }
}

/// 查询条件
#[derive(Debug, Clone)]
pub struct WhereCondition {
    pub field: String,
    pub op: WhereOp,
    pub value: Value,
    /// For Between / In / NotIn，副值列表
    pub extra_values: Vec<Value>,
}

impl WhereCondition {
    /// 创建单值条件（Eq/Ne/Gt/Ge/Lt/Le/Like）
    pub fn new(field: impl Into<String>, op: WhereOp, value: Value) -> Self {
        Self {
            field: field.into(),
            op,
            value,
            extra_values: Vec::new(),
        }
    }

    /// 创建 IsNull / IsNotNull 条件
    pub fn null_check(field: impl Into<String>, op: WhereOp) -> Self {
        Self {
            field: field.into(),
            op,
            value: Value::Null,
            extra_values: Vec::new(),
        }
    }

    /// 创建 In / NotIn 条件
    pub fn in_op(field: impl Into<String>, op: WhereOp, values: Vec<Value>) -> Self {
        Self {
            field: field.into(),
            op,
            value: Value::Null,
            extra_values: values,
        }
    }

    /// 创建 Between 条件
    pub fn between(field: impl Into<String>, low: Value, high: Value) -> Self {
        Self {
            field: field.into(),
            op: WhereOp::Between,
            value: low,
            extra_values: vec![high],
        }
    }
}

// ============================================================================
// PageResult — 分页结果
// ============================================================================

/// 分页结果
#[derive(Debug, Clone)]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

impl<T> PageResult<T> {
    /// 创建分页结果
    pub fn new(items: Vec<T>, total: u64, page: u64, page_size: u64) -> Self {
        Self {
            items,
            total,
            page,
            page_size,
        }
    }

    /// 总页数
    pub fn total_pages(&self) -> u64 {
        if self.page_size == 0 {
            return 0;
        }
        self.total.div_ceil(self.page_size)
    }

    /// 是否有下一页
    pub fn has_next(&self) -> bool {
        self.page < self.total_pages()
    }

    /// 是否有上一页
    pub fn has_prev(&self) -> bool {
        self.page > 1
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 当前页条数
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 映射为其他类型
    pub fn map<U, F: Fn(T) -> U>(self, f: F) -> PageResult<U> {
        PageResult {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
            page: self.page,
            page_size: self.page_size,
        }
    }
}

// ============================================================================
// RepositoryError — 错误类型
// ============================================================================

/// Repository 错误
#[derive(Debug, Clone, PartialEq)]
pub enum RepositoryError {
    /// 实体未找到
    NotFound,
    /// 数据库错误
    DatabaseError(String),
    /// 实体无效（如缺主键）
    InvalidEntity(String),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepositoryError::NotFound => write!(f, "entity not found"),
            RepositoryError::DatabaseError(msg) => write!(f, "database error: {}", msg),
            RepositoryError::InvalidEntity(msg) => write!(f, "invalid entity: {}", msg),
            RepositoryError::Other(msg) => write!(f, "repository error: {}", msg),
        }
    }
}

impl std::error::Error for RepositoryError {}

/// Repository 结果类型
pub type RepositoryResult<T> = Result<T, RepositoryError>;

// ============================================================================
// Repository trait — 仓储接口
// ============================================================================

/// 仓储接口（generic over Model-like entity E, primary key K）
///
/// E 不必是 `Model` trait 实现，仅需满足存储基本要求。
/// 这样设计允许存储任意结构（DTO、聚合、领域实体等）。
pub trait Repository<E>: Send + Sync {
    /// 主键类型
    type Key: Clone + Debug + PartialEq + Send + Sync;

    /// 提取实体的主键
    fn key_of(&self, entity: &E) -> Self::Key;

    /// 按主键查找
    fn find_by_id(&self, key: &Self::Key) -> RepositoryResult<Option<E>>;

    /// 查询所有
    fn find_all(&self) -> RepositoryResult<Vec<E>>;

    /// 按条件查询
    fn find_by(&self, conditions: &[WhereCondition]) -> RepositoryResult<Vec<E>>;

    /// 按条件查询单条
    fn find_one_by(&self, conditions: &[WhereCondition]) -> RepositoryResult<Option<E>> {
        let mut items = self.find_by(conditions)?;
        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(items.remove(0)))
        }
    }

    /// 保存（INSERT 或 UPDATE）
    ///
    /// 返回保存后的实体（可能是克隆，主键可能被填充）
    fn save(&self, entity: E) -> RepositoryResult<E>;

    /// 批量保存
    fn save_many(&self, entities: Vec<E>) -> RepositoryResult<Vec<E>> {
        let mut saved = Vec::with_capacity(entities.len());
        for e in entities {
            saved.push(self.save(e)?);
        }
        Ok(saved)
    }

    /// 按主键删除
    fn delete(&self, key: &Self::Key) -> RepositoryResult<usize>;

    /// 按条件删除
    fn delete_by(&self, conditions: &[WhereCondition]) -> RepositoryResult<usize> {
        let items = self.find_by(conditions)?;
        let mut count = 0;
        for item in items {
            let key = self.key_of(&item);
            count += self.delete(&key)?;
        }
        Ok(count)
    }

    /// 总数
    fn count(&self) -> RepositoryResult<u64>;

    /// 按条件计数
    fn count_by(&self, conditions: &[WhereCondition]) -> RepositoryResult<u64> {
        let items = self.find_by(conditions)?;
        Ok(items.len() as u64)
    }

    /// 主键是否存在
    fn exists(&self, key: &Self::Key) -> RepositoryResult<bool> {
        Ok(self.find_by_id(key)?.is_some())
    }

    /// 分页查询
    fn paginate(&self, page: u64, page_size: u64) -> RepositoryResult<PageResult<E>>
    where
        E: Clone,
    {
        let all = self.find_all()?;
        let total = all.len() as u64;
        let start = ((page.saturating_sub(1)) * page_size) as usize;
        let end = (start + page_size as usize).min(all.len());

        let items = if start < all.len() {
            all[start..end].to_vec()
        } else {
            Vec::new()
        };

        Ok(PageResult::new(items, total, page, page_size))
    }

    /// 按条件分页查询
    fn paginate_by(
        &self,
        conditions: &[WhereCondition],
        page: u64,
        page_size: u64,
    ) -> RepositoryResult<PageResult<E>>
    where
        E: Clone,
    {
        let all = self.find_by(conditions)?;
        let total = all.len() as u64;
        let start = ((page.saturating_sub(1)) * page_size) as usize;
        let end = (start + page_size as usize).min(all.len());

        let items = if start < all.len() {
            all[start..end].to_vec()
        } else {
            Vec::new()
        };

        Ok(PageResult::new(items, total, page, page_size))
    }
}

// ============================================================================
// InMemoryRepository — 内存仓储实现
// ============================================================================

/// 内存仓储实现
///
/// 使用 `Vec<E>` 存储实体，主键通过 `key_of` 提取。
/// 适合单元测试、原型开发、无数据库场景。
pub struct InMemoryRepository<E: Clone + Send + Sync + 'static> {
    storage: RwLock<Vec<E>>,
}

impl<E: Clone + Send + Sync + 'static> InMemoryRepository<E> {
    /// 创建空仓储
    pub fn new() -> Self {
        Self {
            storage: RwLock::new(Vec::new()),
        }
    }

    /// 从已有集合创建
    pub fn from_vec(items: Vec<E>) -> Self {
        Self {
            storage: RwLock::new(items),
        }
    }

    /// 当前存储条数
    pub fn len(&self) -> usize {
        let storage = self.storage.read().unwrap();
        storage.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 清空
    pub fn clear(&self) {
        let mut storage = self.storage.write().unwrap();
        storage.clear();
    }
}

impl<E: Clone + Send + Sync + 'static> Default for InMemoryRepository<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// 值比较辅助函数（支持基本类型 + Value 比较）
fn value_matches(value: &Value, op: WhereOp, target: &Value, extras: &[Value]) -> bool {
    use Value::*;
    match op {
        WhereOp::Eq => value == target,
        WhereOp::Ne => value != target,
        WhereOp::Gt => match (value, target) {
            (I64(a), I64(b)) => a > b,
            (F64(a), F64(b)) => a > b,
            (F64(a), I64(b)) => a > &(*b as f64),
            (I64(a), F64(b)) => a > &(*b as i64),
            (String(a), String(b)) => a > b,
            _ => false,
        },
        WhereOp::Ge => match (value, target) {
            (I64(a), I64(b)) => a >= b,
            (F64(a), F64(b)) => a >= b,
            (String(a), String(b)) => a >= b,
            _ => false,
        },
        WhereOp::Lt => match (value, target) {
            (I64(a), I64(b)) => a < b,
            (F64(a), F64(b)) => a < b,
            (String(a), String(b)) => a < b,
            _ => false,
        },
        WhereOp::Le => match (value, target) {
            (I64(a), I64(b)) => a <= b,
            (F64(a), F64(b)) => a <= b,
            (String(a), String(b)) => a <= b,
            _ => false,
        },
        WhereOp::Like => match (value, target) {
            (String(a), String(b)) => {
                // 简化 LIKE：将 % 转换为 .*，其他字符转义
                let pattern = b.replace('%', ".*").replace('_', ".");
                let full_pattern = format!("^{}$", pattern);
                if let Ok(re) = simple_regex::compile(&full_pattern) {
                    re.is_match(a)
                } else {
                    false
                }
            }
            _ => false,
        },
        WhereOp::In => extras.iter().any(|v| v == value),
        WhereOp::NotIn => !extras.iter().any(|v| v == value),
        WhereOp::IsNull => matches!(value, Null),
        WhereOp::IsNotNull => !matches!(value, Null),
        WhereOp::Between => {
            if extras.is_empty() {
                return false;
            }
            let low = target;
            let high = &extras[0];
            // value >= low AND value <= high
            value_matches(value, WhereOp::Ge, low, &[])
                && value_matches(value, WhereOp::Le, high, &[])
        }
    }
}

/// 实体属性提取 trait（用户为实体实现此 trait 以支持 find_by）
pub trait EntityAttributes: Send + Sync {
    /// 按字段名获取属性值
    fn get_attribute(&self, field: &str) -> Option<Value>;
}

/// InMemoryRepository 的 EntityAttributes-based 实现
impl<E: Clone + Send + Sync + 'static + EntityAttributes> Repository<E> for InMemoryRepository<E> {
    type Key = Value;

    fn key_of(&self, entity: &E) -> Self::Key {
        entity.get_attribute("id").unwrap_or(Value::Null)
    }

    fn find_by_id(&self, key: &Self::Key) -> RepositoryResult<Option<E>> {
        let storage = self.storage.read().unwrap();
        Ok(storage.iter().find(|e| self.key_of(e) == *key).cloned())
    }

    fn find_all(&self) -> RepositoryResult<Vec<E>> {
        let storage = self.storage.read().unwrap();
        Ok(storage.clone())
    }

    fn find_by(&self, conditions: &[WhereCondition]) -> RepositoryResult<Vec<E>> {
        let storage = self.storage.read().unwrap();
        let result: Vec<E> = storage
            .iter()
            .filter(|e| {
                conditions.iter().all(|c| {
                    let attr = e.get_attribute(&c.field);
                    match (attr, c.op) {
                        (None, WhereOp::IsNull) => true,
                        (None, _) => false,
                        (Some(v), _) => value_matches(&v, c.op, &c.value, &c.extra_values),
                    }
                })
            })
            .cloned()
            .collect();
        Ok(result)
    }

    fn save(&self, mut entity: E) -> RepositoryResult<E> {
        let mut storage = self.storage.write().unwrap();
        let key = self.key_of(&entity);

        // 查找是否已存在
        let existing_idx = storage.iter().position(|e| self.key_of(e) == key);

        match existing_idx {
            Some(idx) => {
                storage[idx] = entity.clone();
            }
            None => {
                storage.push(entity.clone());
            }
        }
        // 注意：entity 可能被修改（如自增主键），这里返回原值
        let _ = &mut entity; // 标记 mut 以符合签名
        Ok(entity)
    }

    fn delete(&self, key: &Self::Key) -> RepositoryResult<usize> {
        let mut storage = self.storage.write().unwrap();
        let before = storage.len();
        storage.retain(|e| self.key_of(e) != *key);
        Ok(before - storage.len())
    }

    fn count(&self) -> RepositoryResult<u64> {
        Ok(self.len() as u64)
    }
}

// ============================================================================
// simple_regex — 极简 LIKE 模式匹配（避免引入正则依赖）
// ============================================================================

mod simple_regex {
    /// 极简正则编译器：仅支持 `.*`、`^`、`$`、字面字符
    pub struct Regex {
        patterns: Vec<Pattern>,
    }

    enum Pattern {
        AnyChars,      // .*（贪婪匹配任意字符）
        Literal(char), // 字面字符
        Start,         // ^
        End,           // $
    }

    pub fn compile(pattern: &str) -> Result<Regex, String> {
        let mut patterns = Vec::new();
        let chars: Vec<char> = pattern.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '^' => {
                    patterns.push(Pattern::Start);
                    i += 1;
                }
                '$' => {
                    patterns.push(Pattern::End);
                    i += 1;
                }
                '.' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                    patterns.push(Pattern::AnyChars);
                    i += 2;
                }
                c => {
                    patterns.push(Pattern::Literal(c));
                    i += 1;
                }
            }
        }
        Ok(Regex { patterns })
    }

    impl Regex {
        pub fn is_match(&self, text: &str) -> bool {
            self.match_from(text, 0, 0)
        }

        fn match_from(&self, text: &str, text_idx: usize, pat_idx: usize) -> bool {
            let chars: Vec<char> = text.chars().collect();
            if pat_idx >= self.patterns.len() {
                return text_idx == chars.len();
            }
            match &self.patterns[pat_idx] {
                Pattern::Start => self.match_from(text, 0, pat_idx + 1),
                Pattern::End => text_idx == chars.len(),
                Pattern::Literal(c) => {
                    if text_idx < chars.len() && chars[text_idx] == *c {
                        self.match_from(text, text_idx + 1, pat_idx + 1)
                    } else {
                        false
                    }
                }
                Pattern::AnyChars => {
                    // 尝试匹配 0 到 len 个字符
                    for skip in 0..=(chars.len() - text_idx) {
                        if self.match_from(text, text_idx + skip, pat_idx + 1) {
                            return true;
                        }
                    }
                    false
                }
            }
        }
    }
}

// ============================================================================
// GenericKeyRepository — 支持任意 Key 类型的内存仓储
// ============================================================================

/// 通用 Key 提取 trait（用户为实体实现此 trait 以支持任意 Key 类型）
pub trait EntityKey<K>: Send + Sync {
    /// 提取主键
    fn key(&self) -> K;
}

/// 通用 Key 内存仓储
pub struct GenericKeyRepository<E, K>
where
    E: Clone + Send + Sync + 'static,
    K: Clone + Debug + PartialEq + Send + Sync + 'static,
{
    storage: RwLock<Vec<E>>,
    _phantom: std::marker::PhantomData<K>,
}

impl<E, K> GenericKeyRepository<E, K>
where
    E: Clone + Send + Sync + 'static,
    K: Clone + Debug + PartialEq + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            storage: RwLock::new(Vec::new()),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn from_vec(items: Vec<E>) -> Self {
        Self {
            storage: RwLock::new(items),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.storage.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        self.storage.write().unwrap().clear();
    }
}

impl<E, K> Default for GenericKeyRepository<E, K>
where
    E: Clone + Send + Sync + 'static,
    K: Clone + Debug + PartialEq + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E, K> Repository<E> for GenericKeyRepository<E, K>
where
    E: Clone + Send + Sync + 'static + EntityKey<K> + EntityAttributes,
    K: Clone + Debug + PartialEq + Send + Sync + 'static,
{
    type Key = K;

    fn key_of(&self, entity: &E) -> Self::Key {
        entity.key()
    }

    fn find_by_id(&self, key: &Self::Key) -> RepositoryResult<Option<E>> {
        let storage = self.storage.read().unwrap();
        Ok(storage.iter().find(|e| &e.key() == key).cloned())
    }

    fn find_all(&self) -> RepositoryResult<Vec<E>> {
        Ok(self.storage.read().unwrap().clone())
    }

    fn find_by(&self, conditions: &[WhereCondition]) -> RepositoryResult<Vec<E>> {
        let storage = self.storage.read().unwrap();
        let result: Vec<E> = storage
            .iter()
            .filter(|e| {
                conditions.iter().all(|c| {
                    let attr = e.get_attribute(&c.field);
                    match (attr, c.op) {
                        (None, WhereOp::IsNull) => true,
                        (None, _) => false,
                        (Some(v), _) => value_matches(&v, c.op, &c.value, &c.extra_values),
                    }
                })
            })
            .cloned()
            .collect();
        Ok(result)
    }

    fn save(&self, entity: E) -> RepositoryResult<E> {
        let mut storage = self.storage.write().unwrap();
        let key = entity.key();
        let existing_idx = storage.iter().position(|e| e.key() == key);
        match existing_idx {
            Some(idx) => {
                storage[idx] = entity.clone();
            }
            None => {
                storage.push(entity.clone());
            }
        }
        Ok(entity)
    }

    fn delete(&self, key: &Self::Key) -> RepositoryResult<usize> {
        let mut storage = self.storage.write().unwrap();
        let before = storage.len();
        storage.retain(|e| &e.key() != key);
        Ok(before - storage.len())
    }

    fn count(&self) -> RepositoryResult<u64> {
        Ok(self.len() as u64)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== 测试用实体 =====

    #[derive(Debug, Clone, PartialEq)]
    struct User {
        id: i64,
        name: String,
        age: i64,
        email: String,
    }

    impl User {
        fn new(id: i64, name: &str, age: i64, email: &str) -> Self {
            Self {
                id,
                name: name.to_string(),
                age,
                email: email.to_string(),
            }
        }
    }

    impl EntityAttributes for User {
        fn get_attribute(&self, field: &str) -> Option<Value> {
            match field {
                "id" => Some(Value::I64(self.id)),
                "name" => Some(Value::String(self.name.clone())),
                "age" => Some(Value::I64(self.age)),
                "email" => Some(Value::String(self.email.clone())),
                _ => None,
            }
        }
    }

    impl EntityKey<i64> for User {
        fn key(&self) -> i64 {
            self.id
        }
    }

    // ===== WhereOp / WhereCondition =====

    #[test]
    fn test_where_op_name() {
        assert_eq!(WhereOp::Eq.name(), "eq");
        assert_eq!(WhereOp::Ne.name(), "ne");
        assert_eq!(WhereOp::Gt.name(), "gt");
        assert_eq!(WhereOp::Like.name(), "like");
        assert_eq!(WhereOp::In.name(), "in");
        assert_eq!(WhereOp::IsNull.name(), "is_null");
        assert_eq!(WhereOp::Between.name(), "between");
    }

    #[test]
    fn test_where_condition_new() {
        let c = WhereCondition::new("age", WhereOp::Ge, Value::I64(18));
        assert_eq!(c.field, "age");
        assert_eq!(c.op, WhereOp::Ge);
        assert_eq!(c.value, Value::I64(18));
        assert!(c.extra_values.is_empty());
    }

    #[test]
    fn test_where_condition_null_check() {
        let c = WhereCondition::null_check("deleted_at", WhereOp::IsNull);
        assert_eq!(c.field, "deleted_at");
        assert_eq!(c.op, WhereOp::IsNull);
        assert_eq!(c.value, Value::Null);
    }

    #[test]
    fn test_where_condition_in() {
        let c = WhereCondition::in_op(
            "id",
            WhereOp::In,
            vec![Value::I64(1), Value::I64(2), Value::I64(3)],
        );
        assert_eq!(c.field, "id");
        assert_eq!(c.op, WhereOp::In);
        assert_eq!(c.extra_values.len(), 3);
    }

    #[test]
    fn test_where_condition_between() {
        let c = WhereCondition::between("age", Value::I64(18), Value::I64(30));
        assert_eq!(c.field, "age");
        assert_eq!(c.op, WhereOp::Between);
        assert_eq!(c.value, Value::I64(18));
        assert_eq!(c.extra_values, vec![Value::I64(30)]);
    }

    // ===== PageResult =====

    #[test]
    fn test_page_result_total_pages() {
        let pr = PageResult::new(vec![1, 2, 3], 100, 1, 10);
        assert_eq!(pr.total_pages(), 10);
    }

    #[test]
    fn test_page_result_total_pages_with_remainder() {
        let pr: PageResult<i32> = PageResult::new(vec![], 105, 1, 10);
        assert_eq!(pr.total_pages(), 11);
    }

    #[test]
    fn test_page_result_total_pages_zero_size() {
        let pr: PageResult<i32> = PageResult::new(vec![], 100, 1, 0);
        assert_eq!(pr.total_pages(), 0);
    }

    #[test]
    fn test_page_result_has_next() {
        let pr = PageResult::new(vec![1, 2, 3], 100, 1, 10);
        assert!(pr.has_next());
        assert!(!pr.has_prev());
    }

    #[test]
    fn test_page_result_has_prev() {
        let pr = PageResult::new(vec![1, 2, 3], 100, 5, 10);
        assert!(pr.has_prev());
        assert!(pr.has_next()); // page 5 < total_pages 10
    }

    #[test]
    fn test_page_result_is_empty() {
        let pr: PageResult<i32> = PageResult::new(vec![], 0, 1, 10);
        assert!(pr.is_empty());
        assert_eq!(pr.len(), 0);
    }

    #[test]
    fn test_page_result_map() {
        let pr = PageResult::new(vec![1, 2, 3], 100, 1, 10);
        let mapped = pr.map(|x| x * 2);
        assert_eq!(mapped.items, vec![2, 4, 6]);
        assert_eq!(mapped.total, 100);
    }

    // ===== RepositoryError =====

    #[test]
    fn test_repository_error_display() {
        let e = RepositoryError::NotFound;
        assert_eq!(e.to_string(), "entity not found");

        let e = RepositoryError::DatabaseError("conn refused".to_string());
        assert_eq!(e.to_string(), "database error: conn refused");

        let e = RepositoryError::InvalidEntity("missing id".to_string());
        assert_eq!(e.to_string(), "invalid entity: missing id");

        let e = RepositoryError::Other("custom".to_string());
        assert_eq!(e.to_string(), "repository error: custom");
    }

    #[test]
    fn test_repository_error_eq() {
        assert_eq!(RepositoryError::NotFound, RepositoryError::NotFound);
        assert_ne!(
            RepositoryError::NotFound,
            RepositoryError::Other("x".to_string())
        );
    }

    // ===== InMemoryRepository 基础 =====

    #[test]
    fn test_inmemory_create_empty() {
        let repo = InMemoryRepository::<User>::new();
        assert!(repo.is_empty());
        assert_eq!(repo.len(), 0);
    }

    #[test]
    fn test_inmemory_from_vec() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "alice@example.com"),
            User::new(2, "Bob", 25, "bob@example.com"),
        ]);
        assert_eq!(repo.len(), 2);
    }

    #[test]
    fn test_inmemory_clear() {
        let repo = InMemoryRepository::from_vec(vec![User::new(1, "Alice", 30, "a@b.com")]);
        assert_eq!(repo.len(), 1);
        repo.clear();
        assert_eq!(repo.len(), 0);
    }

    // ===== Repository trait CRUD =====

    #[test]
    fn test_repo_save_and_find_by_id() {
        let repo = InMemoryRepository::<User>::new();
        let user = User::new(1, "Alice", 30, "alice@example.com");
        let saved = repo.save(user.clone()).unwrap();
        assert_eq!(saved, user);

        let found = repo.find_by_id(&Value::I64(1)).unwrap();
        assert_eq!(found, Some(user));
    }

    #[test]
    fn test_repo_find_by_id_missing() {
        let repo = InMemoryRepository::<User>::new();
        let found = repo.find_by_id(&Value::I64(999)).unwrap();
        assert_eq!(found, None);
    }

    #[test]
    fn test_repo_save_many() {
        let repo = InMemoryRepository::<User>::new();
        let users = vec![
            User::new(1, "Alice", 30, "alice@example.com"),
            User::new(2, "Bob", 25, "bob@example.com"),
            User::new(3, "Carol", 28, "carol@example.com"),
        ];
        let saved = repo.save_many(users.clone()).unwrap();
        assert_eq!(saved.len(), 3);
        assert_eq!(repo.len(), 3);
    }

    #[test]
    fn test_repo_save_update_existing() {
        let repo = InMemoryRepository::<User>::new();
        repo.save(User::new(1, "Alice", 30, "alice@example.com"))
            .unwrap();

        // 更新
        repo.save(User::new(1, "Alice Updated", 31, "alice2@example.com"))
            .unwrap();

        assert_eq!(repo.len(), 1);
        let found = repo.find_by_id(&Value::I64(1)).unwrap().unwrap();
        assert_eq!(found.name, "Alice Updated");
        assert_eq!(found.age, 31);
    }

    #[test]
    fn test_repo_find_all() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
        ]);
        let all = repo.find_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_repo_find_all_empty() {
        let repo = InMemoryRepository::<User>::new();
        let all = repo.find_all().unwrap();
        assert!(all.is_empty());
    }

    #[test]
    fn test_repo_delete() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
        ]);
        let deleted = repo.delete(&Value::I64(1)).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(repo.len(), 1);
    }

    #[test]
    fn test_repo_delete_missing() {
        let repo = InMemoryRepository::from_vec(vec![User::new(1, "Alice", 30, "a@b.com")]);
        let deleted = repo.delete(&Value::I64(999)).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(repo.len(), 1);
    }

    #[test]
    fn test_repo_count() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
            User::new(3, "Carol", 28, "c@b.com"),
        ]);
        assert_eq!(repo.count().unwrap(), 3);
    }

    #[test]
    fn test_repo_count_empty() {
        let repo = InMemoryRepository::<User>::new();
        assert_eq!(repo.count().unwrap(), 0);
    }

    #[test]
    fn test_repo_exists() {
        let repo = InMemoryRepository::from_vec(vec![User::new(1, "Alice", 30, "a@b.com")]);
        assert!(repo.exists(&Value::I64(1)).unwrap());
        assert!(!repo.exists(&Value::I64(999)).unwrap());
    }

    // ===== Repository 条件查询 =====

    #[test]
    fn test_repo_find_by_eq() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 30, "b@b.com"),
            User::new(3, "Carol", 25, "c@b.com"),
        ]);

        let result = repo
            .find_by(&[WhereCondition::new("age", WhereOp::Eq, Value::I64(30))])
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_repo_find_by_gt() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
            User::new(3, "Carol", 35, "c@b.com"),
        ]);

        let result = repo
            .find_by(&[WhereCondition::new("age", WhereOp::Gt, Value::I64(28))])
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_repo_find_by_like() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "alice@example.com"),
            User::new(2, "Bob", 25, "bob@example.com"),
            User::new(3, "Alicia", 28, "alicia@test.com"),
        ]);

        let result = repo
            .find_by(&[WhereCondition::new(
                "name",
                WhereOp::Like,
                Value::String("Ali%".to_string()),
            )])
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_repo_find_by_in() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
            User::new(3, "Carol", 28, "c@b.com"),
            User::new(4, "Dave", 32, "d@b.com"),
        ]);

        let result = repo
            .find_by(&[WhereCondition::in_op(
                "id",
                WhereOp::In,
                vec![Value::I64(1), Value::I64(3)],
            )])
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_repo_find_by_not_in() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
            User::new(3, "Carol", 28, "c@b.com"),
        ]);

        let result = repo
            .find_by(&[WhereCondition::in_op(
                "id",
                WhereOp::NotIn,
                vec![Value::I64(1)],
            )])
            .unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|u| u.id != 1));
    }

    #[test]
    fn test_repo_find_by_between() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
            User::new(3, "Carol", 35, "c@b.com"),
            User::new(4, "Dave", 22, "d@b.com"),
        ]);

        let result = repo
            .find_by(&[WhereCondition::between(
                "age",
                Value::I64(25),
                Value::I64(35),
            )])
            .unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_repo_find_by_multiple_conditions() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 30, "b@b.com"),
            User::new(3, "Alice", 25, "c@b.com"),
        ]);

        let result = repo
            .find_by(&[
                WhereCondition::new("name", WhereOp::Eq, Value::String("Alice".to_string())),
                WhereCondition::new("age", WhereOp::Ge, Value::I64(30)),
            ])
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
    }

    #[test]
    fn test_repo_find_one_by() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
        ]);

        let result = repo
            .find_one_by(&[WhereCondition::new(
                "name",
                WhereOp::Eq,
                Value::String("Bob".to_string()),
            )])
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 2);
    }

    #[test]
    fn test_repo_find_one_by_missing() {
        let repo = InMemoryRepository::from_vec(vec![User::new(1, "Alice", 30, "a@b.com")]);

        let result = repo
            .find_one_by(&[WhereCondition::new(
                "name",
                WhereOp::Eq,
                Value::String("Missing".to_string()),
            )])
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_repo_count_by() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 30, "b@b.com"),
            User::new(3, "Carol", 25, "c@b.com"),
        ]);

        let count = repo
            .count_by(&[WhereCondition::new("age", WhereOp::Eq, Value::I64(30))])
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_repo_delete_by() {
        let repo = InMemoryRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 30, "b@b.com"),
            User::new(3, "Carol", 25, "c@b.com"),
        ]);

        let deleted = repo
            .delete_by(&[WhereCondition::new("age", WhereOp::Eq, Value::I64(30))])
            .unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(repo.len(), 1);
    }

    // ===== 分页 =====

    #[test]
    fn test_repo_paginate() {
        let users: Vec<User> = (1..=25)
            .map(|i| User::new(i, &format!("User{}", i), 20 + (i % 30), "u@b.com"))
            .collect();
        let repo = InMemoryRepository::from_vec(users);

        let page = repo.paginate(1, 10).unwrap();
        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 10);
        assert_eq!(page.total, 25);
        assert_eq!(page.total_pages(), 3);
        assert_eq!(page.items.len(), 10);
        assert!(page.has_next());
        assert!(!page.has_prev());
    }

    #[test]
    fn test_repo_paginate_last_page() {
        let users: Vec<User> = (1..=25)
            .map(|i| User::new(i, &format!("User{}", i), 20, "u@b.com"))
            .collect();
        let repo = InMemoryRepository::from_vec(users);

        let page = repo.paginate(3, 10).unwrap();
        assert_eq!(page.items.len(), 5);
        assert!(page.has_prev());
        assert!(!page.has_next());
    }

    #[test]
    fn test_repo_paginate_out_of_range() {
        let users: Vec<User> = (1..=5)
            .map(|i| User::new(i, &format!("User{}", i), 20, "u@b.com"))
            .collect();
        let repo = InMemoryRepository::from_vec(users);

        let page = repo.paginate(10, 10).unwrap();
        assert_eq!(page.items.len(), 0);
        assert_eq!(page.total, 5);
    }

    #[test]
    fn test_repo_paginate_by() {
        let users: Vec<User> = (1..=20)
            .map(|i| User::new(i, &format!("User{}", i), 20 + (i % 5), "u@b.com"))
            .collect();
        let repo = InMemoryRepository::from_vec(users);

        // age=22 的用户：i=2,7,12,17 → 4 个
        let page = repo
            .paginate_by(
                &[WhereCondition::new("age", WhereOp::Eq, Value::I64(22))],
                1,
                2,
            )
            .unwrap();
        assert_eq!(page.total, 4);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total_pages(), 2);
    }

    // ===== GenericKeyRepository =====

    #[test]
    fn test_generic_key_repo_basic() {
        let repo: GenericKeyRepository<User, i64> = GenericKeyRepository::new();
        assert!(repo.is_empty());

        let user = User::new(1, "Alice", 30, "a@b.com");
        repo.save(user.clone()).unwrap();
        assert_eq!(repo.len(), 1);

        let found = repo.find_by_id(&1).unwrap();
        assert_eq!(found, Some(user));
    }

    #[test]
    fn test_generic_key_repo_delete() {
        let repo: GenericKeyRepository<User, i64> = GenericKeyRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
        ]);

        let deleted = repo.delete(&1).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(repo.len(), 1);

        let remaining = repo.find_all().unwrap();
        assert_eq!(remaining[0].id, 2);
    }

    #[test]
    fn test_generic_key_repo_find_by() {
        let repo: GenericKeyRepository<User, i64> = GenericKeyRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 30, "b@b.com"),
            User::new(3, "Carol", 25, "c@b.com"),
        ]);

        let result = repo
            .find_by(&[WhereCondition::new("age", WhereOp::Eq, Value::I64(30))])
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_generic_key_repo_paginate() {
        let users: Vec<User> = (1..=15)
            .map(|i| User::new(i, &format!("User{}", i), 20, "u@b.com"))
            .collect();
        let repo: GenericKeyRepository<User, i64> = GenericKeyRepository::from_vec(users);

        let page = repo.paginate(2, 10).unwrap();
        assert_eq!(page.items.len(), 5);
        assert_eq!(page.total, 15);
        assert_eq!(page.page, 2);
    }

    #[test]
    fn test_generic_key_repo_count() {
        let repo: GenericKeyRepository<User, i64> = GenericKeyRepository::from_vec(vec![
            User::new(1, "Alice", 30, "a@b.com"),
            User::new(2, "Bob", 25, "b@b.com"),
        ]);
        assert_eq!(repo.count().unwrap(), 2);
    }

    #[test]
    fn test_generic_key_repo_exists() {
        let repo: GenericKeyRepository<User, i64> =
            GenericKeyRepository::from_vec(vec![User::new(1, "Alice", 30, "a@b.com")]);
        assert!(repo.exists(&1).unwrap());
        assert!(!repo.exists(&999).unwrap());
    }

    // ===== simple_regex =====

    #[test]
    fn test_simple_regex_literal() {
        let re = simple_regex::compile("^abc$").unwrap();
        assert!(re.is_match("abc"));
        assert!(!re.is_match("abcd"));
    }

    #[test]
    fn test_simple_regex_wildcard() {
        let re = simple_regex::compile("^Ali.*$").unwrap();
        assert!(re.is_match("Alice"));
        assert!(re.is_match("Alicia"));
        assert!(!re.is_match("Bob"));
    }

    #[test]
    fn test_simple_regex_no_anchors() {
        let re = simple_regex::compile("ab").unwrap();
        assert!(re.is_match("ab"));
    }

    // ===== 端到端场景 =====

    #[test]
    fn test_e2e_repository_workflow() {
        let repo = InMemoryRepository::<User>::new();

        // 1. 批量插入
        let users = vec![
            User::new(1, "Alice", 30, "alice@example.com"),
            User::new(2, "Bob", 25, "bob@example.com"),
            User::new(3, "Carol", 35, "carol@example.com"),
            User::new(4, "Dave", 28, "dave@example.com"),
            User::new(5, "Eve", 32, "eve@example.com"),
        ];
        repo.save_many(users).unwrap();
        assert_eq!(repo.count().unwrap(), 5);

        // 2. 查找成年人（age >= 30）
        let adults = repo
            .find_by(&[WhereCondition::new("age", WhereOp::Ge, Value::I64(30))])
            .unwrap();
        assert_eq!(adults.len(), 3);

        // 3. 分页查询
        let page1 = repo.paginate(1, 2).unwrap();
        assert_eq!(page1.items.len(), 2);
        assert_eq!(page1.total_pages(), 3);

        let page2 = repo.paginate(2, 2).unwrap();
        assert_eq!(page2.items.len(), 2);

        let page3 = repo.paginate(3, 2).unwrap();
        assert_eq!(page3.items.len(), 1);

        // 4. 条件分页查询（age >= 30）
        let adult_page = repo
            .paginate_by(
                &[WhereCondition::new("age", WhereOp::Ge, Value::I64(30))],
                1,
                2,
            )
            .unwrap();
        assert_eq!(adult_page.total, 3);
        assert_eq!(adult_page.items.len(), 2);

        // 5. 更新
        repo.save(User::new(1, "Alice Smith", 31, "alice.smith@example.com"))
            .unwrap();
        let updated = repo.find_by_id(&Value::I64(1)).unwrap().unwrap();
        assert_eq!(updated.name, "Alice Smith");
        assert_eq!(updated.age, 31);

        // 6. 删除
        let deleted = repo.delete(&Value::I64(2)).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(repo.count().unwrap(), 4);
        assert!(!repo.exists(&Value::I64(2)).unwrap());

        // 7. 条件删除 age >= 31：Alice(31) + Carol(35) + Eve(32) = 3 个
        let deleted_by = repo
            .delete_by(&[WhereCondition::new("age", WhereOp::Ge, Value::I64(31))])
            .unwrap();
        assert_eq!(deleted_by, 3);
        assert_eq!(repo.count().unwrap(), 1); // 仅剩 Dave(28)
    }

    #[test]
    fn test_e2e_pagination_navigation() {
        let users: Vec<User> = (1..=100)
            .map(|i| User::new(i, &format!("User{}", i), 20, "u@b.com"))
            .collect();
        let repo = InMemoryRepository::from_vec(users);

        let mut current_page = 1u64;
        let mut visited: Vec<u64> = Vec::new();
        loop {
            let page = repo.paginate(current_page, 10).unwrap();
            visited.push(current_page);
            if !page.has_next() {
                break;
            }
            current_page += 1;
        }
        assert_eq!(visited.len(), 10);
        assert_eq!(visited, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }
}
